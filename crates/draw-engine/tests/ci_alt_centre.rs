//! Alt while drawing: the new element grows about the press, not out of it.
//!
//! `shouldResizeFromCenter` is `event.altKey` (`packages/common/src/keys.ts@1118751f:
//! 145-146`) and the draw path hands it straight to `dragNewElement`
//! (`App.tsx@1118751f:13425`), which doubles the reach on each axis and hangs the box off
//! the press (`dragElements.ts@1118751f:371-376`):
//!
//! ```text
//! let newX = x < originX ? originX - width : originX;
//! let newY = y < originY ? originY - height : originY;
//! if (shouldResizeFromCenter) {
//!   width += width;
//!   height += height;
//!   newX = originX - width / 2;
//!   newY = originY - height / 2;
//! }
//! ```
//!
//! Two things follow, and both are what these tests pin. The press is the box's **centre**,
//! not its corner — so the same reach in the four directions gives one box, and a width and
//! a height say nothing about where it landed. And the doubling happens *after* the aspect
//! block (`:325-350`), so Shift's side is the reach and the box is twice it.

mod common;
use common::*;
use draw_engine::*;

/// A rectangle drawn from `press` through `via` to the end, with Alt held on every move.
/// Returns the element the gesture left, or `None` when it left nothing.
fn draw(press: (f64, f64), via: &[(f64, f64)], alt: bool) -> Option<DrawElement> {
    let mut engine = engine_at(IDENTITY);
    engine.set_tool(DrawTool::Rectangle);
    engine.set_alt_held(alt);
    engine.begin_pointer(press.0, press.1, false, alt);
    for (x, y) in via {
        engine.set_alt_held(alt);
        engine.move_pointer(*x, *y, false, false);
    }
    engine.end_pointer();
    engine.get_scene().into_iter().last()
}

/// The press-to-`to` drag with Alt, in one move.
fn centred(press: (f64, f64), to: (f64, f64)) -> DrawElement {
    draw(press, &[to], true).expect("a drag this long leaves an element")
}

/// The draft mid-gesture, which is the box Alt is a function of.
///
/// The properties read the draft rather than the released element because a release throws
/// away anything under a couple of units (`end_draft`, `pointer_end.rs@1118751f:413`), and
/// a quarter of the reaches here are fractions of a pixel across. That threshold is the
/// oracle's only for an element of *no* size
/// (`sizeHelpers.ts@1118751f:77` — `width === 0 && height === 0`), so ours is the wider one
/// and is pinned on its own, in
/// `a_drag_too_small_to_keep_is_thrown_away_at_the_release` below.
fn drafted(press: (f64, f64), to: (f64, f64)) -> DrawElement {
    let mut engine = engine_at(IDENTITY);
    engine.set_tool(DrawTool::Rectangle);
    engine.set_alt_held(true);
    engine.begin_pointer(press.0, press.1, false, true);
    engine.set_alt_held(true);
    engine.move_pointer(to.0, to.1, false, false);
    engine.get_scene().into_iter().last().expect("drafting")
}

fn engine_at(camera: Camera) -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, camera.scale);
    engine.set_camera(camera);
    engine
}

fn box_of(element: &DrawElement) -> [f64; 4] {
    [element.x, element.y, element.width, element.height]
}

/// Whether two boxes are the same to the precision the arithmetic carries.
fn same_box(got: [f64; 4], want: [f64; 4]) -> bool {
    got.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-9)
}

fn press_point(press: (f64, f64)) -> Point {
    Point {
        x: press.0,
        y: press.1,
    }
}

/// Whether a point is a world point, to the precision the rest of the engine holds.
fn same_point(at: Point, want: (f64, f64)) -> bool {
    (at.x - want.0).abs() < EPS && (at.y - want.1).abs() < EPS
}

/// Alt's answer to one drag: the press is the centre, and the reach is doubled.
#[test]
fn alt_draws_a_shape_about_the_press() {
    let shape = centred((100.0, 60.0), (160.0, 100.0));
    assert_eq!(box_of(&shape), [40.0, 20.0, 120.0, 80.0]);
}

