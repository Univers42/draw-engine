//! Z-order: Bring forward, Send backward, Bring to front, Send to back.
//!
//! Two parts.
//!
//! - **The oracle's own cases.** Every z-order case of
//!   `packages/element/tests/zindex.test.tsx@1118751f`, same stacks in, same stacks out,
//!   each citing the line of its `assertZindex` call. Deleted elements are in them: the
//!   port keeps the oracle's handling of tombstones even though the engine reorders only
//!   the live stack (tombstones stay at the bottom, `Scene::set_order`). Not ported: the
//!   duplication cases (`:919-1160`), which test Ctrl+D, not a z-order command.
//! - **Through the engine.** Frames with children, a group inside a frame, a label on a
//!   frame child, a locked element in the selection, and a reorder that moves nothing.

mod common;
use common::*;
use draw_engine::edit::reorder_within;
use draw_engine::*;
use std::collections::HashSet;

// ---------------------------------------------------------------------------------------
// The oracle's cases
// ---------------------------------------------------------------------------------------

use ZOrderMode::{Back, Backward, Forward, Front};

/// One `assertZindex` call (`zindex.test.tsx:137-160`): a stack, the group being edited,
/// and each command with the stack it must leave, applied one after another.
struct Case {
    /// The line of the oracle's `assertZindex` (or `assertReorderPreservesElements`).
    line: u32,
    /// The stack, bottom first, one token per element — see [`parse`].
    stack: &'static str,
    editing: Option<&'static str>,
    ops: &'static [(ZOrderMode, &'static str)],
    /// The broken-contiguity block applies each command to a fresh copy of the stack
    /// (`zindex.test.tsx:1578-1597`) rather than one after another.
    fresh: bool,
}

const fn case(line: u32, stack: &'static str, ops: &'static [(ZOrderMode, &'static str)]) -> Case {
    Case {
        line,
        stack,
        editing: None,
        ops,
        fresh: false,
    }
}

const fn editing(
    line: u32,
    stack: &'static str,
    group: &'static str,
    ops: &'static [(ZOrderMode, &'static str)],
) -> Case {
    Case {
        line,
        stack,
        editing: Some(group),
        ops,
        fresh: false,
    }
}

const fn fresh(
    line: u32,
    stack: &'static str,
    group: Option<&'static str>,
    ops: &'static [(ZOrderMode, &'static str)],
) -> Case {
    Case {
        line,
        stack,
        editing: group,
        ops,
        fresh: true,
    }
}

