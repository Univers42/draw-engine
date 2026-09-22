//! Picking a shape up from the middle of it.
//!
//! A shape with no fill is hit on its **outline** only — the middle of it is a hole you
//! can click through to whatever is behind. That is right, and Excalidraw does the same:
//! `shouldTestInside` in `packages/element/src/collision.ts` returns false for a
//! transparent background, so an empty rectangle drawn over a diagram does not swallow
//! every click in the area it covers.
//!
//! But it is only right for a shape you have *not* selected. Once a shape is selected it
//! is the thing you are working on, its frame and handles are drawn around it, and a
//! click inside that frame can only sensibly mean "move this". Falling through to a
//! marquee there loses the selection you just made, and on a transparent rectangle there
//! is no way to move it at all except by aiming at a two-pixel line.
//!
//! This diverges from the oracle deliberately.
//! `isHittingCommonBoundingBoxOfSelectedElements` (`App.tsx:9783`) bails out at
//! `selectedElements.length < 2`, so Excalidraw offers this for a multi-selection and not
//! for a single element. The asymmetry is not something anyone asks for; a selected shape
//! behaves one way alone and another way with a friend.

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

    /// A click with no drag must not move it, and must not lose the selection either.
    #[test]
    fn a_click_without_a_drag_keeps_the_selection_and_the_position() {
        let rect = shape(false);
        let id = rect.id.clone();
        let mut engine = engine_with_scene(vec![rect]);
        engine.select(vec![id.clone()]);

        engine.begin_pointer(INSIDE.0, INSIDE.1, false, false);
        engine.end_pointer();

        assert_eq!(engine.get_selection(), vec![id.clone()]);
        assert_eq!(position_of(&engine, &id), (100.0, 100.0));
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
}
