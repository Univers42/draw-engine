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
//! # Once per commit, not per move
//!
//! Excalidraw stamps on every mutation (`mutateElement.ts:142-144`), so a drag bumps
//! once per pointer event. Here nothing leaves the engine between commits — the host is
//! told at `push_history` — so one stamp per element per commit says the same thing
//! with a hundred times fewer nonces. The rule, applied at commit to every element
//! touched since the last one:
//!
//! - **new since the last commit** — keep the stamp it was created with, so a shape
//!   drawn by hand commits at version 1 however many moves sized it;
//! - **unchanged** (the same `Rc`, or equal content) — nothing, so a drag away and back
//!   records no step and sends nothing;
//! - **changed, stamp unchanged** — bump;
//! - **changed, already re-stamped** by a path that bumps its own (style, radius,
//!   group, …) — nothing more: one edit, one bump.
//!
//! # Undo is a new edit
//!
//! Restoring a snapshot restored its *stamps* too, so an undone edit came back with a
//! version lower than the one already sent — refused by the server and by every peer as
//! stale. Excalidraw applies undo "as a new user action" with fresh stamps
//! (`history.ts:24-34`, `delta.ts:1732-1781`), and so does this: each restored element
//! is stamped above both what is there now and what it is restored to.
//!
//! And it restores **only what that step changed**. Snapshots are whole scenes, and a
//! peer's edit that arrived before your next commit is in that commit's snapshot — so
//! restoring the whole previous snapshot reverted the peer's work, and with fresh
//! stamps would now have *propagated* that revert. Each history entry therefore records
//! the ids its commit changed, and undo touches those and nothing else.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::engine::DrawEngine;
use crate::scene::element::rand_int;
use crate::scene::{DrawElement, Scene};

/// One step of undo history: the scene after it, and the ids this step changed.
#[derive(Clone, Debug, Default)]
pub(crate) struct HistoryEntry {
    pub elements: Vec<Rc<DrawElement>>,
    /// Behind an `Rc` because undo clones the entry it leaves, and a large paste can
    /// change thousands of ids.
    pub changed: Rc<HashSet<String>>,
}

/// Equal in everything but the stamp.
fn same_content(a: &DrawElement, b: &DrawElement) -> bool {
    let mut a = a.clone();
    a.version = b.version;
    a.version_nonce = b.version_nonce;
    a.updated = b.updated;
    a == *b
}

fn stamped(mut element: DrawElement, version: u32, now: f64) -> DrawElement {
    element.version = version;
    element.version_nonce = rand_int();
    element.updated = now;
    element
}

impl DrawEngine {
    /// Stamps every element this commit changed and returns their ids.
    pub(super) fn stamp_local_edits(&mut self) -> HashSet<String> {
        let touched = self.scene.take_touched();
        let floors = std::mem::take(&mut self.remote_floor);
        if touched.is_empty() {
            return HashSet::new();
        }

        // The last commit's copy of each touched element. One pass over the snapshot,
        // looking only for the touched ids, rather than an index of the whole scene.
        let before: HashMap<&str, &Rc<DrawElement>> = self
            .history
            .current()
            .elements
            .iter()
            .filter(|element| touched.contains(&element.id))
            .map(|element| (element.id.as_str(), element))
            .collect();

        let mut changed = HashSet::new();
        let mut bumps: Vec<(String, u32)> = Vec::new();
        for id in &touched {
            let Some(now) = self.scene.get_rc(id) else {
                // Discarded before it was ever committed: a draft below the minimum size.
                continue;
            };
            let floor = floors.get(id).copied().unwrap_or(0);
            match before.get(id.as_str()) {
                None => {
                    changed.insert(id.clone());
                    // New, and its first stamp stands — unless a peer sent a copy of it
                    // meanwhile, which only a paste of the same id could cause.
                    if now.version <= floor {
                        bumps.push((id.clone(), floor + 1));
                    }
                }
                Some(then) if Rc::ptr_eq(now, then) => {}
                Some(then) => {
                    let same_stamp =
                        now.version == then.version && now.version_nonce == then.version_nonce;
                    if same_stamp && same_content(now, then) {
                        // Put back unchanged.
                        continue;
                    }
                    changed.insert(id.clone());
                    if same_stamp || now.version <= floor {
                        bumps.push((id.clone(), now.version.max(floor) + 1));
                    }
                }
            }
        }

        let clock = self.now_ms;
        for (id, version) in bumps {
            self.scene.update(&id, |element| {
                element.version = version;
                element.version_nonce = rand_int();
                element.updated = clock;
            });
        }
        // Stamping is not itself an edit to stamp at the next commit.
        let _ = self.scene.take_touched();
        changed
    }

    /// Makes the scene what `target` recorded for `ids`, as a new edit.
    ///
    /// Every other element is left as it is now — which is what keeps a peer's edit
    /// from being undone by yours. Z-order follows `target`, and anything it does not
    /// know about (a peer's new element, say) stays on top in its current order.
    pub(super) fn restore_step(&mut self, target: &HistoryEntry, ids: &HashSet<String>) {
        let clock = self.now_ms;
        let in_target: HashSet<&str> = target.elements.iter().map(|e| e.id.as_str()).collect();
        let mut next: Vec<Rc<DrawElement>> = Vec::with_capacity(target.elements.len());

        for then in &target.elements {
            let now = self.scene.get_rc(&then.id);
            if !ids.contains(&then.id) {
                // Not this step's: whatever it is now is the truth.
                if let Some(now) = now {
                    next.push(Rc::clone(now));
                }
                continue;
            }
            next.push(match now {
                Some(now) if same_content(now, then) => Rc::clone(now),
                Some(now) => Rc::new(stamped(
                    (**then).clone(),
                    now.version.max(then.version) + 1,
                    clock,
                )),
                None => Rc::new(stamped((**then).clone(), then.version + 1, clock)),
            });
        }

        for now in self.scene.snapshot() {
            if in_target.contains(now.id.as_str()) {
                continue;
            }
            if ids.contains(&now.id) && !now.is_deleted {
                // Created by the step being undone. Tombstoned rather than dropped: a
                // deletion has to reach the server and the peers as a stamped element,
                // and redo then resurrects it with a stamp above the tombstone's.
                let mut tombstone = stamped((*now).clone(), now.version + 1, clock);
                tombstone.is_deleted = true;
                next.push(Rc::new(tombstone));
            } else {
                next.push(now);
            }
        }

        self.scene = Scene::from_snapshot(next);
        self.remote_floor.clear();
        // The entry we landed on keeps its own record of what *it* changed; only its
        // picture of the scene is brought up to date.
        self.history.replace_current(HistoryEntry {
            elements: self.scene.snapshot(),
            changed: Rc::clone(&target.changed),
        });
    }
}
