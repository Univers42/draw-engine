//! Tab and Shift+Tab switch the selected shapes between rectangle, diamond and ellipse —
//! the generic branch of Excalidraw's shape switch
//! (`ConvertElementTypePopup.tsx@1118751f:418-500`). The label sizes and outline points
//! below are worked out by hand from `engine_with_measure`'s metrics: a character is half
//! the font size wide plus four, and a line is 1.25 font sizes high.

mod common;
use common::*;
use draw_engine::scene::element::BindMode;
use draw_engine::*;

fn with_id(mut element: DrawElement, id: &str) -> DrawElement {
    element.id = id.into();
    element
}

fn get(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|element| element.id == id)
        .expect("element exists")
}

fn selected(elements: Vec<DrawElement>, ids: &[&str]) -> DrawEngine {
    let mut engine = engine_with_measure(elements);
    engine.select(ids.iter().map(|id| id.to_string()).collect());
    engine
}

/// A 160x60 rectangle "s" holding the label "hello world" at size 20.
fn labelled_rectangle() -> Vec<DrawElement> {
    let mut shape = with_id(box_at(0.0, 0.0, 160.0, 60.0), "s");
    shape.bound_text_id = Some("label".into());
    let mut label = with_id(text_at(23.0, 17.5, 114.0, 25.0), "label");
    label.text = Some("hello world".into());
    label.original_text = Some("hello world".into());
    label.font_size = Some(20.0);
    label.container_id = Some("s".into());
    vec![shape, label]
}

/// A 200x100 rectangle "b" at the origin, and an arrow whose end is bound to it at
/// `fixed_point`.
fn arrow_into_b(fixed_point: [f64; 2]) -> Vec<DrawElement> {
    let b = with_id(filled(box_at(0.0, 0.0, 200.0, 100.0)), "b");
    let mut arrow = with_id(
        connector(400.0, -200.0, 200.0, 0.0, DrawElementType::Arrow),
        "arrow",
    );
    arrow.end_binding = Some("b".into());
    arrow.end_fixed_point = Some(fixed_point);
    arrow.end_bind_mode = Some(BindMode::Orbit);
    vec![b, arrow]
}

#[test]
fn tab_walks_rectangle_diamond_ellipse_and_round() {
    let mut engine = selected(vec![with_id(box_at(0.0, 0.0, 100.0, 60.0), "s")], &["s"]);
    let mut kinds = Vec::new();
    for _ in 0..3 {
        assert!(engine.convert_selection(None, true));
        kinds.push(get(&engine, "s").kind);
    }
    assert_eq!(
        kinds,
        [
            DrawElementType::Diamond,
            DrawElementType::Ellipse,
            DrawElementType::Rectangle
        ]
    );
    assert!(engine.convert_selection(None, false));
    assert_eq!(get(&engine, "s").kind, DrawElementType::Ellipse);
}

#[test]
fn shapes_of_different_kinds_all_become_the_first_or_second_type() {
    let elements = vec![
        with_id(box_at(0.0, 0.0, 100.0, 60.0), "r"),
        with_id(ellipse_at(200.0, 0.0, 100.0, 60.0), "e"),
    ];
    let mut engine = selected(elements.clone(), &["r", "e"]);
    engine.convert_selection(None, true);
    assert_eq!(get(&engine, "r").kind, DrawElementType::Rectangle);
    assert_eq!(get(&engine, "e").kind, DrawElementType::Rectangle);

    let mut engine = selected(elements, &["r", "e"]);
    engine.convert_selection(None, false);
    assert_eq!(get(&engine, "r").kind, DrawElementType::Diamond);
    assert_eq!(get(&engine, "e").kind, DrawElementType::Diamond);
}

