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
//!   frame child, a locked element in the selection, a label whose shape stays (locked,
//!   held by a peer, or not selected), and a reorder that moves nothing.

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

/// R, filled, with its label T, then X: bottom first. What `hold` does to R — locks it,
/// or has a peer hold it — is done before Select All, which here takes locked elements
/// and labels (`DrawEngine::select_all`) where the oracle's takes neither
/// (`actionSelectAll.ts:32-38`).
fn labelled_under_select_all(hold: fn(&mut DrawEngine, &Cast)) -> (DrawEngine, Cast) {
    let (mut engine, mut cast) = framed(&[("R", &[], false), ("X", &[], false)]);
    let mut scene = engine.get_scene();
    let mut label = text_at(30.0, 420.0, 40.0, 20.0);
    label.text = Some("hi".into());
    label.container_id = Some(id(&cast, "R"));
    scene[0].bound_text_id = Some(label.id.clone());
    cast.push(("T", label.id.clone()));
    scene.insert(1, label);
    engine.set_scene(Scene::new(scene));
    hold(&mut engine, &cast);
    engine.select_all();
    (engine, cast)
}

fn lock_r(engine: &mut DrawEngine, cast: &Cast) {
    let mut scene = engine.get_scene();
    let r = id(cast, "R");
    for element in scene.iter_mut().filter(|el| el.id == r) {
        element.locked = Some(true);
    }
    engine.set_scene(Scene::new(scene));
}

fn peer_holds_r(engine: &mut DrawEngine, cast: &Cast) {
    engine.set_peers(vec![Peer {
        id: "ana".into(),
        name: "Ana".into(),
        color: "#e03131".into(),
        holds: [id(cast, "R")].into_iter().collect(),
        preview: Vec::new(),
    }]);
}

/// A label goes only with its shape. Left in the moving set on its own while its locked
/// shape stayed, it went to the back alone — under its own filled shape, out of sight.
/// The result is the oracle's, whose Select All holds X alone.
#[test]
fn select_all_leaves_a_locked_shapes_label_on_it() {
    for mode in [Back, Backward] {
        let (mut engine, cast) = labelled_under_select_all(lock_r);
        assert_eq!(engine.get_selection().len(), 3, "setup");

        engine.reorder_selection(mode);

        assert_eq!(stack(&engine, &cast), vec!["X", "R", "T"], "{mode:?}");
    }
}

/// The same for a shape a peer holds: the selection takes neither it nor its label
/// (`peers.rs` › `is_held`).
#[test]
fn select_all_leaves_a_held_shapes_label_on_it() {
    for mode in [Back, Backward] {
        let (mut engine, cast) = labelled_under_select_all(peer_holds_r);
        assert_eq!(engine.get_selection(), vec![id(&cast, "X")], "setup: X");

        engine.reorder_selection(mode);

        assert_eq!(stack(&engine, &cast), vec!["X", "R", "T"], "{mode:?}");
    }
}

