//! Inserting an image.
//!
//! Decoding a file is the host's job — only a browser can turn a PNG into pixels, and
//! only it can report the natural size. Everything that follows from that size is
//! arithmetic, and is tested here, because two frontends dropping the same file in the
//! same place must produce the same element rather than two different ideas of "a
//! sensible size".

mod common;
use common::*;
use draw_engine::*;

/// A typical board: a tall enough window at 1:1.
const VIEWPORT_H: f64 = 900.0;

fn centre() -> Point {
    Point { x: 0.0, y: 0.0 }
}

// --------------------------------------------------------------------- sizing

#[test]
fn a_small_image_arrives_at_its_own_size() {
    // Never enlarged. An icon blown up to fill the screen is not what anyone dropping an
    // icon meant, and undoing the scaling by hand is fiddly.
    let fit = fit_image(64.0, 64.0, VIEWPORT_H, 1.0, centre());
    assert_close(fit.width, 64.0);
    assert_close(fit.height, 64.0);
}

#[test]
fn a_huge_image_is_brought_down_to_fit_the_viewport() {
    // A photograph at its natural size can be several screens tall, and lands as a wall
    // of pixels with its handles somewhere off the board.
    let fit = fit_image(4000.0, 3000.0, VIEWPORT_H, 1.0, centre());
    assert!(
        fit.height <= VIEWPORT_H * IMAGE_VIEWPORT_FRACTION,
        "an image taller than the allowance: {}",
        fit.height
    );
    assert!(fit.height > 0.0);
}

#[test]
fn the_proportions_survive_the_fit() {
    // The one thing that must never change. A photograph subtly stretched on insert is a
    // mistake nobody notices until much later.
    for (w, h) in [
        (4000.0, 3000.0),
        (100.0, 2500.0),
        (2500.0, 100.0),
        (7.0, 13.0),
    ] {
        let fit = fit_image(w, h, VIEWPORT_H, 1.0, centre());
        assert_near(fit.width / fit.height, w / h, 1e-9);
    }
}

#[test]
fn an_image_lands_centred_on_the_point_it_was_dropped_at() {
    let fit = fit_image(400.0, 200.0, VIEWPORT_H, 1.0, Point { x: 500.0, y: 300.0 });
    assert_close(fit.x + fit.width / 2.0, 500.0);
    assert_close(fit.y + fit.height / 2.0, 300.0);
}

#[test]
fn zooming_out_drops_a_physically_larger_image() {
    // The allowance is in screen pixels, so dividing by the zoom converts it to world
    // units: the image is bigger on the board and the same size on screen. Without the
    // division, dropping a file while zoomed out would produce a postage stamp.
    let near = fit_image(4000.0, 3000.0, VIEWPORT_H, 1.0, centre());
    let far = fit_image(4000.0, 3000.0, VIEWPORT_H, 0.25, centre());
    assert!(
        far.height > near.height,
        "zoomed out: {} vs {}",
        far.height,
        near.height
    );

    // Larger, but not four times larger: the allowance is also clamped by the window's
    // own height less its margin, which is what stops an image dropped at a far-out zoom
    // from being a hundred screens wide.
    let ceiling = (VIEWPORT_H - IMAGE_VIEWPORT_MARGIN).max(IMAGE_MIN_VIEWPORT_HEIGHT);
    assert_close(far.height, ceiling);
    assert!(far.height / near.height < 4.0, "the clamp did not apply");
}

#[test]
fn a_short_window_still_gets_a_usable_image() {
    // The floor under the allowance. In a 200px-tall embed the viewport fraction alone
    // would leave a 100px image, which is too small to work with.
    let fit = fit_image(4000.0, 3000.0, 200.0, 1.0, centre());
    assert!(fit.height > 0.0);
    assert!(
        fit.height <= IMAGE_MIN_VIEWPORT_HEIGHT.max(200.0),
        "height {}",
        fit.height
    );
}

#[test]
fn an_image_that_could_not_be_measured_produces_nothing() {
    // A zero or NaN natural size means the host failed to decode the file. Dividing by
    // it yields an element with NaN bounds — one that cannot be selected, moved or
    // deleted, so the only way to get rid of it is to clear the board.
    for (w, h) in [
        (0.0, 100.0),
        (100.0, 0.0),
        (f64::NAN, 100.0),
        (100.0, f64::NAN),
        (f64::INFINITY, 100.0),
        (-10.0, 100.0),
    ] {
        let fit = fit_image(w, h, VIEWPORT_H, 1.0, centre());
        assert_eq!(fit.width, 0.0, "{w}x{h} produced a width");
        assert_eq!(fit.height, 0.0, "{w}x{h} produced a height");
    }
}

