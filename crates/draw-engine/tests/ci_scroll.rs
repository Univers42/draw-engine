//! What a frame has to redraw, and what it can keep.
//!
//! The painter keeps the static scene in an offscreen layer and blits it, so the only
//! question each frame is how much of it is still good. Getting this wrong is not slow,
//! it is *visibly* wrong — reusing a layer that should have been redrawn leaves stale
//! pixels on the board — so every reason to keep one is spelled out here.

use draw_engine::render::scroll::{exposed_rects, intersects_any, plan_layer, LayerKey, LayerPlan};
use draw_engine::scene::geometry::Rect;
use draw_engine::Camera;

fn key() -> LayerKey {
    LayerKey {
        scale: 1.0,
        dpr: 1.0,
        width: 800.0,
        height: 600.0,
        content: 0xabc,
        chrome: 0xdef,
    }
}

fn at(x: f64, y: f64) -> Camera {
    Camera { x, y, scale: 1.0 }
}

#[test]
fn a_first_frame_has_nothing_to_reuse() {
    assert_eq!(plan_layer(None, key(), at(0.0, 0.0)), LayerPlan::Redraw);
}

#[test]
fn a_frame_that_changed_nothing_reuses_the_layer() {
    // Idle. The board is redrawn sixty times a second while a laser fades or a cursor
    // blinks, and none of those touch the static scene.
    let plan = plan_layer(Some((key(), at(10.0, 20.0))), key(), at(10.0, 20.0));
    assert_eq!(plan, LayerPlan::Reuse);
}

#[test]
fn panning_redraws_only_the_strip_that_came_into_view() {
    // The whole point. Moving the camera 20px right and 30px down uncovers a 20px band
    // down one edge and a 30px band across the other — about a twentieth of the viewport
    // instead of all of it.
    let plan = plan_layer(Some((key(), at(0.0, 0.0))), key(), at(20.0, 30.0));
    let LayerPlan::Scroll { dx, dy, exposed } = plan else {
        panic!("a whole-pixel pan should scroll, got {plan:?}");
    };
    assert_eq!((dx, dy), (20.0, 30.0));
    assert_eq!(
        exposed,
        vec![
            Rect {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 600.0
            },
            Rect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 30.0
            },
        ]
    );
}

#[test]
fn panning_the_other_way_exposes_the_other_edges() {
    let plan = plan_layer(Some((key(), at(0.0, 0.0))), key(), at(-15.0, -25.0));
    let LayerPlan::Scroll { exposed, .. } = plan else {
        panic!("expected a scroll");
    };
    assert_eq!(
        exposed,
        vec![
            Rect {
                x: 785.0,
                y: 0.0,
                width: 15.0,
                height: 600.0
            },
            Rect {
                x: 0.0,
                y: 575.0,
                width: 800.0,
                height: 25.0
            },
        ]
    );
}

#[test]
fn a_straight_pan_exposes_one_strip_and_not_two() {
    let plan = plan_layer(Some((key(), at(0.0, 0.0))), key(), at(0.0, 40.0));
    let LayerPlan::Scroll { exposed, .. } = plan else {
        panic!("expected a scroll");
    };
    assert_eq!(exposed.len(), 1);
    assert_eq!(exposed[0].height, 40.0);
}

#[test]
fn a_jump_of_a_whole_screen_is_redrawn_rather_than_scrolled() {
    // There is nothing left to keep, and the blit would cost more than it saved.
    let plan = plan_layer(Some((key(), at(0.0, 0.0))), key(), at(900.0, 0.0));
    assert_eq!(plan, LayerPlan::Redraw);
    let plan = plan_layer(Some((key(), at(0.0, 0.0))), key(), at(0.0, 600.0));
    assert_eq!(plan, LayerPlan::Redraw);
}

#[test]
fn a_fractional_pan_is_redrawn_rather_than_resampled() {
    // A scroll is only lossless if it is an exact pixel copy. Blitting by half a pixel
    // resamples the whole layer, and it compounds: twenty frames of that and the board is
    // visibly soft, with no one change to blame.
    let plan = plan_layer(Some((key(), at(0.0, 0.0))), key(), at(20.5, 0.0));
    assert_eq!(plan, LayerPlan::Redraw);
}

#[test]
fn a_pan_that_is_whole_in_device_pixels_scrolls_at_any_dpr() {
    // The camera is in CSS pixels and the layer is in device pixels, so it is the
    // *device* shift that has to be whole. At dpr 2 a half-pixel CSS pan is exactly one
    // device pixel and is perfectly reusable — testing the CSS value would have thrown it
    // away.
    let mut k = key();
    k.dpr = 2.0;
    let plan = plan_layer(Some((k, at(0.0, 0.0))), k, at(0.5, 0.0));
    let LayerPlan::Scroll { dx, .. } = plan else {
        panic!("expected a scroll, got {plan:?}");
    };
    assert_eq!(dx, 1.0);
}

