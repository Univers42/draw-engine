//! Frames.
//!
//! A frame is an area, not a set. Almost every test here is really asking the same
//! question — does membership follow from where things are? — because that is the one
//! property that separates a frame from a group, and the one that breaks quietly if the
//! engine ever starts remembering membership instead of deriving it.

mod common;
use common::*;
use draw_engine::*;

fn frame_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
    let mut frame = create_element_default(
        DrawElementType::Frame,
        Geometry {
            x,
            y,
            width,
            height,
        },
    );
    frame.name = Some("Frame 1".to_string());
    frame
}

/// A frame drawn by dragging from one corner to the other, through the engine.
fn draw_frame(engine: &mut DrawEngine, x1: f64, y1: f64, x2: f64, y2: f64) -> String {
    engine.set_tool(DrawTool::Frame);
    engine.begin_pointer(x1, y1, false, false);
    engine.move_pointer(x2, y2, false, false);
    engine.end_pointer();
    engine
        .get_scene()
        .into_iter()
        .rev()
        .find(|el| is_frame(el) && !el.is_deleted)
        .map(|el| el.id)
        .expect("no frame was created")
}

fn child_frame_of(engine: &DrawEngine, id: &str) -> Option<String> {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .and_then(|el| el.frame_id)
}

// ------------------------------------------------------------------- containment

#[test]
fn a_frame_takes_what_lies_wholly_inside_it() {
    let frame = frame_at(0.0, 0.0, 400.0, 300.0);
    let inside = box_at(50.0, 50.0, 100.0, 80.0);
    let outside = box_at(500.0, 50.0, 100.0, 80.0);

    assert!(element_in_frame_bounds(&inside, &frame));
    assert!(!element_in_frame_bounds(&outside, &frame));
}

#[test]
fn an_element_hanging_over_the_edge_is_not_captured() {
    // Containment rather than overlap, and this is the case that makes the difference.
    // Capture is silent — nothing asks first — so a rule that swept in everything a
    // frame merely touched would adopt the neighbouring diagram the moment you drew one.
    let frame = frame_at(0.0, 0.0, 400.0, 300.0);
    let straddling = box_at(350.0, 50.0, 200.0, 80.0);

    assert!(!element_in_frame_bounds(&straddling, &frame));
    assert!(element_intersects_frame(&straddling, &frame));
    assert!(element_overlaps_frame(&straddling, &frame));
}

#[test]
fn an_element_big_enough_to_swallow_the_frame_counts_as_overlapping() {
    // A background rectangle behind a frame contains it without ever crossing its
    // border, so neither of the other two tests would notice it.
    let frame = frame_at(100.0, 100.0, 200.0, 150.0);
    let backdrop = box_at(0.0, 0.0, 800.0, 600.0);

    assert!(element_contains_frame(&backdrop, &frame));
    assert!(!element_intersects_frame(&backdrop, &frame));
    assert!(element_overlaps_frame(&backdrop, &frame));
}

#[test]
fn a_diagonal_line_is_judged_by_its_path_and_not_by_its_box() {
    // The reason intersection is tested against outlines. A diagonal from the top-left
    // to the bottom-right of the board has a bounding box covering almost everything,
    // and a frame tucked into a corner it passes nowhere near would look like a hit.
    let frame = frame_at(600.0, 20.0, 120.0, 90.0);
    let diagonal = connector(0.0, 0.0, 800.0, 600.0, DrawElementType::Line);

    let boxes_overlap = {
        let f = element_rotated_bounds(&frame);
        let l = element_rotated_bounds(&diagonal);
        f.min_x <= l.max_x && f.max_x >= l.min_x && f.min_y <= l.max_y && f.max_y >= l.min_y
    };
    assert!(boxes_overlap, "the test is pointless unless the boxes do");
    assert!(!element_intersects_frame(&diagonal, &frame));
}

#[test]
fn a_rotated_element_is_judged_by_the_room_it_actually_takes() {
    // Turned 45 degrees, a wide flat rectangle reaches further vertically than its
    // unrotated height, and can poke out of a frame that would otherwise hold it.
    let frame = frame_at(0.0, 0.0, 200.0, 60.0);
    let mut flat = box_at(20.0, 10.0, 160.0, 30.0);
    assert!(element_in_frame_bounds(&flat, &frame), "upright it fits");

    flat.angle = std::f64::consts::FRAC_PI_4;
    assert!(
        !element_in_frame_bounds(&flat, &frame),
        "turned, it reaches past the frame but was still counted as inside"
    );
}

