//! The right-button pan session.
//!
//! A right-button press is a pan only once the pointer has travelled past the drag
//! threshold; let go before that and it is a right-click, and the two are told apart by
//! the one 5px rule. What makes that work on every platform is a two-flag state machine:
//! the platform fires `contextmenu` on mouseup (Windows) or on mousedown (macOS, Linux),
//! and the one belonging to a session is never a new click whichever side of the session
//! it lands on.
//!
//! A port of Excalidraw's `AppPan` (`packages/excalidraw/components/App.pan.ts`@1118751f)
//! with the two call sites that reach it: `handleCanvasContextMenu`
//! (`App.tsx@1118751f:13225-13246`) and the `pan.start` on the pointer down
//! (`App.tsx@1118751f:8698`, which returns before the press is a scene gesture at all).

mod common;
use common::*;
use draw_engine::*;

/// Where every session here presses. Far enough from the origin that a camera read is
/// about the pan and not about the world running off a screen edge.
const PRESS: (f64, f64) = (400.0, 300.0);

/// A live secondary-button session: one pointer down, started with no text open.
fn session() -> DrawEngine {
    let mut engine = DrawEngine::new();
    assert_eq!(
        engine.begin_secondary_pan(PRESS.0, PRESS.1, 1),
        SecondaryPanStart::Started,
        "a right press with one pointer down is a session"
    );
    engine
}

fn move_to(engine: &mut DrawEngine, x: f64, y: f64) {
    engine.move_pointer(x, y, false, false);
}

// ---------------------------------------------------------------------------
// The threshold. Pinned from both sides, because `<=` against `<` is invisible
// until a hand lands exactly on the boundary.
// ---------------------------------------------------------------------------

/// 4px is a right-click. The case that matters most and is easiest to get wrong: a right
/// press that never moves must not shift the board by a pixel, or every right-click
/// nudges the canvas a little and the drawing is never where it was left.
#[test]
fn a_press_moved_four_pixels_is_a_right_click_and_moves_nothing() {
    let mut engine = session();
    let before = engine.camera_target();
    move_to(&mut engine, PRESS.0 + 4.0, PRESS.1);
    assert_eq!(
        engine.end_secondary_pan(),
        SecondaryPanEnd::None,
        "no drag, and the platform's own contextmenu is still to come"
    );
    assert_eq!(engine.camera_target(), before, "a right-click pans nothing");
}

/// 6px is a pan. The distance is measured from the press and not from the last move, so
/// a slow drag that creeps the same way still counts from where it started.
#[test]
fn a_drag_past_the_threshold_pans() {
    let mut engine = session();
    let before = engine.camera_target();
    move_to(&mut engine, PRESS.0 + 6.0, PRESS.1);
    move_to(&mut engine, PRESS.0 + 26.0, PRESS.1);
    engine.end_secondary_pan();
    assert_eq!(engine.camera_target().x, before.x + 20.0);
    assert_eq!(engine.camera_target().y, before.y);
}

/// The boundary itself, from the click side: `Math.hypot` of a 3-4-5 triangle is exactly
/// 5, and the oracle's test is `<=`, so exactly 5px is still a right-click.
#[test]
fn exactly_five_pixels_is_still_a_right_click() {
    let mut engine = session();
    let before = engine.camera_target();
    move_to(&mut engine, PRESS.0 + 3.0, PRESS.1 + 4.0);
    assert_eq!(engine.end_secondary_pan(), SecondaryPanEnd::None);
    assert_eq!(engine.camera_target(), before);
}

/// And from the pan side, as close to the boundary as pixels allow: 5.0024px engages.
/// A `<` where the oracle has `<=` would make this a right-click, and the pair above
/// would be the only thing standing between the two behaviours.
#[test]
fn one_hundredth_of_a_pixel_past_five_is_a_pan() {
    let mut engine = session();
    let before = engine.camera_target();
    move_to(&mut engine, PRESS.0 + 3.0, PRESS.1 + 4.05);
    move_to(&mut engine, PRESS.0 + 13.0, PRESS.1 + 4.05);
    engine.end_secondary_pan();
    assert_eq!(
        engine.camera_target().x,
        before.x + 10.0,
        "the engaging move only starts the pan; the 10px after it is what moves the board"
    );
}

