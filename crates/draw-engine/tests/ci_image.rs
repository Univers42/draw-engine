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

/// An inserted image starts sharp, as Excalidraw's does (`App.tsx@1118751f:10092` inserts with
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

// ---------------------------------------------------------------------------
// Turning an image (design.md:478 "Rotate")
// ---------------------------------------------------------------------------

/// A 400×300 picture dropped at (400, 300), and its id, at a viewport 1:1.
fn placed_image(url: &str) -> (DrawEngine, String) {
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, VIEWPORT_H, 1.0);
    let id = engine
        .insert_image(url, 400.0, 300.0, 400.0, 300.0)
        .expect("image was not inserted");
    (engine, id)
}

/// The rotate handle where the engine paints it.
fn rotate_handle(engine: &DrawEngine, id: &str) -> Point {
    let image = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("image vanished");
    let view = engine.paint_view();
    selection_handles(&image, view.handle_layout)
        .into_iter()
        .find(|h| h.kind == HandleKind::Rotate)
        .map(|h| Point { x: h.x, y: h.y })
        .expect("an image offers a rotation handle")
}

/// A press, a few moves, a release.
fn drag(engine: &mut DrawEngine, from: Point, to: Point) {
    engine.begin_pointer(from.x, from.y, false, false);
    for step in 1..=4 {
        let t = f64::from(step) / 4.0;
        engine.move_pointer(
            from.x + (to.x - from.x) * t,
            from.y + (to.y - from.y) * t,
            false,
            false,
        );
    }
    engine.end_pointer();
}

/// "Rotate", for the one kind whose whole surface is a bitmap: the angle is the pointer's,
/// and the picture is drawn turned while its own box stays put.
///
/// The angle is the oracle's expression for every element
/// (`resizeElements.ts@1118751f:221-233`), so there is nothing image-specific to get
/// wrong about the number itself. What is image-specific is what the turn does to the
/// *drawn* box: the stored one is the unturned rectangle, and only the painted extent
/// swaps its sides. Reading the stored box to find out what a turned image covers gets
/// 400×300 for a picture that is standing up.
#[test]
fn turning_an_image_gives_it_the_angles_angle() {
    let (mut engine, id) = placed_image("data:image/png;base64,AAAA");
    let before = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("image vanished");
    assert_close(before.angle, 0.0);
    // The oracle's centre, taken from the element's absolute coords rather than from the
    // engine's own pivot — see `ci_line_multipoint.rs`'s note on why that matters.
    let box_before = element_bounds(&before);
    let centre = (
        (box_before.min_x + box_before.max_x) / 2.0,
        (box_before.min_y + box_before.max_y) / 2.0,
    );

    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.clone()]);
    // Due right of the centre, which asks for a quarter turn exactly.
    let handle = rotate_handle(&engine, &id);
    drag(
        &mut engine,
        handle,
        Point {
            x: centre.0 + 300.0,
            y: centre.1,
        },
    );

    let after = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("image vanished");
    assert!(
        (after.angle - std::f64::consts::FRAC_PI_2).abs() < 1e-6,
        "expected a quarter turn ({}), got {}",
        std::f64::consts::FRAC_PI_2,
        after.angle
    );
    // A quarter-turned 400×300 picture stands up: 300 across, 400 down.
    let drawn = element_rotated_bounds(&after);
    assert!(
        (drawn.max_x - drawn.min_x - 300.0).abs() < 1e-6
            && (drawn.max_y - drawn.min_y - 400.0).abs() < 1e-6,
        "a quarter-turned 400×300 picture takes a 300×400 box, got {}×{}",
        drawn.max_x - drawn.min_x,
        drawn.max_y - drawn.min_y
    );
}

/// The rest of it. A turn is a number on the element, so a picture keeps the size it was
/// inserted at and the file it was inserted with. Nothing about a turn may reach the
/// bitmap: a resized image is a stretched one, and a lost `data_url` is a grey box.
#[test]
fn turning_an_image_leaves_its_size_and_its_picture_alone() {
    let url = "data:image/png;base64,iVBORw0KGgo=";
    let (mut engine, id) = placed_image(url);
    let before = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("image vanished");

    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.clone()]);
    let box_before = element_bounds(&before);
    let centre = (
        (box_before.min_x + box_before.max_x) / 2.0,
        (box_before.min_y + box_before.max_y) / 2.0,
    );
    let handle = rotate_handle(&engine, &id);
    drag(
        &mut engine,
        handle,
        Point {
            x: centre.0,
            y: centre.1 + 300.0,
        },
    );

    let after = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("image vanished");
    // Due *below* the centre asks for `atan2(+300, 0) + pi/2` — a half turn, not a
    // quarter. Stated rather than approximated, because a turn that came out as anything
    // else would still leave the stored box and the data URL alone.
    assert!(
        (after.angle - std::f64::consts::PI).abs() < 1e-6,
        "expected a half turn ({}), got {}",
        std::f64::consts::PI,
        after.angle
    );
    assert_close(after.x, before.x);
    assert_close(after.y, before.y);
    assert_close(after.width, before.width);
    assert_close(after.height, before.height);
    assert_eq!(
        after.data_url.as_deref(),
        Some(url),
        "the picture is the element's only copy of the file"
    );
    assert_eq!(after.kind, DrawElementType::Image);
}

