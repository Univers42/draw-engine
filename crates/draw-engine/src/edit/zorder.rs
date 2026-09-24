//! The four z-order commands, and the one gathering that grouping does.
//!
//! Ported from `packages/element/src/zindex.ts`. The stack used to be shuffled as a plain
//! array, blind to groups: stepping forward slid a shape into the middle of a group, and
//! bring-to-front inside an entered group carried the element out of it.

use std::collections::HashSet;
use std::ops::RangeInclusive;

use crate::edit::group::{is_in_group, selected_group_for, with_labels};
use crate::scene::element::DrawElement;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZOrderMode {
    Front,
    Back,
    Forward,
    Backward,
}

impl ZOrderMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "front" => Some(Self::Front),
            "back" => Some(Self::Back),
            "forward" => Some(Self::Forward),
            "backward" => Some(Self::Backward),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Back => "back",
            Self::Forward => "forward",
            Self::Backward => "backward",
        }
    }
}

/// [`reorder_within`] with no group entered.
pub fn reorder_elements(
    live: &[DrawElement],
    ids: &HashSet<String>,
    mode: ZOrderMode,
) -> Vec<DrawElement> {
    reorder_within(live, ids, mode, None)
}

/// Moves `ids` through the stack, a group at a time.
///
/// - **Front / back** go to the end of the stack, or, inside an entered group, to the end
///   of that group — `shiftElementsToEnd`, `packages/element/src/zindex.ts:443-553`.
/// - **Forward / backward** step over one neighbour, and a neighbour in a group is the
///   whole group a click on it would take, so nothing lands between its members —
///   `getTargetIndex`, `zindex.ts:205-311`.
///
/// A label moves with its shape and is stepped over with it: selecting a shape never
/// selects its words, and a shape brought forward alone would cover them —
/// `includeBoundTextElement: true`, `zindex.ts:51-54`.
pub fn reorder_within(
    live: &[DrawElement],
    ids: &HashSet<String>,
    mode: ZOrderMode,
    editing: Option<&str>,
) -> Vec<DrawElement> {
    let moved = with_labels(live, ids);
    let Some(selected) = span(live, |el| moved.contains(&el.id)) else {
        return live.to_vec();
    };
    // Only a selection inside the entered group is confined to it. The oracle never holds
    // any other — select-all leaves the group, `packages/excalidraw/actions/
    // actionSelectAll.ts:49` — but this engine can, and confining a selection from
    // outside the group would stop it at the group's end, or not move it at all.
    let editing = editing.filter(|group| {
        live.iter()
            .filter(|el| ids.contains(&el.id))
            .all(|el| is_in_group(el, group))
    });
    let ends = editing
        .and_then(|group| span(live, |el| is_in_group(el, group)))
        .unwrap_or(0..=live.len() - 1);
    // Through the label of the shape at either end: one saved before labels joined
    // groups lies outside the group, and stopping short of it put the moved shape
    // between that shape and its words.
    let ends = with_label(live, &live[*ends.start()], *ends.start(), false)
        ..=with_label(live, &live[*ends.end()], *ends.end(), true);
    match mode {
        ZOrderMode::Front => to_end(live, &moved, *selected.start()..=*ends.end(), true),
        ZOrderMode::Back => to_end(live, &moved, *ends.start()..=*selected.end(), false),
        ZOrderMode::Forward => step(live, &moved, editing, true),
        ZOrderMode::Backward => step(live, &moved, editing, false),
    }
}

/// A new group's members, gathered directly under the topmost of them in the order they
/// had — `packages/excalidraw/actions/actionGroup.tsx:170-186`.
///
/// A group is one run of the stack. Left apart, whatever lay between the members would be
/// painted through the group, and stepping the group forward could only move its members
/// past their own neighbours.
pub fn gather(live: &[DrawElement], members: &HashSet<String>) -> Vec<DrawElement> {
    match span(live, |el| members.contains(&el.id)) {
        Some(range) => to_end(live, members, range, true),
        None => live.to_vec(),
    }
}

/// The first and last index of what `wanted` picks out, or `None` if nothing is.
fn span(
    live: &[DrawElement],
    wanted: impl Fn(&DrawElement) -> bool,
) -> Option<RangeInclusive<usize>> {
    let first = live.iter().position(&wanted)?;
    let last = live.iter().rposition(&wanted)?;
    Some(first..=last)
}

