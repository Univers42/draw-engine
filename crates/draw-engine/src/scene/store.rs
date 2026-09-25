//! The scene: elements in z-order, addressable by id.
//!
//! # Why a `Vec`, not a `HashMap` plus an order list
//!
//! The obvious shape is `HashMap<String, DrawElement>` for lookup plus `Vec<String>` for
//! z-order, and it was measurably the wrong one. Walking the scene in order then costs a
//! **string-keyed hash lookup per element**, and walking the scene in order is what
//! every frame and every pointer event does. At 50k elements that was ~5ms per frame
//! spent hashing ids — far more than the work it was hashing them to reach, and enough
//! on its own to miss 60fps with nothing moving.
//!
//! Elements now live in one `Vec` in z-order and the map holds indices. Iteration is a
//! contiguous scan with no hashing at all; lookup by id is unchanged.
//!
//! Tombstones stay in the `Vec` — deletion is soft, so the merge rule can tell "deleted"
//! from "never seen" — and are skipped by the ordered iterators.
//!
//! # Why the elements are behind `Rc`
//!
//! Undo takes a snapshot of the whole scene on every mutation. Holding elements
//! directly meant deep-copying every one of them — three `String`s and a point vector
//! apiece — for the act of drawing a single shape, so the cost of drawing grew with the
//! size of the board.
//!
//! Behind an `Rc`, a snapshot is a vector of refcount bumps. Mutation goes through
//! `Rc::make_mut`, which clones an element only when a snapshot still refers to it —
//! so the first change after a snapshot copies that one element and nothing else. This
//! engine is single-threaded (it runs in one WASM instance), so `Rc` is the right tool
//! and `Arc` would only add atomics.

use std::collections::HashMap;
use std::rc::Rc;

use crate::camera::WorldBounds;
use crate::scene::element::DrawElement;
use crate::scene::geometry::scene_bounds;

/// A revision number no other change, in any scene, has had.
///
/// One counter for the whole process rather than one per scene: the painter keys its
/// cached layers on these numbers, and a scene loaded wholesale used to start counting
/// again from nothing — so a layer painted for the old scene could carry the same number
/// as the new one, match, and be shown in its place.
fn next_revision() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// One change to a scene, as the painter needs to know it: see [`Scene::changes_since`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// A new element, put on top of everything.
    Appended(String),
    /// An existing element, changed where it stands.
    Touched(String),
    /// Anything that moves elements in the stack or takes one out of it.
    Rearranged,
}

/// How many changes the journal keeps. A painter whose picture is older than that simply
/// draws it again.
const JOURNAL_CAP: usize = 1024;