#[rustfmt::skip]
const CASES: &[Case] = &[
    // it("send back") — :167
    case(168, "A B- C- D*", &[(Backward, "D A B C"), (Backward, "D A B C")]),
    case(182, "A* B* C*", &[(Backward, "A B C")]),
    case(194, "A- B C- D*", &[(Backward, "A D B C")]),
    case(204, "A B- C- D* E* F", &[(Backward, "D E A B C F"), (Backward, "D E A B C F")]),
    case(220, "A B C- D- E* F G*", &[
        (Backward, "A E B C D G F"), (Backward, "E A G B C D F"),
        (Backward, "E G A B C D F"), (Backward, "E G A B C D F"),
    ]),
    case(239, "A B C- D* E- F* G", &[
        (Backward, "A D E F B C G"), (Backward, "D E F A B C G"), (Backward, "D E F A B C G"),
    ]),
    case(258, "A>C B C*", &[(Backward, "A C B"), (Backward, "A C B")]),
    case(274, "A B/g1 C/g1 D- E- F*", &[
        (Backward, "A F B C D E"), (Backward, "F A B C D E"), (Backward, "F A B C D E"),
    ]),
    case(291, "A B/g2,g1 C/g2,g1 D/g1 E- F*", &[
        (Backward, "A F B C D E"), (Backward, "F A B C D E"), (Backward, "F A B C D E"),
    ]),
    case(308, "A B/g1 C/g2,g1 D/g2,g1 E- F*", &[
        (Backward, "A F B C D E"), (Backward, "F A B C D E"), (Backward, "F A B C D E"),
    ]),
    case(325, "A B1/g1 C1/g1 D2/g2* E2/g2*", &[(Backward, "A D2 E2 B1 C1")]),
    editing(342, "A B/g1 C/g2,g1 D/g2,g1*", "g2", &[(Backward, "A B D C"), (Backward, "A B D C")]),
    editing(359, "A B/g2,g1 C/g2,g1 D/g1*", "g1", &[(Backward, "A D B C"), (Backward, "A D B C")]),
    editing(376, "A B/g1 C/g2,g1* D/g2,g1- E/g2,g1*", "g1", &[
        (Backward, "A C D E B"), (Backward, "A C D E B"),
    ]),
    editing(394, "A B/g1 C/g2,g1 D/g2,g1 E/g3,g1* F/g3,g1*", "g1", &[
        (Backward, "A B E F C D"), (Backward, "A E F B C D"), (Backward, "A E F B C D"),
    ]),
    editing(415, "A/g1 B/g2 C/g1 D/g2* E/g2*", "g2", &[
        (Backward, "A D E B C"), (Backward, "A D E B C"),
    ]),
    editing(434, "A/g1 B/g2 C/g1 D/g2* F G/g2*", "g2", &[
        (Backward, "A D G B C F"), (Backward, "A D G B C F"),
    ]),
    // it("bring forward") — :454
    case(455, "A B* C* D- E", &[(Forward, "A D E B C"), (Forward, "A D E B C")]),
    case(470, "A* B* C*", &[(Forward, "A B C")]),
    case(482, "A* B- C- D E* F- G", &[
        (Forward, "B C D A F G E"), (Forward, "B C D F G A E"), (Forward, "B C D F G A E"),
    ]),
    case(503, "A* B- C- D/g1 E/g1 F", &[
        (Forward, "B C D E A F"), (Forward, "B C D E F A"), (Forward, "B C D E F A"),
    ]),
    case(520, "A B* C/g2,g1 D/g2,g1 E/g1 F", &[
        (Forward, "A C D E B F"), (Forward, "A C D E F B"), (Forward, "A C D E F B"),
    ]),
    case(537, "A B* C/g1 D/g2,g1 E/g2,g1 F", &[
        (Forward, "A C D E B F"), (Forward, "A C D E F B"), (Forward, "A C D E F B"),
    ]),
    editing(557, "A B/g2,g1* C/g2,g1 D/g1", "g2", &[(Forward, "A C B D"), (Forward, "A C B D")]),
    editing(574, "A/g1* B/g2,g1 C/g2,g1 D", "g1", &[(Forward, "B C A D"), (Forward, "B C A D")]),
    editing(591, "A/g2,g1* B/g2,g1* C/g1 D", "g1", &[(Forward, "C A B D"), (Forward, "C A B D")]),
    editing(609, "A/g2* B/g2* C/g1 D/g2 E/g1", "g2", &[
        (Forward, "C D A B E"), (Forward, "C D A B E"),
    ]),
    editing(628, "A/g2* B C/g2* D/g1 E/g2 F/g1", "g2", &[
        (Forward, "B D E A C F"), (Forward, "B D E A C F"),
    ]),
    // it("bring to front") — :648
    case(649, "0 A* B- C- D E* F- G", &[
        (Front, "0 B C D F G A E"), (Front, "0 B C D F G A E"),
    ]),
    case(667, "A* B* C*", &[(Front, "A B C")]),
    case(679, "A B* C*", &[(Front, "A B C")]),
    case(691, "A* B* C", &[(Front, "C A B"), (Front, "C A B")]),
    editing(707, "A B/g1 C/g1* D/g1 E/g1* F/g2,g1 G/g2,g1 H/g3,g1 I/g3,g1", "g1", &[
        (Front, "A B D F G H I C E"), (Front, "A B D F G H I C E"),
    ]),
    editing(729, "A B/g2,g1* D/g2,g1 C/g1", "g2", &[(Front, "A D B C"), (Front, "A D B C")]),
    editing(747, "A/g2,g3* B/g1,g3 C/g2,g3 D/g1,g3", "g2", &[
        (Front, "B C A D"), (Front, "B C A D"),
    ]),
    editing(765, "A/g2* B/g1 C/g2 D/g1", "g2", &[(Front, "B C A D"), (Front, "B C A D")]),
    // it("send to back") — :783
    case(784, "A B- C D- E* F- G H* I", &[
        (Back, "E H A B C D F G I"), (Back, "E H A B C D F G I"),
    ]),
    case(803, "A* B* C*", &[(Back, "A B C")]),
    case(815, "A* B* C", &[(Back, "A B C")]),
    case(827, "A B* C*", &[(Back, "B C A"), (Back, "B C A")]),
    editing(843, "A B/g2,g1 C/g2,g1 D/g3,g1 E/g3,g1 F/g1* G/g1 H/g1* I/g1", "g1", &[
        (Back, "A F H B C D E G I"), (Back, "A F H B C D E G I"),
    ]),
    editing(865, "A B/g1 C/g2,g1 D/g2,g1*", "g2", &[(Back, "A B D C"), (Back, "A B D C")]),
    editing(883, "A/g1,g3 B/g2,g3 C/g1,g3 D/g2,g3*", "g2", &[
        (Back, "A D B C"), (Back, "A D B C"),
    ]),
    editing(901, "A/g1 B/g2 C/g1 D/g2*", "g2", &[(Back, "A D B C"), (Back, "A D B C")]),
    // it("text-container binding should be atomic") — :1162
    case(1163, "A* B C>B", &[(Forward, "B C A"), (Backward, "A B C")]),
    case(1175, "A B* C>B", &[(Backward, "B C A"), (Forward, "A B C")]),
    editing(1187, "A/g1* B/g1 C>B/g1", "g1", &[(Forward, "B C A"), (Backward, "A B C")]),
    editing(1202, "A/g1 B/g1* C>B/g1", "g1", &[(Backward, "B C A"), (Forward, "A B C")]),
    editing(1217, "A/g1 B/g1* C D>C", "g1", &[(Forward, "A B C D")]),
    // describe("z-indexing with frames") — :1232
    case(1244, "F1_1@F1 F1_2@F1 F1#* R1 R2", &[
        (Forward, "R1 F1_1 F1_2 F1 R2"), (Forward, "R1 R2 F1_1 F1_2 F1"),
        (Forward, "R1 R2 F1_1 F1_2 F1"), (Backward, "R1 F1_1 F1_2 F1 R2"),
        (Backward, "F1_1 F1_2 F1 R1 R2"), (Backward, "F1_1 F1_2 F1 R1 R2"),
    ]),
    case(1271, "F1_1@F1 F1#* F1_2@F1 R1 R2", &[
        (Forward, "R1 F1_1 F1 F1_2 R2"), (Forward, "R1 R2 F1_1 F1 F1_2"),
        (Forward, "R1 R2 F1_1 F1 F1_2"),
    ]),
    case(1290, "F1_1@F1 F1#* R1 F1_2@F1 R2", &[
        (Forward, "R1 F1_1 F1 R2 F1_2"), (Forward, "R1 R2 F1_1 F1 F1_2"),
        (Forward, "R1 R2 F1_1 F1 F1_2"),
    ]),
    // The oracle marks its second and third steps FIXME (`:1322`, `:1325`); they are what
    // it does, so they are what is pinned.
    case(1309, "F1_1@F1 R1 F1#* R2 F1_2@F1 R3", &[
        (Forward, "R1 F1_1 R2 F1 R3 F1_2"), (Forward, "R1 R2 F1_1 R3 F1 F1_2"),
        (Forward, "R1 R2 R3 F1_1 F1 F1_2"),
    ]),
    case(1331, "F1_1@F1 R1 F1#* R2 F1_2@F1 R3", &[
        (Backward, "F1_1 F1 R1 F1_2 R2 R3"), (Backward, "F1_1 F1 F1_2 R1 R2 R3"),
    ]),
    case(1351, "F1_1@F1* F1_2@F1 F1# R1", &[
        (Forward, "F1_2 F1_1 F1 R1"), (Forward, "F1_2 F1_1 F1 R1"),
    ]),
    case(1367, "F1_1@F1* F1_2@F1 F1# R1 F2_1@F2* F2_2@F2 F2# R2", &[
        (Forward, "F1_2 F1_1 F1 R1 F2_2 F2_1 F2 R2"),
        (Forward, "F1_2 F1_1 F1 R1 F2_2 F2_1 F2 R2"),
    ]),
    case(1395, "F1_1@F1* F1# F1_2@F1 R1", &[
        (Forward, "F1 F1_2 F1_1 R1"), (Forward, "F1 F1_2 F1_1 R1"),
        (Backward, "F1 F1_1 F1_2 R1"), (Backward, "F1 F1_1 F1_2 R1"),
    ]),
    case(1416, "F1_1@F1* R1 F1# F1_2@F1 R2", &[
        (Forward, "R1 F1 F1_2 F1_1 R2"), (Forward, "R1 F1 F1_2 F1_1 R2"),
        (Backward, "R1 F1 F1_1 F1_2 R2"), (Backward, "R1 F1 F1_1 F1_2 R2"),
    ]),
    case(1440, "F1_1@F1 F1_2@F1 F1#* R1 R2", &[
        (Front, "R1 R2 F1_1 F1_2 F1"), (Front, "R1 R2 F1_1 F1_2 F1"),
        (Back, "F1_1 F1_2 F1 R1 R2"), (Back, "F1_1 F1_2 F1 R1 R2"),
    ]),
    case(1461, "F1_1@F1 F1#* F1_2@F1 R1 R2", &[
        (Front, "R1 R2 F1_1 F1 F1_2"), (Front, "R1 R2 F1_1 F1 F1_2"),
        (Back, "F1_1 F1 F1_2 R1 R2"), (Back, "F1_1 F1 F1_2 R1 R2"),
    ]),
    case(1482, "F1_1@F1 F1#* R1 F1_2@F1 R2", &[(Front, "R1 R2 F1_1 F1 F1_2")]),
    case(1497, "F1_1@F1 R1 F1#* R2 F1_2@F1 R3", &[(Front, "R1 R2 R3 F1_1 F1 F1_2")]),
    case(1514, "F1_1@F1* F1_2@F1 F1# F2_1@F2* F2_2@F2 F2#", &[
        (Front, "F1_2 F1 F1_1 F2_2 F2 F2_1"), (Back, "F1_1 F1_2 F1 F2_1 F2_2 F2"),
    ]),
    editing(1533, "F1_1@F1/g1 F1_2@F1/g1* F1# F2_1@F2/g2 F2_2@F2/g2 F2#", "g1", &[
        (Back, "F1_2 F1_1 F1 F2_1 F2_2 F2"), (Front, "F1_1 F1 F1_2 F2_1 F2_2 F2"),
    ]),
    // describe("z-index reordering with broken contiguity") — :1573
    fresh(1610, "F1_1@F1* F2_1@F2 F1_2@F1 F1# F2#", None, &[
        (Forward, "F2_1 F1_2 F1_1 F1 F2"), (Backward, "F1_1 F2_1 F1_2 F1 F2"),
        (Front, "F2_1 F1_2 F1 F1_1 F2"), (Back, "F1_1 F2_1 F1_2 F1 F2"),
    ]),
    fresh(1626, "A/g1* B C/g1* D", None, &[
        (Forward, "B A D C"), (Backward, "A C B D"), (Front, "B D A C"), (Back, "A C B D"),
    ]),
    fresh(1643, "A/g1 B C/g1* D", Some("g1"), &[
        (Forward, "A B C D"), (Backward, "C A B D"), (Front, "A B C D"), (Back, "C A B D"),
    ]),
    fresh(1659, "A/g1* X/g2* C/g1* Y/g2* Z", None, &[
        (Forward, "Z A X C Y"), (Backward, "A X C Y Z"), (Front, "Z A X C Y"),
        (Back, "A X C Y Z"),
    ]),
    // describe("z-index reordering with inconsistent group-editing state") — :1668
    editing(1674, "A/g1* C/g1 X/g2 Y/g2 R", "g2", &[(Back, "A C X Y R")]),
    editing(1686, "A/g1 C/g1 X/g2* Y/g2 R", "g1", &[(Front, "A C X Y R")]),
];