#[test]
fn a_nonsense_zoom_does_not_produce_a_nonsense_image() {
    for zoom in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let fit = fit_image(400.0, 300.0, VIEWPORT_H, zoom, centre());
        assert!(
            fit.width.is_finite() && fit.height.is_finite() && fit.width > 0.0,
            "zoom {zoom} gave {}x{}",
            fit.width,
            fit.height
        );
    }
}

// ----------------------------------------------------------------- proportions

#[test]
fn an_image_holds_its_proportions_unless_shift_is_held() {
    // Excalidraw's rule, and their comment states it exactly: images are proportional by
    // default and Shift frees them; every other shape is free by default and Shift
    // constrains it.
    let image = create_element_default(
        DrawElementType::Image,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        },
    );
    assert!(locks_aspect_ratio(&image, false), "free by default");
    assert!(!locks_aspect_ratio(&image, true), "shift did not free it");

    let rect = box_at(0.0, 0.0, 200.0, 100.0);
    assert!(!locks_aspect_ratio(&rect, false));
    assert!(locks_aspect_ratio(&rect, true));
}

#[test]
fn dragging_an_images_corner_keeps_it_in_proportion() {
    // The rule, through the engine rather than through the predicate. Resizing by a
    // corner with no modifier must not stretch the picture.
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, VIEWPORT_H, 1.0);
    let id = engine
        .insert_image("data:image/png;base64,AAAA", 400.0, 200.0, 600.0, 450.0)
        .expect("image was not inserted");

    let before = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("image vanished");
    let ratio = before.width / before.height;

    engine.select(vec![id.clone()]);
    // Aimed at the handle the engine actually paints, not at the element's bare corner:
    // handles sit a few pixels outside the outline, which is further than their own hit
    // radius, so a click on the corner itself misses them and starts a drag instead.
    let corner = {
        let view = engine.paint_view();
        let handles = selection_handles(&before, view.handle_layout);
        let se = handles
            .iter()
            .find(|handle| handle.kind == HandleKind::Se)
            .expect("no south-east handle");
        Point { x: se.x, y: se.y }
    };
    engine.begin_pointer(corner.x, corner.y, false, false);
    engine.move_pointer(corner.x + 300.0, corner.y, false, false);
    engine.end_pointer();

    let after = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("image vanished");
    assert!(after.width > before.width, "the corner drag did nothing");
    assert_near(after.width / after.height, ratio, 1e-6);
}

// -------------------------------------------------------------- through the engine

#[test]
fn inserting_an_image_puts_it_on_the_board_and_selects_it() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, VIEWPORT_H, 1.0);

    let id = engine
        .insert_image("data:image/png;base64,AAAA", 800.0, 600.0, 400.0, 300.0)
        .expect("image was not inserted");

    let scene = engine.get_scene();
    let image = scene.iter().find(|el| el.id == id).expect("image vanished");
    assert_eq!(image.kind, DrawElementType::Image);
    assert_eq!(
        image.data_url.as_deref(),
        Some("data:image/png;base64,AAAA")
    );
    assert_eq!(engine.get_selection(), vec![id]);
}

#[test]
fn an_image_takes_no_sloppiness_from_the_current_style() {
    // An image brings its own appearance. A roughness inherited from the last rectangle
    // would make the box around the picture wobble.
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, VIEWPORT_H, 1.0);
    engine.set_next_style(stroke_patch("#e03131"));

    let id = engine
        .insert_image("data:image/png;base64,AAAA", 400.0, 300.0, 400.0, 300.0)
        .expect("image was not inserted");
    let image = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("image vanished");

    assert_eq!(image.roughness, 0.0);
    assert!(is_transparent(&image.background_color));
}

#[test]
fn an_unmeasurable_image_is_refused_rather_than_inserted_broken() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, VIEWPORT_H, 1.0);

    assert!(engine
        .insert_image("data:image/png;base64,AAAA", 0.0, 0.0, 400.0, 300.0)
        .is_none());
    assert!(engine.get_scene().iter().all(|el| el.is_deleted));
}

#[test]
fn an_image_dropped_inside_a_frame_belongs_to_it() {
    // Membership follows position, and an image is no different from a shape drawn
    // there. Without this it would be the one element a frame did not carry.
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, VIEWPORT_H, 1.0);

    engine.set_tool(DrawTool::Frame);
    engine.begin_pointer(100.0, 100.0, false, false);
    engine.move_pointer(700.0, 700.0, false, false);
    engine.end_pointer();
    let frame_id = engine
        .get_scene()
        .into_iter()
        .find(is_frame)
        .map(|el| el.id)
        .expect("no frame");

    let id = engine
        .insert_image("data:image/png;base64,AAAA", 200.0, 200.0, 400.0, 400.0)
        .expect("image was not inserted");
    let image = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("image vanished");

    assert_eq!(image.frame_id.as_deref(), Some(frame_id.as_str()));
}

