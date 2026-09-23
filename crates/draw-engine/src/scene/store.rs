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
        scene
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

    /// Mutable access to one element, without disturbing z-order or the index.
    ///
    /// This is how a drag should move an element: no clone, no reinsert, no
    /// invalidation of anything.
    /// Mutable access to one element, without disturbing z-order or the index.
    ///
    /// Copy-on-write: `Rc::make_mut` clones the element only if a history snapshot
    /// still holds it, so a drag that touches one shape copies one shape.
    pub fn update<F: FnOnce(&mut DrawElement)>(&mut self, id: &str, f: F) -> bool {
        self.revision = self.revision.wrapping_add(1);
        match self.index.get(id) {
            Some(&i) => {
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
        self.revision = self.revision.wrapping_add(1);
        self.put(element);
    }

    /// Inserts or replaces an element, **keeping its existing z-position** when it is
    /// already present. A style change must not bring a shape to the front.
    pub fn put(&mut self, element: DrawElement) {
        self.revision = self.revision.wrapping_add(1);
        self.dirty.insert(element.id.clone());
        match self.index.get(&element.id) {
            Some(&i) => self.elements[i] = Rc::new(element),
            None => {
                self.index.insert(element.id.clone(), self.elements.len());
                self.elements.push(Rc::new(element));
            }
        }
    }

    /// Soft delete: the element stays, marked, so a later merge can distinguish a
    /// deletion from an element it has simply never seen.
    pub fn remove(&mut self, id: &str, now: f64) {
        self.revision = self.revision.wrapping_add(1);
        self.update(id, |element| {
            element.is_deleted = true;
            element.version += 1;
            element.updated = now;
        });
    }

    /// Hard delete, leaving no tombstone. Used when discarding an element that was
    /// never committed, such as a drag that ended below the minimum size.
    pub fn discard(&mut self, id: &str) {
        self.revision = self.revision.wrapping_add(1);
        if let Some(&i) = self.index.get(id) {
            self.elements.remove(i);
            self.reindex();
            // A hard delete leaves no tombstone, so a delta cannot express it.
            self.structural = true;
        }
    }

    pub fn bring_to_front(&mut self, id: &str) {
        self.revision = self.revision.wrapping_add(1);
        if let Some(&i) = self.index.get(id) {
            if i + 1 != self.elements.len() {
                let element = self.elements.remove(i);
                self.elements.push(element);
                self.reindex();
                self.structural = true;
            }
        }
    }

    /// Replaces the z-order with `live`, keeping tombstones at the back of the stack.
    ///
    /// Tombstones go first so that restoring one by undo puts it beneath everything
    /// drawn since, which is what the user expects.
    pub fn set_order(&mut self, live: Vec<DrawElement>) {
        self.revision = self.revision.wrapping_add(1);
        let mut next: Vec<Rc<DrawElement>> = self
            .elements
            .iter()
            .filter(|element| element.is_deleted)
            .filter(|element| !live.iter().any(|l| l.id == element.id))
            .cloned()
            .collect();
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

    pub fn take_delta(&mut self) -> Option<SceneDelta> {
        let structural = std::mem::take(&mut self.structural);
        let dirty = std::mem::take(&mut self.dirty);
        if structural {
            return None;
        }

        let mut delta = SceneDelta::default();
        for id in dirty {
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
        scene.revision = scene.revision.wrapping_add(1);
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
