//! Version stamps: when an element gets a new one, and what undo does with them.
//!
//! `version` / `versionNonce` is the **only** change signal anything outside the engine
//! has. The host's autosave sends an element when its stamp differs from the last one
//! the server acknowledged; the server's merge and every peer keep the higher stamp. An
//! edit that does not move the stamp is invisible to all three.
//!
//! Gestures did not move it. A drag, a resize, a rotation, a nudge, a point dragged on
//! a line, an arrow re-routed after its shape — all of them changed geometry and left
//! the stamp alone, so none of them was ever saved, and a peer's later recolour of the
//! same element (stamped, from the unmoved copy) reverted the move on every screen
//! including the one that made it.
//!
//! # Once per commit, against the element as it was first touched
//!
//! Excalidraw stamps on every mutation (`mutateElement.ts:142-144`), so a drag bumps
//! once per pointer event. Here nothing leaves the engine between commits — the host is
//! told at `push_history` — so one stamp per element per commit says the same thing.
//! The scene records each element as it was before the first change since the last
//! commit (its *baseline*), and the commit compares against that:
//!
//! - **did not exist** — keep the stamp it was created with, so a shape drawn by hand
//!   commits at version 1 however many moves sized it;
//! - **unchanged** — nothing, so a drag away and back records no step and sends
//!   nothing;
//! - **changed, stamp unchanged** — bump;
//! - **changed, already re-stamped** by a path that bumps its own (style, radius,
//!   group, …) — nothing more: one edit, one bump.
//!
//! The baseline is taken at the first touch, not read from the last commit, because a
//! peer's patch can land in between. Comparing a peer's fresh element with a snapshot
//! that predates it took it for new and never stamped the move; comparing a peer's
//! re-stamped element with its stale copy took the peer's stamp for ours.
//!
//! # A gesture in progress wins
//!
//! A peer's copy of an element with an uncommitted local change is refused, as
//! Excalidraw refuses it while the element is being edited (`data/reconcile.ts:31-33`):
//! taking it would hand the gesture's final state the peer's stamp. The refused copy is
//! kept. If the gesture changed the element, the commit stamps *above* it — the later
//! edit wins, here and everywhere it is sent. If the gesture came to nothing, the copy
//! is adopted then, rather than lost.
//!
//! # Undo is a new edit, of only what the step changed
//!
//! Restoring a snapshot restored its *stamps* too, so an undone edit came back with a
//! version lower than the one already sent — refused by the server and by every peer as
//! stale. Excalidraw applies undo "as a new user action" with fresh stamps
//! (`history.ts:24-34`, `delta.ts:1732-1781`), and so does this.
//!
//! And each step records the elements it changed, before and after, so undo puts back
//! those and nothing else — beyond the arrows bound to them, which follow them as they
//! follow any edit. Whole-scene snapshots carried every peer edit that had arrived before
//! the commit, and restoring one reverted them — with fresh stamps, now, that revert
//! would have been sent to everyone.
//!
//! # Undo puts the selection back
//!
//! A step also records what was selected, and the group being edited, when it began and
//! when it was committed; undo selects the first again and redo the second, keeping what
//! is still on the board. The oracle's entries carry the same in their app-state delta
//! (`AppStateDelta`, `packages/element/src/delta.ts@1118751f:526-1015`). Every undo used to
//! let go of everything, which put the properties panel away under the Ctrl+Z meant to
//! check what it showed.
//!
//! "When it began" is the oracle's store snapshot: the selection as the last gesture or
//! command left it (`store.ts@1118751f:376-385`, captured at pointer-up,
//! `App.tsx@1118751f:12451-12464`), never as a press changes it. So a shape dragged from
//! unselected comes back unselected, as on excalidraw.com. Settled here whenever the
//! selection changes with no pointer down and nothing pending, at every release, and at
//! every commit ([`DrawEngine::settle_selection`]).
//!
//! A divergence: the oracle also records a change of selection alone as a step
//! (`history.ts@1118751f:117-137`), so its Ctrl+Z after a click reselects what the click
//! let go of. Here a step is only recorded when it changed the board, and a click is not
//! one — undo always undoes an edit.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::engine::DrawEngine;
use crate::scene::element::rand_int;
use crate::scene::DrawElement;

/// One element's part in a step: as it was before, and as the step left it. `None`
/// means absent — created by the step, or discarded by it.
#[derive(Clone, Debug)]
pub(crate) struct Change {
    pub before: Option<Rc<DrawElement>>,
    pub after: Option<Rc<DrawElement>>,
}