#[test]
fn a_frame_never_holds_another_frame() {
    // Nested frames are out of scope, and silently adopting one would produce a child
    // that moves its own parent.
    let outer = frame_at(0.0, 0.0, 800.0, 600.0);
    let inner = frame_at(100.0, 100.0, 200.0, 150.0);
    let shape = box_at(120.0, 120.0, 40.0, 40.0);

    let captured = elements_captured_by([&inner, &shape].into_iter(), &outer);
    assert_eq!(captured, vec![shape.id.clone()]);
}

#[test]
fn a_bound_label_is_not_captured_on_its_own() {
    // The label belongs to its container, which carries membership for both. Capturing
    // it separately would let a frame own a label whose shape lives outside it.
    let frame = frame_at(0.0, 0.0, 400.0, 300.0);
    let mut label = text_at(50.0, 50.0, 60.0, 20.0);
    label.container_id = Some("some-shape".to_string());

    assert!(elements_captured_by([&label].into_iter(), &frame).is_empty());
}

// -------------------------------------------------------------------- the tool

#[test]
fn drawing_a_frame_over_existing_work_adopts_it() {
    let inside = box_at(60.0, 60.0, 80.0, 60.0);
    let outside = box_at(600.0, 60.0, 80.0, 60.0);
    let (inside_id, outside_id) = (inside.id.clone(), outside.id.clone());
    let mut engine = engine_with_scene(vec![inside, outside]);

    let frame_id = draw_frame(&mut engine, 20.0, 20.0, 400.0, 300.0);

    assert_eq!(
        child_frame_of(&engine, &inside_id).as_deref(),
        Some(frame_id.as_str())
    );
    assert_eq!(child_frame_of(&engine, &outside_id), None);
}

#[test]
fn a_frame_that_was_barely_dragged_is_discarded() {
    // Drawn by dragging, so a click that moved a couple of pixels is a click. Left
    // behind, it would capture nothing and be nearly impossible to grab again.
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Frame);
    engine.begin_pointer(100.0, 100.0, false, false);
    engine.move_pointer(103.0, 102.0, false, false);
    engine.end_pointer();

    assert!(engine.get_scene().iter().all(|el| el.is_deleted));
}

#[test]
fn a_frame_takes_no_style_from_the_current_settings() {
    // A frame is a boundary, not a drawing. Tinted with whatever colour you last used it
    // would read as a rectangle someone drew, which is exactly what it must not look
    // like.
    let mut engine = engine_with_scene(vec![]);
    engine.set_next_style(stroke_patch("#e03131"));

    let frame_id = draw_frame(&mut engine, 20.0, 20.0, 300.0, 200.0);
    let frame = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == frame_id)
        .expect("frame vanished");

    assert_eq!(frame.stroke_color, FRAME_STROKE);
    assert_eq!(frame.stroke_width, FRAME_STROKE_WIDTH);
    assert_eq!(frame.roughness, 0.0);
    assert_eq!(frame.roundness, Some(FRAME_RADIUS));
}

#[test]
fn a_new_frame_is_named_after_the_highest_number_already_used() {
    // Numbered from the highest existing rather than from the count, so deleting
    // "Frame 2" of three does not produce a second "Frame 3".
    let mut one = frame_at(0.0, 0.0, 10.0, 10.0);
    one.name = Some("Frame 1".into());
    let mut three = frame_at(0.0, 0.0, 10.0, 10.0);
    three.name = Some("Frame 3".into());

    assert_eq!(default_frame_name([&one, &three].into_iter()), "Frame 4");
    assert_eq!(default_frame_name(std::iter::empty()), "Frame 1");
}

#[test]
fn an_oddly_named_frame_does_not_confuse_the_numbering() {
    let mut renamed = frame_at(0.0, 0.0, 10.0, 10.0);
    renamed.name = Some("Architecture".into());
    assert_eq!(default_frame_name([&renamed].into_iter()), "Frame 1");
}

// ------------------------------------------------------------------- dragging

#[test]
fn moving_a_frame_carries_what_it_holds() {
    let child = box_at(60.0, 60.0, 80.0, 60.0);
    let child_id = child.id.clone();
    let mut engine = engine_with_scene(vec![child]);
    let frame_id = draw_frame(&mut engine, 20.0, 20.0, 400.0, 300.0);

    engine.set_tool(DrawTool::Select);
    engine.select(vec![frame_id.clone()]);
    // Grab the frame by its edge, which is the only part of it that is not a hole.
    engine.begin_pointer(20.0, 20.0, false, false);
    engine.move_pointer(120.0, 70.0, false, false);
    engine.end_pointer();

    let moved = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == child_id)
        .expect("child vanished");
    assert_close(moved.x, 160.0);
    assert_close(moved.y, 110.0);
    assert_eq!(moved.frame_id.as_deref(), Some(frame_id.as_str()));
}

