//! The four z-order commands, and the one gathering that grouping does.
//!
//! The commands are a transcription of `packages/element/src/zindex.ts@1118751f`, function
//! for function, and pinned by that file's own tests (`tests/ci_zorder.rs`). What they
//! keep, as the oracle does:
//!
//! - a group is one block to step over — the outermost group, or inside an entered group
//!   the next level in — and inside an entered group nothing leaves it;
//! - a frame and its children are one block to step over, a selected frame takes its
//!   children with it, and a child moved alone stays within its frame's range;
//! - a label moves with its shape and is stepped over with it.
//!
//! Two things are read differently from the oracle's elements, because this engine's
//! elements say them differently; on a board the oracle made, both read the same:
//!
//! - a label's frame and groups are its shape's. Here a label carries no `frameId` — the
//!   shape holds the membership for both (`scene/frame.rs`, `can_belong_to_frame`) — and
//!   one saved before labels joined groups carries none; the oracle's bound text carries
//!   its container's (`frame.ts:562-583`, `App.tsx:7081`);
//! - a label whose shape is not in the stack (not live) is read as unlabelled, where the
//!   oracle would still find the deleted shape (`zindex.ts:94-107`).
//!
//! The stack is rearranged as positions, never as elements: one pass per command and one
//! clone of each element at the end, however many runs the selection is cut into.

use std::collections::{HashMap, HashSet};
use std::ops::RangeInclusive;

use crate::scene::element::DrawElement;
use crate::scene::frame::is_frame;

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

/// `elements` restacked by `mode`, with `ids` selected and `editing` the group entered.
pub fn reorder_within(
    elements: &[DrawElement],
    ids: &HashSet<String>,
    mode: ZOrderMode,
    editing: Option<&str>,
) -> Vec<DrawElement> {
    let refs: Vec<&DrawElement> = elements.iter().collect();
    reorder_positions(&refs, ids, mode, editing)
        .into_iter()
        .map(|i| elements[i].clone())
        .collect()
}

/// The new stack as indices into `elements`, bottom first: `moveOneLeft`, `moveOneRight`,
/// `moveAllLeft`, `moveAllRight` (`zindex.ts:626-664`).
///
/// `ids` is the selection as the oracle holds it (`selectedElementIds`); the labels of
/// selected shapes and the children of selected frames are read in here, as
/// `getSelectedElements(…, { includeBoundTextElement, includeElementsInFrames })` does
/// (`packages/element/src/selection.ts:161-213`).
pub fn reorder_positions(
    elements: &[&DrawElement],
    ids: &HashSet<String>,
    mode: ZOrderMode,
    editing: Option<&str>,
) -> Vec<usize> {
    let mut stack = Stack::new(elements, editing);
    let selected = stack.selected(ids);
    match mode {
        ZOrderMode::Backward => stack.shift_by_one(&selected, Direction::Left),
        ZOrderMode::Forward => stack.shift_by_one(&selected, Direction::Right),
        ZOrderMode::Back => stack.shift_accounting_for_frames(&selected, Direction::Left),
        ZOrderMode::Front => stack.shift_accounting_for_frames(&selected, Direction::Right),
    }
    stack.order
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    /// Towards the bottom of the stack.
    Left,
    /// Towards the top.
    Right,
}

/// The stack being rearranged, and what the oracle reads off each element.
struct Stack<'a> {
    elements: &'a [&'a DrawElement],
    editing: Option<&'a str>,
    /// The element at each position, as an index into `elements`.
    order: Vec<usize>,
    /// Where each element is — the inverse of `order`, kept in step with it.
    at: Vec<usize>,
    by_id: HashMap<&'a str, usize>,
    /// `frameId` and `groupIds` as the oracle reads them: a label's are its shape's.
    frame: Vec<Option<&'a str>>,
    groups: Vec<&'a [String]>,
    /// The members of each group, and of each frame with the frame itself
    /// (`isOfTargetFrame`, `zindex.ts:24-26`): a block's ends are found from its members
    /// rather than from a scan of the whole stack per step.
    group_members: HashMap<&'a str, Vec<usize>>,
    frame_members: HashMap<&'a str, Vec<usize>>,
}