/// The trap this whole file is about: a shape of the right **size** in the wrong place.
///
/// Growing the box out of the corner the press made gives 120×80 as well — a hundred and
/// twenty twice the thirty, eighty twice the twenty — but it sits at (100, 60) rather than
/// (40, 20), so the press is its top-left corner and not its middle. A width and a height
/// are both satisfied by that and nothing catches it, which is why the stored box is what
/// gets pinned: a right-size, wrong-place implementation fails on `x` and `y`.
#[test]
fn alt_puts_the_press_at_the_centre_not_the_corner() {
    let shape = centred((100.0, 60.0), (160.0, 100.0));
    assert_point_close(element_center(&shape), press_point((100.0, 60.0)));
    assert_ne!(shape.x, 100.0, "the press is not the top-left corner");
    assert_ne!(shape.y, 60.0, "the press is not the top-left corner");
}

/// The sign of the drag cannot reach the box, because the oracle computes the corner from
/// the doubled reach alone and never looks at which side the pointer went
/// (`dragElements.ts@1118751f:374-375`). All four directions are therefore one box.
#[test]
fn alt_ignores_which_way_the_drag_went() {
    let press = (100.0, 60.0);
    let out = centred(press, (160.0, 100.0));
    for to in [(40.0, 20.0), (160.0, 20.0), (40.0, 100.0)] {
        let other = centred(press, to);
        assert_eq!(box_of(&other), box_of(&out), "drag to {to:?}");
        assert_point_close(element_center(&other), press_point(press));
    }
}

/// A rectangle is not special: the same press and reach, drawn as each thing that grows
/// from a drag, is centred on the press.
#[test]
fn alt_centres_every_shape_that_grows_from_a_drag() {
    for tool in [
        DrawTool::Rectangle,
        DrawTool::Ellipse,
        DrawTool::Diamond,
        DrawTool::Figure,
        DrawTool::Frame,
    ] {
        let shape = drawn_with(tool, (200.0, 150.0), (260.0, 210.0));
        assert!(
            same_point(element_center(&shape), (200.0, 150.0)),
            "{tool:?} is not centred on the press",
        );
    }
}

/// The press is a world point. Scaled and panned, the same press and the same reach reach
/// the pointer through different screen coordinates, and the element must not know.
#[test]
fn alt_centres_on_the_world_press_through_any_camera() {
    let (press, to) = ((100.0, 60.0), (160.0, 100.0));
    let flat = centred(press, to);
    for camera in [
        Camera {
            x: 0.0,
            y: 0.0,
            scale: 0.25,
        },
        Camera {
            x: -37.5,
            y: 900.0,
            scale: 3.0,
        },
        Camera {
            x: 12.0,
            y: -12.0,
            scale: 7.5,
        },
    ] {
        let shape = drawn_through(camera, press, to);
        assert!(
            (element_center(&shape).x - press.0).abs() < EPS
                && (element_center(&shape).y - press.1).abs() < EPS,
            "{camera:?} is not centred on the press",
        );
        assert_rect_close(
            Rect {
                x: shape.x,
                y: shape.y,
                width: shape.width,
                height: shape.height,
            },
            Rect {
                x: flat.x,
                y: flat.y,
                width: flat.width,
                height: flat.height,
            },
        );
    }
}

/// A click is a click: Alt held or not, a press with no drag leaves nothing behind.
/// Excalidraw drops an `isInvisiblySmallElement` new element on release
/// (`App.tsx@1118751f:11900-11903`, `sizeHelpers.ts@1118751f:77`); `end_draft` drops ours
/// (`pointer_end.rs@1118751f:413-421`).
///
/// **The zero drag is the oracle's answer, not a call of mine.** The oracle decides it the
/// same way for every element: a new element that is still of no size at the release is
/// removed, and "of no size" is `width === 0 && height === 0` exactly — a drag of a tenth of
/// a pixel is *kept*, and only a press that never moved is thrown out. Ours uses a two-unit
/// floor instead, so a hair's breadth of travel is also thrown out. That is one notch wider
/// than the oracle's and is pinned on its own below, so a parity fix has a red to start
/// from.
#[test]
fn alt_without_a_drag_draws_nothing() {
    assert!(draw((100.0, 60.0), &[], true).is_none());
    assert!(draw((100.0, 60.0), &[], false).is_none());
}

