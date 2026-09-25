//! Picking a shape up from the middle of it.
//!
//! A shape with no fill is hit on its **outline** only — the middle of it is a hole you
//! can click through to whatever is behind. That is right, and Excalidraw does the same:
//! `shouldTestInside` in `packages/element/src/collision.ts` returns false for a
//! transparent background, so an empty rectangle drawn over a diagram does not swallow
//! every click in the area it covers.
//!
//! But it is only right for a shape you have *not* selected. Once a shape is selected it
//! is hit anywhere in its box — the oracle's `hitElement` tests a selected element
//! against its bounding box (`packages/excalidraw/components/App.tsx@1118751f:6784-6808`), and two
//! or more selected share one common box (`isHittingCommonBoundingBoxOfSelectedElements`,
//! `App.tsx@1118751f:9791-9814`). So a press there keeps the selection and a drag moves it.
//!
//! A press there that is let go without moving is a click on nothing, and lets go of the
//! selection (`App.tsx@1118751f:12352-12395`, `hitElementBoundingBoxOnly`). A selected line or
//! arrow of two points has no box at all (`hasBoundingBox`,
//! `packages/element/src/transformHandles.ts@1118751f:328-353`), so nothing but the line grabs it.

mod common;
use common::*;
use draw_engine::*;

/// The viewport is 800x600 at scale 1 with the camera at the origin, so screen and world
/// coordinates are the same number and a test can say where it is clicking.
fn shape(filled: bool) -> DrawElement {
    let rect = box_at(100.0, 100.0, 300.0, 200.0);
    if filled {
        self::filled(rect)
    } else {
        rect
    }
}

/// Well inside the shape, far from any edge or handle.
const INSIDE: (f64, f64) = (250.0, 200.0);

fn drag(engine: &mut DrawEngine, from: (f64, f64), by: (f64, f64)) {
    engine.begin_pointer(from.0, from.1, false, false);
    engine.move_pointer(from.0 + by.0, from.1 + by.1, false, false);
    engine.end_pointer();
}

fn position_of(engine: &DrawEngine, id: &str) -> (f64, f64) {
    let element = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == id)
        .expect("still in the scene");
    (element.x, element.y)
}

mod not_selected {
    use super::*;

    /// The hole stays a hole. This is the behaviour the other tests must not break.
    #[test]
    fn an_empty_shape_ignores_a_click_in_its_middle() {
        let rect = shape(false);
        let id = rect.id.clone();
        let mut engine = engine_with_scene(vec![rect]);

        engine.begin_pointer(INSIDE.0, INSIDE.1, false, false);
        engine.end_pointer();

        assert!(
            engine.get_selection().is_empty(),
            "an unselected transparent shape is not hit from inside"
        );
        assert_eq!(position_of(&engine, &id), (100.0, 100.0));
    }

    #[test]
    fn an_empty_shape_is_still_caught_by_its_outline() {
        let rect = shape(false);
        let id = rect.id.clone();
        let mut engine = engine_with_scene(vec![rect]);

        // On the top edge.
        engine.begin_pointer(250.0, 100.0, false, false);
        engine.end_pointer();

        assert_eq!(engine.get_selection(), vec![id]);
    }

    /// A background makes the shape solid, so the middle of it is the shape.
    #[test]
    fn a_filled_shape_is_grabbed_from_inside() {
        let rect = shape(true);
        let id = rect.id.clone();
        let mut engine = engine_with_scene(vec![rect]);

        drag(&mut engine, INSIDE, (40.0, 25.0));

        assert_eq!(engine.get_selection(), vec![id.clone()]);
        let (x, y) = position_of(&engine, &id);
        assert_close(x, 140.0);
        assert_close(y, 125.0);
    }
}

mod already_selected {
    use super::*;

    /// The bug. Before this, the click fell through to a marquee: the selection was
    /// dropped and the shape stayed exactly where it was, so an empty rectangle could
    /// only be moved by aiming at its outline.
    #[test]
    fn an_empty_shape_can_be_dragged_from_inside_once_selected() {
        let rect = shape(false);
        let id = rect.id.clone();
        let mut engine = engine_with_scene(vec![rect]);
        engine.select(vec![id.clone()]);

        drag(&mut engine, INSIDE, (40.0, 25.0));

        assert_eq!(
            engine.get_selection(),
            vec![id.clone()],
            "the selection must survive the grab"
        );
        let (x, y) = position_of(&engine, &id);
        assert_close(x, 140.0);
        assert_close(y, 125.0);
    }

    #[test]
    fn a_filled_shape_still_drags_from_inside() {
        let rect = shape(true);
        let id = rect.id.clone();
        let mut engine = engine_with_scene(vec![rect]);
        engine.select(vec![id.clone()]);

        drag(&mut engine, INSIDE, (-30.0, 15.0));

        let (x, y) = position_of(&engine, &id);
        assert_close(x, 70.0);
        assert_close(y, 115.0);
    }

