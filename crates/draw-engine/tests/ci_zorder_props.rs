//! What z-order promises beyond matching the oracle case by case (`ci_zorder.rs`):
//! **every command is a permutation** — each element of the stack comes back exactly
//! once, unchanged, whatever the stack holds and whatever is selected.
//!
//! `ci_zorder.rs` holds seventy named stacks and states which order each must leave.
//! This states only that an order comes out at all — same length, same ids, the same
//! elements — over two hundred stacks nobody chose, each put through all four commands
//! in a row. Which element moved is the seventy cases' business, and a command that
//! moved the wrong one is invisible here; that is what this file is not.
//!
//! A failure prints the seed, the step, the command, the selection and the stack in the
//! token language of `ci_zorder.rs:254-303`, so the counterexample is a line that file's
//! `CASES` can be pasted into and shrunk from by hand.

mod common;
use common::*;
use draw_engine::edit::reorder_within;
use draw_engine::*;
use std::collections::HashSet;

use ZOrderMode::{Back, Backward, Forward, Front};

const MODES: [ZOrderMode; 4] = [Front, Back, Forward, Backward];

/// Seeds, cases. Every failure names the one it came from.
const SEEDS: u64 = 200;

/// A stack's width: wide enough that a group, a frame and a tombstone can all sit in it
/// with room left over, which is the shape a real board is and the shape the branches in
/// `zorder.rs` (`target_index`, `shift_accounting_for_frames`, `indices_to_move`) exist for.
const WIDTH: usize = 12;

/// The seed a case is drawn from: the text properties' own seed
/// (`ci_text_wrap_props.rs:107`), stepped by the case number so a printed seed names one
/// stack and not two hundred.
const BASE_SEED: u64 = 0x2545_f491_4f6c_dd1d;

/// xorshift64, the generator `ci_text_wrap_props.rs:107-113` already runs the text
/// properties on. A struct and not a closure so a failure can print what it drew.
///
/// `| 1` because xorshift is stuck at zero, and a zero seed would make every case
/// identical — a property test that only ever runs one input.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// `[0, n)`, and a coin as `below(2) == 1`.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// A stack of `WIDTH` elements, bottom first, wired as a board is: a frame with children
/// under it, a group with members, a label on the shape below it, and the occasional
/// tombstone — the four things `zorder.rs` reads a stack through, and the four its
/// branches are about.
///
/// A label's `bound_text_id` is set only when the label lies directly above its shape,
/// which is the same rule `populate` applies (`ci_zorder.rs:334-338`): every stack this
/// draws is one that file's token language can write down, and says out loud.
fn stack(rng: &mut Rng) -> (Vec<DrawElement>, Vec<String>) {
    let mut elements: Vec<DrawElement> = Vec::with_capacity(WIDTH);
    let mut groups: Vec<String> = Vec::new();
    for i in 0..WIDTH {
        let id = format!("e{i:02}");
        let mut element = match rng.below(8) {
            0 => {
                let mut frame = box_at(0.0, 0.0, 100.0, 100.0);
                frame.kind = DrawElementType::Frame;
                frame
            }
            1 if i > 0 => text_at(0.0, 0.0, 100.0, 100.0),
            2 | 3 => {
                let group = format!("g{}", rng.below(2));
                if !groups.contains(&group) {
                    groups.push(group.clone());
                }
                let mut element = box_at(0.0, 0.0, 100.0, 100.0);
                // Half the members sit in a group inside another, the nesting
                // `target_index` reads the level of (`zorder.rs:341-347`).
                if rng.below(2) == 0 {
                    element.group_ids = vec![group, "g0".into()];
                } else {
                    element.group_ids = vec![group];
                }
                element
            }
            4 if i > 0 => {
                let mut element = box_at(0.0, 0.0, 100.0, 100.0);
                element.is_deleted = true;
                element
            }
            _ => box_at(0.0, 0.0, 100.0, 100.0),
        };
        element.id = id;
        if element.kind != DrawElementType::Text && rng.below(4) == 0 {
            // A frame child, under a frame already in the stack.
            if let Some(frame) = elements
                .iter()
                .find(|el| el.kind == DrawElementType::Frame)
                .map(|el| el.id.clone())
            {
                element.frame_id = Some(frame);
            }
        }
        if element.kind == DrawElementType::Text {
            // Its shape: one of what is already on the stack, never a frame and never a
            // tombstone — an erased shape's label is not a label.
            let shape = elements
                .iter()
                .position(|el| el.kind != DrawElementType::Frame && !el.is_deleted);
            if let Some(shape) = shape {
                element.container_id = Some(elements[shape].id.clone());
            }
        }
        elements.push(element);
    }
    for i in 0..elements.len().saturating_sub(1) {
        if elements[i + 1].container_id.as_deref() == Some(elements[i].id.as_str()) {
            elements[i].bound_text_id = Some(elements[i + 1].id.clone());
        }
    }
    (elements, groups)
}