/// The wider half of the zero-drag decision, pinned so it is a known quantity rather than an
/// accident: a hair's breadth of travel is dropped on the release, while a drag two units
/// across is kept. `end_draft` (`pointer_end.rs@1118751f:413`) is a two-unit floor; the
/// oracle's `isInvisiblySmallElement` (`sizeHelpers.ts@1118751f:77`) is an exact zero, so a
/// drag of 0.25 units survives there and not here. Alt and no Alt are the same, which is the
/// point: the threshold is the release's, and the centre is not.
#[test]
fn a_drag_too_small_to_keep_is_thrown_away_at_the_release() {
    for alt in [false, true] {
        assert!(
            draw((100.0, 60.0), &[(100.25, 60.0)], alt).is_none(),
            "a quarter-unit drag kept something, alt={alt}",
        );
        assert!(
            draw((100.0, 60.0), &[(102.0, 60.0)], alt).is_some(),
            "a two-unit drag drew nothing, alt={alt}",
        );
    }
}

/// Shift's side is the reach, doubled once, because the oracle squares before it centres
/// (`dragElements.ts@1118751f:325-350` runs before `:371-376`). 60 the short way, 100 the
/// long way: the side is 100, so the box is 200 square, and the 60 is not in it anywhere.
#[test]
fn shift_and_alt_lock_the_side_to_the_longer_reach() {
    let square = shift_and_alt((200.0, 150.0), (300.0, 210.0));
    assert_eq!(box_of(&square), [100.0, 50.0, 200.0, 200.0]);
}

/// The aspect lock is not a fence at some number: whichever reach is longer sets the side
/// and the other one only has to be shorter. 900×600 and its mirror 600×900 are one shape.
#[test]
fn the_longer_reach_decides_the_side_from_either_axis() {
    let press = (400.0, 300.0);
    let wide = shift_and_alt(press, (1300.0, 900.0));
    let tall = shift_and_alt(press, (1000.0, 1200.0));
    assert_eq!(box_of(&wide), [-500.0, -600.0, 1800.0, 1800.0]);
    assert_eq!(box_of(&tall), box_of(&wide));
}

/// The oracle's aspect lock reads the pointer's *current* position, not what the first
/// movement settled on (`dragElements.ts@1118751f:332`: `|y - originY| > |x - originX|`).
///
/// So the same drag walked out sideways first and the same drag walked out downwards first
/// are the same element. A "keeps the proportion the press chose" implementation gives
/// 600×600 for the first (its first movement is 300 wide) and 1200×1200 for the second (its
/// first is 600 tall) — and a test that only ever dragged horizontally would see nothing
/// wrong with either.
#[test]
fn the_walk_to_the_end_does_not_change_the_shape() {
    let press = (400.0, 300.0);
    let to = (1300.0, 900.0);
    let sideways = shift_and_alt_via(press, &[(700.0, 300.0)], to);
    let downwards = shift_and_alt_via(press, &[(400.0, 900.0)], to);
    assert_eq!(box_of(&sideways), [-500.0, -600.0, 1800.0, 1800.0]);
    assert_eq!(box_of(&downwards), box_of(&sideways));
    // And the same for an unsquared drag, where a corner-growing implementation would also
    // agree, so the two halves of this feature are pinned apart. Each walk passes through
    // the press and well outside it, and every one of them ends at the same point.
    let plain = centred(press, to);
    for via in [
        vec![press, to],
        vec![(0.0, 0.0), (1300.0, 0.0), to],
        vec![to, press, to],
    ] {
        let walked = draw(press, &via, true).expect("drawn");
        assert_eq!(box_of(&walked), box_of(&plain), "walk {via:?}");
    }
}