/// One stack token: `id`, then any of `*` selected, `-` deleted, `#` a frame,
/// `/g2,g1` its groups (innermost first), `>C` the shape it labels, `@F` its frame.
/// The fields of `populateElements`, `zindex.test.tsx:45-129`.
struct Token {
    id: String,
    selected: bool,
    deleted: bool,
    frame: bool,
    groups: Vec<String>,
    container: Option<String>,
    frame_id: Option<String>,
}

fn parse(token: &str) -> Token {
    let ident = |s: &str| -> usize {
        s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ','))
            .unwrap_or(s.len())
    };
    let end = ident(token);
    let mut out = Token {
        id: token[..end].to_string(),
        selected: false,
        deleted: false,
        frame: false,
        groups: Vec::new(),
        container: None,
        frame_id: None,
    };
    let mut rest = &token[end..];
    while let Some(mark) = rest.chars().next() {
        rest = &rest[1..];
        match mark {
            '*' => out.selected = true,
            '-' => out.deleted = true,
            '#' => out.frame = true,
            '/' | '>' | '@' => {
                let end = ident(rest);
                let value = rest[..end].to_string();
                rest = &rest[end..];
                match mark {
                    '/' => out.groups = value.split(',').map(str::to_string).collect(),
                    '>' => out.container = Some(value),
                    _ => out.frame_id = Some(value),
                }
            }
            other => panic!("unknown mark {other:?} in {token:?}"),
        }
    }
    out
}