/// What was selected, and the group being edited: see "Undo puts the selection back".
#[derive(Clone, Debug, Default)]
pub(crate) struct SelectionState {
    pub ids: HashSet<String>,
    pub editing: Option<String>,
}

/// One step of undo history.
#[derive(Clone, Debug, Default)]
pub(crate) struct HistoryEntry {
    /// Identifies the entry to `SnapshotHistory`, whose deduplication by signature is
    /// not wanted here: a step is only recorded when it changed something.
    pub seq: u64,
    pub changes: Rc<HashMap<String, Change>>,
    /// The z-order before and after, when the step reordered anything.
    pub order: Option<Rc<(Vec<String>, Vec<String>)>>,
    /// The selection the step began with, and the one it left.
    pub selection: (Rc<SelectionState>, Rc<SelectionState>),
}

/// Equal in everything but the stamp.
pub(super) fn same_content(a: &DrawElement, b: &DrawElement) -> bool {
    let mut a = a.clone();
    a.version = b.version;
    a.version_nonce = b.version_nonce;
    a.updated = b.updated;
    a == *b
}

fn same_stamp(a: &DrawElement, b: &DrawElement) -> bool {
    a.version == b.version && a.version_nonce == b.version_nonce
}

fn stamped(mut element: DrawElement, version: u32, now: f64) -> DrawElement {
    element.version = version;
    element.version_nonce = rand_int();
    element.updated = now;
    element
}

impl DrawEngine {
    /// Stamps what this commit changed, settles refused peer copies, and returns the
    /// step to record — `None` when nothing changed. `selected` is what the step leaves
    /// selected.
    pub(super) fn take_local_step(
        &mut self,
        selected: &Rc<SelectionState>,
    ) -> Option<HistoryEntry> {
        let baseline = self.scene.take_baseline();
        let order_before = self.scene.take_order_baseline();
        let refused = std::mem::take(&mut self.remote_refused);
        let clock = self.now_ms;

        let mut changes: HashMap<String, Change> = HashMap::new();
        for (id, before) in baseline {
            let now = self.scene.get_rc(&id).cloned();
            let peer = refused.get(&id);
            let unchanged = match (&before, &now) {
                (None, None) => true,
                (Some(b), Some(n)) => Rc::ptr_eq(b, n) || (same_stamp(b, n) && same_content(b, n)),
                _ => false,
            };
            if unchanged {
                // The gesture came to nothing. A peer's copy refused meanwhile is the
                // newest thing anyone has, and taking it now is the only way to have it.
                if let (Some(peer), Some(now)) = (peer, &now) {
                    if super::clipboard::remote_wins(peer, now) {
                        let mut taken = peer.clone();
                        super::clipboard::inherit_picture(&mut taken, now);
                        self.scene.put(taken);
                    }
                }
                continue;
            }

            let mut after = now.clone();
            if let Some(n) = &now {
                let floor = peer.map_or(0, |p| p.version);
                let unstamped = before.as_ref().is_some_and(|b| same_stamp(b, n));
                if unstamped || n.version <= floor {
                    let version = n.version.max(floor) + 1;
                    self.scene.update(&id, |element| {
                        element.version = version;
                        element.version_nonce = rand_int();
                        element.updated = clock;
                    });
                    after = self.scene.get_rc(&id).cloned();
                }
            }
            changes.insert(id, Change { before, after });
        }
        // Stamping and adopting are not themselves edits for the next commit to find.
        let _ = self.scene.take_baseline();

        let order = order_before
            .map(|before| (before, self.scene.ids()))
            .filter(|(before, after)| before != after);
        if changes.is_empty() && order.is_none() {
            return None;
        }
        self.history_seq += 1;
        Some(HistoryEntry {
            seq: self.history_seq,
            changes: Rc::new(changes),
            order: order.map(Rc::new),
            selection: (Rc::clone(&self.settled_selection), Rc::clone(selected)),
        })
    }

    /// What is selected now, and the group being edited.
    pub(super) fn selection_state(&self) -> SelectionState {
        SelectionState {
            ids: self.selected_ids.clone(),
            editing: self.editing_group_id.clone(),
        }
    }

    /// Takes the selection as the one the next step begins with — unless a pointer is down
    /// or a change is pending, when it is still the gesture's or the command's own. See
    /// "Undo puts the selection back".
    pub(super) fn settle_selection(&mut self) {
        if !self.pointer_open && !self.scene.has_pending() {
            self.settled_selection = Rc::new(self.selection_state());
        }
    }