impl<'a> Stack<'a> {
    fn new(elements: &'a [&'a DrawElement], editing: Option<&'a str>) -> Self {
        let by_id: HashMap<&str, usize> = elements
            .iter()
            .enumerate()
            .map(|(i, el)| (el.id.as_str(), i))
            .collect();
        let shape = |el: &DrawElement| {
            el.container_id
                .as_deref()
                .and_then(|id| by_id.get(id))
                .map(|&i| elements[i])
        };
        let frame: Vec<Option<&str>> = elements
            .iter()
            .map(|el| shape(el).unwrap_or(el).frame_id.as_deref())
            .collect();
        let groups: Vec<&[String]> = elements
            .iter()
            .map(|el| shape(el).unwrap_or(el).group_ids.as_slice())
            .collect();
        let mut group_members: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut frame_members: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, el) in elements.iter().enumerate() {
            for group in groups[i] {
                group_members.entry(group.as_str()).or_default().push(i);
            }
            if let Some(owner) = frame[i] {
                frame_members.entry(owner).or_default().push(i);
            }
            if is_frame(el) {
                frame_members.entry(el.id.as_str()).or_default().push(i);
            }
        }
        Self {
            elements,
            editing,
            order: (0..elements.len()).collect(),
            at: (0..elements.len()).collect(),
            by_id,
            frame,
            groups,
            group_members,
            frame_members,
        }
    }

    fn deleted(&self, i: usize) -> bool {
        self.elements[i].is_deleted
    }

    fn in_group(&self, i: usize, group: &str) -> bool {
        self.groups[i].iter().any(|g| g == group)
    }

    /// `getSelectedElements` with `includeBoundTextElement` and `includeElementsInFrames`
    /// (`selection.ts:161-213`): the selected live elements, the live labels of selected
    /// shapes, and every child of a selected frame.
    fn selected(&self, ids: &HashSet<String>) -> HashSet<usize> {
        let mut out: HashSet<usize> = HashSet::new();
        for (i, el) in self.elements.iter().enumerate() {
            if ids.contains(&el.id) {
                if !el.is_deleted {
                    out.insert(i);
                }
            } else if !el.is_deleted && el.container_id.as_ref().is_some_and(|c| ids.contains(c)) {
                out.insert(i);
            }
        }
        let frames: Vec<&str> = out
            .iter()
            .filter(|&&i| is_frame(self.elements[i]))
            .map(|&i| self.elements[i].id.as_str())
            .collect();
        for frame in frames {
            let members = self.frame_members.get(frame).into_iter().flatten();
            out.extend(members.filter(|&&i| self.frame[i] == Some(frame)));
        }
        out
    }

    /// `getIndicesToMove` (`zindex.ts:36-70`): the positions of `moving`, and of deleted
    /// elements lying between two of them.
    ///
    /// Scanned from the lowest of them to the highest, where the oracle scans the whole
    /// stack: nothing outside can be taken, and a Bring to front over the children of many
    /// frames makes one pass per frame.
    fn indices_to_move(&self, moving: &HashSet<usize>) -> Vec<usize> {
        let Some((lowest, highest)) = self.ends(Some(moving)) else {
            return Vec::new();
        };
        let mut selected = Vec::new();
        let mut deleted = Vec::new();
        let mut include_deleted = None;
        for p in lowest..=highest {
            let i = self.order[p];
            if moving.contains(&i) {
                selected.append(&mut deleted);
                selected.push(p);
                include_deleted = Some(p + 1);
            } else if self.deleted(i) && include_deleted == Some(p) {
                include_deleted = Some(p + 1);
                deleted.push(p);
            } else {
                deleted.clear();
            }
        }
        selected
    }

    /// The first and last position of `members`.
    fn ends<'m>(
        &self,
        members: Option<impl IntoIterator<Item = &'m usize>>,
    ) -> Option<(usize, usize)> {
        let mut positions = members?.into_iter().map(|&i| self.at[i]);
        let first = positions.next()?;
        Some(positions.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }

    /// `getContiguousFrameRangeElements` (`zindex.ts:129-147`), as its first and last
    /// position.
    fn frame_range(&self, frame: &str) -> Option<(usize, usize)> {
        self.ends(self.frame_members.get(frame))
    }

    /// `getElementsInGroup` (`packages/element/src/groups.ts:289-304`), as the first and
    /// last position of the group's members.
    fn group_range(&self, group: &str) -> Option<(usize, usize)> {
        self.ends(self.group_members.get(group))
    }

    /// `getTargetIndexAccountingForBinding` (`zindex.ts:88-127`): a shape and its label
    /// are one unit, so the step goes to the far one of the two.
    fn accounting_for_binding(&self, candidate: usize, direction: Direction) -> Option<usize> {
        let next = self.elements[self.order[candidate]];
        let partner = match next.container_id.as_deref() {
            Some(container) => container,
            None => next.bound_text_id.as_deref()?,
        };
        let partner = self.at[*self.by_id.get(partner)?];
        Some(match direction {
            Direction::Left => partner.min(candidate),
            Direction::Right => partner.max(candidate),
        })
    }

    /// `getTargetIndex` (`zindex.ts:193-299`): where the run ending at `boundary` steps
    /// to, or `None` where the oracle returns -1.
    fn target_index(
        &self,
        boundary: usize,
        direction: Direction,
        containing_frame: Option<&str>,
    ) -> Option<usize> {
        let source = self.order[boundary];
        let wanted = |p: &usize| {
            let i = self.order[*p];
            !self.deleted(i)
                && match (containing_frame, self.editing) {
                    (Some(frame), _) => self.frame[i] == Some(frame),
                    // Inside an entered group the closest member, whatever lies between
                    // (`zindex.ts:214-218`).
                    (None, Some(group)) => self.in_group(i, group),
                    (None, None) => true,
                }
        };
        // `findLastIndex` from `max(0, boundary - 1)`: at the bottom that is the boundary
        // itself, and the step then comes to nothing (`zindex.ts:222-229`).
        let candidate = match direction {
            Direction::Left => (0..=boundary.saturating_sub(1)).rev().find(wanted),
            Direction::Right => (boundary + 1..self.order.len()).find(wanted),
        }?;
        let next = self.order[candidate];
        let binding = || {
            self.accounting_for_binding(candidate, direction)
                .or(Some(candidate))
        };

        if let Some(group) = self.editing {
            // `groupIds.join("")`, as the oracle compares them (`zindex.ts:240`).
            if self.groups[source].concat() == self.groups[next].concat() {
                return binding();
            }
            if !self.in_group(next, group) {
                return None;
            }
        }

        if containing_frame.is_none()
            && (self.frame[next].is_some() || is_frame(self.elements[next]))
        {
            let frame = self.frame[next].unwrap_or(self.elements[next].id.as_str());
            let (first, last) = self.frame_range(frame)?;
            return Some(match direction {
                Direction::Left => first,
                Direction::Right => last,
            });
        }

        if self.groups[next].is_empty() {
            return binding();
        }

        // The block a click on `next` would take: its outermost group, or inside the
        // entered group the level just inside it — none when `next` sits directly in it.
        let sibling_group = match self.editing {
            Some(group) => {
                let level = self.groups[next].iter().position(|g| g == group)?;
                level.checked_sub(1).map(|inner| &self.groups[next][inner])
            }
            None => self.groups[next].last(),
        };
        match sibling_group.and_then(|group| self.group_range(group)) {
            Some((first, last)) => Some(match direction {
                Direction::Left => first,
                Direction::Right => last,
            }),
            None => Some(candidate),
        }
    }

    /// Rotates `range` of the stack by `by`, left or right, and keeps `at` in step.
    fn rotate(&mut self, range: RangeInclusive<usize>, by: usize, direction: Direction) {
        let (start, end) = (*range.start(), *range.end());
        match direction {
            Direction::Left => self.order[range].rotate_right(by),
            Direction::Right => self.order[range].rotate_left(by),
        }
        for p in start..=end {
            self.at[self.order[p]] = p;
        }
    }

    /// `shiftElementsByOne` (`zindex.ts:345-429`): each contiguous run of what moves
    /// changes places with the unit beyond it, the run nearest the end it heads for first.
    ///
    /// The positions of the runs are taken before any moves, as the oracle's are: a run
    /// moved rearranges only the stack beyond it, so the ones still to go stay put.
    fn shift_by_one(&mut self, moving: &HashSet<usize>, direction: Direction) {
        let indices = self.indices_to_move(moving);
        let selected_frames: HashSet<&str> = indices
            .iter()
            .map(|&p| self.elements[self.order[p]])
            .filter(|el| is_frame(el))
            .map(|el| el.id.as_str())
            .collect();
        let mut runs: Vec<RangeInclusive<usize>> = Vec::new();
        for &p in &indices {
            match runs.last_mut() {
                Some(run) if *run.end() + 1 == p => *run = *run.start()..=p,
                _ => runs.push(p..=p),
            }
        }
        if direction == Direction::Right {
            runs.reverse();
        }
        for run in runs {
            let (leading, trailing) = (*run.start(), *run.end());
            let boundary = match direction {
                Direction::Left => leading,
                Direction::Right => trailing,
            };
            // A child whose frame moves too is not confined to it (`zindex.ts:372-377`).
            let containing_frame = if run.clone().any(|p| {
                self.frame[self.order[p]].is_some_and(|frame| selected_frames.contains(frame))
            }) {
                None
            } else {
                self.frame[self.order[boundary]]
            };
            let Some(target) = self.target_index(boundary, direction, containing_frame) else {
                continue;
            };
            let length = trailing + 1 - leading;
            match direction {
                Direction::Left if target < leading => {
                    self.rotate(target..=trailing, length, direction);
                }
                Direction::Right if target > trailing => {
                    self.rotate(leading..=target, length, direction);
                }
                // At the boundary: the oracle's `boundaryIndex === targetIndex` no-op.
                _ => {}
            }
        }
    }

    /// `shiftElementsToEnd` (`zindex.ts:431-541`): `moving` to the bottom or the top of
    /// the stack — of its frame's range, or of the entered group — each side keeping its
    /// order. Refused, as the oracle's is, when something to move lies outside that range.
    fn shift_to_end(
        &mut self,
        moving: &HashSet<usize>,
        direction: Direction,
        containing_frame: Option<&str>,
    ) {
        let indices = self.indices_to_move(moving);
        let (Some(&lowest), Some(&highest)) = (indices.first(), indices.last()) else {
            return;
        };
        let (leading, trailing) = match direction {
            Direction::Left => {
                let leading = if let Some(frame) = containing_frame {
                    // `-1` becomes `0`, `zindex.ts:493-495`.
                    self.frame_range(frame).map_or(0, |(first, _)| first)
                } else if let Some(group) = self.editing {
                    let Some((first, _)) = self.group_range(group) else {
                        return;
                    };
                    first
                } else {
                    0
                };
                (leading, highest)
            }
            Direction::Right => {
                let trailing = if let Some(frame) = containing_frame {
                    let Some((_, last)) = self.frame_range(frame) else {
                        return;
                    };
                    last
                } else if let Some(group) = self.editing {
                    let Some((_, last)) = self.group_range(group) else {
                        return;
                    };
                    last
                } else {
                    self.order.len() - 1
                };
                (lowest, trailing)
            }
        };
        if leading > trailing || lowest < leading || highest > trailing {
            return;
        }
        let moved: HashSet<usize> = indices.into_iter().collect();
        let (targets, displaced): (Vec<usize>, Vec<usize>) =
            (leading..=trailing).partition(|p| moved.contains(p));
        let (first, second) = match direction {
            Direction::Left => (targets, displaced),
            Direction::Right => (displaced, targets),
        };
        let middle: Vec<usize> = first
            .into_iter()
            .chain(second)
            .map(|p| self.order[p])
            .collect();
        self.order.splice(leading..=trailing, middle);
        for p in leading..=trailing {
            self.at[self.order[p]] = p;
        }
    }

    /// `shiftElementsAccountingForFrames` (`zindex.ts:543-621`): the children of each
    /// frame that is not itself moving go to the end of that frame's range, then
    /// everything else to the end of the stack (or of the entered group).
    fn shift_accounting_for_frames(&mut self, moving: &HashSet<usize>, direction: Direction) {
        let whole_frames: HashSet<&str> = moving
            .iter()
            .map(|&i| self.elements[i])
            .filter(|el| is_frame(el))
            .map(|el| el.id.as_str())
            .collect();
        let mut regular: HashSet<usize> = HashSet::new();
        // In the order each frame is first met, as the oracle's `Map` iterates.
        let mut children: Vec<(&str, HashSet<usize>)> = Vec::new();
        for &i in &self.order {
            if !moving.contains(&i) {
                continue;
            }
            match self.frame[i] {
                Some(frame) if !is_frame(self.elements[i]) && !whole_frames.contains(frame) => {
                    match children.iter_mut().find(|(id, _)| *id == frame) {
                        Some((_, set)) => {
                            set.insert(i);
                        }
                        None => children.push((frame, HashSet::from([i]))),
                    }
                }
                _ => {
                    regular.insert(i);
                }
            }
        }
        for (frame, set) in &children {
            self.shift_to_end(set, direction, Some(frame));
        }
        self.shift_to_end(&regular, direction, None);
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
/// its own order; nothing outside the range is touched.
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