/// The stack and the selection, as `populateElements` builds them: a text for an element
/// with a container, a frame for `#`, a rectangle otherwise, and a container's bound text
/// set only when the text lies directly above it (`zindex.test.tsx:97-113`).
fn populate(stack: &str) -> (Vec<DrawElement>, HashSet<String>) {
    let tokens: Vec<Token> = stack.split_whitespace().map(parse).collect();
    let mut selected = HashSet::new();
    let mut elements: Vec<DrawElement> = tokens
        .iter()
        .map(|token| {
            let mut element = if token.frame {
                let mut frame = box_at(100.0, 100.0, 100.0, 100.0);
                frame.kind = DrawElementType::Frame;
                frame
            } else if token.container.is_some() {
                text_at(100.0, 100.0, 100.0, 100.0)
            } else {
                box_at(100.0, 100.0, 100.0, 100.0)
            };
            element.id = token.id.clone();
            element.is_deleted = token.deleted;
            element.group_ids = token.groups.clone();
            element.container_id = token.container.clone();
            element.frame_id = token.frame_id.clone();
            if token.selected {
                selected.insert(token.id.clone());
            }
            element
        })
        .collect();
    for i in 0..elements.len().saturating_sub(1) {
        if elements[i + 1].container_id.as_deref() == Some(elements[i].id.as_str()) {
            elements[i].bound_text_id = Some(elements[i + 1].id.clone());
        }
    }
    (elements, selected)
}