    /// Selects what a step recorded: before it for undo, after it for redo — only what is
    /// still on the board (`filterSelectedElements`, `delta.ts@1118751f:875-902`), and the
    /// group being edited only while it holds (`set_selection`, which drops it once the step
    /// took it away, `delta.ts@1118751f:806-818`).
    pub(super) fn restore_selection(&mut self, recorded: &SelectionState) {
        self.editing_group_id = recorded.editing.clone();
        let live: Vec<String> = recorded
            .ids
            .iter()
            .filter(|id| self.scene.get(id).is_some_and(|el| !el.is_deleted))
            .cloned()
            .collect();
        self.set_selection(live);
    }

    /// Gives up this client's uncommitted change to `id`, as if it had not been made: the
    /// element goes back to what it was, a peer's copy refused meanwhile is taken if it is
    /// the newer — as a commit takes one for a gesture that came to nothing — and the id
    /// is no longer pending, so the next copy a peer sends is taken as it comes.
    pub(super) fn drop_local_change(&mut self, id: &str) {
        let Some((_, Some(before))) = self
            .scene
            .baseline()
            .into_iter()
            .find(|(pending, _)| pending == id)
        else {
            return;
        };
        let mut settled = (*before).clone();
        if let Some(peer) = self.remote_refused.remove(id) {
            if super::clipboard::remote_wins(&peer, &settled) {
                let mut taken = peer;
                super::clipboard::inherit_picture(&mut taken, &settled);
                settled = taken;
            }
        }
        self.scene.put(settled);
        self.scene.retain_baseline(|pending| pending != id);
    }

    /// Replays one step backwards (`forward == false`) or forwards, as a new edit.
    ///
    /// Every element the step changed is made what it was before it (or after), and
    /// stamped above both what is there now and what it becomes. Everything else is
    /// left exactly as it is — which is what keeps a peer's edit from being undone by
    /// yours.
    pub(super) fn replay_step(&mut self, step: &HistoryEntry, forward: bool) {
        let clock = self.now_ms;
        // Whatever local change was pending before the replay stays pending; what the
        // replay itself does is not a new edit.
        let pending = self.scene.pending_ids();
        let pending_order = self.scene.order_baseline();
        let mut replayed: HashSet<String> = HashSet::new();

        for (id, change) in step.changes.iter() {
            // What someone else holds is theirs right now. Undoing an old edit of it
            // would snap it back under their hands; the rest of the step still applies.
            if self.held.contains_key(id) {
                continue;
            }
            let want = if forward {
                &change.after
            } else {
                &change.before
            };
            let now = self.scene.get_rc(id).cloned();
            match (want, now) {
                (Some(want), Some(now)) => {
                    if !same_content(want, &now) {
                        let version = now.version.max(want.version) + 1;
                        self.scene.put(stamped((**want).clone(), version, clock));
                        replayed.insert(id.clone());
                    }
                }
                (Some(want), None) => {
                    self.scene
                        .put(stamped((**want).clone(), want.version + 1, clock));
                    replayed.insert(id.clone());
                }
                (None, Some(now)) if !now.is_deleted => {
                    // Created by the step being undone. Tombstoned rather than dropped:
                    // a deletion has to reach the server and the peers as a stamped
                    // element, and redo then resurrects it above the tombstone.
                    let mut tombstone = stamped((*now).clone(), now.version + 1, clock);
                    tombstone.is_deleted = true;
                    tombstone.data_url = None;
                    self.scene.put(tombstone);
                    replayed.insert(id.clone());
                }
                _ => {}
            }
        }
        if let Some(order) = &step.order {
            self.scene
                .apply_order(if forward { &order.1 } else { &order.0 });
        }

        // What the replay put back takes the arrows bound to it along, and their labels,
        // as the oracle redraws the bound arrows of what a step changed
        // (`ElementsDelta.applyTo`, `packages/element/src/delta.ts:2044-2047,2107-2114`).
        // An arrow outside the step — one a peer drew since — kept the end the undone
        // edit had given it: nothing else re-routes it, since a commit re-routes only
        // what it touches. Stamped as the replay is, or it would hold other geometry
        // than the same version on a peer that re-routed it on receiving the shape.
        crate::scene::binding::refresh_bindings_in_place(&mut self.scene, &replayed);
        for id in self.scene.pending_ids() {
            if !pending.contains(&id) && !replayed.contains(&id) {
                self.scene.update(&id, |element| {
                    element.version += 1;
                    element.version_nonce = rand_int();
                    element.updated = clock;
                });
            }
        }

        self.scene.retain_baseline(|id| pending.contains(id));
        self.scene.set_order_baseline(pending_order);
    }
}