#[test]
fn a_switch_keeps_place_size_and_style_and_drops_an_explicit_radius() {
    let mut shape = with_id(box_at(10.0, 20.0, 100.0, 60.0), "s");
    shape.stroke_color = "#e03131".into();
    shape.background_color = "#a5d8ff".into();
    shape.fill_style = FillStyle::Solid;
    shape.stroke_width = 4.0;
    shape.roughness = 2.0;
    shape.opacity = 50.0;
    shape.roundness = Some(8.0);
    shape.corner_radius = Some(12.0);
    let mut engine = selected(vec![shape.clone()], &["s"]);

    assert!(engine.convert_selection(Some(ConvertTo::Generic(DrawElementType::Diamond)), true));

    let after = get(&engine, "s");
    let mut expected = shape;
    expected.kind = DrawElementType::Diamond;
    expected.corner_radius = None;
    // The commit stamps it, as every edit is stamped.
    expected.version = after.version;
    expected.version_nonce = after.version_nonce;
    expected.updated = after.updated;
    assert_eq!(after, expected);
    assert!(engine.get_selection().contains(&"s".to_string()));
    assert_eq!(engine.get_tool(), DrawTool::Select);
}

#[test]
fn only_rectangles_diamonds_and_ellipses_switch() {
    let mut text = with_id(text_at(300.0, 0.0, 60.0, 25.0), "t");
    text.text = Some("hi".into());
    let elements = vec![with_id(box_at(0.0, 0.0, 100.0, 60.0), "s"), text];

    let mut engine = selected(elements.clone(), &["t"]);
    assert!(!engine.can_convert_selection());
    assert!(!engine.convert_selection(None, true));

    let mut engine = selected(elements.clone(), &["s", "t"]);
    assert!(engine.can_convert_selection());
    assert!(engine.convert_selection(None, true));
    assert_eq!(get(&engine, "s").kind, DrawElementType::Diamond);
    assert_eq!(get(&engine, "t"), elements[1]);
}

#[test]
fn a_switch_is_one_step_of_undo() {
    let mut engine = selected(vec![with_id(box_at(0.0, 0.0, 100.0, 60.0), "s")], &["s"]);
    engine.convert_selection(None, true);
    engine.undo();
    assert_eq!(get(&engine, "s").kind, DrawElementType::Rectangle);
}

/// Into the diamond, "hello world" may be 70 wide and 20 high: at 12 it is 70 wide.
/// Into the ellipse, 103 wide and 32 high: at 18 it is 103. The rectangle holds it at 20.
#[test]
fn a_label_shrinks_to_fit_and_comes_back_while_the_switch_is_open() {
    let mut engine = selected(labelled_rectangle(), &["s"]);
    engine.begin_conversion();
    let mut sizes = Vec::new();
    for _ in 0..3 {
        engine.convert_selection(None, true);
        sizes.push(get(&engine, "label").font_size);
    }
    assert_eq!(sizes, [Some(12.0), Some(18.0), Some(20.0)]);
    // Shrunk to fit, the shape did not have to grow.
    assert_close(get(&engine, "s").height, 60.0);
}

#[test]
fn with_the_switch_closed_a_shrunk_label_stays_shrunk() {
    let mut engine = selected(labelled_rectangle(), &["s"]);
    engine.begin_conversion();
    engine.convert_selection(None, true);
    engine.end_conversion();
    engine.convert_selection(Some(ConvertTo::Generic(DrawElementType::Rectangle)), true);
    assert_eq!(get(&engine, "label").font_size, Some(12.0));
}

/// A corner of the rectangle is well outside the ellipse, so the end moves to where the
/// ray from the centre (100, 50) toward it leaves the ellipse: (100, 50) + (100, -50)/√2.
#[test]
fn an_end_bound_at_a_corner_moves_onto_the_ellipse() {
    let mut engine = selected(arrow_into_b([1.0, 0.0]), &["b"]);
    engine.convert_selection(Some(ConvertTo::Generic(DrawElementType::Ellipse)), true);
    let [fx, fy] = get(&engine, "arrow").end_fixed_point.expect("still bound");
    let half = std::f64::consts::FRAC_1_SQRT_2 / 2.0;
    assert_close(fx, 0.5 + half);
    assert_close(fy, 0.5 - half);
    assert_eq!(get(&engine, "arrow").end_binding.as_deref(), Some("b"));
}