#[test]
fn dragging_an_element_out_of_a_frame_ends_its_membership() {
    // Filled, because it is dragged by its middle and a transparent shape is hollow to
    // the hit test — grabbing one there starts a marquee, exactly as on a real board.
    let child = filled(box_at(60.0, 60.0, 80.0, 60.0));
    let child_id = child.id.clone();
    let mut engine = engine_with_scene(vec![child]);
    let frame_id = draw_frame(&mut engine, 20.0, 20.0, 400.0, 300.0);
    assert_eq!(
        child_frame_of(&engine, &child_id).as_deref(),
        Some(frame_id.as_str())
    );

    engine.set_tool(DrawTool::Select);
    engine.select(vec![child_id.clone()]);
    engine.begin_pointer(100.0, 90.0, false, false);
    engine.move_pointer(700.0, 90.0, false, false);
    engine.end_pointer();

    assert_eq!(child_frame_of(&engine, &child_id), None);
}

#[test]
fn dragging_an_element_into_a_frame_starts_it() {
    let stray = filled(box_at(600.0, 60.0, 80.0, 60.0));
    let stray_id = stray.id.clone();
    let mut engine = engine_with_scene(vec![stray]);
    let frame_id = draw_frame(&mut engine, 20.0, 20.0, 400.0, 300.0);
    assert_eq!(child_frame_of(&engine, &stray_id), None);

    engine.set_tool(DrawTool::Select);
    engine.select(vec![stray_id.clone()]);
    engine.begin_pointer(640.0, 90.0, false, false);
    engine.move_pointer(140.0, 120.0, false, false);
    engine.end_pointer();

    assert_eq!(
        child_frame_of(&engine, &stray_id).as_deref(),
        Some(frame_id.as_str())
    );
}

#[test]
fn an_element_half_out_of_a_frame_does_not_belong_to_it() {
    // The same containment rule as capture, applied while dragging. Dropping something
    // across a frame's edge is ambiguous, and Excalidraw resolves it as "out".
    let child = filled(box_at(60.0, 60.0, 80.0, 60.0));
    let child_id = child.id.clone();
    let mut engine = engine_with_scene(vec![child]);
    draw_frame(&mut engine, 20.0, 20.0, 400.0, 300.0);

    engine.set_tool(DrawTool::Select);
    engine.select(vec![child_id.clone()]);
    engine.begin_pointer(100.0, 90.0, false, false);
    engine.move_pointer(400.0, 90.0, false, false);
    engine.end_pointer();

    assert_eq!(child_frame_of(&engine, &child_id), None);
}

// -------------------------------------------------------------------- deleting

#[test]
fn deleting_a_frame_keeps_what_it_held() {
    // This pinned the opposite — the child went with its frame — under a comment that
    // claimed Excalidraw's behaviour. The oracle keeps the children, clears their frame
    // and selects them (`actionDeleteSelected.tsx@1118751f:115-122`, pinned by
    // `actionDeleteSelected.test.tsx@1118751f:11`): the frame goes, the work in it does not.
    let child = box_at(60.0, 60.0, 80.0, 60.0);
    let bystander = box_at(600.0, 60.0, 80.0, 60.0);
    let (child_id, bystander_id) = (child.id.clone(), bystander.id.clone());
    let mut engine = engine_with_scene(vec![child, bystander]);
    let frame_id = draw_frame(&mut engine, 20.0, 20.0, 400.0, 300.0);

    engine.select(vec![frame_id.clone()]);
    engine.delete_selection();

    let scene = engine.get_scene();
    let gone = |id: &str| {
        scene
            .iter()
            .find(|el| el.id == id)
            .is_none_or(|el| el.is_deleted)
    };
    assert!(gone(&frame_id), "the frame survived");
    assert!(!gone(&child_id), "the child went with its frame");
    assert_eq!(child_frame_of(&engine, &child_id), None);
    assert_eq!(engine.get_selection(), vec![child_id]);
    assert!(
        !gone(&bystander_id),
        "an element outside the frame was deleted"
    );
}

// -------------------------------------------------------------------- painting