/// A click on a label selects the label alone here (`begin_select`), where the oracle's
/// selects its shape. Whatever the command, the label stays directly above its shape.
#[test]
fn a_label_selected_alone_stays_on_its_shape() {
    for mode in [Back, Backward, Front, Forward] {
        let (mut engine, mut cast) = framed(&[("X", &[], false), ("R", &[], false)]);
        let mut scene = engine.get_scene();
        let mut label = text_at(30.0, 420.0, 40.0, 20.0);
        label.text = Some("hi".into());
        label.container_id = Some(id(&cast, "R"));
        scene[1].bound_text_id = Some(label.id.clone());
        cast.push(("T", label.id.clone()));
        scene.push(label);
        engine.set_scene(Scene::new(scene));
        engine.select(vec![id(&cast, "T")]);

        engine.reorder_selection(mode);

        let order = stack(&engine, &cast);
        let r = order.iter().position(|name| *name == "R").unwrap();
        assert_eq!(order.get(r + 1), Some(&"T"), "{mode:?}: {order:?}");
    }
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

// ---------------------------------------------------------------------------------------
// What joins a frame goes directly below it
// ---------------------------------------------------------------------------------------
//
// The oracle keeps a frame's children in one run under it as they join: a new element is
// inserted there (`insertNewElements`, `App.tsx@1118751f:7754-7782`), and one dragged in,
// pasted in or taken in by a new frame is moved there (`addElementsToFrame`,
// `packages/element/src/frame.ts@1118751f:538-635`) — directly below the frame, or directly
// above its highest child when a child sits above it (`getFrameChildrenInsertionIndex`,
// `frame.ts@1118751f:521-536`). Each case below was run on excalidraw.com, 2026-09-25
// (`scratchpad/history-frames/oracle.cjs`, F4-F7).

fn drag(engine: &mut DrawEngine, from: (f64, f64), to: (f64, f64)) {
    engine.begin_pointer(from.0, from.1, false, false);
    for step in 1..=5 {
        let t = f64::from(step) / 5.0;
        engine.move_pointer(
            from.0 + (to.0 - from.0) * t,
            from.1 + (to.1 - from.1) * t,
            false,
            false,
        );
    }
    engine.end_pointer();
}

/// The one element the last command left selected, named `name` in the cast.
fn selected_as(engine: &DrawEngine, cast: &mut Cast, name: &'static str) {
    let selected = engine.get_selection();
    assert_eq!(selected.len(), 1, "one element selected: {selected:?}");
    cast.push((name, selected[0].clone()));
}

fn frame_of(engine: &DrawEngine, id: &str) -> Option<String> {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .and_then(|el| el.frame_id)
}

/// OBSERVED (F4): a rectangle drawn in a frame lands under it, above the frame's other
/// children — not on top of the board, over the frame.
#[test]
fn a_shape_drawn_inside_a_frame_goes_directly_below_it() {
    let (mut engine, mut cast) = framed(&[("C1", &[], true), ("F", &[], false), ("X", &[], false)]);
    engine.set_tool(DrawTool::Rectangle);

    drag(&mut engine, (300.0, 50.0), (360.0, 120.0));
    selected_as(&engine, &mut cast, "N");

    assert_eq!(stack(&engine, &cast), vec!["C1", "N", "F", "X"]);
    assert_eq!(frame_of(&engine, &id(&cast, "N")), Some(id(&cast, "F")));
}

/// OBSERVED (F5): a shape dragged into a frame moves to just below it, and undo puts the
/// stack back with the membership.
#[test]
fn a_shape_dragged_into_a_frame_goes_directly_below_it() {
    let (mut engine, cast) = framed(&[
        ("A", &[], false),
        ("C1", &[], true),
        ("F", &[], false),
        ("X", &[], false),
    ]);

    // A sits at (20, 400); into the frame at (320, 60).
    drag(&mut engine, (50.0, 430.0), (350.0, 90.0));

    assert_eq!(frame_of(&engine, &id(&cast, "A")), Some(id(&cast, "F")));
    assert_eq!(stack(&engine, &cast), vec!["C1", "A", "F", "X"]);

    engine.undo();
    assert_eq!(frame_of(&engine, &id(&cast, "A")), None);
    assert_eq!(stack(&engine, &cast), vec!["A", "C1", "F", "X"]);

    engine.redo();
    assert_eq!(stack(&engine, &cast), vec!["C1", "A", "F", "X"]);
}

/// OBSERVED (F6): a copy pasted into a frame goes just below it.
#[test]
fn a_paste_into_a_frame_goes_directly_below_it() {
    let (mut engine, mut cast) = framed(&[("C1", &[], true), ("F", &[], false), ("X", &[], false)]);
    engine.select(vec![id(&cast, "X")]);
    let copied = engine.copy_selection();

    assert!(engine.paste_json(copied.as_deref(), Some((400.0, 100.0))));
    selected_as(&engine, &mut cast, "P");

    assert_eq!(frame_of(&engine, &id(&cast, "P")), Some(id(&cast, "F")));
    assert_eq!(stack(&engine, &cast), vec!["C1", "P", "F", "X"]);

    // Away from any frame, a paste goes on top of the board.
    assert!(engine.paste_json(copied.as_deref(), Some((800.0, 500.0))));
    selected_as(&engine, &mut cast, "Q");
    assert_eq!(frame_of(&engine, &id(&cast, "Q")), None);
    assert_eq!(stack(&engine, &cast), vec!["C1", "P", "F", "X", "Q"]);
}

/// OBSERVED (F7): a frame drawn over a shape takes it in, and the shape goes directly below
/// the new frame, which is on top of the board.
#[test]
fn a_frame_drawn_over_a_shape_takes_it_directly_below_it() {
    let a = filled(box_at(40.0, 40.0, 60.0, 60.0));
    let x = filled(box_at(500.0, 400.0, 60.0, 60.0));
    let mut cast: Cast = vec![("A", a.id.clone()), ("X", x.id.clone())];
    let mut engine = engine_with_scene(vec![a, x]);
    engine.set_tool(DrawTool::Frame);

    drag(&mut engine, (0.0, 0.0), (200.0, 200.0));
    selected_as(&engine, &mut cast, "F");

    assert_eq!(stack(&engine, &cast), vec!["X", "A", "F"]);
}

/// A label goes with its shape, directly above it, as the oracle adds bound text with its
/// container (`frame.ts@1118751f:578-582`).
#[test]
fn a_labelled_shape_dragged_into_a_frame_takes_its_label_along() {
    let (engine, mut cast) = framed(&[("F", &[], false), ("X", &[], false), ("R", &[], false)]);
    let mut scene = engine.get_scene();
    let mut label = text_at(200.0, 420.0, 40.0, 20.0);
    label.text = Some("hi".into());
    label.container_id = Some(id(&cast, "R"));
    scene[2].bound_text_id = Some(label.id.clone());
    cast.push(("T", label.id.clone()));
    scene.push(label);
    let mut engine = engine_with_scene(scene);
    engine.set_tool(DrawTool::Select);

    // R sits at (200, 400); into the frame at (300, 60).
    drag(&mut engine, (230.0, 405.0), (330.0, 65.0));

    assert_eq!(frame_of(&engine, &id(&cast, "R")), Some(id(&cast, "F")));
    assert_eq!(stack(&engine, &cast), vec!["R", "T", "F", "X"]);
}

/// What already belongs to the frame is not restacked by a move inside it
/// (`commonFrameId === frame.id`, `frame.ts@1118751f:601-608`).
#[test]
fn a_child_moved_inside_its_frame_keeps_its_place() {
    let (mut engine, cast) = framed(&[
        ("C1", &[], true),
        ("C2", &[], true),
        ("F", &[], false),
        ("X", &[], false),
    ]);

    // C1 sits at (20, 60).
    drag(&mut engine, (50.0, 90.0), (450.0, 110.0));

    assert_eq!(frame_of(&engine, &id(&cast, "C1")), Some(id(&cast, "F")));
    assert_eq!(stack(&engine, &cast), vec!["C1", "C2", "F", "X"]);
}

/// A child sitting above its frame — a stack the oracle's own tests call denormalised —
/// takes a newcomer directly above it rather than below the frame.
#[test]
fn a_child_above_its_frame_takes_the_newcomer_above_it() {
    let (mut engine, mut cast) = framed(&[("F", &[], false), ("C1", &[], true), ("X", &[], false)]);
    engine.set_tool(DrawTool::Rectangle);

    drag(&mut engine, (300.0, 50.0), (360.0, 120.0));
    selected_as(&engine, &mut cast, "N");

    assert_eq!(stack(&engine, &cast), vec!["F", "C1", "N", "X"]);
}

/// A selection dragged in together, part of it in the frame already, goes below the frame
/// together, in its own order: the oracle adds what the drag carried, members included,
/// whenever they do not all share the frame (`getCommonFrameId`, `frame.ts@1118751f:503-519`).
#[test]
fn a_selection_dragged_in_partly_from_inside_goes_below_the_frame_together() {
    let mut frame = box_at(0.0, 0.0, 600.0, 200.0);
    frame.kind = DrawElementType::Frame;
    let mut c1 = filled(box_at(20.0, 100.0, 60.0, 60.0));
    c1.frame_id = Some(frame.id.clone());
    let a = filled(box_at(300.0, 210.0, 60.0, 60.0));
    let mut c2 = filled(box_at(450.0, 60.0, 60.0, 60.0));
    c2.frame_id = Some(frame.id.clone());
    let x = filled(box_at(700.0, 400.0, 60.0, 60.0));
    let cast: Cast = vec![
        ("C1", c1.id.clone()),
        ("A", a.id.clone()),
        ("C2", c2.id.clone()),
        ("F", frame.id.clone()),
        ("X", x.id.clone()),
    ];
    let mut engine = engine_with_scene(vec![c1, a, c2, frame, x]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id(&cast, "C1"), id(&cast, "A")]);

    drag(&mut engine, (330.0, 240.0), (330.0, 160.0));

    assert_eq!(frame_of(&engine, &id(&cast, "A")), Some(id(&cast, "F")));
    assert_eq!(stack(&engine, &cast), vec!["C2", "C1", "A", "F", "X"]);
}