/// The top middle of the rectangle is on the ellipse too: that end stays where it was.
#[test]
fn an_end_still_on_the_new_outline_stays() {
    let mut engine = selected(arrow_into_b([0.5, 0.0]), &["b"]);
    engine.convert_selection(Some(ConvertTo::Generic(DrawElementType::Ellipse)), true);
    assert_eq!(get(&engine, "arrow").end_fixed_point, Some([0.5, 0.0]));
}

// ── The linear branch ─────────────────────────────────────────────────────────
//
// `LINEAR_TYPES` and the linear half of `convertElementTypes`
// (`ConvertElementTypePopup.tsx@1118751f:113-120`, `:519-534`, `:833-927`). The closed
// shapes above pin the generic branch; these pin the other one.
//
// Every case asserts what *survived* as well as what changed. A test that asserted only
// the type would pass against a switch that dropped the id, the points, the group and the
// bindings — and the type is the one thing every implementation gets right.

/// A two-point line "l" from (0, 0) to (200, 120), as the Line tool leaves it.
fn plain_line() -> DrawElement {
    with_id(
        connector(0.0, 0.0, 200.0, 120.0, DrawElementType::Line),
        "l",
    )
}

/// A line with the fields a switch must not touch, and nothing a switch may read.
fn marked_line(id: &str, points: Vec<[f64; 2]>) -> DrawElement {
    let (dx, dy) = (points[points.len() - 1][0], points[points.len() - 1][1]);
    let mut element = with_id(connector(0.0, 0.0, dx, dy, DrawElementType::Line), id);
    element.points = Some(points);
    element.stroke_color = "#e03131".into();
    element.stroke_width = 4.0;
    element.roughness = 2.0;
    element.opacity = 50.0;
    element.group_ids = vec!["g".into()];
    element
}

/// The linear sub-type, as the oracle reads it off two fields
/// (`packages/element/src/typeChecks.ts@1118751f:375-389`): ours spells "curved" "round".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Linear {
    Line,
    Sharp,
    Curved,
    Elbow,
}

fn linear_of(element: &DrawElement) -> Linear {
    assert!(
        matches!(element.kind, DrawElementType::Line | DrawElementType::Arrow),
        "not a linear element: {:?}",
        element.kind
    );
    if element.kind == DrawElementType::Line {
        return Linear::Line;
    }
    if crate::scene::elbow::is_elbow(element) {
        Linear::Elbow
    } else if element.roundness.is_some() {
        Linear::Curved
    } else {
        Linear::Sharp
    }
}

#[test]
fn tab_walks_line_sharp_curved_elbow_and_back() {
    let mut engine = selected(vec![plain_line()], &["l"]);
    let mut seen = Vec::new();
    for _ in 0..4 {
        assert!(engine.convert_selection(None, true));
        seen.push(linear_of(&get(&engine, "l")));
    }
    assert_eq!(
        seen,
        [Linear::Sharp, Linear::Curved, Linear::Elbow, Linear::Line],
        "LINEAR_TYPES, in the order the oracle walks them"
    );
    engine.convert_selection(None, false);
    assert_eq!(linear_of(&get(&engine, "l")), Linear::Elbow);
}

#[test]
fn a_switched_line_keeps_its_id_points_bounds_group_and_style() {
    let before = marked_line("l", vec![[0.0, 0.0], [200.0, 120.0]]);
    let mut engine = selected(vec![before.clone()], &["l"]);

    assert!(engine.convert_selection(None, true));

    let after = get(&engine, "l");
    assert_eq!(
        after.id, before.id,
        "the id survives, so bindings by id still find it"
    );
    assert_eq!(
        after.points, before.points,
        "its points are local and unchanged"
    );
    assert_eq!((after.x, after.y), (before.x, before.y), "its place");
    assert_eq!(
        (after.width, after.height),
        (before.width, before.height),
        "its bounds"
    );
    assert_eq!(after.group_ids, before.group_ids, "its group");
    assert_eq!(after.stroke_color, "#e03131", "its style");
    assert_eq!(after.stroke_width, 4.0);
    assert_eq!(after.roughness, 2.0);
    assert_eq!(after.opacity, 50.0);
    assert_eq!(
        after.version,
        before.version + 1,
        "stamped once, as every edit is"
    );
}