fn names(elements: &[DrawElement]) -> String {
    elements
        .iter()
        .map(|el| el.id.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn the_oracles_z_index_cases() {
    let mut failures = Vec::new();
    for case in CASES {
        let (start, selected) = populate(case.stack);
        let mut stack = start.clone();
        for (step, (mode, expected)) in case.ops.iter().enumerate() {
            let from = if case.fresh { &start } else { &stack };
            let next = reorder_within(from, &selected, *mode, case.editing);
            let got = names(&next);
            if got != *expected {
                failures.push(format!(
                    "zindex.test.tsx:{} step {} {:?}: expected [{}], got [{}]",
                    case.line,
                    step + 1,
                    mode,
                    expected,
                    got
                ));
            }
            stack = next;
        }
    }
    assert!(
        failures.is_empty(),
        "{} of the oracle's steps differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------------------------
// Through the engine
// ---------------------------------------------------------------------------------------

type Cast = Vec<(&'static str, String)>;

fn id(cast: &Cast, name: &str) -> String {
    cast.iter()
        .find(|(n, _)| *n == name)
        .map(|(_, id)| id.clone())
        .expect("no such name")
}

fn stack(engine: &DrawEngine, cast: &Cast) -> Vec<&'static str> {
    engine
        .get_scene()
        .into_iter()
        .filter(|el| !el.is_deleted)
        .map(|el| {
            cast.iter()
                .find(|(_, id)| *id == el.id)
                .map(|(name, _)| *name)
                .expect("an element nobody named")
        })
        .collect()
}

/// A frame `F` at (0, 0, 600, 200) and whatever `members` lists, bottom first: a name, the
/// groups, and whether it sits inside the frame (then it carries `frameId`). The frame
/// itself is the member named "F".
fn framed(members: &[(&'static str, &[&str], bool)]) -> (DrawEngine, Cast) {
    let frame_id = new_element_id();
    let mut elements = Vec::new();
    let mut cast = Vec::new();
    for (index, (name, groups, inside)) in members.iter().enumerate() {
        let element = if *name == "F" {
            let mut frame = box_at(0.0, 0.0, 600.0, 200.0);
            frame.kind = DrawElementType::Frame;
            frame.id = frame_id.clone();
            frame
        } else {
            let x = 20.0 + 90.0 * index as f64;
            let y = if *inside { 60.0 } else { 400.0 };
            let mut element = filled(box_at(x, y, 60.0, 60.0));
            element.group_ids = groups.iter().map(|g| g.to_string()).collect();
            element.frame_id = inside.then(|| frame_id.clone());
            element
        };
        cast.push((*name, element.id.clone()));
        elements.push(element);
    }
    let mut engine = engine_with_scene(elements);
    engine.set_tool(DrawTool::Select);
    (engine, cast)
}

/// The oracle keeps a frame child inside its frame's range: to the front of that range,
/// not of the board (`shiftElementsAccountingForFrames`, `zindex.ts:543-621`).
#[test]
fn a_frame_child_brought_to_the_front_stays_in_its_frame() {
    let (mut engine, cast) = framed(&[
        ("C1", &[], true),
        ("C2", &[], true),
        ("F", &[], false),
        ("X", &[], false),
    ]);
    engine.select(vec![id(&cast, "C1")]);

    engine.reorder_selection(Front);

    assert_eq!(stack(&engine, &cast), vec!["C2", "F", "C1", "X"]);
}

#[test]
fn a_frame_child_sent_to_the_back_stays_in_its_frame() {
    let (mut engine, cast) = framed(&[
        ("X", &[], false),
        ("C1", &[], true),
        ("C2", &[], true),
        ("F", &[], false),
    ]);
    engine.select(vec![id(&cast, "C2")]);

    engine.reorder_selection(Back);

    assert_eq!(stack(&engine, &cast), vec!["X", "C2", "C1", "F"]);
}

/// A frame and its children are one block to step over (`getTargetIndex`,
/// `zindex.ts:256-267`), so nothing lands between a frame's members.
#[test]
fn stepping_forward_from_below_a_frame_steps_over_all_of_it() {
    let (mut engine, cast) = framed(&[
        ("X", &[], false),
        ("C1", &[], true),
        ("C2", &[], true),
        ("F", &[], false),
    ]);
    engine.select(vec![id(&cast, "X")]);

    engine.reorder_selection(Forward);

    assert_eq!(stack(&engine, &cast), vec!["C1", "C2", "F", "X"]);
}

#[test]
fn stepping_backward_from_above_a_frame_steps_under_all_of_it() {
    let (mut engine, cast) = framed(&[
        ("C1", &[], true),
        ("C2", &[], true),
        ("F", &[], false),
        ("X", &[], false),
    ]);
    engine.select(vec![id(&cast, "X")]);

    engine.reorder_selection(Backward);

    assert_eq!(stack(&engine, &cast), vec!["X", "C1", "C2", "F"]);
}

/// A selected frame takes its children with it (`includeElementsInFrames`,
/// `packages/element/src/selection.ts:196-210`).
#[test]
fn a_selected_frame_takes_its_children_to_the_front() {
    let (mut engine, cast) = framed(&[
        ("C1", &[], true),
        ("C2", &[], true),
        ("F", &[], false),
        ("X", &[], false),
    ]);
    engine.select(vec![id(&cast, "F")]);

    engine.reorder_selection(Front);

    assert_eq!(stack(&engine, &cast), vec!["X", "C1", "C2", "F"]);
}

#[test]
fn a_selected_frame_takes_its_children_one_step_back() {
    let (mut engine, cast) = framed(&[
        ("X", &[], false),
        ("C1", &[], true),
        ("C2", &[], true),
        ("F", &[], false),
    ]);
    engine.select(vec![id(&cast, "F")]);

    engine.reorder_selection(Backward);

    assert_eq!(stack(&engine, &cast), vec!["C1", "C2", "F", "X"]);
}

/// A child steps to the next element of its own frame, over what else lies in the frame's
/// range (`indexFilter`, `zindex.ts:211-213`), and a group there is still one block to
/// step over (`:269-296`): Y, not in the frame, is passed, the group taken whole.
#[test]
fn inside_a_frame_a_group_is_stepped_over_whole() {
    let (mut engine, cast) = framed(&[
        ("C", &[], true),
        ("Y", &[], false),
        ("G1", &["g"], true),
        ("G2", &["g"], true),
        ("F", &[], false),
        ("X", &[], false),
    ]);
    engine.select(vec![id(&cast, "C")]);

    engine.reorder_selection(Forward);

    assert_eq!(stack(&engine, &cast), vec!["Y", "G1", "G2", "C", "F", "X"]);
}

/// A label carries no `frameId` here — its shape holds the membership for both
/// (`scene/frame.rs`, `can_belong_to_frame`) — where the oracle's bound text carries its
/// container's (`addElementsToFrame`, `frame.ts:562-583`). Read through its shape, the
/// label stays on its shape and inside the frame.
#[test]
fn a_frame_childs_label_stays_on_it_inside_the_frame() {
    let (mut engine, mut cast) = framed(&[("R", &[], true), ("F", &[], false), ("X", &[], false)]);
    let mut scene = engine.get_scene();
    let mut label = text_at(30.0, 80.0, 40.0, 20.0);
    label.text = Some("hi".into());
    label.container_id = Some(id(&cast, "R"));
    scene[0].bound_text_id = Some(label.id.clone());
    cast.push(("T", label.id.clone()));
    scene.insert(1, label);
    engine.set_scene(Scene::new(scene));
    engine.select(vec![id(&cast, "R")]);

    engine.reorder_selection(Front);

    assert_eq!(stack(&engine, &cast), vec!["F", "R", "T", "X"]);
}

/// A locked element in the selection stays where it is. The oracle never holds a loose
/// locked element — Select All skips them (`actionSelectAll.ts:32-38`) — but this
/// engine's does, so they can be unlocked from the menu; the carried set is the one the
/// oracle would have.
#[test]
fn a_locked_element_in_the_selection_stays_where_it_is() {
    let (mut engine, cast) = framed(&[("A", &[], false), ("L", &[], false), ("B", &[], false)]);
    let mut scene = engine.get_scene();
    scene[1].locked = Some(true);
    engine.set_scene(Scene::new(scene));
    engine.select(vec![id(&cast, "A"), id(&cast, "L"), id(&cast, "B")]);

    engine.reorder_selection(Front);

    assert_eq!(stack(&engine, &cast), vec!["L", "A", "B"]);
}

/// A group is one thing, locked member and all (`groups.ts:94-132` selects it with no lock
/// filter), so the member goes where its group goes — beside a loose locked element in
/// the same selection, which stays.
#[test]
fn a_locked_group_member_goes_with_its_group() {
    let (mut engine, cast) = framed(&[
        ("M1", &["g"], false),
        ("M2", &["g"], false),
        ("L", &[], false),
        ("X", &[], false),
    ]);
    let mut scene = engine.get_scene();
    scene[0].locked = Some(true);
    scene[2].locked = Some(true);
    engine.set_scene(Scene::new(scene));
    engine.select(vec![id(&cast, "M1"), id(&cast, "M2"), id(&cast, "L")]);

    engine.reorder_selection(Front);

    assert_eq!(stack(&engine, &cast), vec!["L", "X", "M1", "M2"]);
}

/// Bringing the topmost element to the front changes nothing, so it is no edit: no step
/// of undo, and no whole scene handed to the host to save.
#[test]
fn a_reorder_that_moves_nothing_is_not_an_edit() {
    let (mut engine, cast) = framed(&[("A", &[], false), ("B", &[], false)]);
    engine.select(vec![id(&cast, "B")]);
    let _ = engine.drain_events();

    engine.reorder_selection(Front);

    assert!(
        engine.drain_events().scene_json.is_none(),
        "no scene sent for nothing"
    );
    assert!(!engine.debug_state().scene.can_undo, "no step recorded");
}