/// Only what the selection carried joins the run. An arrow of the frame's that the drag
/// re-routed — it is bound to S1 — was changed by the commit but never carried, and keeps
/// its place: the oracle adds the selected elements that are in the frame
/// (`App.tsx@1118751f:12046-12059`), with their labels (`frame.ts@1118751f:567-584`).
/// Transcribed: the others are [Ar, S2, F], the run goes in at F (`:521-536`).
#[test]
fn an_arrow_the_drag_rerouted_keeps_its_place() {
    let mut frame = box_at(0.0, 0.0, 800.0, 300.0);
    frame.kind = DrawElementType::Frame;
    let mut s1 = filled(box_at(20.0, 100.0, 60.0, 60.0));
    s1.frame_id = Some(frame.id.clone());
    let mut s2 = filled(box_at(600.0, 100.0, 60.0, 60.0));
    s2.frame_id = Some(frame.id.clone());
    let mut ar = connector(80.0, 130.0, 600.0, 130.0, DrawElementType::Arrow);
    ar.frame_id = Some(frame.id.clone());
    ar.start_binding = Some(s1.id.clone());
    ar.end_binding = Some(s2.id.clone());
    let t = filled(box_at(200.0, 305.0, 60.0, 60.0));
    let cast: Cast = vec![
        ("Ar", ar.id.clone()),
        ("S1", s1.id.clone()),
        ("S2", s2.id.clone()),
        ("F", frame.id.clone()),
        ("T", t.id.clone()),
    ];
    let mut engine = engine_with_scene(vec![ar, s1, s2, frame, t]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id(&cast, "S1"), id(&cast, "T")]);

    drag(&mut engine, (230.0, 335.0), (230.0, 265.0));

    assert_eq!(frame_of(&engine, &id(&cast, "T")), Some(id(&cast, "F")));
    assert_eq!(stack(&engine, &cast), vec!["Ar", "S2", "S1", "T", "F"]);
}