#[test]
fn a_switched_line_keeps_the_selection_and_lands_on_the_select_tool() {
    let mut engine = selected(vec![plain_line()], &["l"]);
    assert!(engine.convert_selection(None, true));
    assert!(engine.get_selection().contains(&"l".to_string()));
    assert_eq!(engine.get_tool(), DrawTool::Select);
}

#[test]
fn only_lines_and_unbound_arrows_switch() {
    let mut text = with_id(text_at(300.0, 0.0, 60.0, 25.0), "t");
    text.text = Some("hi".into());
    let bound = {
        let b = with_id(filled(box_at(0.0, 0.0, 200.0, 100.0)), "b");
        let mut arrow = with_id(
            connector(400.0, -200.0, 200.0, 0.0, DrawElementType::Arrow),
            "arrow",
        );
        arrow.end_binding = Some("b".into());
        vec![b, arrow]
    };

    let mut engine = selected(vec![text], &["t"]);
    assert!(
        !engine.can_convert_selection(),
        "text has nothing to switch"
    );
    assert!(!engine.convert_selection(None, true));

    // A bound arrow is not switchable at all: the oracle's `isEligibleLinearElement`
    // (`ConvertElementTypePopup.tsx@1118751f:666-672`) refuses it, so Tab never opens.
    let mut engine = selected(bound, &["arrow"]);
    assert!(
        !engine.can_convert_selection(),
        "a bound arrow is not switchable"
    );
    assert!(!engine.convert_selection(None, true));
    assert_eq!(get(&engine, "arrow").kind, DrawElementType::Arrow);
    assert_eq!(
        get(&engine, "arrow").end_binding.as_deref(),
        Some("b"),
        "and its binding is untouched"
    );
}

#[test]
fn an_unbound_arrow_switches_to_a_line() {
    let mut arrow = with_id(connector(0.0, 0.0, 200.0, 0.0, DrawElementType::Arrow), "a");
    arrow.roundness = None;
    let points = arrow.points.clone();
    let mut engine = selected(vec![arrow], &["a"]);
    assert!(engine.can_convert_selection());
    assert!(engine.convert_selection(Some(ConvertTo::Linear(LinearType::Line)), true));
    assert_eq!(get(&engine, "a").kind, DrawElementType::Line);
    assert_eq!(
        get(&engine, "a").points,
        points,
        "its points survive the way back"
    );
}

#[test]
fn a_switch_is_one_step_of_undo_not_two_and_not_zero() {
    let mut engine = selected(vec![plain_line()], &["l"]);
    assert!(engine.convert_selection(None, true));
    assert_eq!(get(&engine, "l").kind, DrawElementType::Arrow);

    engine.undo();
    let after = get(&engine, "l");
    assert_eq!(
        after.kind,
        DrawElementType::Line,
        "one undo is the whole switch"
    );
    assert_eq!(after.points, Some(vec![[0.0, 0.0], [200.0, 120.0]]));

    // A switch that forked history would leave a second entry here, and its undo would
    // take something else off the board. An empty stack leaves the line alone.
    engine.undo();
    assert_eq!(
        get(&engine, "l").kind,
        DrawElementType::Line,
        "there was no second step to undo"
    );
    assert_eq!(
        get(&engine, "l").points,
        Some(vec![[0.0, 0.0], [200.0, 120.0]])
    );
}