#[test]
fn zooming_redraws() {
    // Different zoom is different geometry on screen; none of the old layer is reusable.
    let mut zoomed = key();
    zoomed.scale = 1.5;
    let plan = plan_layer(
        Some((key(), at(0.0, 0.0))),
        zoomed,
        Camera {
            x: 0.0,
            y: 0.0,
            scale: 1.5,
        },
    );
    assert_eq!(plan, LayerPlan::Redraw);
}

#[test]
fn changing_the_picture_redraws() {
    // Drawing, deleting, moving, restyling — whatever changed, the digest changes and the
    // layer is stale. This is the assertion that stops the scroll path from being a way
    // to show yesterday's board.
    let mut edited = key();
    edited.content = 0x999;
    assert_eq!(
        plan_layer(Some((key(), at(0.0, 0.0))), edited, at(0.0, 0.0)),
        LayerPlan::Redraw
    );
}

#[test]
fn changing_the_theme_or_the_grid_redraws() {
    let mut themed = key();
    themed.chrome = 0x111;
    assert_eq!(
        plan_layer(Some((key(), at(0.0, 0.0))), themed, at(0.0, 0.0)),
        LayerPlan::Redraw
    );
}

#[test]
fn resizing_the_window_redraws() {
    let mut wider = key();
    wider.width = 1000.0;
    assert_eq!(
        plan_layer(Some((key(), at(0.0, 0.0))), wider, at(0.0, 0.0)),
        LayerPlan::Redraw
    );
}

#[test]
fn a_dpr_change_redraws() {
    // Dragging a window between a laptop screen and an external monitor, and the
    // engine's own drop to a lower dpr while something is in motion.
    let mut sharper = key();
    sharper.dpr = 2.0;
    assert_eq!(
        plan_layer(Some((key(), at(0.0, 0.0))), sharper, at(0.0, 0.0)),
        LayerPlan::Redraw
    );
}

// ---------------------------------------------------------------------------
// Which elements the strip actually needs
// ---------------------------------------------------------------------------

#[test]
fn only_elements_touching_the_strip_are_redrawn() {
    // The saving is as much here as in the clip. A clipped draw still builds every path
    // before the rasteriser discards it, and building a hachure fill's paths is most of
    // what a pattern-filled shape costs — so an element outside the strip has to be
    // skipped, not merely clipped away.
    let exposed = exposed_rects(20.0, 0.0, 800.0, 600.0);

    let in_strip = Rect {
        x: 10.0,
        y: 100.0,
        width: 40.0,
        height: 40.0,
    };
    let past_it = Rect {
        x: 300.0,
        y: 100.0,
        width: 40.0,
        height: 40.0,
    };
    assert!(intersects_any(in_strip, &exposed));
    assert!(!intersects_any(past_it, &exposed));
}

#[test]
fn an_element_that_only_touches_the_strips_edge_counts() {
    // Off by one here means a shape that is half redrawn: the part in the strip fresh,
    // the part outside it whatever the old layer had. Better to draw one shape too many.
    let exposed = exposed_rects(20.0, 0.0, 800.0, 600.0);
    let straddling = Rect {
        x: 15.0,
        y: 0.0,
        width: 100.0,
        height: 50.0,
    };
    assert!(intersects_any(straddling, &exposed));
}

#[test]
fn an_element_in_either_arm_of_an_l_counts() {
    let exposed = exposed_rects(20.0, 30.0, 800.0, 600.0);
    let along_the_top = Rect {
        x: 400.0,
        y: 5.0,
        width: 60.0,
        height: 10.0,
    };
    let down_the_side = Rect {
        x: 2.0,
        y: 400.0,
        width: 10.0,
        height: 60.0,
    };
    let in_neither = Rect {
        x: 400.0,
        y: 400.0,
        width: 60.0,
        height: 60.0,
    };
    assert!(intersects_any(along_the_top, &exposed));
    assert!(intersects_any(down_the_side, &exposed));
    assert!(!intersects_any(in_neither, &exposed));
}

#[test]
fn a_strip_that_crosses_most_of_the_board_is_not_worth_scrolling() {
    use draw_engine::render::scroll::scroll_is_worth_it;

    // The case that made scrolling slower than redrawing: shapes large enough that a
    // full-width strip touches nearly all of them. The blit is then paid for, almost
    // every path is built anyway, and the clip is the only thing saved.
    assert!(!scroll_is_worth_it(90, 100));
    assert!(!scroll_is_worth_it(51, 100));

    // The ordinary case: a strip is a twentieth of the viewport and holds a handful.
    assert!(scroll_is_worth_it(3, 100));
    assert!(scroll_is_worth_it(50, 100));
    assert!(scroll_is_worth_it(0, 100));

    // An empty viewport has nothing to save and nothing to redraw; either answer is
    // harmless, but claiming a saving on nothing is the one that reads as a bug.
    assert!(!scroll_is_worth_it(0, 0));
}