/// The frame's south-east handle, dragged by `by`.
fn resize_frame(engine: &mut DrawEngine, cast: &Cast, by: (f64, f64)) {
    let frame = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id(cast, "F"))
        .expect("the frame");
    engine.select(vec![frame.id.clone()]);
    let offset = HandleLayout::screen(8.0, 26.0, 1.0).handle_offset;
    let at = (
        frame.x + frame.width + offset,
        frame.y + frame.height + offset,
    );
    drag(engine, at, (at.0 + by.0, at.1 + by.1));
}

/// A frame resized puts its children in one run directly below it even when nothing
/// joined: the oracle takes them all out and adds them back (`replaceAllElementsInFrame`,
/// `frame.ts@1118751f:684-694`) on every resize (`App.tsx@1118751f:12097-12117`). A child
/// above its frame — a Ctrl+D copy of one, here — goes below it.
#[test]
fn a_resized_frame_takes_a_child_above_it_below_it() {
    let (mut engine, cast) = framed(&[("F", &[], false), ("C1", &[], true), ("X", &[], false)]);

    resize_frame(&mut engine, &cast, (50.0, 50.0));

    assert_eq!(frame_of(&engine, &id(&cast, "C1")), Some(id(&cast, "F")));
    assert_eq!(stack(&engine, &cast), vec!["C1", "F", "X"]);
}