#[test]
fn a_line_and_an_arrow_of_different_kinds_both_become_the_first_or_second_type() {
    let line = marked_line("l", vec![[0.0, 0.0], [100.0, 0.0]]);
    let mut sharp = with_id(
        connector(200.0, 0.0, 300.0, 0.0, DrawElementType::Arrow),
        "a",
    );
    sharp.roundness = None;

    // A selection that disagrees has no shared type, so the index is -1 and the step is
    // `(−1 + 4 ± 1) % 4`: forward lands on the first of the four, back on the third.
    let mut engine = selected(vec![line.clone(), sharp.clone()], &["l", "a"]);
    engine.convert_selection(None, true);
    assert_eq!(linear_of(&get(&engine, "l")), Linear::Line);
    assert_eq!(linear_of(&get(&engine, "a")), Linear::Line);

    let mut engine = selected(vec![line, sharp], &["l", "a"]);
    engine.convert_selection(None, false);
    assert_eq!(linear_of(&get(&engine, "l")), Linear::Curved);
    assert_eq!(linear_of(&get(&engine, "a")), Linear::Curved);
}

#[test]
fn converting_away_from_an_arrow_drops_its_heads() {
    // The oracle's `newLinearElement` writes both heads to null, whatever the spread
    // carried (`packages/element/src/newElement.ts@1118751f:583-586`).
    let mut arrow = with_id(connector(0.0, 0.0, 200.0, 0.0, DrawElementType::Arrow), "a");
    arrow.roundness = None;
    arrow.start_arrowhead = Some(Arrowhead::Circle);
    arrow.end_arrowhead = Some(Arrowhead::Triangle);
    let mut engine = selected(vec![arrow], &["a"]);

    assert!(engine.convert_selection(Some(ConvertTo::Linear(LinearType::Line)), true));
    let after = get(&engine, "a");
    assert_eq!(after.kind, DrawElementType::Line);
    assert_eq!(after.start_arrowhead, None, "the tail's head goes");
    assert_eq!(after.end_arrowhead, None, "and the head's head goes");
}

#[test]
fn a_shaped_arrow_keeps_its_own_heads_and_an_elbow_forces_the_arrow_head() {
    // To `sharpArrow` the oracle takes the heads from the toolbar, not the element
    // (`ConvertElementTypePopup.tsx@1118751f:891-892`); to `elbowArrow` it keeps the
    // element's own and then forces `endArrowhead: "arrow"` (`:910-919`, `:591-594`).
    let mut arrow = with_id(connector(0.0, 0.0, 200.0, 0.0, DrawElementType::Arrow), "a");
    arrow.roundness = None;
    arrow.start_arrowhead = Some(Arrowhead::Circle);
    arrow.end_arrowhead = Some(Arrowhead::Triangle);
    let points = arrow.points.clone();
    let mut engine = selected(vec![arrow], &["a"]);

    engine.convert_selection(Some(ConvertTo::Linear(LinearType::Line)), true);
    let line = get(&engine, "a");
    assert_eq!(line.start_arrowhead, None);
    assert_eq!(line.end_arrowhead, None);

    engine.convert_selection(Some(ConvertTo::Linear(LinearType::SharpArrow)), true);
    let sharp = get(&engine, "a");
    assert_eq!(
        sharp.end_arrowhead,
        Some(Arrowhead::Arrow),
        "the toolbar's head"
    );
    assert_eq!(
        sharp.start_arrowhead,
        Some(Arrowhead::None),
        "the toolbar's tail, unchosen — an explicit no head, not an absent field"
    );
    assert_eq!(sharp.points, points, "and its points are still its own");

    engine.convert_selection(None, true); // curved
    engine.convert_selection(None, true); // elbow
    let elbow = get(&engine, "a");
    assert!(crate::scene::elbow::is_elbow(&elbow));
    assert_eq!(elbow.end_arrowhead, Some(Arrowhead::Arrow));
}

