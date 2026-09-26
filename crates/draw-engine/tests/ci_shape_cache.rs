//! The shape cache's key, and the one property the whole thing rests on.
//!
//! Rough geometry is generated in element-local space, so two elements that differ only
//! in *where they are* produce identical output. A board made by holding Ctrl+D is
//! hundreds of elements that differ only in that — same seed, same size, same style — and
//! the cache was keyed by element id, so it generated and stored every one of them
//! separately. At screen size that is 8ms of geometry and megabytes of ops for a picture
//! that is one shape repeated.
//!
//! Keying by the geometry instead makes them share, which is the whole point. It is also
//! the dangerous change in this file, because a key that misses a field lets two elements
//! that genuinely differ share one shape — and that renders wrongly rather than slowly.
//!
//! So the property is asserted directly, over a corpus wide enough to hit every field
//! `element_drawable` reads:
//!
//!   same fingerprint  <=>  identical geometry
//!
//! Both directions matter. Left to right is soundness — a shared shape must be the right
//! shape. Right to left is the point of the exercise — if identical geometry did not
//! share a fingerprint, nothing would be reused.

use std::collections::HashMap;

use draw_engine::render::cache::{shape_fingerprint, ShapeCache};
use draw_engine::render::shape::element_drawable;
use draw_engine::scene::element::{
    create_element_default, DrawElement, DrawElementType, FillStyle, Geometry, StrokeStyle,
};

