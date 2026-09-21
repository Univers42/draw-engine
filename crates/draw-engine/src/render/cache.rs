//! Caches the rough geometry of each element between frames.
//!
//! Generating a rough drawable is not cheap — a hachure-filled rectangle is thousands
//! of ops — and the naive painter regenerated every element on every frame. Because
//! shapes are produced in **element-local space** (see [`crate::render::shape`]), the
//! geometry depends only on the element's size and seed, so:
//!
//! - **translation is free** — dragging never invalidates anything;
//! - **resizing invalidates**, which it must, since rough's output depends on w/h.
//!
//! # Why the key is not just `version`
//!
//! The obvious key is `(id, version)`, and it is wrong here: the drag handlers call
//! `Scene::put` without `bump_version` while a gesture is in flight, so a resize would
//! reuse the pre-resize geometry and the shape would visibly lag the handle. The key
//! therefore includes the dimensions, which is both smaller than fixing every call site
//! and correct by construction.

use std::collections::HashMap;

use draw_rough::Drawable;

use crate::render::shape::element_drawable;
use crate::scene::element::DrawElement;

/// Identifies a generated shape. Dimensions are compared by bit pattern so the key is
/// hashable and exact — two sizes that differ in the last ulp are genuinely different
/// geometry.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ShapeKey {
    version: u32,
    seed: u32,
    width: u64,
    height: u64,
}

impl ShapeKey {
    /// Collapses the key to one integer.
    ///
    /// The painter caches a `Path2D` per shape and needs to know when to rebuild it,
    /// but `Path2D` is browser-only and cannot live in this module. A fingerprint lets
    /// the two caches agree on "same shape" without the platform-specific one having to
    /// reach into this type.
    pub fn fingerprint(&self) -> u64 {
        // FNV-1a over the four fields. Collisions would show as a stale shape, so the
        // full 64 bits are kept rather than truncated.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for part in [
            u64::from(self.version),
            u64::from(self.seed),
            self.width,
            self.height,
        ] {
            for byte in part.to_le_bytes() {
                h ^= u64::from(byte);
                h = h.wrapping_mul(0x1000_0000_01b3);
            }
        }
        h
    }

    fn of(element: &DrawElement) -> Self {
        Self {
            version: element.version,
            seed: element.seed,
            width: element.width.to_bits(),
            height: element.height.to_bits(),
        }
    }
}

/// The fingerprint of an element's current geometry.
///
/// Two elements with the same fingerprint produce identical rough output, so a painter
/// may reuse anything it derived from that output.
pub fn shape_fingerprint(element: &DrawElement) -> u64 {
    ShapeKey::of(element).fingerprint()
}

/// Element id to its generated geometry.
#[derive(Default)]
pub struct ShapeCache {
    entries: HashMap<String, (ShapeKey, Option<Drawable>)>,
    hits: u64,
    misses: u64,
}

impl ShapeCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The element's geometry, generating it only if this element has changed shape.
    ///
    /// `None` means the element is not rough-drawn at all (text, freedraw) — cached
    /// just the same, so the dispatch is not repeated per frame.
    pub fn get(&mut self, element: &DrawElement) -> Option<&Drawable> {
        let key = ShapeKey::of(element);

        let stale = match self.entries.get(&element.id) {
            Some((cached, _)) => *cached != key,
            None => true,
        };

        if stale {
            self.misses += 1;
            let drawable = element_drawable(element);
            self.entries.insert(element.id.clone(), (key, drawable));
        } else {
            self.hits += 1;
        }

        self.entries.get(&element.id).and_then(|(_, d)| d.as_ref())
    }

    /// Drops entries for elements no longer in the scene.
    ///
    /// Without this the cache is a leak: every deleted element's geometry would be held
    /// for the life of the session, and a long editing session churns through many.
    pub fn retain_ids<'a>(&mut self, live: impl IntoIterator<Item = &'a str>) {
        let live: std::collections::HashSet<&str> = live.into_iter().collect();
        self.entries.retain(|id, _| live.contains(id.as_str()));
    }

    /// Hit/miss counts, for the benchmark harness.
    pub fn stats(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::element::{create_element, DrawElementStyle, DrawElementType, Geometry};

    fn element() -> DrawElement {
        let mut e = create_element(
            DrawElementType::Rectangle,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 60.0,
            },
            DrawElementStyle::default(),
            0.0,
        );
        e.seed = 12345;
        e
    }

    /// The point of the whole cache: dragging must not regenerate geometry.
    #[test]
    fn moving_an_element_is_a_cache_hit() {
        let mut cache = ShapeCache::new();
        let mut e = element();

        cache.get(&e);
        let (_, misses_before) = cache.stats();

        e.x += 250.0;
        e.y -= 40.0;
        cache.get(&e);

        let (hits, misses) = cache.stats();
        assert_eq!(misses, misses_before, "a move must not regenerate");
        assert_eq!(hits, 1);
    }

    /// ...and resizing must, even mid-gesture when `version` has not been bumped.
    #[test]
    fn resizing_invalidates_even_without_a_version_bump() {
        let mut cache = ShapeCache::new();
        let mut e = element();

        cache.get(&e);
        e.width = 140.0; // no bump_version, exactly as the drag handlers do
        cache.get(&e);

        let (_, misses) = cache.stats();
        assert_eq!(misses, 2, "the new size must produce new geometry");
    }

    #[test]
    fn restyling_via_a_version_bump_invalidates() {
        let mut cache = ShapeCache::new();
        let mut e = element();
        cache.get(&e);
        e.version += 1;
        cache.get(&e);
        assert_eq!(cache.stats().1, 2);
    }

    #[test]
    fn deleted_elements_are_evicted() {
        let mut cache = ShapeCache::new();
        let e = element();
        cache.get(&e);
        assert_eq!(cache.len(), 1);

        cache.retain_ids(std::iter::empty());
        assert!(cache.is_empty());
    }

    /// The painter's Path2D cache is keyed off this, so it must change exactly when
    /// the geometry does — no more, no less.
    #[test]
    fn the_fingerprint_tracks_geometry_and_nothing_else() {
        let base = element();

        let mut moved = base.clone();
        moved.x += 500.0;
        assert_eq!(
            shape_fingerprint(&base),
            shape_fingerprint(&moved),
            "moving does not change geometry"
        );

        let mut resized = base.clone();
        resized.width += 1.0;
        assert_ne!(shape_fingerprint(&base), shape_fingerprint(&resized));

        let mut restyled = base.clone();
        restyled.version += 1;
        assert_ne!(shape_fingerprint(&base), shape_fingerprint(&restyled));

        let mut reseeded = base.clone();
        reseeded.seed = base.seed.wrapping_add(1);
        assert_ne!(shape_fingerprint(&base), shape_fingerprint(&reseeded));
    }

    #[test]
    fn text_is_cached_as_having_no_geometry() {
        let mut cache = ShapeCache::new();
        let mut e = element();
        e.kind = DrawElementType::Text;

        assert!(cache.get(&e).is_none());
        assert!(cache.get(&e).is_none());
        assert_eq!(
            cache.stats(),
            (1, 1),
            "the second lookup must not re-dispatch"
        );
    }
}