#[test]
fn an_elbow_is_a_reroute_between_the_same_two_ends() {
    // The one conversion that changes the points: `convertLineToElbow` builds an
    // orthogonal route between the same ends (`ConvertElementTypePopup.tsx@1118751f:567-594`).
    let line = marked_line("l", vec![[0.0, 0.0], [200.0, 120.0]]);
    let mut engine = selected(vec![line], &["l"]);
    engine.begin_conversion();

    engine.convert_selection(None, true);
    engine.convert_selection(None, true);
    engine.convert_selection(None, true);
    let elbow = get(&engine, "l");
    assert!(crate::scene::elbow::is_elbow(&elbow), "it is an elbow");

    let world = draw_engine::selection::linear::world_points(&elbow);
    assert_close(world[0].x, 0.0);
    assert_close(world[0].y, 0.0);

    let last = world[world.len() - 1];
    // The head stays on the end it was drawn to.
    assert_close(last.x, 200.0);
    assert_close(last.y, 120.0);
    assert!(
        world.len() > 2,
        "an orthogonal route between two points that share no axis has a corner"
    );
    for pair in world.windows(2) {
        assert!(
            (pair[0].x - pair[1].x).abs() < 1e-6 || (pair[0].y - pair[1].y).abs() < 1e-6,
            "every run of an elbow is horizontal or vertical: {pair:?}"
        );
    }
}

// ── The round trip ────────────────────────────────────────────────────────────
//
// A table of forward cases cannot catch a lossy half: every row says what Tab *does*,
// and none of them says what the element looks like after coming back. The property is
// that a line switched to an arrow and switched back is the line it started as —
// the same id, the same points, the same group, the same bindings.
//
// The oracle gets this from a cache, not from a formula
// (`ConvertElementTypePopup.tsx@1118751f:157-161`, `:543-561`, `:600-613`): the panel
// remembers each linear element under the sub-type it had, and returning to that sub-type
// hands the remembered element back whole. A lossy half here is a straight line that
// comes back bent, or a group that comes back gone, and every forward case above would
// still be green.

/// A fixed-seed LCG. A fuzz corpus needs no dependency and no wall clock — the same corpus
/// on every machine, on every run, forever. The shape of `Corpus` in
/// `ci_cardinality_props.rs`.
struct Corpus(u64);

impl Corpus {
    fn next(&mut self, low: f64, high: f64) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        let unit = ((self.0 >> 11) as f64) / ((1u64 << 53) as f64);
        low + unit * (high - low)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next(0.0, bound as f64)) as usize % bound
    }
}

/// One generated linear element: `count` points, possibly turned, possibly in a group,
/// sometimes with heads, of the given kind. `count` is 2, 3 or many.
///
/// The kind is a parameter and not a coin toss because it is the thing under test: Tab's
/// arithmetic over `LINEAR_TYPES` depends on where it starts, and a corpus that moved the
/// start would fail on the arithmetic rather than on what the switch does to the element.
fn generated_line(rng: &mut Corpus, id: &str, count: usize, kind: DrawElementType) -> DrawElement {
    let mut points = vec![[rng.next(-300.0, 300.0), rng.next(-300.0, 300.0)]];
    for _ in 1..count {
        // Away from the last point, so no two are equal and the route is well defined.
        let [lx, ly] = points[points.len() - 1];
        points.push([lx + rng.next(-250.0, 250.0), ly + rng.next(-250.0, 250.0)]);
    }
    let (last, first) = (points[points.len() - 1], points[0]);
    let mut element = with_id(
        connector(0.0, 0.0, last[0] - first[0], last[1] - first[1], kind),
        id,
    );
    element.x = first[0];
    element.y = first[1];
    element.points = Some(points);
    if kind == DrawElementType::Arrow {
        // An unbound arrow, or the switch refuses it (`:666-672`).
        element.roundness = if rng.below(2) == 0 { None } else { Some(8.0) };
        if rng.below(3) == 0 {
            element.start_arrowhead = Some(Arrowhead::Circle);
        }
        if rng.below(3) == 0 {
            element.end_arrowhead = Some(Arrowhead::Triangle);
        }
    }
    if rng.below(3) == 0 {
        element.group_ids = vec!["g".into()];
    }
    element.angle = if rng.below(4) == 0 {
        rng.next(-1.0, 1.0)
    } else {
        0.0
    };
    element
}