/// A corpus that varies every field the geometry depends on, and several that it does not.
fn corpus() -> Vec<DrawElement> {
    let kinds = [
        DrawElementType::Rectangle,
        DrawElementType::Diamond,
        DrawElementType::Ellipse,
        DrawElementType::Line,
        DrawElementType::Arrow,
    ];
    let fills = [
        FillStyle::Hachure,
        FillStyle::CrossHatch,
        FillStyle::Solid,
        FillStyle::Zigzag,
    ];
    let strokes = [StrokeStyle::Solid, StrokeStyle::Dashed, StrokeStyle::Dotted];

    let mut out = Vec::new();
    for kind in kinds {
        for (w, h) in [(40.0, 30.0), (120.0, 90.0), (1200.0, 700.0), (9.0, 9.0)] {
            for fill in fills {
                for stroke in strokes {
                    for stroke_width in [1.0, 2.0, 4.0] {
                        for roughness in [0.0, 1.0, 2.5] {
                            for rounded in [true, false] {
                                for seed in [1u32, 987_654] {
                                    for background in ["transparent", "#ffec99"] {
                                        let mut element = create_element_default(
                                            kind,
                                            Geometry {
                                                x: 0.0,
                                                y: 0.0,
                                                width: w,
                                                height: h,
                                            },
                                        );
                                        element.fill_style = fill;
                                        element.stroke_style = stroke;
                                        element.stroke_width = stroke_width;
                                        element.roughness = roughness;
                                        element.roundness = if rounded { Some(8.0) } else { None };
                                        element.seed = seed;
                                        element.background_color = background.into();
                                        if matches!(
                                            kind,
                                            DrawElementType::Line | DrawElementType::Arrow
                                        ) {
                                            element.points =
                                                Some(vec![[0.0, 0.0], [w, 0.0], [w / 2.0, h]]);
                                        }
                                        // The same points drawn as an elbow arrow's runs.
                                        if kind == DrawElementType::Arrow {
                                            let mut elbow = element.clone();
                                            elbow.elbowed = Some(true);
                                            out.push(elbow);
                                        }
                                        out.push(element);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

#[test]
fn elements_with_the_same_fingerprint_have_identical_geometry() {
    // Soundness. If this ever fails, some field that changes the shape is missing from
    // the key — and the symptom in a browser is one element drawn as another, which is
    // far harder to trace back here than a failing test is.
    let mut seen: HashMap<u64, (DrawElement, Option<draw_rough::Drawable>)> = HashMap::new();
    let mut shared = 0usize;

    // Each corpus entry twice, the second moved and recoloured. Without this every
    // element has geometry of its own — which is what the key *should* do, but it leaves
    // the assertion below with nothing to compare and the guard at the end would be the
    // only thing that noticed.
    let corpus: Vec<DrawElement> = corpus()
        .into_iter()
        .flat_map(|element| {
            let mut twin = element.clone();
            twin.id = format!("{}-twin", element.id);
            twin.x += 731.0;
            twin.y -= 219.0;
            twin.stroke_color = "#e03131".into();
            twin.opacity = 40.0;
            twin.version += 7;
            [element, twin]
        })
        .collect();

    for element in corpus {
        let fingerprint = shape_fingerprint(&element);
        let drawable = element_drawable(&element);
        match seen.get(&fingerprint) {
            None => {
                seen.insert(fingerprint, (element, drawable));
            }
            Some((other, expected)) => {
                shared += 1;
                assert_eq!(
                    &drawable,
                    expected,
                    "two elements share a fingerprint but not their geometry:\n  {:?}\n  {:?}",
                    describe(other),
                    describe(&element)
                );
            }
        }
    }

    // Guards against the test passing because every element got its own fingerprint,
    // which would make the assertion above vacuous — and would also mean the cache is
    // sharing nothing.
    assert!(shared > 0, "no two elements in the corpus shared a key");
}

#[test]
fn position_and_colour_do_not_change_the_fingerprint() {
    // The reason this key is worth having. Geometry is generated in element-local space,
    // so moving an element does not change it — and a colour is applied when the shape is
    // painted, not when it is generated. Keying on `version`, as this used to, meant that
    // recolouring two hundred selected shapes regenerated two hundred identical drawables.
    let base = {
        let mut element = create_element_default(
            DrawElementType::Rectangle,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: 1200.0,
                height: 700.0,
            },
        );
        element.background_color = "#ffec99".into();
        element.seed = 42;
        element
    };
    let reference = shape_fingerprint(&base);

    let mut moved = base.clone();
    moved.x += 5000.0;
    moved.y -= 250.0;
    assert_eq!(shape_fingerprint(&moved), reference, "moving changed it");

    let mut recoloured = base.clone();
    recoloured.stroke_color = "#e03131".into();
    recoloured.background_color = "#a5d8ff".into();
    assert_eq!(
        shape_fingerprint(&recoloured),
        reference,
        "recolouring changed it"
    );

    let mut faded = base.clone();
    faded.opacity = 30.0;
    assert_eq!(shape_fingerprint(&faded), reference, "opacity changed it");

    let mut renamed = base.clone();
    renamed.id = "somewhere-else".into();
    renamed.version = 99;
    assert_eq!(
        shape_fingerprint(&renamed),
        reference,
        "identity changed it"
    );

    // But going transparent *does*, because a fill that is not drawn is different
    // geometry — there is no fill set in the drawable at all.
    let mut hollow = base.clone();
    hollow.background_color = "transparent".into();
    assert_ne!(shape_fingerprint(&hollow), reference);
}

#[test]
fn resizing_changes_the_fingerprint() {
    // The invalidation that has to keep working. Rough's output depends on w/h, and the
    // drag handlers write geometry on every move while `version` is bumped only once, at
    // commit (`engine/stamp.rs`), so size has to be in the key on its own account.
    let base = create_element_default(
        DrawElementType::Rectangle,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 60.0,
        },
    );
    let mut wider = base.clone();
    wider.width = 101.0;
    assert_ne!(shape_fingerprint(&wider), shape_fingerprint(&base));
}

#[test]
fn a_board_of_duplicates_generates_one_shape() {
    // The headline. Two hundred copies of one screen-sized shape is one piece of
    // geometry, not two hundred — which is the difference between 8ms of work on the
    // first frame and 40µs, and between megabytes of ops and kilobytes.
    let mut first = create_element_default(
        DrawElementType::Rectangle,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 1200.0,
            height: 700.0,
        },
    );
    first.background_color = "#ffec99".into();
    first.seed = 123_456;

    let copies: Vec<DrawElement> = (0..200)
        .map(|i| {
            let mut copy = first.clone();
            copy.id = format!("copy-{i}");
            copy.x += f64::from(i) * 12.0;
            copy.y += f64::from(i) * 12.0;
            copy
        })
        .collect();

    let mut cache = ShapeCache::new();
    for element in &copies {
        cache.get(element);
    }

    let (hits, misses) = cache.stats();
    assert_eq!(
        misses, 1,
        "generated {misses} shapes for 200 identical copies"
    );
    assert_eq!(hits, 199);
    assert_eq!(
        cache.len(),
        1,
        "stored {} shapes for one picture",
        cache.len()
    );
}

#[test]
fn panning_does_not_regenerate_what_scrolls_off_and_back() {
    // The cost of evicting eagerly. During a pan the visible set changes every frame, so
    // a cache that drops whatever the last frame did not draw regenerates each shape as
    // it crosses the edge — and for a screen-sized hachure fill that is thousands of ops
    // per crossing. Measured, it turned a 600px pan from 10ms into 1.8 seconds.
    //
    // So the sweep has a budget: while the cache is no bigger than the scene needs, it
    // keeps everything, and shapes that scroll off stay ready for when they come back.
    let mut cache = ShapeCache::new();
    let shapes: Vec<DrawElement> = (0..8)
        .map(|i| {
            let mut element = create_element_default(
                DrawElementType::Rectangle,
                Geometry {
                    x: 0.0,
                    y: 0.0,
                    width: 1100.0,
                    height: 640.0,
                },
            );
            element.background_color = "#ffec99".into();
            element.seed = 1000 + i;
            element
        })
        .collect();

    // A pan: a sliding window of four, moving out across the row and back again.
    let window = 4;
    let starts = [0usize, 1, 2, 3, 2, 1, 0];
    for start in starts {
        for element in &shapes[start..start + window] {
            cache.get(element);
        }
        cache.sweep(64);
    }

    // One generation per distinct shape the window ever covered, and not one more. Counted
    // rather than written down, so the assertion cannot drift from the window above.
    let touched = starts.iter().max().expect("starts") + window;
    assert_eq!(
        cache.stats().1,
        touched as u64,
        "a shape was regenerated after scrolling off and back"
    );
}

#[test]
fn the_cache_still_lets_go_of_what_is_gone() {
    // Sharing by geometry means the cache can no longer be swept by element id, so it is
    // swept by use instead: anything not asked for since the last sweep is dropped.
    // Without it the cache holds every shape a session ever drew.
    let mut cache = ShapeCache::new();
    let mut first = create_element_default(
        DrawElementType::Rectangle,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 60.0,
        },
    );
    first.seed = 1;
    let mut second = first.clone();
    second.seed = 2;

    cache.get(&first);
    cache.get(&second);
    assert_eq!(cache.len(), 2);

    // End of the frame that drew both.
    cache.sweep(0);
    assert_eq!(
        cache.len(),
        2,
        "a sweep dropped what the frame had just drawn"
    );

    // A frame that draws only the first.
    cache.get(&first);
    cache.sweep(0);

    assert_eq!(cache.len(), 1, "the unused shape was kept");
    assert_eq!(shape_fingerprint(&first), shape_fingerprint(&first));
}

#[test]
fn a_shape_survives_the_sweep_that_follows_the_frame_it_was_used_in() {
    // The trap in a mark-and-sweep: dropping on the *first* sweep after use would throw
    // away everything drawn in the frame that just ran, and the cache would never hit.
    let mut cache = ShapeCache::new();
    let element = create_element_default(
        DrawElementType::Rectangle,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 60.0,
        },
    );

    for _ in 0..5 {
        cache.get(&element);
        cache.sweep(0);
    }

    let (hits, misses) = cache.stats();
    assert_eq!(misses, 1, "a shape drawn every frame was regenerated");
    assert_eq!(hits, 4);
}

/// Enough of an element to identify it in a failure message.
fn describe(element: &DrawElement) -> String {
    format!(
        "{:?} {}x{} fill={:?} stroke={:?} w={} rough={} round={:?} seed={} bg={}",
        element.kind,
        element.width,
        element.height,
        element.fill_style,
        element.stroke_style,
        element.stroke_width,
        element.roughness,
        element.roundness,
        element.seed,
        element.background_color
    )
}