#[derive(Clone, Debug, Default)]
pub struct Scene {
    /// Every element, live and tombstoned, in z-order.
    elements: Vec<Rc<DrawElement>>,
    /// Element id to its position in `elements`.
    index: HashMap<String, usize>,
    /// Ids touched since the last [`Self::take_delta`].
    ///
    /// The host is told what changed rather than being handed the whole document: a
    /// scene serialised on every mutation costs time proportional to the size of the
    /// board, so drawing one shape on a large board became slower than drawing the
    /// first one. Measured at 20k elements, that was ~60ms of JSON per shape.
    dirty: std::collections::HashSet<String>,
    /// Every element changed since the last commit, as it was **before** the first
    /// change — `None` for one that did not exist yet.
    ///
    /// This is what a commit compares against to decide what it changed and what to
    /// stamp, and what undo puts back. It is taken at the first touch rather than read
    /// from the last commit's snapshot because a peer's edit can land in between: an
    /// element a peer created or re-stamped since then is not in that snapshot as it
    /// is now, and comparing against the stale copy got both "is it new" and "is it
    /// already stamped" wrong. See `engine/stamp.rs`.
    ///
    /// Not the same record as `dirty`, which is drained whenever the host is told
    /// something — including when a peer's patch lands in the middle of a drag.
    baseline: HashMap<String, Option<Rc<DrawElement>>>,
    /// The z-order before the first local reorder since the last commit, if any.
    order_baseline: Option<Vec<String>>,
    /// Set when a change cannot be expressed as "these elements differ" — a z-order
    /// rearrangement or a hard delete. The host then needs the whole scene.
    structural: bool,
    /// Bumped by every mutation.
    ///
    /// The painter keeps the static scene in an offscreen layer and needs one question
    /// answered per frame: has the picture changed? Hashing the visible elements answers
    /// it, but *only* for the elements that are visible — so panning changed the answer
    /// merely by culling different ones, and the layer was thrown away exactly when it
    /// was most reusable. A counter is O(1), exact, and says nothing about the camera.
    revision: u64,
    /// The elements the gesture in progress changes from frame to frame. Set by the
    /// engine; see [`Self::set_live`].
    live: std::collections::HashSet<String>,
    /// Like `revision`, but moved only by changes to elements **outside** `live`.
    ///
    /// The painter caches everything that is not live in its layers and draws the live
    /// elements over them each frame, so a drag or a stroke costs what it touches rather
    /// than the whole board. This is what tells it the cached part is still good: it
    /// stays put while only live elements change, and moves the moment anything else
    /// does — a peer's edit arriving mid-drag, say.
    static_revision: u64,
    /// Every change since `journal_from`, each with the revision it produced.
    ///
    /// For the painter: a cached picture of this scene at some revision can be brought up
    /// to date by painting on top of it when every change since was an element added on
    /// top — which is what drawing, pasting and duplicating are. Measured on a board of
    /// 9,000 shapes, redrawing the picture after each Ctrl+D was ten milliseconds of
    /// rasterising for one new shape.
    journal: Vec<(u64, Change)>,
    /// The revision the journal starts from. A picture older than this, or of another
    /// scene, cannot be brought up to date from it. `None` until the journal is started:
    /// a scene that never was has no revision a picture could share with it.
    journal_from: Option<u64>,
}

/// What changed since the host was last told.
#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneDelta {
    /// Elements that were added or modified, in no particular order.
    pub updated: Vec<DrawElement>,
    /// Ids that were tombstoned.
    pub removed: Vec<String>,
}

impl Scene {
    pub fn new(elements: impl IntoIterator<Item = DrawElement>) -> Self {
        let mut scene = Self::default();
        for element in elements {
            scene.add(element);
        }
        scene.start_journal();
        scene
    }

    /// Starts the journal afresh, at a revision no picture can already hold.
    ///
    /// Called whenever a scene takes the place of another. Without it a picture painted
    /// for the old scene would find a journal of nothing but additions since — the new
    /// scene's own elements — and have them painted over the old board.
    pub(crate) fn start_journal(&mut self) {
        self.revision = next_revision();
        self.journal.clear();
        self.journal_from = Some(self.revision);
    }

    /// Moves the revision and notes what the change was.
    fn record(&mut self, change: Change) {
        self.revision = next_revision();
        if self.journal.len() >= JOURNAL_CAP {
            // Nothing older is provable any more; the picture that old is redrawn.
            self.journal.clear();
            self.journal_from = Some(self.revision);
            return;
        }
        self.journal.push((self.revision, change));
    }

    /// Every change since `revision`, in order — or `None` when the journal does not
    /// reach back that far, or `revision` was never one of this scene's.
    pub fn changes_since(&self, revision: u64) -> Option<&[(u64, Change)]> {
        if revision < self.journal_from? || revision > self.revision {
            return None;
        }
        let start = self.journal.partition_point(|(rev, _)| *rev <= revision);
        Some(&self.journal[start..])
    }

    /// Rebuilds the id-to-position map. Only needed after an operation that moves
    /// elements within the vector.
    fn reindex(&mut self) {
        self.index.clear();
        self.index.reserve(self.elements.len());
        for (i, element) in self.elements.iter().enumerate() {
            self.index.insert(element.id.clone(), i);
        }
    }