/// Without Alt the press is still a corner, and the reach is still itself. The one thing
/// this port adds is a centre; a press that stopped being a corner would be a different
/// change.
#[test]
fn without_alt_the_press_is_still_the_corner() {
    let shape = draw((100.0, 60.0), &[(160.0, 100.0)], false).expect("drawn");
    assert_eq!(box_of(&shape), [100.0, 60.0, 60.0, 40.0]);
}

/// A drag back over the press and out the far side is one shape, since the box reads the
/// reach and not its direction (`:374-375`). The press stays the middle throughout, not
/// only at the end.
#[test]
fn a_drag_that_crosses_the_press_keeps_it_in_the_middle() {
    let press = (250.0, 200.0);
    let mut engine = engine_at(IDENTITY);
    engine.set_tool(DrawTool::Rectangle);
    engine.set_alt_held(true);
    engine.begin_pointer(press.0, press.1, false, true);
    for to in [(450.0, 320.0), (100.0, 50.0), (350.0, 300.0)] {
        engine.set_alt_held(true);
        engine.move_pointer(to.0, to.1, false, false);
        let live = engine
            .get_scene()
            .into_iter()
            .last()
            .expect("still drafting");
        assert!(
            same_point(element_center(&live), press),
            "after a move to {to:?}",
        );
    }
    engine.end_pointer();
}

/// A sticky note is the one shape that moves again on release, and it moves back: the note
/// is snapped up to its minimum size and re-anchored so that the edge the drag left behind
/// stays put (`App.tsx@1118751f:11857-11869`, which our `end_sticky` is a line-for-line
/// port of). A centred draft puts the note's west edge *before* the press, so that rule
/// hands the release the oracle's answer: the press is the note's **east** edge, not its
/// middle.
///
/// This is the oracle's own behaviour, not a rounding of ours, so it is pinned as it stands
/// rather than corrected.
#[test]
fn a_released_sticky_note_re_anchors_onto_the_press() {
    let press = (300.0, 300.0);
    let note = the_note(press, (400.0, 340.0), false);
    // The press is the note's east edge: the west edge re-anchors onto it on release.
    assert_close(note.x + note.width, press.0);
    assert_close(note.y + note.height, press.1);
}

/// Shift still frees a sticky note from its square — `shouldMaintainAspectRatio` is
/// inverted for a note and for an image (`App.tsx@1118751f:13421-13424`) — and Alt still
/// centres the draft, because the two flags are read independently.
#[test]
fn shift_frees_a_sticky_note_without_moving_the_drafts_centre() {
    let press = (300.0, 300.0);
    let (draft, note) = sticky_gesture(press, (400.0, 340.0), true);
    assert_point_close(element_center(&draft), press_point(press));
    assert_eq!(
        [draft.width, draft.height],
        [200.0, 80.0],
        "Shift freed the note"
    );
    assert!(note.width > note.height, "and it stays freed: {note:?}");
}

/// An arrow is placed by its points rather than by a box, and the oracle never offers
/// `dragNewElement` to one: `App.tsx@1118751f:13409` gates the call on
/// `!isBindingElement`, and `:11209` sends every linear down a different branch. Alt is not
/// a second thing that makes an arrow, and the gesture is unchanged by it.
#[test]
fn alt_leaves_an_arrow_alone() {
    let without = drawn_with(DrawTool::Arrow, (100.0, 100.0), (300.0, 200.0));
    let with = drawn_with_alt(DrawTool::Arrow, (100.0, 100.0), (300.0, 200.0), false);
    assert_eq!(with.points, without.points, "Alt moved an arrow's points");
    assert_eq!(box_of(&with), box_of(&without), "Alt moved an arrow's box");
}