/// The distance is straight-line, not per axis. Four pixels across and four down is 4px
/// on each axis and 5.66px from the press: past the threshold, so a pan. Read per axis it
/// would be a right-click, and a diagonal drag — the most ordinary kind there is — would
/// never pan at all.
#[test]
fn the_distance_is_straight_line_not_per_axis() {
    let mut engine = session();
    let before = engine.camera_target();
    for step in 1..=4 {
        move_to(&mut engine, PRESS.0 + step as f64, PRESS.1 + step as f64);
    }
    assert_eq!(
        engine.camera_target(),
        before,
        "the engaging move moved nothing"
    );
    move_to(&mut engine, PRESS.0 + 14.0, PRESS.1 + 14.0);
    engine.end_secondary_pan();
    assert_close(engine.camera_target().x, before.x + 10.0);
    assert_close(engine.camera_target().y, before.y + 10.0);
}

// ---------------------------------------------------------------------------
// The reset at App.pan.ts:146-148 — the one line a port loses, and the canvas jumps
// by up to 5px on the first move when it does.
// ---------------------------------------------------------------------------

/// The move that crosses the threshold starts the pan; it does not replay the distance
/// that got there. The oracle says so in as many words: "pans from here on; the threshold
/// distance is not caught up". Dropping the reset catches it up — here 17px of jump, the
/// 20px move measured from the 3px the pointer had already travelled.
#[test]
fn the_move_that_engages_the_pan_does_not_catch_up_the_threshold_distance() {
    let mut engine = session();
    let before = engine.camera_target();
    move_to(&mut engine, PRESS.0 + 3.0, PRESS.1);
    move_to(&mut engine, PRESS.0 + 20.0, PRESS.1);
    assert_eq!(
        engine.camera_target(),
        before,
        "the engaging move re-bases the pan and moves nothing; a lost reset would have \
         moved the board 17px here"
    );
    move_to(&mut engine, PRESS.0 + 30.0, PRESS.1);
    assert_eq!(
        engine.camera_target().x,
        before.x + 10.0,
        "and the pan runs from the point it engaged at"
    );
}

/// The same on a diagonal, where the two axes disagree and a per-axis reset would show:
/// the press to the engaging move is (9, 12) — 15px of travel — and none of it is caught up.
#[test]
fn a_diagonal_engage_moves_nothing_either() {
    let mut engine = session();
    let before = engine.camera_target();
    move_to(&mut engine, PRESS.0 + 9.0, PRESS.1 + 12.0);
    assert_eq!(engine.camera_target(), before);
    move_to(&mut engine, PRESS.0 + 19.0, PRESS.1 + 22.0);
    assert_eq!(engine.camera_target().x, before.x + 10.0);
    assert_eq!(engine.camera_target().y, before.y + 10.0);
}

/// And with the press itself not at the origin, so the read cannot be a coincidence of
/// a camera that starts at zero.
#[test]
fn the_reset_holds_away_from_the_origin() {
    let mut engine = DrawEngine::new();
    engine.pan_by(1234.0, -567.0);
    let before = engine.camera_target();
    engine.begin_secondary_pan(PRESS.0, PRESS.1, 1);
    move_to(&mut engine, PRESS.0 + 4.0, PRESS.1 + 2.0);
    move_to(&mut engine, PRESS.0 + 40.0, PRESS.1 + 2.0);
    assert_eq!(
        engine.camera_target(),
        before,
        "the engaging move moved nothing"
    );
    move_to(&mut engine, PRESS.0 + 50.0, PRESS.1 + 2.0);
    engine.end_secondary_pan();
    assert_close(engine.camera_target().x, before.x + 10.0);
    assert_close(engine.camera_target().y, before.y);
}

// ---------------------------------------------------------------------------
// The platform difference. This is the crux: on macOS and Linux the browser fires
// `contextmenu` with the *press*, before anything can know whether this is a drag.
// ---------------------------------------------------------------------------