#[test]
fn only_the_children_that_stick_out_are_clipped() {
    // Clipping costs a path per element per frame. A child wholly inside has nothing to
    // clip, so paying for it would be pure waste — and the difference is invisible.
    let frame = frame_at(0.0, 0.0, 400.0, 300.0);
    let tucked_in = box_at(50.0, 50.0, 100.0, 80.0);
    let straddling = box_at(350.0, 50.0, 200.0, 80.0);
    let backdrop = box_at(-100.0, -100.0, 900.0, 700.0);

    assert!(!needs_frame_clip(&tucked_in, &frame));
    assert!(needs_frame_clip(&straddling, &frame));
    assert!(needs_frame_clip(&backdrop, &frame));
}

#[test]
fn a_frames_name_sits_above_it_rather_than_inside() {
    // Inside, the name would be drawn over the frame's contents and clipped along with
    // them — a label you cannot read once the frame is full.
    let frame = frame_at(100.0, 200.0, 400.0, 300.0);
    let anchor = frame_name_anchor(&frame);

    assert_close(anchor.x, 100.0);
    assert!(anchor.y < 200.0, "the name was placed inside the frame");
    assert_close(anchor.y, 200.0 - FRAME_NAME_OFFSET_Y);
}

#[test]
fn the_paint_view_carries_a_name_and_a_clip_for_every_frame() {
    let straddling = filled(box_at(350.0, 50.0, 200.0, 80.0));
    let straddling_id = straddling.id.clone();
    let mut engine = engine_with_scene(vec![straddling]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    draw_frame(&mut engine, 0.0, 0.0, 400.0, 300.0);

    // Nudge the straddling element so it is a child that pokes out. Drawing the frame
    // over it would not have captured it, so it has to be put there deliberately.
    engine.set_tool(DrawTool::Select);
    engine.select(vec![straddling_id.clone()]);
    engine.begin_pointer(400.0, 90.0, false, false);
    engine.move_pointer(200.0, 90.0, false, false);
    engine.end_pointer();

    let view = engine.paint_view();
    assert_eq!(view.frame_names.len(), 1, "the frame has no name to paint");
    assert_eq!(view.frame_names[0].1, "Frame 1");
}

// -------------------------------------------------------------------- the tool table

#[test]
fn the_frame_is_reachable_by_its_own_key() {
    assert_eq!(tool_for_key("f"), Some(DrawTool::Frame));
    assert_eq!(tool_for_key("F"), Some(DrawTool::Frame));
}

#[test]
fn undo_puts_a_moved_frame_and_its_children_back_together() {
    // One history entry for the whole gesture. If the children were moved outside the
    // move interaction they would land in their own entry, and one undo would tear the
    // frame away from its contents.
    let child = box_at(60.0, 60.0, 80.0, 60.0);
    let child_id = child.id.clone();
    let mut engine = engine_with_scene(vec![child]);
    let frame_id = draw_frame(&mut engine, 20.0, 20.0, 400.0, 300.0);

    engine.set_tool(DrawTool::Select);
    engine.select(vec![frame_id.clone()]);
    engine.begin_pointer(20.0, 20.0, false, false);
    engine.move_pointer(320.0, 20.0, false, false);
    engine.end_pointer();

    engine.undo();

    let scene = engine.get_scene();
    let child = scene
        .iter()
        .find(|el| el.id == child_id)
        .expect("child gone");
    let frame = scene
        .iter()
        .find(|el| el.id == frame_id)
        .expect("frame gone");
    assert_close(child.x, 60.0);
    assert_close(frame.x, 20.0);
}

#[test]
fn a_frame_is_grabbed_by_its_border_and_not_through_its_middle() {
    // A frame is a boundary you reach through. Counted as solid it swallows every click
    // that lands in it, so the moment you framed a diagram its contents became
    // unselectable — and dragging one of them dragged the frame instead, which is how
    // this was found.
    let child = filled(box_at(60.0, 60.0, 80.0, 60.0));
    let child_id = child.id.clone();
    let mut engine = engine_with_scene(vec![child]);
    let frame_id = draw_frame(&mut engine, 20.0, 20.0, 400.0, 300.0);

    engine.set_tool(DrawTool::Select);
    engine.clear_selection();

    // Empty space inside the frame: the click should reach neither.
    engine.begin_pointer(300.0, 250.0, false, false);
    engine.end_pointer();
    assert!(
        engine.get_selection().is_empty(),
        "clicking inside a frame selected something"
    );

    // Over a child: the child, not the frame around it.
    engine.begin_pointer(100.0, 90.0, false, false);
    engine.end_pointer();
    assert_eq!(engine.get_selection(), vec![child_id]);

    // On the border: the frame.
    engine.clear_selection();
    engine.begin_pointer(20.0, 150.0, false, false);
    engine.end_pointer();
    assert_eq!(engine.get_selection(), vec![frame_id]);
}

// ------------------------------------------------------------------- rename
//
// Excalidraw parity for `editingFrame` (`App.tsx@1118751f:2166-2170` `resetEditingFrame`,
// `:2219-2261` the input, `:2334-2340` the double click that opens it) and
// `getFrameLikeTitle` (`packages/element/src/frame.ts@1118751f:974-980`).

mod rename {
    use super::*;

    fn element(engine: &DrawEngine, id: &str) -> DrawElement {
        engine
            .get_scene()
            .into_iter()
            .find(|el| el.id == id)
            .expect("the element is in the scene")
    }

    #[test]
    fn the_name_label_is_hit_and_nothing_else_is() {
        let frame = frame_at(100.0, 100.0, 400.0, 300.0);
        let id = frame.id.clone();
        let engine = engine_with_scene(vec![frame]);

        // Just above the frame's top-left corner, where `frame_name_anchor` and
        // `paint_frame_names` put the label's baseline.
        assert_eq!(engine.frame_name_at(105.0, 95.0), Some(id));
        // Inside the frame's own body: the label sits outside the frame, not on it.
        assert_eq!(engine.frame_name_at(300.0, 250.0), None);
        // Nowhere near either.
        assert_eq!(engine.frame_name_at(-1000.0, -1000.0), None);
    }

    #[test]
    fn a_double_click_on_the_name_requests_a_rename_and_not_a_text() {
        let mut engine = engine_with_scene(vec![]);
        let id = draw_frame(&mut engine, 100.0, 100.0, 500.0, 400.0);
        engine.drain_events();
        let before = engine.get_scene().len();

        engine.handle_double_click(105.0, 95.0);
        let events = engine.drain_events();
        let request = events
            .frame_rename
            .expect("the double click opens a frame rename");
        assert_eq!(request.id, id);
        assert_eq!(request.name, "Frame 1");
        assert!(events.text_edit.is_none(), "not a text edit");
        assert!(engine.text_edit_session().is_none());
        assert_eq!(
            engine.get_scene().len(),
            before,
            "no stray text was created"
        );
    }

    #[test]
    fn rename_writes_the_trimmed_name_as_one_stamped_step() {
        let frame = frame_at(50.0, 50.0, 300.0, 200.0);
        let id = frame.id.clone();
        let created_version = frame.version;
        let mut engine = engine_with_scene(vec![frame]);

        engine.rename_frame(&id, "  Clients  ");
        let now = element(&engine, &id);
        assert_eq!(now.name.as_deref(), Some("Clients"));
        assert!(now.version > created_version, "the stamp moves");
        let delta = engine
            .drain_events()
            .scene_delta
            .expect("it syncs to peers and autosave like any edit");
        assert!(delta.updated.iter().any(|el| el.id == id));

        // One undo step: back to the original name, and no further.
        engine.undo();
        assert_eq!(element(&engine, &id).name.as_deref(), Some("Frame 1"));
        engine.undo();
        assert_eq!(
            element(&engine, &id).name.as_deref(),
            Some("Frame 1"),
            "there was only one step to undo"
        );
    }

    #[test]
    fn an_emptied_name_falls_back_to_the_generic_default() {
        let frame = frame_at(50.0, 50.0, 300.0, 200.0);
        let id = frame.id.clone();
        let mut engine = engine_with_scene(vec![frame]);

        engine.rename_frame(&id, "   ");
        let now = element(&engine, &id);
        assert_eq!(now.name, None, "an empty name is stored as none, not blank");
        assert_eq!(frame_display_name(&now), "Frame");

        let painted = engine.paint_view().frame_names;
        let shown = painted
            .iter()
            .find(|(_, _, painted_id)| *painted_id == id)
            .map(|(_, name, _)| name.clone());
        assert_eq!(
            shown,
            Some("Frame".to_string()),
            "still painted, never blank"
        );
    }

    #[test]
    fn renaming_to_the_same_effective_name_is_not_a_step() {
        let frame = frame_at(50.0, 50.0, 300.0, 200.0);
        let id = frame.id.clone();
        let mut engine = engine_with_scene(vec![frame]);

        engine.rename_frame(&id, "Frame 1");
        assert!(
            engine.drain_events().scene_delta.is_none(),
            "no change, no edit"
        );
    }

    #[test]
    fn renaming_a_missing_frame_does_nothing() {
        let mut engine = engine_with_scene(vec![]);
        engine.rename_frame("nope", "X");
        assert!(engine.drain_events().scene_delta.is_none());
    }
}