#[test]
fn inserting_an_image_is_one_undoable_step() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, VIEWPORT_H, 1.0);
    engine
        .insert_image("data:image/png;base64,AAAA", 400.0, 300.0, 400.0, 300.0)
        .expect("image was not inserted");

    engine.undo();
    assert!(
        engine.get_scene().iter().all(|el| el.is_deleted),
        "one undo did not remove the image"
    );
}

#[test]
fn the_image_travels_with_the_element_through_a_save_and_load() {
    // The reason the data rides on the element rather than in a side table: export,
    // autosave, copy/paste and realtime all carry it with no extra plumbing.
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, VIEWPORT_H, 1.0);
    engine
        .insert_image("data:image/png;base64,SGVsbG8=", 400.0, 300.0, 400.0, 300.0)
        .expect("image was not inserted");

    let json = engine.export_json();
    let restored = elements_from_json(&json).expect("the scene did not round-trip");
    let image = restored
        .iter()
        .find(|el| el.kind == DrawElementType::Image)
        .expect("the image did not survive");
    assert_eq!(
        image.data_url.as_deref(),
        Some("data:image/png;base64,SGVsbG8=")
    );
}

fn assert_near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() < tolerance,
        "expected {actual} within {tolerance} of {expected}"
    );
}

// ---------------------------------------------------------------------------
// Corners and export
// ---------------------------------------------------------------------------

/// An inserted image starts sharp, as Excalidraw's does (`App.tsx:10084` inserts with
/// `roundness: null`). It used to inherit the default style's rounding, so the panel said
/// Round for a picture whose bitmap is square — a control that described nothing.
#[test]
fn an_inserted_image_starts_with_sharp_corners() {
    let mut engine = engine_with_scene(vec![]);
    let id = engine
        .insert_image("data:image/png;base64,AAAA", 400.0, 200.0, 400.0, 300.0)
        .expect("inserted");
    let image = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .unwrap();
    assert_eq!(image.roundness, None);
}

/// The exported SVG carries the picture. It used to fall through to the shapes' arm and
/// write a stroked `<rect>`, so exporting a board with a photo on it produced an empty box.
#[test]
fn an_svg_export_carries_the_picture() {
    let mut engine = engine_with_scene(vec![]);
    let url = "data:image/png;base64,iVBORw0KGgo=";
    engine
        .insert_image(url, 400.0, 200.0, 400.0, 300.0)
        .expect("inserted");
    let scene = engine.get_scene();
    let bounds = scene_bounds(&scene).unwrap();
    let svg = scene_to_svg(&scene, bounds, 10.0, "#ffffff");
    assert!(svg.contains("<image"), "no <image> element in {svg}");
    assert!(svg.contains(url), "the data URL is not in the export");
    assert_eq!(
        svg.matches("<rect").count(),
        1,
        "the background only — no empty box standing in for the picture: {svg}"
    );
}

/// Flipped, it exports flipped: a flip is a negative width, which the canvas draws
/// through a scale of -1 and a bare `<image>` with the normalized box does not.
#[test]
fn a_flipped_image_exports_flipped() {
    let mut engine = engine_with_scene(vec![]);
    let id = engine
        .insert_image(
            "data:image/png;base64,iVBORw0KGgo=",
            400.0,
            200.0,
            400.0,
            300.0,
        )
        .expect("inserted");
    engine.select(vec![id]);
    engine.flip_selection(FlipAxis::Horizontal);
    let scene = engine.get_scene();
    assert!(scene[0].width < 0.0, "setup: a flip is a negative width");

    let bounds = scene_bounds(&scene).unwrap();
    let svg = scene_to_svg(&scene, bounds, 10.0, "#ffffff");

    assert!(svg.contains("scale(-1 1)"), "{svg}");
}

/// An image with no picture yet — a board loaded without its file — exports as nothing
/// rather than as an `<image>` pointing nowhere.
#[test]
fn an_image_without_a_picture_exports_no_broken_reference() {
    let mut element = create_element_default(
        DrawElementType::Image,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
        },
    );
    element.data_url = None;
    let bounds = element_bounds(&element);
    let svg = scene_to_svg(&[element], bounds, 10.0, "#ffffff");
    assert!(!svg.contains("<image"), "{svg}");
    // Nothing at all: not an `<image>`, and not the stroked box it used to fall through to.
    assert_eq!(
        svg.matches("<rect").count(),
        1,
        "the background only: {svg}"
    );
}