/// macOS and Linux: the event arrives with the press, is swallowed, and the menu opens on
/// the release instead. Without the swallow the board menu opens under the pointer the
/// instant you press, and every right-drag to pan is impossible — the menu is already up.
#[test]
fn a_context_menu_with_the_press_is_swallowed_and_the_menu_opens_on_the_release() {
    let mut engine = session();
    assert!(
        engine.consumes_context_menu(),
        "the event that came with the press is not a new click"
    );
    assert_eq!(
        engine.end_secondary_pan(),
        SecondaryPanEnd::OpenMenu,
        "so the release opens the menu itself, at the release point"
    );
    assert!(
        !engine.consumes_context_menu(),
        "and the session is over, so nothing is swallowed afterwards"
    );
}

/// Windows: the event follows the release. A click is that event opening the menu as it
/// always has, and the host is told nothing — which is the whole of the platform
/// difference, and the reason the oracle can open the menu on only one of the two.
#[test]
fn a_context_menu_with_the_release_opens_for_a_click() {
    let mut engine = session();
    assert_eq!(
        engine.end_secondary_pan(),
        SecondaryPanEnd::None,
        "the platform's own event is still to come and will open the menu"
    );
    assert!(
        !engine.consumes_context_menu(),
        "nothing to swallow: this one is a click, as it always was"
    );
}

/// And for a drag, on the same platform, it is swallowed: a release that turned out to be
/// a drag is not a click, and the menu that follows it must not open.
#[test]
fn a_context_menu_with_the_release_is_swallowed_after_a_drag() {
    let mut engine = session();
    move_to(&mut engine, PRESS.0 + 6.0, PRESS.1);
    move_to(&mut engine, PRESS.0 + 16.0, PRESS.1);
    assert_eq!(engine.end_secondary_pan(), SecondaryPanEnd::Drag);
    assert!(
        engine.consumes_context_menu(),
        "the release was a drag, so the event following it is not a click"
    );
    assert!(
        !engine.consumes_context_menu(),
        "and only that one: the flag is a latch, not a switch"
    );
}

/// A drag on the platform that sends the menu with the press: the event was swallowed at
/// the press, so the release owes nothing and the host is told nothing to do.
#[test]
fn a_drag_after_the_menu_came_with_the_press_owes_the_host_nothing() {
    let mut engine = session();
    assert!(engine.consumes_context_menu());
    move_to(&mut engine, PRESS.0 + 6.0, PRESS.1);
    move_to(&mut engine, PRESS.0 + 16.0, PRESS.1);
    assert_eq!(engine.end_secondary_pan(), SecondaryPanEnd::None);
    assert!(
        !engine.consumes_context_menu(),
        "the press's event was the one swallowed; a second is a menu the user asked for"
    );
}

/// A new press clears what the last session left pending, whatever it becomes
/// (`App.pan.ts:1118751f:90-91`, "a new press supersedes whatever the previous session
/// left pending"). The case that needs it is a press that does *not* become a session —
/// a second finger was already down — arriving after a drag that armed the latch: with
/// the latch left set, the menu that press was meant to open is swallowed, and the
/// right-click does nothing at all.
#[test]
fn a_new_press_clears_the_previous_sessions_pending_menu() {
    let mut engine = session();
    move_to(&mut engine, PRESS.0 + 6.0, PRESS.1);
    move_to(&mut engine, PRESS.0 + 16.0, PRESS.1);
    assert_eq!(engine.end_secondary_pan(), SecondaryPanEnd::Drag);
    assert_eq!(
        engine.begin_secondary_pan(PRESS.0, PRESS.1, 2),
        SecondaryPanStart::Declined
    );
    assert!(
        !engine.consumes_context_menu(),
        "the drag's latch does not reach past the press that followed it"
    );
}

/// And the same press when it *does* start a session: the menu that arrives with it
/// belongs to that session, which swallows it and opens the menu on its own release.
#[test]
fn a_live_session_answers_for_its_own_menu() {
    let mut engine = session();
    move_to(&mut engine, PRESS.0 + 6.0, PRESS.1);
    move_to(&mut engine, PRESS.0 + 16.0, PRESS.1);
    engine.end_secondary_pan();
    engine.begin_secondary_pan(PRESS.0, PRESS.1, 1);
    assert!(engine.consumes_context_menu());
    assert_eq!(engine.end_secondary_pan(), SecondaryPanEnd::OpenMenu);
}

// ---------------------------------------------------------------------------
// What makes a press a session at all.
// ---------------------------------------------------------------------------