/// A text is a column — it is laid out downwards, so there is no height for the press to be
/// the middle of, and the draft arm is left exactly as it was.
///
/// **A known divergence, pinned on purpose.** Excalidraw does read Alt for a text, but as
/// an *anchor ratio* rather than a centre: `anchorRatio: shouldResizeFromCenter ? 0.5 : …`
/// and `x: anchorX - width * 0.5` (`dragElements.ts@1118751f:358`, `:278`), with `y` left
/// at the origin (`:360`). So the oracle's Alt-text grows to both sides of the press on the
/// horizontal axis and not at all on the vertical one. This port leaves the whole text
/// gesture alone rather than half-porting a different behaviour into it, so the two halves
/// are pinned here and a parity fix starts from a red rather than a shrug — the treatment
/// `docs/reference/resize.md` gives its open rows.
#[test]
fn alt_leaves_a_dragged_out_text_alone() {
    let mut without = engine_at(IDENTITY);
    without.set_tool(DrawTool::Text);
    without.begin_pointer(100.0, 100.0, false, false);
    without.move_pointer(260.0, 240.0, false, false);
    without.end_pointer();
    let plain = without.get_scene().into_iter().last().expect("drafted");
    let mut with = engine_at(IDENTITY);
    with.set_tool(DrawTool::Text);
    with.set_alt_held(true);
    with.begin_pointer(100.0, 100.0, false, true);
    with.set_alt_held(true);
    with.move_pointer(260.0, 240.0, false, false);
    with.end_pointer();
    let held = with.get_scene().into_iter().last().expect("drafted");
    assert_eq!(box_of(&held), box_of(&plain), "Alt changed a text's box");
    assert_eq!(held.x, 100.0, "and the press is still its corner");
}

// --------------------------------------------------------------------- properties

/// The invariant the whole feature rests on: for any press, any drag and any camera, the
/// press is the middle of the box Alt produces.
///
/// A thousand presses over five cameras, with reaches that cross zero, come back inside the
/// press, and are a fraction of a pixel across. A `rect_from_drag` that grew from the corner
/// would put the middle a whole reach away from the press on every one of them, so this
/// cannot pass by accident.
#[test]
fn the_press_is_the_centre_of_every_box_alt_draws() {
    let cameras = [
        Camera {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
        },
        Camera {
            x: -250.0,
            y: 125.0,
            scale: 0.5,
        },
        Camera {
            x: 1500.0,
            y: -900.0,
            scale: 4.0,
        },
        Camera {
            x: -12.5,
            y: 33.25,
            scale: 2.75,
        },
        Camera {
            x: 0.0,
            y: 0.0,
            scale: 12.0,
        },
    ];
    for camera in cameras {
        for (press, to) in reach_cases() {
            let shape = drafted_through(camera, press, to);
            assert!(
                same_point(element_center(&shape), press),
                "press {press:?} to {to:?} at {camera:?}",
            );
            assert!(
                shape.width.is_finite() && shape.height.is_finite(),
                "{shape:?}"
            );
        }
    }
}

/// Under Shift+Alt both axes move together, so the box is square whatever the drag did —
/// `width / height` is 1 for every input. Only a dominant-axis rule guarantees it: a fence
/// at some size, or a square of the *first* movement, both hold for the horizontals and
/// fail here.
#[test]
fn shift_and_alt_always_produce_a_square() {
    for (press, to) in reach_cases() {
        let shape = shift_and_alt_drafted(press, to);
        let (dx, dy) = (to.0 - press.0, to.1 - press.1);
        let side = 2.0 * dx.abs().max(dy.abs());
        assert_eq!(shape.width, shape.height, "press {press:?} by ({dx}, {dy})");
        assert_eq!([shape.width, shape.height], [side, side]);
        assert_close(shape.x, press.0 - side / 2.0);
        assert_close(shape.y, press.1 - side / 2.0);
    }
}