    /// A click with no drag moves nothing and lets go, as a click on empty canvas does:
    /// the oracle deselects when the release hit only the selected element's box
    /// (`App.tsx@1118751f:12352-12395`). This used to keep the selection, on a misreading of the
    /// oracle as offering the box to two or more elements only.
    #[test]
    fn a_click_in_the_hole_lets_go() {
        let rect = shape(false);
        let id = rect.id.clone();
        let mut engine = engine_with_scene(vec![rect]);
        engine.select(vec![id.clone()]);

        engine.begin_pointer(INSIDE.0, INSIDE.1, false, false);
        assert_eq!(engine.get_selection(), vec![id.clone()], "not on the press");
        engine.end_pointer();

        assert!(engine.get_selection().is_empty());
        assert_eq!(position_of(&engine, &id), (100.0, 100.0));
    }

    /// The box is where the pointer offers a move, so the cursor says so.
    #[test]
    fn the_hole_shows_the_move_cursor() {
        let rect = shape(false);
        let id = rect.id.clone();
        let mut engine = engine_with_scene(vec![rect]);
        engine.set_tool(DrawTool::Select);
        assert_eq!(
            engine.hover_cursor(INSIDE.0, INSIDE.1),
            HoverCursor::Default,
            "setup: an unselected hole offers nothing"
        );
        engine.select(vec![id]);

        assert_eq!(engine.hover_cursor(INSIDE.0, INSIDE.1), HoverCursor::Move);
    }

    /// Outside the selection there is no shape and nothing selected, so the click means
    /// what it always meant. Without this the fix would make it impossible to ever
    /// deselect by clicking the canvas.
    #[test]
    fn a_click_outside_the_selection_still_lets_go() {
        let rect = shape(false);
        let id = rect.id.clone();
        let mut engine = engine_with_scene(vec![rect]);
        engine.select(vec![id]);

        engine.begin_pointer(600.0, 450.0, false, false);
        engine.end_pointer();

        assert!(engine.get_selection().is_empty());
    }

    /// The hole is only closed for the shape that is selected. A second empty shape
    /// somewhere else stays clickable-through, or selecting one thing would make the
    /// whole board solid.
    #[test]
    fn another_empty_shape_keeps_its_hole() {
        let selected = shape(false);
        let other = box_at(450.0, 100.0, 200.0, 200.0);
        let selected_id = selected.id.clone();
        let mut engine = engine_with_scene(vec![selected, other]);
        engine.select(vec![selected_id]);

        // Inside the *other* rectangle, which nobody selected.
        engine.begin_pointer(550.0, 200.0, false, false);
        engine.end_pointer();

        assert!(
            engine.get_selection().is_empty(),
            "clicking through an unselected empty shape clears the selection"
        );
    }
}

mod several_selected {
    use super::*;

    /// Two empty shapes with a gap between them. The gap is inside the selection's
    /// common box and inside no shape, which is exactly the case the oracle handles and
    /// the case a marquee would otherwise steal.
    #[test]
    fn the_gap_between_two_selected_shapes_drags_them_both() {
        let left = box_at(100.0, 100.0, 120.0, 200.0);
        let right = box_at(400.0, 100.0, 120.0, 200.0);
        let (left_id, right_id) = (left.id.clone(), right.id.clone());
        let mut engine = engine_with_scene(vec![left, right]);
        engine.select(vec![left_id.clone(), right_id.clone()]);

        // 300,200 is between them: in neither shape, inside the common box.
        drag(&mut engine, (300.0, 200.0), (20.0, 10.0));

        assert_eq!(engine.get_selection().len(), 2);
        assert_close(position_of(&engine, &left_id).0, 120.0);
        assert_close(position_of(&engine, &right_id).0, 420.0);
    }

    /// And a click in the gap lets go of both.
    #[test]
    fn a_click_in_the_gap_lets_go() {
        let left = box_at(100.0, 100.0, 120.0, 200.0);
        let right = box_at(400.0, 100.0, 120.0, 200.0);
        let (left_id, right_id) = (left.id.clone(), right.id.clone());
        let mut engine = engine_with_scene(vec![left, right]);
        engine.select(vec![left_id, right_id]);

        engine.begin_pointer(300.0, 200.0, false, false);
        engine.end_pointer();

        assert!(engine.get_selection().is_empty());
    }
}

mod two_point_linear {
    use super::*;

    /// A selected arrow of two points is edited by its ends and has no box drawn round
    /// it, so there is no box to grab: a press in the empty corner of its bounds starts a
    /// marquee, as it would with nothing selected. It used to pick the arrow up from
    /// anywhere inside that invisible box.
    #[test]
    fn a_selected_two_point_arrow_has_no_grab_box() {
        let arrow = connector(100.0, 100.0, 400.0, 300.0, DrawElementType::Arrow);
        let id = arrow.id.clone();
        let mut engine = engine_with_scene(vec![arrow]);
        engine.set_tool(DrawTool::Select);
        engine.select(vec![id.clone()]);
        assert_eq!(engine.hover_cursor(350.0, 130.0), HoverCursor::Default);

        drag(&mut engine, (350.0, 130.0), (40.0, 20.0));

        assert_eq!(position_of(&engine, &id), (100.0, 100.0));
        assert!(engine.get_selection().is_empty());
    }
}
