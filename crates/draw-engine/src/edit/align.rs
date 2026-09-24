use std::collections::{HashMap, HashSet};

use crate::camera::WorldBounds;
use crate::edit::group::selected_group_for;
use crate::scene::element::DrawElement;
use crate::scene::frame::is_frame;
use crate::scene::geometry::scene_bounds;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignMode {
    Left,
    CenterX,
    Right,
    Top,
    CenterY,
    Bottom,
}

impl AlignMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "left" => Some(Self::Left),
            "centerX" => Some(Self::CenterX),
            "right" => Some(Self::Right),
            "top" => Some(Self::Top),
            "centerY" => Some(Self::CenterY),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }
}

/// What align and distribute move as one: each group a click at this level would
/// select, or an element on its own — the oracle's `getSelectedElementsByGroup`
/// (`packages/element/src/groups.ts:417-466`). A selection that is exactly one group is
/// cut one level further in, so what lines up is what that group holds
/// (`isSingleSelectedGroupCase`, `groups.ts:424-426`).
///
/// A label is no unit: it follows its shape through the binding pass of the same commit.
/// No lock filter — `ids` is what the selection carries, locked group members included,
/// as a drag carries them (`crate::edit::carried_by`).
///
/// Empty when a frame is among `ids`: neither action is offered for one
/// (`actionAlign.tsx:39-52`, `actionDistribute.tsx:35-46`), since the frame would move
/// without what it holds, or what it holds out from under it.
pub fn units<'a>(
    elements: impl IntoIterator<Item = &'a DrawElement>,
    ids: &HashSet<String>,
    editing: Option<&str>,
) -> Vec<Vec<&'a DrawElement>> {
    let members: Vec<&DrawElement> = elements
        .into_iter()
        .filter(|el| ids.contains(&el.id) && !el.is_deleted && el.container_id.is_none())
        .collect();
    if members.iter().any(|el| is_frame(el)) {
        return Vec::new();
    }
    let cut = |editing: Option<&str>| {
        let mut keyed: Vec<(Option<&'a String>, Vec<&'a DrawElement>)> = Vec::new();
        // Where each group's unit is: a lookup rather than a walk of the units found so
        // far, which made a large grouped selection quadratic.
        let mut at: HashMap<&'a String, usize> = HashMap::new();
        for &element in &members {
            let group = selected_group_for(element, editing);
            match group.and_then(|group| at.get(group)) {
                Some(&index) => keyed[index].1.push(element),
                None => {
                    if let Some(group) = group {
                        at.insert(group, keyed.len());
                    }
                    keyed.push((group, vec![element]));
                }
            }
        }
        keyed
    };
    let keyed = cut(editing);
    let keyed = match keyed.as_slice() {
        [(Some(group), _)] => cut(Some(group.as_str())),
        _ => keyed,
    };
    keyed.into_iter().map(|(_, unit)| unit).collect()
}

fn bounds_of(unit: &[&DrawElement]) -> WorldBounds {
    scene_bounds(unit.iter().copied()).expect("a unit is never empty")
}

fn shifted<'a>(
    unit: &'a [&'a DrawElement],
    dx: f64,
    dy: f64,
) -> impl Iterator<Item = DrawElement> + 'a {
    unit.iter().map(move |element| {
        let mut element = (*element).clone();
        element.x += dx;
        element.y += dy;
        element
    })
}

/// Lines the units up against the box around all of them, each moved whole
/// (`alignElements`, `packages/element/src/align.ts:19-82`).
pub fn align_elements(
    elements: &[DrawElement],
    ids: &HashSet<String>,
    editing: Option<&str>,
    mode: AlignMode,
) -> Vec<DrawElement> {
    let units = units(elements, ids, editing);
    if units.len() < 2 {
        return Vec::new();
    }
    let bounds = bounds_of(&units.concat());
    units
        .iter()
        .flat_map(|unit| {
            let own = bounds_of(unit);
            let (dx, dy) = match mode {
                AlignMode::Left => (bounds.min_x - own.min_x, 0.0),
                AlignMode::Right => (bounds.max_x - own.max_x, 0.0),
                AlignMode::CenterX => (
                    (bounds.min_x + bounds.max_x) / 2.0 - (own.min_x + own.max_x) / 2.0,
                    0.0,
                ),
                AlignMode::Top => (0.0, bounds.min_y - own.min_y),
                AlignMode::Bottom => (0.0, bounds.max_y - own.max_y),
                AlignMode::CenterY => (
                    0.0,
                    (bounds.min_y + bounds.max_y) / 2.0 - (own.min_y + own.max_y) / 2.0,
                ),
            };
            shifted(unit, dx, dy)
        })
        .collect()
}

/// Spaces the units along `axis` ('x' or 'y') with equal gaps between them, each moved
/// whole — `distributeElements`, `packages/element/src/distribute.ts:19-114`.
///
/// Units that overlap too much for gaps (their extents add up to more than the whole)
/// fall back to the oracle's centre rule, transcribed with its quirk: the two units that
/// reach the selection's ends — found by their edges, not by their order — stay, and every
/// other one is stepped from the first's centre by the centre distance over n − 1.
pub fn distribute_elements(
    elements: &[DrawElement],
    ids: &HashSet<String>,
    editing: Option<&str>,
    axis: char,
) -> Vec<DrawElement> {
    let span = |b: &WorldBounds| {
        if axis == 'x' {
            (b.min_x, b.max_x)
        } else {
            (b.min_y, b.max_y)
        }
    };
    let mid = |b: &WorldBounds| {
        let (lo, hi) = span(b);
        (lo + hi) / 2.0
    };
    let mut units: Vec<(Vec<&DrawElement>, WorldBounds)> = units(elements, ids, editing)
        .into_iter()
        .map(|unit| {
            let own = bounds_of(&unit);
            (unit, own)
        })
        .collect();
    if units.len() < 3 {
        return Vec::new();
    }
    let n = units.len();
    // Stable, so units whose centres tie keep the order they were found in, as the
    // oracle's `Array.prototype.sort` does.
    units.sort_by(|a, b| mid(&a.1).total_cmp(&mid(&b.1)));
    let all: Vec<&DrawElement> = units.iter().flat_map(|(unit, _)| unit.clone()).collect();
    let (start, end) = span(&bounds_of(&all));
    let taken: f64 = units
        .iter()
        .map(|(_, own)| {
            let (lo, hi) = span(own);
            hi - lo
        })
        .sum();
    let gap = (end - start - taken) / (n - 1) as f64;
    let mut deltas = vec![0.0; n];
    if gap < 0.0 {
        let first = units.iter().position(|(_, own)| span(own).0 == start);
        let last = units.iter().position(|(_, own)| span(own).1 == end);
        let (Some(first), Some(last)) = (first, last) else {
            return Vec::new();
        };
        let step = (mid(&units[last].1) - mid(&units[first].1)) / (n - 1) as f64;
        let mut at = mid(&units[first].1);
        for (index, (_, own)) in units.iter().enumerate() {
            if index != first && index != last {
                at += step;
                deltas[index] = at - mid(own);
            }
        }
    } else {
        let mut at = start;
        for (index, (_, own)) in units.iter().enumerate() {
            let (lo, hi) = span(own);
            deltas[index] = at - lo;
            at += gap + hi - lo;
        }
    }
    units
        .iter()
        .zip(deltas)
        .flat_map(|((unit, _), delta)| {
            let (dx, dy) = if axis == 'x' {
                (delta, 0.0)
            } else {
                (0.0, delta)
            };
            shifted(unit, dx, dy)
        })
        .collect()
}