/// The sign of a drag cannot reach the box, so the four ways off a press are one box. Half
/// the inputs here are negative on at least one axis, and the reaches are meant to be the
/// awkward ones — a hair, a mile, exactly zero on the other axis.
#[test]
fn the_result_never_depends_on_which_way_the_drag_went() {
    for (press, to) in reach_cases() {
        let (dx, dy) = (to.0 - press.0, to.1 - press.1);
        let out = drafted(press, to);
        assert_eq!([out.width, out.height], [2.0 * dx.abs(), 2.0 * dy.abs()]);
        for (mx, my) in [(-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
            let mirrored = drafted(press, (press.0 + dx * mx, press.1 + dy * my));
            // Compared to the precision the arithmetic carries, not bit for bit: a sign
            // flip is exact, but `press.0 + dx * -1.0` is not always the same double as
            // `press.0 - dx`, and a property that turns on the last bit of a reach is a
            // property about rounding, not about the centre.
            assert!(
                same_box(box_of(&mirrored), box_of(&out)),
                "press {press:?} by ({dx}, {dy}): {:?} != {:?}",
                box_of(&mirrored),
                box_of(&out),
            );
        }
    }
}

/// A drag is re-read from the press on every move, so how many moves it took is not part of
/// the answer. Each of these presses is walked from a different direction, with the walk
/// deliberately passing through the press and outside it, and every walk lands on the box a
/// single jump would.
#[test]
fn the_result_never_depends_on_how_many_moves_it_took() {
    for (press, to) in reach_cases() {
        let direct = drafted(press, to);
        let walks: [&[(f64, f64)]; 3] = [
            &[press, to],
            &[to, press, to],
            &[(0.0, 0.0), (9000.0, 9000.0), to],
        ];
        for via in walks {
            let mut engine = engine_at(IDENTITY);
            engine.set_tool(DrawTool::Rectangle);
            engine.set_alt_held(true);
            engine.begin_pointer(press.0, press.1, false, true);
            for (x, y) in via {
                engine.set_alt_held(true);
                engine.move_pointer(*x, *y, false, false);
            }
            let walked = engine.get_scene().into_iter().last().expect("drafting");
            assert_eq!(
                box_of(&walked),
                box_of(&direct),
                "press {press:?} to {to:?}"
            );
        }
    }
}

// ----------------------------------------------------------------------- helpers

/// A hundred presses with reaches, from a fixed seed so a failure is a fixed failure: a
/// quarter of them on the far side of the origin, a tenth a fraction of a pixel across, and
/// the lengths spread over four orders of magnitude so that a dominant axis which is really
/// an absolute value still comes out right and one which is a sign does not.
fn reach_cases() -> Vec<((f64, f64), (f64, f64))> {
    let mut seed = 0x5eed_1234_u64;
    let mut next = move || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((seed >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    };
    (0..100)
        .map(|i| {
            let press = ((next() * 4000.0).round(), (next() * 4000.0).round());
            let hair = i % 10 == 0;
            let reach = if hair { 0.25 } else { next() * 400.0 };
            let other = if hair { 0.0 } else { next() * 60.0 };
            (press, (press.0 + reach, press.1 + other))
        })
        .collect()
}

/// A shape of `tool` drawn from `press` to `to` in one move, with Alt on that move.
fn drawn_with(tool: DrawTool, press: (f64, f64), to: (f64, f64)) -> DrawElement {
    let mut engine = engine_at(IDENTITY);
    engine.set_tool(tool);
    engine.set_alt_held(true);
    engine.begin_pointer(press.0, press.1, false, true);
    engine.set_alt_held(true);
    engine.move_pointer(to.0, to.1, false, false);
    engine.end_pointer();
    engine
        .get_scene()
        .into_iter()
        .last()
        .expect("a drag this long leaves an element")
}

/// The note a sticky-note gesture left, once released.
///
/// Not `.last()`: a released note has a bound label, and the label is the newer element.
/// Everything a note says about itself is on the note.
fn the_note(press: (f64, f64), to: (f64, f64), square: bool) -> DrawElement {
    sticky_gesture(press, to, square).1
}

/// A sticky-note gesture, as the draft mid-move and the note after the release.
fn sticky_gesture(press: (f64, f64), to: (f64, f64), square: bool) -> (DrawElement, DrawElement) {
    let mut engine = engine_at(IDENTITY);
    engine.set_tool(DrawTool::StickyNote);
    engine.set_alt_held(true);
    engine.begin_pointer(press.0, press.1, false, true);
    engine.set_alt_held(true);
    engine.move_pointer(to.0, to.1, square, false);
    let draft = the_note_in(&engine);
    engine.end_pointer();
    (draft, the_note_in(&engine))
}

fn the_note_in(engine: &DrawEngine) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|element| element.kind == DrawElementType::StickyNote)
        .expect("a note")
}

