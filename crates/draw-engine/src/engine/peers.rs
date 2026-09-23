//! The other people in the room: what each of them holds, and what their gesture in
//! progress looks like right now.
//!
//! Two things came up drawing together. A shape someone was drawing or moving appeared
//! on everyone else's screen only when they let go — nothing moved in between. And two
//! people could take hold of the same shape at once: one erased it while the other was
//! moving it, and the other's next edit brought it back. Correct, by the merge rule, and
//! nothing like what either of them meant.
//!
//! So the host tells the engine, as it learns it from the live link:
//!
//! - **what each peer holds** — what they have selected, and what their gesture is
//!   changing. Nobody else can touch it: it cannot be selected, moved, resized, edited,
//!   deleted or erased, exactly as if it were locked, and it is outlined in that peer's
//!   colour with their name on it, the way Figma shows who is where;
//! - **their gesture in progress** — the elements as they are this instant, before the
//!   gesture is committed. They are painted in place of the committed ones, so a shape
//!   moves on every screen while it is being moved, and grows while it is being drawn.
//!   Painted only: they are not in the scene, not in its history, not saved. The commit
//!   arrives afterwards as an ordinary patch.
//!
//! Who holds what when two people take the same shape at the same moment is the host's
//! to settle before it gets here (`peerClaims.ts`): both sides see both claims and pick
//! the same winner. The engine only obeys — and when what it had selected is now held
//! by someone else, it lets go.

use std::collections::HashMap;

use crate::engine::DrawEngine;
use crate::scene::DrawElement;

/// One other person in the room, as the host last heard of them.
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub id: String,
    pub name: String,
    /// A CSS colour: their cursor's, their outline's.
    pub color: String,
    /// What they hold: nobody else may touch these.
    #[serde(default)]
    pub holds: Vec<String>,
    /// Their gesture in progress: these elements as they are right now, uncommitted.
    #[serde(default)]
    pub preview: Vec<DrawElement>,
}

impl DrawEngine {
    /// Replaces what the engine knows of the other people in the room.
    pub fn set_peers(&mut self, mut peers: Vec<Peer>) {
        // Read the way a scene is, so a preview from an engine a version behind still
        // paints.
        for element in peers.iter_mut().flat_map(|peer| peer.preview.iter_mut()) {
            crate::scene::element::normalize_group_ids(element);
        }
        let mut held = HashMap::new();
        for (index, peer) in peers.iter().enumerate() {
            let previewed = peer.preview.iter().map(|element| &element.id);
            for id in peer.holds.iter().chain(previewed) {
                held.entry(id.clone()).or_insert(index);
            }
        }
        self.peers = peers;
        self.held = held;

        // What someone else now holds is no longer ours. A gesture on it is abandoned —
        // put back as it was — rather than committed over their work.
        let lost: Vec<String> = self
            .selected_ids
            .iter()
            .filter(|id| self.held.contains_key(*id))
            .cloned()
            .collect();
        if !lost.is_empty() {
            if self
                .local_live_ids()
                .iter()
                .any(|id| self.held.contains_key(id))
            {
                self.abandon_gesture();
            }
            let kept: Vec<String> = self
                .selected_ids
                .iter()
                .filter(|id| !self.held.contains_key(*id))
                .cloned()
                .collect();
            self.set_selection(kept);
        }
        self.refresh_live();
        self.request_draw();
    }

    /// Puts everything the gesture in progress changed back as it was, and ends it.
    ///
    /// Not `cancel_pointer`, which commits a drag where it was left — right for Escape,
    /// where the change is yours to keep, and wrong here, where it would be stamped over
    /// the work of whoever holds it now. Committed after the restore, so what was
    /// refused from peers during the gesture is taken as it is for any gesture that came
    /// to nothing (`stamp.rs`).
    fn abandon_gesture(&mut self) {
        self.interaction = None;
        self.snap_guides.clear();
        for (id, before) in self.scene.baseline() {
            match before {
                Some(before) => self.scene.put((*before).clone()),
                None => self.scene.discard(&id),
            }
        }
        self.push_history();
        self.refresh_live();
    }

    /// The peer holding `id`, if anyone does.
    pub fn held_by(&self, id: &str) -> Option<&Peer> {
        self.held.get(id).and_then(|&index| self.peers.get(index))
    }

    /// Whether an element may not be touched here: locked, or held by someone else.
    pub(crate) fn untouchable(&self, element: &DrawElement) -> bool {
        element.locked() || self.held.contains_key(&element.id)
    }

    /// The peer holding the topmost element under a screen point, if one does — so a
    /// click on something in use can say who is using it instead of doing nothing.
    pub fn peer_at(&self, sx: f64, sy: f64) -> Option<&Peer> {
        let world = self.screen_to_world(sx, sy);
        let tolerance = self.collision_tolerance();
        let topmost = self
            .scene
            .iter_ordered()
            .rev()
            .find(|el| crate::hit_test_element(el, world.x, world.y, tolerance))?;
        self.held_by(&topmost.id)
    }

    /// The elements this engine's gesture in progress is changing, as they are right now
    /// — what the host sends peers while the gesture runs. Empty between gestures.
    pub fn gesture_elements(&self) -> Vec<DrawElement> {
        let live = self.local_live_ids();
        if live.is_empty() {
            return Vec::new();
        }
        self.scene
            .iter_ordered()
            .filter(|element| live.contains(&element.id))
            .cloned()
            .collect()
    }

    /// Every element a peer is previewing, by id: painted in place of the scene's.
    pub(crate) fn previews(&self) -> HashMap<&str, &DrawElement> {
        self.current_previews()
            .map(|element| (element.id.as_str(), element))
            .collect()
    }

    /// The ids peers are previewing, which the painter treats as live.
    pub(crate) fn previewed_ids(&self) -> impl Iterator<Item = &String> {
        self.current_previews().map(|element| &element.id)
    }

    /// The previews not yet overtaken by a commit.
    ///
    /// A gesture's preview carries the version the element had before it; the commit
    /// stamps above that. So once the commit has landed here the scene outranks the
    /// preview and is shown instead — whichever of the commit and the end of the preview
    /// arrives first, the shape never jumps back to where it was.
    fn current_previews(&self) -> impl Iterator<Item = &DrawElement> {
        self.peers
            .iter()
            .flat_map(|peer| peer.preview.iter())
            .filter(|preview| {
                self.scene
                    .get(&preview.id)
                    .is_none_or(|committed| committed.version <= preview.version)
            })
    }

    pub fn peers(&self) -> &[Peer] {
        &self.peers
    }
}