/// A non-empty selection, drawn into a `Vec` and collected afterwards.
///
/// `reorder_elements` takes `&HashSet<String>` (`zorder.rs:64`) because the engine's own
/// selection is one (`engine/mod.rs:196`), and a `HashSet` iterates in an arbitrary order
/// that differs between runs. That order cannot reach this property: `zorder.rs` only
/// ever asks the set `ids.contains(..)` (`:196`, `:200`) and never walks it, so a set
/// built from any order gives the same answer — which is why nothing below can be flaky.
/// What the seed does fix is *which* ids are selected, and a `Vec` is what makes that
/// reproducible from the printed seed.
fn selection(rng: &mut Rng, elements: &[DrawElement]) -> HashSet<String> {
    let mut ids: Vec<String> = (0..WIDTH)
        .map(|i| format!("e{i:02}"))
        .filter(|_| rng.below(2) == 0)
        .collect();
    if ids.is_empty() {
        ids.push(elements[rng.below(elements.len())].id.clone());
    }
    ids.into_iter().collect()
}

/// The stack as `ci_zorder.rs`'s own token language writes it: `e00*e01/g1#-`, so a
/// failure is a line that file's `CASES` can be pasted into rather than a Rust literal to
/// be retyped.
fn tokens(elements: &[DrawElement], selected: &HashSet<String>) -> String {
    elements
        .iter()
        .map(|element| {
            let mut token = element.id.clone();
            if selected.contains(&element.id) {
                token.push('*');
            }
            if element.is_deleted {
                token.push('-');
            }
            if element.kind == DrawElementType::Frame {
                token.push('#');
            }
            if !element.group_ids.is_empty() {
                token.push('/');
                token.push_str(&element.group_ids.join(","));
            }
            if let Some(container) = &element.container_id {
                token.push('>');
                token.push_str(container);
            }
            if let Some(frame) = &element.frame_id {
                token.push('@');
                token.push_str(frame);
            }
            token
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The one assertion: the same elements, in some order, each exactly once.
///
/// Length, then the ids as a sorted pair — a set would forgive a duplicate and a drop at
/// once, which is precisely the failure being looked for — then each element against the
/// one that came in under that id, because a command that restacks the scene is allowed
/// to change nothing else about it.
fn assert_permutation(where_: &str, before: &[DrawElement], after: &[DrawElement]) {
    assert_eq!(
        after.len(),
        before.len(),
        "{where_}: {} elements in, {} out",
        before.len(),
        after.len()
    );
    let mut in_ids: Vec<&str> = before.iter().map(|el| el.id.as_str()).collect();
    let mut out_ids: Vec<&str> = after.iter().map(|el| el.id.as_str()).collect();
    in_ids.sort_unstable();
    out_ids.sort_unstable();
    assert_eq!(in_ids, out_ids, "{where_}: the stack is not a permutation");
    for element in after {
        let original = before
            .iter()
            .find(|el| el.id == element.id)
            .unwrap_or_else(|| panic!("{where_}: {} came from nowhere", element.id));
        assert_eq!(
            element, original,
            "{where_}: {} came back changed",
            element.id
        );
    }
}

/// Every command, on every generated stack, returns the stack it was given.
///
/// The four modes run one after another on the same stack, each on the last one's
/// output: a user holds one key down, and a bug that needs the second press to appear
/// is a bug this misses if every command starts from the same place.
#[test]
fn every_command_returns_the_same_elements_once_each() {
    for seed in 1..=SEEDS {
        let mut rng = Rng::new(BASE_SEED.wrapping_mul(seed));
        let (start, _) = stack(&mut rng);
        let selected = selection(&mut rng, &start);
        let mut current = start;
        for (step, mode) in MODES.into_iter().enumerate() {
            let next = reorder_elements(&current, &selected, mode);
            let where_ = format!(
                "seed {seed} step {} {}:\n  stack   {}\n  outcome {}",
                step + 1,
                mode.as_str(),
                tokens(&current, &selected),
                tokens(&next, &selected)
            );
            assert_permutation(&where_, &current, &next);
            current = next;
        }
    }
}

/// The same, inside a group that has been entered — the fourth argument of
/// `reorder_within`, where a step that finds nothing to step over is abandoned
/// mid-run (`zorder.rs:406-408`) and the run it had already gathered has to be put back
/// exactly as it was.
///
/// A stack whose generator drew no group has no group to enter, so those seeds are
/// counted and said out loud rather than quietly passed over.
#[test]
fn a_command_inside_an_entered_group_keeps_the_whole_stack() {
    let mut entered = 0;
    for seed in 1..=SEEDS {
        let mut rng = Rng::new(BASE_SEED.wrapping_mul(seed));
        let (start, groups) = stack(&mut rng);
        let Some(group) = groups.first().cloned() else {
            continue;
        };
        let selected = selection(&mut rng, &start);
        let mut current = start;
        for (step, mode) in MODES.into_iter().enumerate() {
            let next = reorder_within(&current, &selected, mode, Some(&group));
            let where_ = format!(
                "seed {seed} step {} {} in {group}:\n  stack   {}\n  outcome {}",
                step + 1,
                mode.as_str(),
                tokens(&current, &selected),
                tokens(&next, &selected)
            );
            assert_permutation(&where_, &current, &next);
            current = next;
        }
        entered += 1;
    }
    assert!(
        entered * 2 > SEEDS as usize,
        "only {entered} stacks drew a group"
    );
}