    /// Live elements in z-order, **borrowed** and without hashing anything.
    ///
    /// This is the accessor for every per-frame and per-event path. Prefer it to
    /// [`Self::ordered_cloned`] anywhere that runs more than once per user action.
    /// Double-ended so a hit test can walk from the top of the z-order down, and `Clone`
    /// so a two-pass scan can restart without materialising the list first.
    pub fn iter_ordered(&self) -> impl DoubleEndedIterator<Item = &DrawElement> + Clone {
        self.elements
            .iter()
            .map(Rc::as_ref)
            .filter(|element| !element.is_deleted)
    }

    /// Live elements in z-order, cloned.
    ///
    /// Kept for the call sites that genuinely need ownership — history snapshots, the
    /// clipboard, export, and the reordering operations that rebuild the whole list.
    /// Those run once per user action, where an allocation does not matter.
    pub fn ordered_cloned(&self) -> Vec<DrawElement> {
        self.iter_ordered().cloned().collect()
    }

    /// Live elements in z-order as references, for the operations that want a slice.
    ///
    /// One pointer-sized allocation, no deep copy.
    pub fn ordered_refs(&self) -> Vec<&DrawElement> {
        self.iter_ordered().collect()
    }

    pub fn get(&self, id: &str) -> Option<&DrawElement> {
        self.index.get(id).map(|&i| self.elements[i].as_ref())
    }

    /// The shared element itself, so a caller can tell "unchanged since that snapshot"
    /// with a pointer comparison instead of a deep one.
    pub(crate) fn get_rc(&self, id: &str) -> Option<&Rc<DrawElement>> {
        self.index.get(id).map(|&i| &self.elements[i])
    }

    /// Ids changed since the last commit. See the `baseline` field.
    pub(crate) fn pending_ids(&self) -> std::collections::HashSet<String> {
        self.baseline.keys().cloned().collect()
    }

    /// Whether anything — an element or the z-order — changed since the last commit.
    pub(crate) fn has_pending(&self) -> bool {
        !self.baseline.is_empty() || self.order_baseline.is_some()
    }

    /// Whether the element was created since the last commit — a draft, a label being
    /// typed — and so can be dropped without a trace, rather than deleted.
    pub(crate) fn created_since_commit(&self, id: &str) -> bool {
        matches!(self.baseline.get(id), Some(None))
    }

    /// What every element changed since the last commit was before it — `None` for one
    /// the change created. Left in place: see `engine/peers.rs`, which puts them back.
    pub(crate) fn baseline(&self) -> Vec<(String, Option<Rc<DrawElement>>)> {
        self.baseline
            .iter()
            .map(|(id, before)| (id.clone(), before.clone()))
            .collect()
    }

    pub(crate) fn take_baseline(&mut self) -> HashMap<String, Option<Rc<DrawElement>>> {
        std::mem::take(&mut self.baseline)
    }

    /// Forgets the changes to every id `keep` rejects — for changes that were not this
    /// client's edits: a peer's patch, or undo putting something back.
    pub(crate) fn retain_baseline(&mut self, keep: impl Fn(&str) -> bool) {
        self.baseline.retain(|id, _| keep(id));
    }

    pub(crate) fn order_baseline(&self) -> Option<Vec<String>> {
        self.order_baseline.clone()
    }

    pub(crate) fn set_order_baseline(&mut self, order: Option<Vec<String>>) {
        self.order_baseline = order;
    }

    pub(crate) fn take_order_baseline(&mut self) -> Option<Vec<String>> {
        self.order_baseline.take()
    }

    fn note_order(&mut self) {
        if self.order_baseline.is_none() {
            self.order_baseline = Some(self.ids());
        }
    }

    /// Every id, tombstones included, in z-order.
    pub(crate) fn ids(&self) -> Vec<String> {
        self.elements.iter().map(|e| e.id.clone()).collect()
    }