/// What of `moved` lies within `range`, stacked at its top (or bottom), each side keeping
/// its own order; nothing outside the range is touched. Only a label saved before labels
/// joined groups can lie outside — it is in none, so it may sit beyond the entered group —
/// and it stays where it is rather than stretching the range out of the group.
fn to_end(
    live: &[DrawElement],
    moved: &HashSet<String>,
    range: RangeInclusive<usize>,
    top: bool,
) -> Vec<DrawElement> {
    let (lo, hi) = (*range.start(), *range.end());
    let (inside, displaced): (Vec<&DrawElement>, Vec<&DrawElement>) =
        live[range].iter().partition(|el| moved.contains(&el.id));
    let middle = if top {
        displaced.into_iter().chain(inside)
    } else {
        inside.into_iter().chain(displaced)
    };
    live[..lo]
        .iter()
        .chain(middle)
        .chain(&live[hi + 1..])
        .cloned()
        .collect()
}

/// Each contiguous run of `moved` changes places with the unit beyond it —
/// `shiftElementsByOne`, `zindex.ts:357-441`.
///
/// The run nearest the end it is heading for goes first, so a run already moved never
/// shifts the indices of one still to go.
fn step(
    live: &[DrawElement],
    moved: &HashSet<String>,
    editing: Option<&str>,
    forward: bool,
) -> Vec<DrawElement> {
    let mut runs: Vec<RangeInclusive<usize>> = Vec::new();
    for (i, el) in live.iter().enumerate() {
        if !moved.contains(&el.id) {
            continue;
        }
        match runs.last_mut() {
            Some(run) if *run.end() + 1 == i => *run = *run.start()..=i,
            _ => runs.push(i..=i),
        }
    }
    if forward {
        runs.reverse();
    }
    let mut out = live.to_vec();
    for run in runs {
        let (lead, trail) = (*run.start(), *run.end());
        let length = trail + 1 - lead;
        let boundary = if forward { trail } else { lead };
        match target_index(&out, boundary, editing, forward) {
            Some(target) if forward && target > trail => out[lead..=target].rotate_left(length),
            Some(target) if !forward && target < lead => out[target..=trail].rotate_right(length),
            _ => {}
        }
    }
    out
}

/// Where the element at `boundary` steps to: past the next element, or past the whole
/// unit that element belongs to — `getTargetIndex`, `zindex.ts:205-311`.
///
/// Inside an entered group only its members are candidates, so nothing steps out of it.
/// The unit is the level a click on the neighbour would take, decided by
/// [`selected_group_for`] — the same rule, so an inner group is stepped over whole.
fn target_index(
    elements: &[DrawElement],
    boundary: usize,
    editing: Option<&str>,
    forward: bool,
) -> Option<usize> {
    let in_scope = |i: &usize| editing.is_none_or(|group| is_in_group(&elements[*i], group));
    let candidate = if forward {
        (boundary + 1..elements.len()).find(in_scope)
    } else {
        (0..boundary).rev().find(in_scope)
    }?;
    let next = &elements[candidate];
    // A sibling on the very same level inside the entered group is a unit on its own,
    // `zindex.ts:249-261`.
    let sibling = editing.is_some() && elements[boundary].group_ids == next.group_ids;
    let group = if sibling {
        None
    } else {
        selected_group_for(next, editing)
    };
    let Some(group) = group else {
        return Some(with_label(elements, next, candidate, forward));
    };
    let range = span(elements, |el| is_in_group(el, group))?;
    Some(if forward {
        *range.end()
    } else {
        *range.start()
    })
}

/// `at`, or the far side of `element`'s label (or of the shape it labels) if that lies
/// beyond: the two are one unit to step over — `getTargetIndexAccountingForBinding`,
/// `zindex.ts:91-130`.
fn with_label(elements: &[DrawElement], element: &DrawElement, at: usize, forward: bool) -> usize {
    let partner = element
        .container_id
        .as_deref()
        .or(element.bound_text_id.as_deref())
        .and_then(|id| elements.iter().position(|el| el.id == id));
    match partner {
        Some(other) if forward => other.max(at),
        Some(other) => other.min(at),
        None => at,
    }
}