/// As [`drawn_with`], for the tools that read a third flag off the move.
fn drawn_with_alt(tool: DrawTool, press: (f64, f64), to: (f64, f64), square: bool) -> DrawElement {
    let mut engine = engine_at(IDENTITY);
    engine.set_tool(tool);
    engine.set_alt_held(true);
    engine.begin_pointer(press.0, press.1, false, true);
    engine.set_alt_held(true);
    engine.move_pointer(to.0, to.1, square, false);
    engine.end_pointer();
    engine
        .get_scene()
        .into_iter()
        .last()
        .expect("a drag this long leaves an element")
}

/// The same world press and world reach as `centred`, reached through a camera.
fn drawn_through(camera: Camera, press: (f64, f64), to: (f64, f64)) -> DrawElement {
    let mut engine = engine_at(camera);
    engine.set_tool(DrawTool::Rectangle);
    let p = world_to_screen(camera, press.0, press.1);
    let t = world_to_screen(camera, to.0, to.1);
    engine.set_alt_held(true);
    engine.begin_pointer(p.x, p.y, false, true);
    engine.set_alt_held(true);
    engine.move_pointer(t.x, t.y, false, false);
    engine.end_pointer();
    engine
        .get_scene()
        .into_iter()
        .last()
        .expect("a drag this long leaves an element")
}

/// The drafted element mid-gesture, which is what the box is a function of.
fn drafted_through(camera: Camera, press: (f64, f64), to: (f64, f64)) -> DrawElement {
    let mut engine = engine_at(camera);
    engine.set_tool(DrawTool::Rectangle);
    let p = world_to_screen(camera, press.0, press.1);
    let t = world_to_screen(camera, to.0, to.1);
    engine.set_alt_held(true);
    engine.begin_pointer(p.x, p.y, false, true);
    engine.set_alt_held(true);
    engine.move_pointer(t.x, t.y, false, false);
    engine.get_scene().into_iter().last().expect("drafting")
}

fn shift_and_alt(press: (f64, f64), to: (f64, f64)) -> DrawElement {
    shift_and_alt_via(press, &[], to)
}

fn shift_and_alt_drafted(press: (f64, f64), to: (f64, f64)) -> DrawElement {
    let mut engine = engine_at(IDENTITY);
    engine.set_tool(DrawTool::Rectangle);
    engine.set_alt_held(true);
    engine.begin_pointer(press.0, press.1, false, true);
    engine.set_alt_held(true);
    engine.move_pointer(to.0, to.1, true, false);
    engine.get_scene().into_iter().last().expect("drafting")
}

fn shift_and_alt_via(press: (f64, f64), via: &[(f64, f64)], to: (f64, f64)) -> DrawElement {
    let mut engine = engine_at(IDENTITY);
    engine.set_tool(DrawTool::Rectangle);
    engine.set_alt_held(true);
    engine.begin_pointer(press.0, press.1, false, true);
    for (x, y) in via {
        engine.set_alt_held(true);
        engine.move_pointer(*x, *y, true, false);
    }
    engine.set_alt_held(true);
    engine.move_pointer(to.0, to.1, true, false);
    engine.end_pointer();
    engine
        .get_scene()
        .into_iter()
        .last()
        .expect("a drag this long leaves an element")
}