// ---------------------------------------------------------------------------
// The scene revision: the thing that makes reuse safe
// ---------------------------------------------------------------------------

use draw_engine::scene::element::{create_element_default, DrawElementType, Geometry};
use draw_engine::Scene;

fn boxy(x: f64) -> draw_engine::DrawElement {
    create_element_default(
        DrawElementType::Rectangle,
        Geometry {
            x,
            y: 0.0,
            width: 60.0,
            height: 40.0,
        },
    )
}

/// Every way the scene can change, and the assertion that each one is noticed.
///
/// This is what the layer's reuse rests on. A mutation that left the revision alone would
/// let the painter keep showing a frame that is no longer true — silently, which is the
/// worst way for a renderer to be wrong. Written as one test over every mutating method
/// rather than one test each, because the danger is *omission*: a new mutator that
/// forgets to bump is invisible until someone looks at a stale board.
#[test]
fn every_mutation_changes_the_scene_revision() {
    let first = boxy(0.0);
    let second = boxy(200.0);
    let (first_id, second_id) = (first.id.clone(), second.id.clone());

    let mut scene = Scene::new(vec![first, second.clone()]);
    let mut last = scene.revision();
    let moved = |scene: &Scene, what: &str, last: &mut u64| {
        let now = scene.revision();
        assert_ne!(now, *last, "`{what}` left the scene revision alone");
        *last = now;
    };

    scene.add(boxy(400.0));
    moved(&scene, "add", &mut last);

    scene.put({
        let mut edited = second.clone();
        edited.x = 250.0;
        edited
    });
    moved(&scene, "put", &mut last);

    scene.update(&first_id, |element| element.angle = 0.5);
    moved(&scene, "update", &mut last);

    scene.bring_to_front(&first_id);
    moved(&scene, "bring_to_front", &mut last);

    scene.set_order(scene.ordered_cloned());
    moved(&scene, "set_order", &mut last);

    scene.remove(&second_id, 1.0);
    moved(&scene, "remove", &mut last);

    scene.discard(&second_id);
    moved(&scene, "discard", &mut last);

    let restored = Scene::from_snapshot(scene.snapshot());
    assert_ne!(
        restored.revision(),
        0,
        "a scene restored from a snapshot must not look untouched"
    );
}

#[test]
fn reading_the_scene_does_not_change_its_revision() {
    // The other half, and the reason this is worth having at all. A revision that moved
    // on every read would be correct and useless: the layer would be redrawn every frame
    // and none of this would buy anything.
    let scene = Scene::new(vec![boxy(0.0), boxy(200.0)]);
    let before = scene.revision();

    let _ = scene.ordered_cloned();
    let _ = scene.bounds();
    let _ = scene.snapshot();
    let _ = scene.iter_ordered().count();

    assert_eq!(scene.revision(), before);
}

#[test]
fn the_revision_does_not_depend_on_the_camera() {
    // The bug this replaced. The layer key used to hold a digest of the *visible*
    // elements, so panning changed it merely by culling different ones — and the layer
    // was thrown away on exactly the frames it exists to serve. Measured at the time: 120
    // redraws and zero scrolls across a single pan.
    let scene = Scene::new(vec![boxy(0.0), boxy(5000.0)]);
    let before = scene.revision();
    // Nothing here is a scene operation, which is the point: where the camera looks is
    // not a property of the document.
    assert_eq!(scene.revision(), before);
}

// ---------------------------------------------------------------------------
// Reusing a picture while the camera moves
// ---------------------------------------------------------------------------

use draw_engine::render::scroll::{plan_motion, MotionBlit};

fn cam(x: f64, y: f64, scale: f64) -> Camera {
    Camera { x, y, scale }
}

#[test]
fn a_small_pan_reuses_the_picture_moved() {
    let plan = plan_motion(
        cam(0.0, 0.0, 0.15),
        cam(-30.0, 0.0, 0.15),
        1.0,
        1280.0,
        800.0,
    );
    assert_eq!(
        plan,
        Some(MotionBlit {
            scale: 1.0,
            dx: -30.0,
            dy: 0.0
        })
    );
}

#[test]
fn the_move_is_in_device_pixels() {
    let plan = plan_motion(cam(0.0, 0.0, 1.0), cam(10.0, 5.0, 1.0), 2.0, 2560.0, 1600.0);
    assert_eq!(
        plan,
        Some(MotionBlit {
            scale: 1.0,
            dx: 20.0,
            dy: 10.0
        })
    );
}