    /// Puts the ids in `order` in that order, beneath everything it does not mention,
    /// which keeps its current order on top.
    ///
    /// For undoing a reorder: what the step reordered goes back, and an element that
    /// arrived since — a peer's — stays where it is, above.
    pub(crate) fn apply_order(&mut self, order: &[String]) {
        self.record(Change::Rearranged);
        self.static_revision = next_revision();
        let mut rest: Vec<Rc<DrawElement>> = Vec::with_capacity(self.elements.len());
        let mut by_id: HashMap<String, Rc<DrawElement>> = HashMap::new();
        let wanted: std::collections::HashSet<&str> = order.iter().map(String::as_str).collect();
        for element in self.elements.drain(..) {
            if wanted.contains(element.id.as_str()) {
                by_id.insert(element.id.clone(), element);
            } else {
                rest.push(element);
            }
        }
        let mut next: Vec<Rc<DrawElement>> =
            order.iter().filter_map(|id| by_id.remove(id)).collect();
        next.extend(rest);
        self.elements = next;
        self.reindex();
        self.structural = true;
    }

    /// Puts an id back into the pending delta, for a change the host has not been told
    /// about yet even though the delta was drained.
    pub(crate) fn mark_dirty(&mut self, id: &str) {
        if self.index.contains_key(id) {
            self.dirty.insert(id.to_string());
        }
    }

    /// Mutable access to one element, without disturbing z-order or the index.
    ///
    /// This is how a drag should move an element: no clone, no reinsert, no
    /// invalidation of anything.
    /// Mutable access to one element, without disturbing z-order or the index.
    ///
    /// Copy-on-write: `Rc::make_mut` clones the element only if a history snapshot
    /// still holds it, so a drag that touches one shape copies one shape.
    pub fn update<F: FnOnce(&mut DrawElement)>(&mut self, id: &str, f: F) -> bool {
        self.record(Change::Touched(id.to_string()));
        self.touch_static(id);
        match self.index.get(id) {
            Some(&i) => {
                if !self.baseline.contains_key(id) {
                    // Shared, so `make_mut` below copies and this keeps the original.
                    self.baseline
                        .insert(id.to_string(), Some(Rc::clone(&self.elements[i])));
                }
                f(Rc::make_mut(&mut self.elements[i]));
                self.dirty.insert(id.to_string());
                true
            }
            None => false,
        }
    }

    pub fn size(&self) -> usize {
        self.iter_ordered().count()
    }

    pub fn add(&mut self, element: DrawElement) {
        self.put(element);
    }

    /// Inserts or replaces an element, **keeping its existing z-position** when it is
    /// already present. A style change must not bring a shape to the front.
    pub fn put(&mut self, element: DrawElement) {
        self.record(if self.index.contains_key(&element.id) {
            Change::Touched(element.id.clone())
        } else {
            Change::Appended(element.id.clone())
        });
        self.touch_static(&element.id);
        self.dirty.insert(element.id.clone());
        if !self.baseline.contains_key(&element.id) {
            let before = self.get_rc(&element.id).cloned();
            self.baseline.insert(element.id.clone(), before);
        }
        match self.index.get(&element.id) {
            Some(&i) => self.elements[i] = Rc::new(element),
            None => {
                self.index.insert(element.id.clone(), self.elements.len());
                self.elements.push(Rc::new(element));
            }
        }
    }

    /// Replaces an element already in the scene as a change to the picture only — not an
    /// edit: nothing becomes pending for the host and the next commit does not stamp it.
    /// For what every client derives for itself, like a text laid out again once its
    /// font has loaded. Returns whether the element was there to replace.
    pub(crate) fn replace_unrecorded(&mut self, element: DrawElement) -> bool {
        let Some(&i) = self.index.get(&element.id) else {
            return false;
        };
        self.record(Change::Touched(element.id.clone()));
        self.touch_static(&element.id);
        self.elements[i] = Rc::new(element);
        true
    }

    /// Soft delete: the element stays, marked, so a later merge can distinguish a
    /// deletion from an element it has simply never seen.
    pub fn remove(&mut self, id: &str, now: f64) {
        self.update(id, |element| {
            element.is_deleted = true;
            element.version += 1;
            // A fresh nonce, as every other stamp gets: a tombstone that kept the live
            // element's nonce ties with a peer's edit of the same version on the nonce
            // too, and is decided by the clock — the one tie-breaker that means nothing
            // across machines.
            element.version_nonce = crate::scene::element::rand_int();
            element.updated = now;
            // Nothing draws a tombstone, and a picture is most of an image's size: kept,
            // every deleted image went on counting against the board's 16MB for good.
            // Undo does not need it — it restores from its own copy of the element.
            element.data_url = None;
        });
    }