/// What a resize takes in goes in above the children the frame had, which keep their
/// order: the oracle's run is the previous children, then the newcomers
/// (`getElementsInResizingFrame`, `frame.ts@1118751f:283-377`).
#[test]
fn a_frame_resized_over_a_shape_puts_it_above_the_children_it_had() {
    let (mut engine, cast) = framed(&[
        ("N", &[], false),
        ("C1", &[], true),
        ("F", &[], false),
        ("X", &[], false),
    ]);

    // F (0, 0, 600, 200) to (0, 0, 260, 470): N at (20, 400) inside, X at (290, 400) not.
    resize_frame(&mut engine, &cast, (-340.0, 270.0));

    assert_eq!(frame_of(&engine, &id(&cast, "N")), Some(id(&cast, "F")));
    assert_eq!(frame_of(&engine, &id(&cast, "X")), None);
    assert_eq!(stack(&engine, &cast), vec!["C1", "N", "F", "X"]);
}

/// Moving in the stack is not a whole scene for the host: the delta carries the order,
/// every live id, and the shape. On a board of 20,000 the whole scene was 10.7MB for one
/// shape drawn into a frame (`cargo bench --bench editing -- frame_join`).
#[test]
fn a_shape_drawn_into_a_frame_reaches_the_host_as_a_delta_with_the_order() {
    let (mut engine, mut cast) = framed(&[("C1", &[], true), ("F", &[], false), ("X", &[], false)]);
    let _ = engine.drain_events();
    engine.set_tool(DrawTool::Rectangle);

    drag(&mut engine, (300.0, 50.0), (360.0, 120.0));
    selected_as(&engine, &mut cast, "N");
    let events = engine.drain_events();

    assert!(events.scene_json.is_none(), "not the whole scene");
    let delta = events.scene_delta.expect("a delta");
    assert_eq!(
        delta
            .updated
            .iter()
            .map(|el| el.id.clone())
            .collect::<Vec<_>>(),
        vec![id(&cast, "N")]
    );
    let order: Vec<String> = ["C1", "N", "F", "X"].iter().map(|n| id(&cast, n)).collect();
    assert_eq!(delta.order, Some(order));
}

/// Its place is part of its creation, which undo takes away and redo gives back: the
/// shape comes back directly below the frame, with no reorder recorded for it.
#[test]
fn undo_and_redo_of_a_shape_drawn_into_a_frame_keep_the_stack() {
    let (mut engine, mut cast) = framed(&[("C1", &[], true), ("F", &[], false), ("X", &[], false)]);
    engine.set_tool(DrawTool::Rectangle);
    drag(&mut engine, (300.0, 50.0), (360.0, 120.0));
    selected_as(&engine, &mut cast, "N");

    engine.undo();
    assert_eq!(stack(&engine, &cast), vec!["C1", "F", "X"]);
    engine.redo();
    assert_eq!(stack(&engine, &cast), vec!["C1", "N", "F", "X"]);
    assert_eq!(frame_of(&engine, &id(&cast, "N")), Some(id(&cast, "F")));
}