/// What a round trip has to bring back: the identity, the geometry and the membership.
/// Deliberately not the version, which the switch stamps and the undo does not put back.
fn same_element(after: &DrawElement, before: &DrawElement) -> bool {
    after.id == before.id
        && after.kind == before.kind
        && after.points == before.points
        && after.x == before.x
        && after.y == before.y
        && after.width == before.width
        && after.height == before.height
        && after.angle == before.angle
        && after.group_ids == before.group_ids
        && after.start_binding == before.start_binding
        && after.end_binding == before.end_binding
    // Deliberately not the heads: the oracle takes them from the toolbar going to an arrow
    // (`ConvertElementTypePopup.tsx@1118751f:891-892`, `:905-906`) and writes both to null
    // coming back to a line (`packages/element/src/newElement.ts@1118751f:583-586`), so an
    // arrow's heads do not survive the round trip and a property that demanded they would
    // demand something the oracle does not do. `an_arrows_heads_do_not_survive_the_round_trip`
    // says so where it can fail.
}

#[test]
fn a_line_switched_to_an_arrow_and_back_is_the_line_it_was() {
    let mut rng = Corpus(0x5EED);
    for case in 0..240 {
        let count = match case % 3 {
            0 => 2,
            1 => 3,
            _ => 5 + case % 7,
        };
        let before = generated_line(&mut rng, "l", count, DrawElementType::Line);
        let mut engine = selected(vec![before.clone()], &["l"]);
        engine.begin_conversion();

        // Away and back by name, which is what the panel does. Two Tab presses would not
        // be the round trip: `LINEAR_TYPES[(0 + 4 + 1) % 4]` is `sharpArrow` and from there
        // `curvedArrow` — a line is four steps round, not two.
        assert!(
            engine.convert_selection(Some(ConvertTo::Linear(LinearType::SharpArrow)), true),
            "case {case}: a line has somewhere to go"
        );
        let away = get(&engine, "l");
        assert_eq!(away.id, "l", "case {case}: the id survives the way out");
        assert_eq!(
            away.points, before.points,
            "case {case}: its points survive too"
        );

        assert!(
            engine.convert_selection(Some(ConvertTo::Linear(LinearType::Line)), true),
            "case {case}: and somewhere to come back to"
        );
        let back = get(&engine, "l");
        assert!(
            same_element(&back, &before),
            "case {case}: the round trip lost something\n  before {before:?}\n  after  {back:?}"
        );
    }
}

#[test]
fn a_line_survives_the_whole_walk_and_comes_back_from_the_elbow() {
    // The elbow is the conversion that re-routes, so it is the one most likely to lose the
    // points. It is also the one the cache exists for: `line -> elbow -> line` hands back
    // the remembered line whole, rather than un-routing the elbow.
    let mut rng = Corpus(0x5EEE);
    for case in 0..120 {
        let before = generated_line(&mut rng, "l", 2 + case % 5, DrawElementType::Line);
        let mut engine = selected(vec![before.clone()], &["l"]);
        engine.begin_conversion();
        for _ in 0..3 {
            assert!(engine.convert_selection(None, true), "case {case}");
        }
        assert_eq!(
            linear_of(&get(&engine, "l")),
            Linear::Elbow,
            "case {case}: three steps from a line is an elbow"
        );
        assert!(
            engine.convert_selection(Some(ConvertTo::Linear(LinearType::Line)), true),
            "case {case}: back to a line"
        );
        let back = get(&engine, "l");
        assert!(
            same_element(&back, &before),
            "case {case}: the elbow round trip lost something\n  before {before:?}\n  after  {back:?}"
        );
    }
}