    /// Hard delete, leaving no tombstone. Used when discarding an element that was
    /// never committed, such as a drag that ended below the minimum size.
    pub fn discard(&mut self, id: &str) {
        self.record(Change::Rearranged);
        self.touch_static(id);
        if let Some(&i) = self.index.get(id) {
            if !self.baseline.contains_key(id) {
                self.baseline
                    .insert(id.to_string(), Some(Rc::clone(&self.elements[i])));
            }
            self.elements.remove(i);
            self.reindex();
            // A hard delete leaves no tombstone, so a delta cannot express it.
            self.structural = true;
        }
    }

    pub fn bring_to_front(&mut self, id: &str) {
        self.record(Change::Rearranged);
        self.static_revision = next_revision();
        if let Some(&i) = self.index.get(id) {
            if i + 1 != self.elements.len() {
                self.note_order();
                let element = self.elements.remove(i);
                self.elements.push(element);
                self.reindex();
                self.structural = true;
            }
        }
    }

    /// Moves `ids`, in the order given, to directly above `anchor`.
    ///
    /// For something new that belongs beside an existing element rather than on top of
    /// the board — a label above its shape, a copy inside the group it was made in —
    /// without cloning the board to say so. Nothing happens when they are already there.
    pub(crate) fn place_above(&mut self, ids: &[String], anchor: &str) {
        let Some(&at) = self.index.get(anchor) else {
            return;
        };
        let in_place = ids
            .iter()
            .enumerate()
            .all(|(k, id)| self.index.get(id) == Some(&(at + 1 + k)));
        let moving: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
        if in_place || moving.contains(anchor) {
            return;
        }
        self.record(Change::Rearranged);
        self.static_revision = next_revision();
        self.note_order();
        let mut taken: HashMap<String, Rc<DrawElement>> = HashMap::new();
        let mut rest: Vec<Rc<DrawElement>> = Vec::with_capacity(self.elements.len());
        for element in self.elements.drain(..) {
            if moving.contains(element.id.as_str()) {
                taken.insert(element.id.clone(), element);
            } else {
                rest.push(element);
            }
        }
        let at = rest
            .iter()
            .position(|el| el.id == anchor)
            .map_or(rest.len(), |i| i + 1);
        rest.splice(at..at, ids.iter().filter_map(|id| taken.remove(id)));
        self.elements = rest;
        self.reindex();
        self.structural = true;
    }

    /// Replaces the z-order with `live`, keeping tombstones at the back of the stack.
    ///
    /// Tombstones go first so that restoring one by undo puts it beneath everything
    /// drawn since, which is what the user expects.
    ///
    /// Tombstones are told from `live` by a set: searched in the list, once per tombstone,
    /// a reorder on a board that had deleted as much as it held cost the product of the
    /// two — 79ms for 5,000 over 5,000 against 4.7ms with none; 5.7ms with the set
    /// (`cargo bench --bench editing -- reorder/one_to_front`). A board with no tombstone
    /// builds no set.
    pub fn set_order(&mut self, live: Vec<DrawElement>) {
        self.record(Change::Rearranged);
        self.static_revision = next_revision();
        self.note_order();
        let mut next: Vec<Rc<DrawElement>> = if !self.elements.iter().any(|e| e.is_deleted) {
            Vec::with_capacity(live.len())
        } else {
            let listed: std::collections::HashSet<&str> =
                live.iter().map(|l| l.id.as_str()).collect();
            self.elements
                .iter()
                .filter(|element| element.is_deleted && !listed.contains(element.id.as_str()))
                .cloned()
                .collect()
        };
        next.extend(live.into_iter().map(Rc::new));
        self.elements = next;
        self.reindex();
        self.structural = true;
    }