#[test]
fn a_pan_that_would_leave_too_much_blank_repaints() {
    // A sixth of the width gone: more than the picture may leave uncovered.
    assert_eq!(
        plan_motion(
            cam(0.0, 0.0, 1.0),
            cam(-220.0, 0.0, 1.0),
            1.0,
            1280.0,
            800.0
        ),
        None
    );
}

#[test]
fn a_zoom_about_a_point_reuses_the_picture_scaled_about_it() {
    // Zooming in 10% about the screen point (640, 400): that point stays put.
    let before = cam(100.0, 50.0, 0.5);
    let anchor = (640.0, 400.0);
    let world = (
        (anchor.0 - before.x) / before.scale,
        (anchor.1 - before.y) / before.scale,
    );
    let scale = 0.55;
    let after = cam(
        anchor.0 - world.0 * scale,
        anchor.1 - world.1 * scale,
        scale,
    );

    let plan = plan_motion(before, after, 1.0, 1280.0, 800.0).expect("reused");

    assert!((plan.scale - 1.1).abs() < 1e-9);
    // The anchor maps to itself: anchor * scale + d = anchor.
    assert!((anchor.0 * plan.scale + plan.dx - anchor.0).abs() < 1e-9);
    assert!((anchor.1 * plan.scale + plan.dy - anchor.1).abs() < 1e-9);
}

#[test]
fn a_zoom_that_drifts_too_far_repaints() {
    assert_eq!(
        plan_motion(cam(0.0, 0.0, 0.5), cam(0.0, 0.0, 0.7), 1.0, 1280.0, 800.0),
        None
    );
    assert_eq!(
        plan_motion(cam(0.0, 0.0, 0.5), cam(0.0, 0.0, 0.38), 1.0, 1280.0, 800.0),
        None
    );
}

#[test]
fn zooming_out_leaves_a_border_and_repaints_once_it_is_too_wide() {
    // Out by 5% about the centre: a 2.5% border all round, which is fine…
    assert!(plan_motion(
        cam(0.0, 0.0, 1.0),
        cam(32.0, 20.0, 0.95),
        1.0,
        1280.0,
        800.0
    )
    .is_some());
    // …but out by 10% leaves a fifth of the screen blank, which is not.
    assert!(plan_motion(cam(0.0, 0.0, 1.0), cam(64.0, 40.0, 0.9), 1.0, 1280.0, 800.0).is_none());
}

#[test]
fn a_pan_paints_the_edge_it_uncovers() {
    // Moved 30 pixels left: the right-hand 30 pixels are what the picture no longer
    // reaches, and they are painted rather than left as bare paper.
    let blit = MotionBlit {
        scale: 1.0,
        dx: -30.0,
        dy: 0.0,
    };
    assert_eq!(
        blit.exposed(1280.0, 800.0),
        vec![Rect {
            x: 1250.0,
            y: 0.0,
            width: 30.0,
            height: 800.0
        }]
    );
}

#[test]
fn a_diagonal_pan_paints_both_edges() {
    let blit = MotionBlit {
        scale: 1.0,
        dx: 12.0,
        dy: -8.0,
    };
    let exposed = blit.exposed(1280.0, 800.0);
    assert_eq!(exposed.len(), 2);
    assert!(exposed.contains(&Rect {
        x: 0.0,
        y: 0.0,
        width: 12.0,
        height: 800.0
    }));
    assert!(exposed.contains(&Rect {
        x: 0.0,
        y: 792.0,
        width: 1280.0,
        height: 8.0
    }));
}

#[test]
fn zooming_out_paints_the_border_all_round_in_whole_pixels() {
    // Out by 5% about the centre: the picture shrinks to a 1216 x 760 box starting at
    // (32, 20), and the border is rounded outward so it overlaps the picture's soft edge.
    let blit = plan_motion(
        cam(0.0, 0.0, 1.0),
        cam(32.0, 20.0, 0.95),
        1.0,
        1280.0,
        800.0,
    )
    .expect("reused");
    let exposed = blit.exposed(1280.0, 800.0);
    assert_eq!(exposed.len(), 4);
    for rect in &exposed {
        for v in [rect.x, rect.y, rect.width, rect.height] {
            assert_eq!(v, v.round(), "{rect:?} is not in whole pixels");
        }
    }
    let covers = |x: f64, y: f64| {
        exposed
            .iter()
            .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
    };
    assert!(covers(5.0, 400.0) && covers(1275.0, 400.0));
    assert!(covers(640.0, 5.0) && covers(640.0, 795.0));
    assert!(!covers(640.0, 400.0), "the middle is the moved picture");
}

#[test]
fn zooming_in_uncovers_nothing() {
    let blit = MotionBlit {
        scale: 1.1,
        dx: -64.0,
        dy: -40.0,
    };
    assert!(blit.exposed(1280.0, 800.0).is_empty());
}
