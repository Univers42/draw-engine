//! Caches the rough geometry of each element between frames.
//!
//! Generating a rough drawable is not cheap — a hachure-filled rectangle is thousands
//! of ops — and the naive painter regenerated every element on every frame. Because
//! shapes are produced in **element-local space** (see [`crate::render::shape`]), the
//! geometry depends only on the element's size, seed and style, so:
//!
//! - **translation is free** — dragging never invalidates anything;
//! - **resizing invalidates**, which it must, since rough's output depends on w/h;
//! - **recolouring does not**, because a colour is applied when the shape is painted.
//!
//! # Why the key is the geometry and not the element
//!
//! It used to be `(id, version, w, h, seed)`, one entry per element, and that is wrong
//! twice over.
//!
//! A board made by holding Ctrl+D is hundreds of elements that differ only in *where
//! they are* — same seed, same size, same style — so keying by id generated and stored
//! every one of them separately. At screen size that is ~8ms of geometry on the first
//! frame and megabytes of ops for a picture that is one shape repeated. Keyed by the
//! geometry they share a single entry.
//!
//! And `version` bumps for changes that do not touch the geometry at all, so recolouring
//! two hundred selected shapes regenerated two hundred identical drawables.
//!
//! The danger of a content key is a field left out of it: two elements that genuinely
//! differ would then share a shape, which renders *wrongly* rather than slowly.
//! `tests/ci_shape_cache.rs` asserts the property directly — same fingerprint if and only
//! if identical geometry — over a corpus that varies every field the generator reads.

use std::collections::HashMap;

use draw_rough::Drawable;

use crate::render::shape::element_drawable;
use crate::scene::element::DrawElement;

/// Identifies a piece of generated geometry.
///
/// Every field [`element_drawable`] and `generate_rough_options` read, and nothing else.
/// Floats are compared by bit pattern so the key is hashable and exact — two sizes that
/// differ in the last ulp are genuinely different geometry.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ShapeKey(u64);

impl ShapeKey {
    /// The key as one integer, for a cache that cannot hold this type.
    ///
    /// The painter caches a `Path2D` per shape and needs to know when to rebuild it, but
    /// `Path2D` is browser-only and cannot live in this module. A fingerprint lets the
    /// two caches agree on "same shape" without the platform-specific one having to reach
    /// into this type — and lets it share one `Path2D` between duplicates, which is where
    /// most of the memory went.
    pub fn fingerprint(&self) -> u64 {
        self.0
    }

    fn of(element: &DrawElement) -> Self {
        // FNV-1a. Collisions would show as one element drawn as another, so the full 64
        // bits are kept rather than truncated.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |bytes: &[u8]| {
            for byte in bytes {
                h ^= u64::from(*byte);
                h = h.wrapping_mul(0x1000_0000_01b3);
            }
        };

        eat(&[element.kind as u8]);
        eat(&element.seed.to_le_bytes());
        // Absolute, so mirroring an element reuses its geometry rather than regenerating
        // it. The sign is a property of the transform, not of the shape.
        eat(&element.width.abs().to_bits().to_le_bytes());
        eat(&element.height.abs().to_bits().to_le_bytes());
        eat(&element.stroke_width.to_bits().to_le_bytes());
        eat(&element.roughness.to_bits().to_le_bytes());
        eat(&[element.stroke_style as u8, element.fill_style as u8]);
        // Only whether it is filled, not with what: the colour is the painter's business,
        // but an unfilled shape has no fill set in its drawable at all.
        eat(&[u8::from(!crate::render::opts::is_transparent(
            &element.background_color,
        ))]);
        match element.roundness {
            Some(r) => {
                eat(&[1]);
                eat(&r.to_bits().to_le_bytes());
            }
            None => eat(&[0]),
        }
        // Linear geometry lives in the points, not in width and height.
        if let Some(points) = element.points.as_deref() {
            eat(&(points.len() as u64).to_le_bytes());
            for point in points {
                eat(&point[0].to_bits().to_le_bytes());
                eat(&point[1].to_bits().to_le_bytes());
            }
        }
        Self(h)
    }
}

/// The fingerprint of an element's current geometry.
///
/// Two elements with the same fingerprint produce identical rough output, so a painter
/// may reuse anything it derived from that output — including, in the browser, one
/// `Path2D` for every copy of a duplicated shape.
pub fn shape_fingerprint(element: &DrawElement) -> u64 {
    ShapeKey::of(element).fingerprint()
}

/// Generated geometry, shared by every element that produces it.
#[derive(Default)]
pub struct ShapeCache {
    entries: HashMap<ShapeKey, Option<Drawable>>,
    /// Keys asked for since the last sweep.
    used: std::collections::HashSet<ShapeKey>,
    hits: u64,
    misses: u64,
}

impl ShapeCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The element's geometry, generating it only if no element has produced it yet.
    ///
    /// `None` means the element is not rough-drawn at all (text, freedraw) — cached just
    /// the same, so the dispatch is not repeated per frame.
    pub fn get(&mut self, element: &DrawElement) -> Option<&Drawable> {
        let key = ShapeKey::of(element);
        self.used.insert(key);

        if let std::collections::hash_map::Entry::Vacant(slot) = self.entries.entry(key) {
            self.misses += 1;
            slot.insert(element_drawable(element));
        } else {
            self.hits += 1;
        }

        self.entries.get(&key).and_then(|d| d.as_ref())
    }

    /// Drops geometry the frame that just ran did not ask for.
    ///
    /// Called once per frame, after painting. Sweeping by element id is no longer
    /// possible — a shape has no single owner — so it is swept by use instead, and
    /// without it the cache holds every shape a session ever drew.
    ///
    /// This assumes a frame asks for every element it draws, which is true while the
    /// painter redraws the whole scene. A partial redraw would have to mark the shapes it
    /// skipped, or it would drop them and regenerate them on the next full frame.
    pub fn sweep(&mut self) {
        let used = std::mem::take(&mut self.used);
        self.entries.retain(|key, _| used.contains(key));
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
    fn a_version_bump_on_its_own_does_not_invalidate() {
        // `version` bumps for every change, including the ones that leave the geometry
        // alone — a colour, an opacity. Keying on it meant recolouring two hundred
        // selected shapes regenerated two hundred identical drawables.
        let mut cache = ShapeCache::new();
        let mut e = element();
        cache.get(&e);
        e.version += 1;
        e.stroke_color = "#e03131".into();
        cache.get(&e);
        assert_eq!(cache.stats().1, 1);
    }

    #[test]
    fn a_restyle_that_changes_the_geometry_still_invalidates() {
        let mut cache = ShapeCache::new();
        let mut e = element();
        cache.get(&e);
        e.stroke_width += 1.0;
        cache.get(&e);
        assert_eq!(cache.stats().1, 2, "stroke width changes the hachure gap");
    }

    #[test]
    fn unused_geometry_is_evicted() {
        let mut cache = ShapeCache::new();
        let e = element();
        cache.get(&e);
        assert_eq!(cache.len(), 1);

        // Two sweeps with nothing asked for in between: one generation of grace, then
        // gone. See `ShapeCache::sweep`.
        cache.sweep();
        cache.sweep();
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

        let mut recoloured = base.clone();
        recoloured.version += 1;
        recoloured.stroke_color = "#e03131".into();
        assert_eq!(
            shape_fingerprint(&base),
            shape_fingerprint(&recoloured),
            "a colour is applied when the shape is painted, not when it is generated"
        );

        let mut widened = base.clone();
        widened.stroke_width += 1.0;
        assert_ne!(shape_fingerprint(&base), shape_fingerprint(&widened));

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