/// A second finger down is not a pan (`getPointerCount() <= 1`): two fingers on a trackpad
/// is a pinch or a two-finger scroll, and turning one of them into a pan fights the
/// gesture. Nothing about the declined press may survive into what follows.
#[test]
fn a_second_pointer_down_is_not_a_pan() {
    let mut engine = DrawEngine::new();
    let before = engine.camera_target();
    assert_eq!(
        engine.begin_secondary_pan(PRESS.0, PRESS.1, 2),
        SecondaryPanStart::Declined
    );
    move_to(&mut engine, PRESS.0 + 60.0, PRESS.1);
    assert_eq!(engine.camera_target(), before, "nothing to pan");
    assert!(
        !engine.consumes_context_menu(),
        "no session, so the menu is the platform's own, as it was before any of this"
    );
    assert_eq!(engine.end_secondary_pan(), SecondaryPanEnd::None);
}

/// One pointer down is a session, from both sides of the count: 0 is a press the browser
/// has not counted yet and 1 is the ordinary case, and neither is a two-finger gesture.
#[test]
fn the_pointer_count_is_at_most_one() {
    for pointers in [0, 1] {
        let mut engine = DrawEngine::new();
        assert_eq!(
            engine.begin_secondary_pan(PRESS.0, PRESS.1, pointers),
            SecondaryPanStart::Started
        );
    }
}

/// A press with a text open is still a session, and the host is told to leave the
/// pointerdown's default alone: preventing it while text is being typed breaks the caret
/// and the focus (`App.pan.ts@1118751f:118-125`, issue #4489).
#[test]
fn a_press_while_editing_text_asks_the_host_not_to_prevent_the_default() {
    let mut engine = DrawEngine::new();
    engine.handle_double_click(PRESS.0, PRESS.1);
    assert!(
        engine.is_editing_text(),
        "the double click opened a text to type into"
    );
    assert_eq!(
        engine.begin_secondary_pan(PRESS.0, PRESS.1, 1),
        SecondaryPanStart::StartedWhileEditingText
    );
    // With no text open, the same press does want the default prevented.
    let mut plain = DrawEngine::new();
    assert_eq!(
        plain.begin_secondary_pan(PRESS.0, PRESS.1, 1),
        SecondaryPanStart::Started
    );
}

/// The guard is about `preventDefault` and nothing else: the session runs exactly as it
/// does over bare canvas, so a right-drag pans with a text open and the typing session is
/// left alone. Surprising enough to be worth pinning — the board moves under a text that
/// is still being typed into — but it is what the oracle does, and the alternative is
/// inventing a rule it does not have.
#[test]
fn a_right_drag_while_editing_text_still_pans() {
    let mut engine = DrawEngine::new();
    engine.handle_double_click(PRESS.0, PRESS.1);
    let before = engine.camera_target();
    engine.begin_secondary_pan(PRESS.0, PRESS.1, 1);
    move_to(&mut engine, PRESS.0 + 6.0, PRESS.1);
    move_to(&mut engine, PRESS.0 + 26.0, PRESS.1);
    assert_eq!(engine.end_secondary_pan(), SecondaryPanEnd::Drag);
    assert_eq!(engine.camera_target().x, before.x + 20.0);
    assert!(
        engine.is_editing_text(),
        "panning the board is not a reason to close the text being typed"
    );
}

/// A right-click over a text is a right-click: no drag, so no pan, and the menu opens.
#[test]
fn a_right_click_over_a_text_still_opens_the_menu() {
    let mut engine = DrawEngine::new();
    engine.handle_double_click(PRESS.0, PRESS.1);
    let before = engine.camera_target();
    engine.begin_secondary_pan(PRESS.0, PRESS.1, 1);
    assert!(engine.consumes_context_menu());
    move_to(&mut engine, PRESS.0 + 2.0, PRESS.1);
    assert_eq!(engine.end_secondary_pan(), SecondaryPanEnd::OpenMenu);
    assert_eq!(engine.camera_target(), before);
}

// ---------------------------------------------------------------------------
// The gesture the host has to be able to see, so it can forward moves with no button
// held and so a debugger can tell a right-click from a right-drag.
// ---------------------------------------------------------------------------