    /// Takes what changed since the last call, clearing the record.
    ///
    /// `None` means the change was structural — a reorder or a hard delete — and the
    /// caller should send the whole scene instead. That is rare: it is a z-order
    /// command or a discarded draft, never the common path of drawing or moving.
    /// A number that changes whenever the scene does, and never otherwise.
    ///
    /// Deliberately not a hash of the contents: this is asked once per frame and has to
    /// cost nothing, and a counter cannot miss a field the way a hash can.
    /// Every element the store holds, tombstones included.
    ///
    /// Tombstones stay in the scene because a deletion has to be *sent* to the other
    /// editors — it is a live element with `is_deleted`, not an absence. So this is the
    /// memory cost, while `iter_ordered().count()` is the drawing.
    pub fn total_len(&self) -> usize {
        self.elements.len()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// See the `static_revision` field.
    pub fn static_revision(&self) -> u64 {
        self.static_revision
    }

    /// The elements the gesture in progress changes from frame to frame.
    pub fn live(&self) -> &std::collections::HashSet<String> {
        &self.live
    }

    /// Declares which elements are live. Changing the set moves `static_revision`,
    /// because what the painter caches is "everything but these".
    pub(crate) fn set_live(&mut self, live: std::collections::HashSet<String>) {
        if live != self.live {
            self.live = live;
            self.static_revision = next_revision();
        }
    }

    fn touch_static(&mut self, id: &str) {
        if !self.live.contains(id) {
            self.static_revision = next_revision();
        }
    }

    pub fn take_delta(&mut self) -> Option<SceneDelta> {
        let structural = std::mem::take(&mut self.structural);
        let dirty = std::mem::take(&mut self.dirty);
        if structural {
            return None;
        }

        let mut delta = SceneDelta::default();
        // In stacking order. The host appends what it has not seen in the order it is
        // listed, and the set this comes from has none: a paste of several elements
        // could land on the host — and from there on the server — stacked differently
        // from the board it came from.
        let mut dirty: Vec<(usize, String)> = dirty
            .into_iter()
            .map(|id| (self.index.get(&id).copied().unwrap_or(usize::MAX), id))
            .collect();
        dirty.sort_unstable();
        for (_, id) in dirty {
            match self.index.get(&id).map(|&i| self.elements[i].as_ref()) {
                Some(element) if element.is_deleted => delta.removed.push(id),
                Some(element) => delta.updated.push(element.clone()),
                // Gone entirely — that is a hard delete, which sets `structural`, so
                // reaching here means the id was never really in the scene.
                None => {}
            }
        }
        Some(delta)
    }

    /// Forgets any pending delta and demands a full sync next time.
    ///
    /// Used after the scene is replaced wholesale — a load, an undo, a paste.
    pub fn invalidate_delta(&mut self) {
        self.dirty.clear();
        self.structural = true;
    }

    /// Forgets any pending delta without asking for a sync: the host already has this
    /// scene, because it is the one that supplied it.
    pub(crate) fn forget_pending(&mut self) {
        self.dirty.clear();
        self.structural = false;
    }

    pub fn bounds(&self) -> Option<WorldBounds> {
        scene_bounds(self.iter_ordered())
    }

    /// Everything, tombstones included, in z-order, as owned elements.
    ///
    /// Deep-copies. For an undo snapshot use [`Self::snapshot`], which does not.
    pub fn to_array(&self) -> Vec<DrawElement> {
        self.elements.iter().map(|e| (**e).clone()).collect()
    }

    /// A snapshot of the whole scene for undo.
    ///
    /// Cheap: a vector of refcount bumps, not a deep copy. The elements stay shared
    /// until something modifies one, at which point [`Self::update`] copies just that
    /// element.
    pub fn snapshot(&self) -> Vec<Rc<DrawElement>> {
        self.elements.clone()
    }

    /// Restores a snapshot taken by [`Self::snapshot`].
    pub fn from_snapshot(snapshot: Vec<Rc<DrawElement>>) -> Self {
        let mut scene = Self {
            elements: snapshot,
            ..Default::default()
        };
        scene.reindex();
        scene.structural = true;
        scene.start_journal();
        scene
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::element::{create_element, DrawElementStyle, DrawElementType, Geometry};

    fn element(id: &str) -> DrawElement {
        let mut e = create_element(
            DrawElementType::Rectangle,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            DrawElementStyle::default(),
            0.0,
        );
        e.id = id.to_string();
        e
    }

    fn ids(scene: &Scene) -> Vec<String> {
        scene.iter_ordered().map(|e| e.id.clone()).collect()
    }

    #[test]
    fn insertion_order_is_z_order() {
        let scene = Scene::new([element("a"), element("b"), element("c")]);
        assert_eq!(ids(&scene), ["a", "b", "c"]);
    }

    /// Replacing an element must not promote it: a style change is not a reorder.
    #[test]
    fn putting_an_existing_element_keeps_its_place() {
        let mut scene = Scene::new([element("a"), element("b"), element("c")]);
        let mut b = element("b");
        b.width = 999.0;
        scene.put(b);

        assert_eq!(ids(&scene), ["a", "b", "c"]);
        assert_eq!(scene.get("b").unwrap().width, 999.0);
    }

    #[test]
    fn tombstones_are_hidden_from_iteration_but_kept_in_the_document() {
        let mut scene = Scene::new([element("a"), element("b")]);
        scene.remove("a", 1.0);

        assert_eq!(ids(&scene), ["b"]);
        assert_eq!(scene.size(), 1);
        assert_eq!(scene.to_array().len(), 2, "the tombstone survives");
        assert!(scene.get("a").unwrap().is_deleted);
    }

    #[test]
    fn discard_leaves_no_trace_and_keeps_the_index_valid() {
        let mut scene = Scene::new([element("a"), element("b"), element("c")]);
        scene.discard("b");

        assert_eq!(ids(&scene), ["a", "c"]);
        assert!(scene.get("b").is_none());
        // The index must have been rebuilt, or "c" would now resolve to the wrong slot.
        assert_eq!(scene.get("c").unwrap().id, "c");
    }

    #[test]
    fn bring_to_front_moves_one_element_and_leaves_lookup_working() {
        let mut scene = Scene::new([element("a"), element("b"), element("c")]);
        scene.bring_to_front("a");

        assert_eq!(ids(&scene), ["b", "c", "a"]);
        for id in ["a", "b", "c"] {
            assert_eq!(scene.get(id).unwrap().id, id);
        }
    }

    #[test]
    fn bring_to_front_of_the_topmost_element_is_a_no_op() {
        let mut scene = Scene::new([element("a"), element("b")]);
        scene.bring_to_front("b");
        assert_eq!(ids(&scene), ["a", "b"]);
    }

    #[test]
    fn update_mutates_in_place() {
        let mut scene = Scene::new([element("a"), element("b")]);
        assert!(scene.update("a", |e| e.x = 42.0));
        assert!(!scene.update("missing", |e| e.x = 1.0));

        assert_eq!(scene.get("a").unwrap().x, 42.0);
        assert_eq!(ids(&scene), ["a", "b"], "order untouched");
    }

    #[test]
    fn set_order_reorders_the_living_and_keeps_tombstones_behind() {
        let mut scene = Scene::new([element("a"), element("b"), element("c")]);
        scene.remove("b", 1.0);

        scene.set_order(vec![element("c"), element("a")]);

        assert_eq!(ids(&scene), ["c", "a"]);
        let all = scene.to_array();
        assert_eq!(
            all[0].id, "b",
            "the tombstone sits beneath the live elements"
        );
        assert!(all[0].is_deleted);
    }

    /// Every mutation has to leave the index consistent with the vector, or lookups
    /// start silently returning the wrong element.
    #[test]
    fn the_index_survives_a_churn_of_operations() {
        let mut scene = Scene::new((0..20).map(|i| element(&format!("e{i}"))));

        scene.discard("e3");
        scene.bring_to_front("e10");
        scene.remove("e5", 1.0);
        scene.put(element("e21"));
        scene.discard("e0");
        scene.set_order(scene.ordered_cloned().into_iter().rev().collect());

        for element in scene.to_array() {
            assert_eq!(
                scene.get(&element.id).map(|e| e.id.as_str()),
                Some(element.id.as_str()),
                "id {} resolves to the wrong slot",
                element.id
            );
        }
    }
}
