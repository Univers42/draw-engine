//! What the pointer would do if you pressed it here.
//!
//! The canvas is one element, so the only way to tell someone that the pixel under their
//! pointer resizes rather than moves is the cursor. Without it every part of a selected
//! shape looks identical and the difference is only discovered by dragging — which is
//! exactly how a resize that was meant to be a move happens.
//!
//! This answers the question without mutating anything, so it can run on hover at pointer
//! rate. It is deliberately a `u8` rather than a string or JSON: it crosses the WASM
//! boundary on every mouse move, and a returned `String` would allocate on both sides
//! hundreds of times a second to say "default" over and over.

use crate::engine::{DrawEngine, Interaction};
use crate::interaction::{is_linear_tool, is_shape_tool, DrawTool};
use crate::selection::{hit_handle, selection_handles, HandleKind};

/// The cursor the host should show. Values are part of the JS contract — append only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HoverCursor {
    /// Nothing under the pointer; the host falls back to its per-tool cursor.
    Default = 0,
    /// Over something that would be picked up and dragged.
    Move = 1,
    ResizeNs = 2,
    ResizeEw = 3,
    ResizeNesw = 4,
    ResizeNwse = 5,
    /// Over the rotation handle, or panning.
    Grab = 6,
    /// A drag is in progress.
    Grabbing = 7,
    /// Over a point handle on a line or arrow.
    PointHandle = 8,
    Crosshair = 9,
    Text = 10,
}

impl HoverCursor {
    pub fn code(self) -> u8 {
        self as u8
    }
}

/// The four resize cursors in the order they rotate through.
///
/// Excalidraw's `rotateResizeCursor`: the cursor is turned with the element to the
/// nearest 45 degrees, so the north handle of a shape lying on its side reads as
/// left-right rather than up-down. Without it a rotated shape's handles all claim to
/// resize along the screen axes, which is never what they do.
const CURSOR_ORDER: [HoverCursor; 4] = [
    HoverCursor::ResizeNs,
    HoverCursor::ResizeNesw,
    HoverCursor::ResizeEw,
    HoverCursor::ResizeNwse,
];

fn resize_cursor(kind: HandleKind, angle: f64) -> HoverCursor {
    let base = match kind {
        HandleKind::N | HandleKind::S => 0,
        HandleKind::Ne | HandleKind::Sw => 1,
        HandleKind::E | HandleKind::W => 2,
        HandleKind::Nw | HandleKind::Se => 3,
        HandleKind::Rotate => return HoverCursor::Grab,
    };
    let steps = (angle / std::f64::consts::FRAC_PI_4).round();
    // `rem_euclid` rather than `%`. Excalidraw indexes with a plain JS remainder, which
    // goes negative for a counter-clockwise angle and yields `undefined` — silently no
    // cursor at all. Wrapping properly is the same answer everywhere their version works
    // and a correct one where it does not.
    let index = (base as f64 + steps).rem_euclid(CURSOR_ORDER.len() as f64) as usize;
    CURSOR_ORDER[index]
}

impl DrawEngine {
    /// The cursor for the pointer at `(sx, sy)`, in screen pixels.
    pub fn hover_cursor(&self, sx: f64, sy: f64) -> HoverCursor {
        // A drag in progress outranks whatever is under the pointer: the cursor must not
        // flicker as a dragged shape passes over other things.
        if let Some(active) = self.cursor_for_active_interaction() {
            return active;
        }

        match self.tool {
            DrawTool::Hand => return HoverCursor::Grab,
            DrawTool::Text => return HoverCursor::Text,
            DrawTool::Eraser | DrawTool::Freedraw | DrawTool::Lasso => {
                return HoverCursor::Crosshair
            }
            tool if is_shape_tool(tool) || is_linear_tool(tool) => {
                return HoverCursor::Crosshair;
            }
            _ => {}
        }

        let world = self.screen_to_world(sx, sy);

        if let Some(single) = self.single_selected() {
            if !single.locked() {
                // Same order as `begin_select`, so what the cursor promises is what a
                // press does.
                if self.radius_handle_at(world).is_some() {
                    return HoverCursor::PointHandle;
                }
                if self.shows_point_handles(&single) {
                    let min_segment = super::LINEAR_MIDPOINT_MIN_PX / self.camera.scale;
                    let handles = crate::selection::linear::handle_points(&single, min_segment);
                    let tol = super::HANDLE_HIT_PX / self.camera.scale;
                    if crate::selection::linear::hit_handle(&handles, world.x, world.y, tol)
                        .is_some()
                    {
                        return HoverCursor::PointHandle;
                    }
                } else {
                    let layout = self.handle_layout();
                    if let Some(kind) = hit_handle(
                        &selection_handles(&single, layout),
                        world.x,
                        world.y,
                        layout.hit,
                    ) {
                        return resize_cursor(kind, single.angle);
                    }
                }
            }
        } else if self.selected_ids.len() > 1 {
            if let Some(kind) = self.group_handle_at(world) {
                // A group's frame is axis-aligned however its members are turned, so the
                // cursor is too.
                return resize_cursor(kind, 0.0);
            }
        }

        let reach = self.collision_tolerance();
        // Anything grabbable under the pointer reads as movable. Locked elements
        // deliberately do not: they are not draggable, so promising otherwise is worse
        // than saying nothing.
        if self
            .scene
            .iter_ordered()
            .rev()
            .any(|el| !el.locked() && crate::hit_test_element(el, world.x, world.y, reach))
        {
            return HoverCursor::Move;
        }

        HoverCursor::Default
    }

    fn cursor_for_active_interaction(&self) -> Option<HoverCursor> {
        let it = self.interaction.as_ref()?;
        Some(match it {
            Interaction::Pan { .. } => HoverCursor::Grabbing,
            Interaction::Move { .. } => HoverCursor::Grabbing,
            Interaction::Rotate { .. } | Interaction::RotateGroup { .. } => HoverCursor::Grabbing,
            Interaction::LinearPoint { .. } | Interaction::CornerRadius { .. } => {
                HoverCursor::PointHandle
            }
            Interaction::Resize { id, handle, .. } => {
                let angle = self.scene.get(id).map(|el| el.angle).unwrap_or(0.0);
                resize_cursor(*handle, angle)
            }
            Interaction::ResizeGroup { handle, .. } => resize_cursor(*handle, 0.0),
            Interaction::Draft { .. }
            | Interaction::TextDraft { .. }
            | Interaction::Linear { .. }
            | Interaction::MultiLinearPress
            | Interaction::Freedraw { .. } => HoverCursor::Crosshair,
            Interaction::Erase { .. } => HoverCursor::Crosshair,
            Interaction::Marquee { .. } => HoverCursor::Default,
            Interaction::Lasso { .. } => HoverCursor::Crosshair,
            // The trail is the pointer here, so a crosshair marks exactly where the beam
            // comes from — an arrow would sit beside its own tip.
            Interaction::Laser => HoverCursor::Crosshair,
        })
    }
}