/// A right-press is a session the engine wants moves for, with no button in the host's
/// sense — the same reason a path being placed point by point wants them. The host's move
/// gate is a cost gate; if the engine says no here, the moves are dropped and a right-drag
/// never sees its own threshold.
#[test]
fn a_live_session_wants_pointer_moves_with_no_button_held() {
    let mut engine = DrawEngine::new();
    assert!(!engine.wants_pointer_moves());
    engine.begin_secondary_pan(PRESS.0, PRESS.1, 1);
    assert!(engine.wants_pointer_moves());
    engine.end_secondary_pan();
    assert!(!engine.wants_pointer_moves());
    // A path being placed wants them for the same reason, and this must not have cost
    // it that: the two are one question to the host.
    let mut placing = DrawEngine::new();
    placing.set_tool(DrawTool::Line);
    placing.begin_pointer(PRESS.0, PRESS.1, false, false);
    placing.end_pointer();
    assert!(
        placing.wants_pointer_moves(),
        "a path placed point by point still follows the cursor between its clicks"
    );
}

/// The debug snapshot names the gesture, and the un-engaged session is not a pan yet: a
/// right-click must not read as "pan" to anything deciding whether the board is moving.
#[test]
fn an_unengaged_session_is_not_reported_as_a_pan() {
    let mut engine = session();
    move_to(&mut engine, PRESS.0 + 2.0, PRESS.1);
    assert_ne!(engine.debug_state().interaction.kind, Some("pan"));
    move_to(&mut engine, PRESS.0 + 12.0, PRESS.1);
    assert_eq!(
        engine.debug_state().interaction.kind,
        Some("pan"),
        "and once it engages it is a pan, like any other"
    );
}

// ---------------------------------------------------------------------------
// Properties of a session, stated once and generated rather than typed: the crate has no
// property-testing dependency and adding one is not on the table, so the cases come out
// of a seeded generator and a failure is reproducible from the seed.
// ---------------------------------------------------------------------------

/// xorshift64*, so a case is reproducible from its seed and nothing else.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// A whole number in `0..n`, so every generated case lands on exact pixels and the
    /// arithmetic under test is the real one rather than a float comparison's.
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A drag: a press and the points the pointer visited. Steps jitter around the threshold,
/// reverse direction, and land exactly on the 3-4-5 boundary often enough that the
/// boundary is not a special case the generator happens to miss.
fn drag(rng: &mut Rng) -> Vec<(f64, f64)> {
    let mut at = (PRESS.0, PRESS.1);
    let mut path = Vec::new();
    let moves = 1 + rng.below(12);
    for _ in 0..moves {
        let roll = rng.below(10);
        if roll == 0 {
            // Exactly 5px away, in one of the four directions: the boundary itself.
            let (dx, dy) = match rng.below(4) {
                0 => (3.0, 4.0),
                1 => (-3.0, 4.0),
                2 => (3.0, -4.0),
                _ => (-3.0, -4.0),
            };
            at = (PRESS.0 + dx, PRESS.1 + dy);
        } else if roll == 1 {
            // A step of a pixel: the jitter of a hand that is deciding.
            let dx = if rng.below(2) == 0 { 1.0 } else { -1.0 };
            let dy = if rng.below(2) == 0 { 1.0 } else { -1.0 };
            at = (at.0 + dx, at.1 + dy);
        } else {
            let span = 9.0;
            at = (
                at.0 + rng.below(span as u64) as f64,
                at.1 + rng.below(span as u64) as f64,
            );
        }
        path.push(at);
    }
    path
}

/// Whether the oracle's rule says this path engaged, and where.
fn engaged_at(path: &[(f64, f64)]) -> Option<usize> {
    path.iter()
        .position(|at| (at.0 - PRESS.0).hypot(at.1 - PRESS.1) > SECONDARY_BUTTON_PAN_THRESHOLD)
}