// ---------------------------------------------------------------------------
// Copying and duplicating an image (design.md:490 "Copy/paste")
// ---------------------------------------------------------------------------

/// Every live image on the board, bottom first, so a copy's minted id never has to be
/// predicted.
fn images(engine: &DrawEngine) -> Vec<DrawElement> {
    engine
        .get_scene()
        .into_iter()
        .filter(|el| !el.is_deleted && el.kind == DrawElementType::Image)
        .collect()
}

/// "Copy/paste" for a picture, which is the one element kind whose whole content is
/// something the copy has to carry.
///
/// Everything else about the element is a handful of numbers that any round trip keeps by
/// itself. An image's content is a `data:` URL measured in kilobytes, and the reason it
/// rides on the element rather than in a side table is that this is the only place that
/// has to be right: a copy that keeps the numbers and drops the picture is a grey box
/// exactly where the picture was, and nothing else on the board would show it.
///
/// The oracle's copy is `deepCopyElement` (`duplicate.ts@1118751f:109`) — a field-for-field
/// clone, so a file reference travels with it.
#[test]
fn a_copied_image_pastes_with_its_picture_and_a_place_of_its_own() {
    let url = "data:image/png;base64,iVBORw0KGgo=";
    let (mut engine, id) = placed_image(url);
    engine.select(vec![id.clone()]);
    let original = images(&engine);
    assert_eq!(original.len(), 1, "setup: one image");

    let json = engine
        .copy_selection()
        .expect("copying an image produced nothing");
    assert!(
        engine.paste_json(Some(&json), None),
        "the clipboard held no scene"
    );

    let pasted = images(&engine);
    assert_eq!(pasted.len(), 2, "the paste did not add an image");
    let copy = pasted
        .iter()
        .find(|el| el.id != id)
        .expect("the copy should be a second image");
    assert_eq!(
        copy.data_url.as_deref(),
        Some(url),
        "the pasted copy lost the picture"
    );
    assert_ne!(
        copy.id, id,
        "a paste is a new element, not the same one again"
    );
    // `PASTE_OFFSET` (12) down and to the right, so the copy is somewhere else and the
    // original is exactly where it was.
    assert_close(copy.x, original[0].x + 12.0);
    assert_close(copy.y, original[0].y + 12.0);
    assert_close(original[0].width, copy.width);
    assert_close(original[0].height, copy.height);
    assert_eq!(
        original[0].data_url.as_deref(),
        Some(url),
        "the original is untouched by a copy"
    );
    assert_eq!(engine.get_selection(), vec![copy.id.clone()]);
}

/// Ctrl+D on a picture, which is the same clone by a different road: in place rather than
/// through the clipboard.
///
/// The offset is the oracle's — `DEFAULT_GRID_SIZE / 2` on each axis, `constants.ts@1118751f:
/// 290` gives the grid and `actionDuplicateSelection.tsx@1118751f:78-79` the half of it —
/// and it is a number rather than "somewhere nearby", so it can be stated.
#[test]
fn a_duplicated_image_keeps_its_picture_and_lands_half_a_grid_away() {
    let url = "data:image/png;base64,iVBORw0KGgo=";
    let (mut engine, id) = placed_image(url);

    engine.select(vec![id.clone()]);
    engine.duplicate_selection(10.0, 10.0);

    let pasted = images(&engine);
    assert_eq!(pasted.len(), 2, "the duplicate did not land");
    let copy = pasted
        .iter()
        .find(|el| el.id != id)
        .expect("the copy should be a second image");
    assert_eq!(
        copy.data_url.as_deref(),
        Some(url),
        "the duplicate lost the picture"
    );
    assert_close(copy.width, pasted[0].width);
    assert_close(copy.height, pasted[0].height);
    let source = pasted.iter().find(|el| el.id == id).unwrap();
    assert_close(copy.x, source.x + 10.0);
    assert_close(copy.y, source.y + 10.0);
    // The copy is what you go on to move, and the original has not been touched.
    assert_eq!(engine.get_selection(), vec![copy.id.clone()]);
}

/// The third road, and the one that catches a clone which only copies what the *engine*
/// knows: a board saved and reloaded. A duplicate that forgot the URL would survive the
/// clipboard and still lose the picture here, because the URL is what the file format has
/// to carry.
#[test]
fn a_duplicated_image_still_has_its_picture_after_a_save_and_reload() {
    let url = "data:image/png;base64,iVBORw0KGgo=";
    let (mut engine, id) = placed_image(url);
    engine.select(vec![id.clone()]);
    engine.duplicate_selection(10.0, 10.0);
    let copy = images(&engine)
        .into_iter()
        .find(|el| el.id != id)
        .expect("the copy should be a second image");

    let json = engine.export_json();
    let restored = elements_from_json(&json).expect("the scene did not round-trip");
    let urls: Vec<Option<String>> = restored
        .iter()
        .filter(|el| el.kind == DrawElementType::Image)
        .map(|el| el.data_url.clone())
        .collect();
    assert_eq!(
        urls,
        vec![Some(url.to_string()); 2],
        "both pictures should survive the save, got {urls:?}"
    );
    assert!(
        restored.iter().any(|el| el.id == copy.id),
        "the copy should survive the save under its own id"
    );
}