#[test]
fn a_closed_switch_still_wins_when_the_selection_holds_a_line_too() {
    // The property's guard: the linear branch must not reach a selection the generic one
    // owns, or a line in a mixed selection would follow a rectangle round the three types.
    let mut rng = Corpus(0x5EEF);
    for case in 0..60 {
        let before = generated_line(&mut rng, "l", 2 + case % 4, DrawElementType::Line);
        let box_ = with_id(box_at(800.0, 0.0, 100.0, 60.0), "s");
        let mut engine = selected(vec![before.clone(), box_], &["l", "s"]);
        engine.begin_conversion();
        engine.convert_selection(None, true);
        let line = get(&engine, "l");
        assert_eq!(line.kind, before.kind, "case {case}: the line held");
        assert_eq!(line.points, before.points, "case {case}: and its points");
        assert_eq!(
            line.group_ids, before.group_ids,
            "case {case}: and its group"
        );
        assert_ne!(
            get(&engine, "s").kind,
            DrawElementType::Rectangle,
            "case {case}: while the shape walked its own round"
        );
    }
}

/// One generated arrow taken to a line and on to a sharp one, and what came back.
fn arrow_round_trip(rng: &mut Corpus, case: usize) -> (DrawElement, DrawElement, bool) {
    let before = generated_line(rng, "a", 2 + case % 5, DrawElementType::Arrow);
    let had_heads = before.start_arrowhead.is_some() || before.end_arrowhead.is_some();
    let mut engine = selected(vec![before.clone()], &["a"]);
    engine.begin_conversion();
    assert!(
        engine.convert_selection(Some(ConvertTo::Linear(LinearType::Line)), true),
        "case {case}: an unbound arrow has somewhere to go"
    );
    assert_eq!(get(&engine, "a").kind, DrawElementType::Line, "case {case}");
    assert!(
        engine.convert_selection(Some(ConvertTo::Linear(LinearType::SharpArrow)), true),
        "case {case}: and somewhere to come back to"
    );
    (before.clone(), get(&engine, "a"), had_heads)
}

#[test]
fn an_arrow_switched_to_a_line_and_back_is_the_arrow_it_was() {
    // The other half of the round trip. Its heads have their own test below, because which
    // rule they follow is not the same one twice.
    let mut rng = Corpus(0x5EF0);
    for case in 0..120 {
        let (before, back, _) = arrow_round_trip(&mut rng, case);
        assert!(
            same_element(&back, &before),
            "case {case}: the round trip lost something\n  before {before:?}\n  after  {back:?}"
        );
    }
}

#[test]
fn an_arrows_heads_follow_whether_it_was_already_the_type_it_returns_to() {
    // The oracle's own asymmetry (`ConvertElementTypePopup.tsx@1118751f:543-561`,
    // `:884-895`): back to a sub-type the element has been in, the whole remembered
    // element is handed over (`:551-556`), heads and all; on to a sub-type it has not been
    // in, the heads are the toolbar's (`:891-892`), so a circle is not a circle any more.
    let mut rng = Corpus(0x5EF1);
    let mut checked_a_lost_head = 0;
    for case in 0..120 {
        let (before, back, had_heads) = arrow_round_trip(&mut rng, case);
        if before.roundness.is_none() {
            assert_eq!(
                (back.start_arrowhead, back.end_arrowhead),
                (before.start_arrowhead, before.end_arrowhead),
                "case {case}: a sharp arrow hands its remembered heads back whole"
            );
        } else {
            assert_eq!(
                back.end_arrowhead,
                Some(Arrowhead::Arrow),
                "case {case}: a curved arrow on to a sharp one takes the toolbar's head"
            );
            if had_heads {
                assert_ne!(
                    back.start_arrowhead, before.start_arrowhead,
                    "case {case}: and the toolbar's tail, not the one it carried"
                );
                checked_a_lost_head += 1;
            }
        }
    }
    // Without this the loop could pass having asserted nothing, which is the failure mode
    // a property test is supposed to be least prone to.
    assert!(
        checked_a_lost_head > 0,
        "the corpus produced no arrow with a head of its own, so nothing was checked"
    );
}
