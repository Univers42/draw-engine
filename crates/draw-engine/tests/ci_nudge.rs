//! How far an arrow key moves the selection: `ELEMENT_TRANSLATE_AMOUNT` and
//! `ELEMENT_SHIFT_TRANSLATE_AMOUNT`, or the grid when it is held to
//! (`App.tsx@1118751f:5801-5810`).

mod common;
use common::*;
use draw_engine::*;

#[test]
fn an_arrow_key_moves_a_unit_or_five_and_by_the_grid_when_held_to_it() {
    let mut engine = engine_with_scene(vec![]);
    assert_eq!(
        (engine.nudge_step(false), engine.nudge_step(true)),
        (1.0, 5.0)
    );
    engine.set_grid(GridSettings {
        enabled: true,
        size: 20.0,
        step: 5,
        snap: true,
    });
    assert_eq!(
        (engine.nudge_step(false), engine.nudge_step(true)),
        (20.0, 1.0)
    );
    // A grid only shown is not held to.
    engine.set_grid(GridSettings {
        enabled: true,
        size: 20.0,
        step: 5,
        snap: false,
    });
    assert_eq!(engine.nudge_step(true), 5.0);
}