/// **The camera moved exactly what the session consumed.** The consumed deltas are the
/// moves after the one that engaged, and they sum to `last - engaging` whatever path the
/// pointer took — forwards, backwards, or back to where it started. A session that never
/// engaged consumed nothing at all.
#[test]
fn the_camera_moved_exactly_what_the_session_consumed() {
    for seed in 1..=500u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let path = drag(&mut rng);
        let mut engine = DrawEngine::new();
        let before = engine.camera_target();
        engine.begin_secondary_pan(PRESS.0, PRESS.1, 1);
        for (x, y) in &path {
            move_to(&mut engine, *x, *y);
        }
        let expected = match engaged_at(&path) {
            None => (0.0, 0.0),
            Some(index) => {
                let (x, y) = path[path.len() - 1];
                (x - path[index].0, y - path[index].1)
            }
        };
        let after = engine.camera_target();
        assert_close(after.x - before.x, expected.0);
        assert_close(after.y - before.y, expected.1);
    }
}

/// **A session opens exactly one menu, or none.** One for a right-click whichever side of
/// the session the platform fires its `contextmenu` on; none for a drag. Driven through
/// both platform orderings, because the two differ in exactly one place and both have to
/// come out the same.
#[test]
fn a_session_opens_one_menu_for_a_click_and_none_for_a_drag() {
    /// The platforms, as the host sees them: `OnPress` is macOS and Linux, `OnRelease` is
    /// Windows. Each returns how many menus the host would have opened.
    fn menus_opened(platform_menu_lands_with_press: bool, path: &[(f64, f64)]) -> usize {
        let mut engine = DrawEngine::new();
        engine.begin_secondary_pan(PRESS.0, PRESS.1, 1);
        let mut opened = 0;
        // The event with the press: the host's handler returns here and opens nothing
        // (`App.tsx@1118751f:13234-13236`).
        if platform_menu_lands_with_press {
            engine.consumes_context_menu();
        }
        for (x, y) in path {
            move_to(&mut engine, *x, *y);
        }
        if engine.end_secondary_pan() == SecondaryPanEnd::OpenMenu {
            opened += 1;
        }
        if !platform_menu_lands_with_press && !engine.consumes_context_menu() {
            opened += 1;
        }
        opened
    }

    for seed in 1..=500u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let path = drag(&mut rng);
        let expected = if engaged_at(&path).is_some() { 0 } else { 1 };
        assert_eq!(
            menus_opened(true, &path),
            expected,
            "menu with the press, seed {seed}"
        );
        assert_eq!(
            menus_opened(false, &path),
            expected,
            "menu with the release, seed {seed}"
        );
    }
}

/// **Engagement is decided once.** The distance is read from the press and from nothing
/// else, so a drag that wanders back inside the threshold stays a pan — the oracle
/// latches on the first crossing and never un-latches — and one that crosses and comes
/// back has still consumed only the moves after the crossing.
#[test]
fn engagement_latches_and_is_never_unwound() {
    for seed in 1..=300u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let path = drag(&mut rng);
        let Some(first) = engaged_at(&path) else {
            continue;
        };
        let mut engine = DrawEngine::new();
        let before = engine.camera_target();
        engine.begin_secondary_pan(PRESS.0, PRESS.1, 1);
        for (x, y) in &path {
            move_to(&mut engine, *x, *y);
        }
        let after = engine.camera_target();
        // A path that crosses and then comes home has moved by the sum of the moves
        // after the crossing, which the telescoping sum gives whatever it does since —
        // and the last move back near the press does not undo the pan.
        let (last_x, last_y) = *path.last().expect("a drag has at least one move");
        assert_close(after.x - before.x, last_x - path[first].0);
        assert_close(after.y - before.y, last_y - path[first].1);
    }
}

/// **A session that never engaged consumed nothing and panned nothing** — including when
/// it is released over a completely different part of the board from where it pressed.
#[test]
fn a_session_that_never_engaged_consumed_nothing() {
    for seed in 1..=300u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let path = drag(&mut rng);
        if engaged_at(&path).is_some() {
            continue;
        }
        let mut engine = DrawEngine::new();
        let before = engine.camera_target();
        engine.begin_secondary_pan(PRESS.0, PRESS.1, 1);
        for (x, y) in &path {
            move_to(&mut engine, *x, *y);
        }
        let (last_x, last_y) = *path.last().expect("a drag has at least one move");
        assert_eq!(engine.end_secondary_pan(), SecondaryPanEnd::None);
        assert_eq!(
            engine.camera_target(),
            before,
            "seed {seed} for {last_x},{last_y}"
        );
    }
}
