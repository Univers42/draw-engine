//! A structured view of what the engine is doing, for an agent or a human to inspect.
//!
//! # Why this exists
//!
//! The scene, geometry, hit testing and selection all live in Rust compiled to WASM.
//! None of it is reachable from the DOM, so anyone debugging this editor from outside —
//! a person in devtools, or an agent driving a browser — has had to infer state from
//! pixels. Inferring "that is probably a rectangle" from a screenshot is not engineering,
//! and a screenshot cannot answer the questions that actually matter: which element is
//! selected, what the camera is, how many elements were drawn, whether the shape cache is
//! being hit.
//!
//! So the engine says what it knows, in one call.
//!
//! # What is here and what is not
//!
//! Everything in [`DebugState`] is state the engine **already holds** — no work is done to
//! produce it and nothing is cached for it. Timings live in the WASM layer instead
//! ([`crate::wasm`]), because a frame is only visible from the frame loop, and clocks
//! belong to the host rather than to a runtime-agnostic core.
//!
//! Three things a debugger usually wants are deliberately absent, and their absence is
//! itself worth recording:
//!
//! - **Dirty regions.** There are none. `take_dirty` is a boolean and the whole canvas
//!   repaints, so "half the screen updated" is not a failure this architecture can have.
//! - **Allocation counts.** Not observable from inside WASM without an allocator shim,
//!   which would cost more than it tells.
//! - **State-update counts.** There is no reconciler to count. [`DebugScene::revision`] is
//!   the closest true analogue: it changes when the scene does.

use serde::Serialize;

use crate::engine::{DrawEngine, Interaction};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugState {
    pub scene: DebugScene,
    pub viewport: DebugViewport,
    pub interaction: DebugInteraction,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugScene {
    /// Live elements. Tombstones are excluded, because they are a synchronisation
    /// detail rather than something anyone drew.
    pub element_count: usize,
    pub deleted_count: usize,
    pub selected_count: usize,
    pub selected_ids: Vec<String>,
    /// Changes whenever the scene does. The nearest thing here to "state updates" in a
    /// framework that has them.
    pub revision: u64,
    pub can_undo: bool,
    pub can_redo: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugViewport {
    pub x: f64,
    pub y: f64,
    /// The camera scale. 1.0 is 100%.
    pub zoom: f64,
    /// CSS pixels. The backing store is this times `dpr`, and a factor-of-two error
    /// between the two is always a device-pixel-ratio mistake.
    pub width: f64,
    pub height: f64,
    pub dpr: f64,
    /// World-space rectangle currently on screen — what culling keeps.
    pub visible_world: [f64; 4],
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugInteraction {
    pub tool: String,
    pub tool_locked: bool,
    /// The gesture in progress, by name, or `null` between gestures.
    pub kind: Option<&'static str>,
    pub dragging: bool,
    pub resizing: bool,
    /// The linear element being placed point by point, if one is.
    pub placing_linear: Option<String>,
    /// The linear element whose points are on offer.
    pub editing_linear: Option<String>,
}

/// The name of a gesture, for a human reading a snapshot.
///
/// A plain `&'static str` rather than the enum: the variants carry gesture state that is
/// meaningless outside the engine, and a debugger wants to know *which* gesture, not its
/// contents.
fn interaction_kind(interaction: &Interaction) -> &'static str {
    match interaction {
        Interaction::Draft { .. } => "draft",
        Interaction::TextDraft { .. } => "text-draft",
        Interaction::Linear { .. } => "linear",
        Interaction::MultiLinearPress => "multi-linear-press",
        Interaction::Freedraw { .. } => "freedraw",
        Interaction::Erase { .. } => "erase",
        Interaction::Pan { .. } => "pan",
        Interaction::Move { .. } => "move",
        Interaction::Resize { .. } => "resize",
        Interaction::Rotate { .. } => "rotate",
        Interaction::ResizeGroup { .. } => "resize-group",
        Interaction::RotateGroup { .. } => "rotate-group",
        Interaction::LinearPoint { .. } => "linear-point",
        Interaction::Lasso { .. } => "lasso",
        Interaction::Laser => "laser",
        Interaction::Marquee { .. } => "marquee",
    }
}

impl DrawEngine {
    /// Everything the engine knows about its own state, in one call.
    ///
    /// Cheap enough to poll: one scene walk for the counts, and nothing else. Nothing is
    /// computed that is not already held.
    pub fn debug_state(&self) -> DebugState {
        // `iter_ordered` already excludes tombstones, so the difference is the dead.
        let element_count = self.scene.iter_ordered().count();
        let deleted_count = self.scene.total_len().saturating_sub(element_count);

        let view = crate::visible_world_rect(self.camera, self.width, self.height);
        let kind = self.interaction.as_ref().map(interaction_kind);

        DebugState {
            scene: DebugScene {
                element_count,
                deleted_count,
                selected_count: self.selected_ids.len(),
                selected_ids: {
                    // Sorted, so two snapshots of the same selection compare equal. A
                    // HashSet's order is not stable between runs and would make every
                    // differential comparison fail for no reason.
                    let mut ids: Vec<String> = self.selected_ids.iter().cloned().collect();
                    ids.sort();
                    ids
                },
                revision: self.scene.revision(),
                can_undo: self.history.can_undo(),
                can_redo: self.history.can_redo(),
            },
            viewport: DebugViewport {
                x: self.camera.x,
                y: self.camera.y,
                zoom: self.camera.scale,
                width: self.width,
                height: self.height,
                dpr: self.dpr,
                visible_world: [view.min_x, view.min_y, view.max_x, view.max_y],
            },
            interaction: DebugInteraction {
                tool: self.tool.as_str().to_string(),
                tool_locked: self.tool_locked,
                kind,
                dragging: matches!(kind, Some("move" | "draft" | "freedraw" | "linear")),
                resizing: matches!(kind, Some("resize" | "resize-group" | "linear-point")),
                placing_linear: self.linear_in_progress(),
                editing_linear: self.editing_linear.clone(),
            },
        }
    }

    /// [`Self::debug_state`] as JSON, for the WASM boundary.
    pub fn debug_state_json(&self) -> String {
        serde_json::to_string(&self.debug_state()).unwrap_or_else(|_| "{}".into())
    }
}
