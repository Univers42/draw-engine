//! Fixed segments: a segment the person dragged keeps its place while the rest of the
//! route follows the ends. The oracle's four handlers, `elbowArrow.ts@1118751f:113-900`:
//! renormalising after an indirect change, releasing a segment, moving one, and dragging
//! an end with segments fixed.

use super::heading::{
    heading_for_point, heading_for_point_is_horizontal, vector_to_heading, Heading,
};
use super::outline::{distance, points_equal, Pt, Target};
use super::route::{corners_of, ends, DEDUP_TRESHOLD};
use super::{normalize, Arrow, Board, FixedSegment, Options, Routed, BASE_PADDING};

/// `Array.prototype.at`: a negative index counts from the end.
fn at(points: &[Pt], index: isize) -> Option<Pt> {
    let i = if index < 0 {
        points.len() as isize + index
    } else {
        index
    };
    usize::try_from(i).ok().and_then(|i| points.get(i).copied())
}

fn global(arrow: &Arrow, p: Pt) -> Pt {
    [arrow.x + p[0], arrow.y + p[1]]
}

/// `handleSegmentRenormalization`: after a change the router did not see (a shape moved
/// under it, a paste), fold the points a fixed route no longer turns at, and the segments
/// shorter than a unit, keeping the fixed segments pinned to their new indices.
pub(super) fn renormalize(arrow: &Arrow, board: &Board) -> Routed {
    let Some(fixed) = &arrow.fixed_segments else {
        return Routed {
            x: Some(arrow.x),
            y: Some(arrow.y),
            points: Some(arrow.points.clone()),
            fixed_segments: Some(None),
            start_is_special: Some(arrow.start_is_special),
            end_is_special: Some(arrow.end_is_special),
            ..Routed::default()
        };
    };
    let mut segments = fixed.clone();
    let points: Vec<Pt> = arrow.points.iter().map(|&p| global(arrow, p)).collect();

    // Merge the points two collinear segments meet at.
    let mut merged: Vec<Pt> = Vec::with_capacity(points.len());
    for (i, &p) in points.iter().enumerate() {
        if i >= 2
            && heading_for_point(p, points[i - 1])
                == heading_for_point(points[i - 1], points[i - 2])
        {
            let previous = segments.iter().position(|s| s.index == i - 1);
            if let Some(k) = segments.iter().position(|s| s.index == i) {
                segments[k].start = [points[i - 2][0] - arrow.x, points[i - 2][1] - arrow.y];
            }
            if let Some(k) = previous {
                segments.remove(k);
            }
            merged.pop();
            for s in &mut segments {
                if s.index > i - 1 {
                    s.index -= 1;
                }
            }
        }
        merged.push(p);
    }

    // Fold away the segments shorter than a unit.
    let mut next: Vec<Pt> = Vec::with_capacity(merged.len());
    for (i, &p) in merged.iter().enumerate() {
        if i >= 3 && distance(merged[i - 2], merged[i - 1]) < DEDUP_TRESHOLD {
            // Both found before either is removed, as the oracle's two `splice`s are.
            let before = segments.iter().position(|s| s.index == i - 2);
            let previous = segments.iter().position(|s| s.index == i - 1);
            if let Some(k) = previous.filter(|&k| k < segments.len()) {
                segments.remove(k);
            }
            if let Some(k) = before.filter(|&k| k < segments.len()) {
                segments.remove(k);
            }
            next.truncate(next.len().saturating_sub(2));
            for s in &mut segments {
                if s.index > i - 2 {
                    s.index -= 2;
                }
            }
            let horizontal = heading_for_point_is_horizontal(p, merged[i - 1]);
            next.push([
                if horizontal { p[0] } else { merged[i - 2][0] },
                if horizontal { merged[i - 2][1] } else { p[1] },
            ]);
            continue;
        }
        next.push(p);
    }

    let last = next.len().saturating_sub(1);
    let kept: Vec<FixedSegment> = segments
        .into_iter()
        .filter(|s| s.index != 1 && s.index != last)
        .collect();
    if kept.is_empty() {
        let local: Vec<Pt> = next
            .iter()
            .map(|p| [p[0] - arrow.x, p[1] - arrow.y])
            .collect();
        let e = ends(arrow, board, &local, &Options::default());
        return normalize(
            corners_of(arrow.start_binding.is_some(), &e),
            Some(kept),
            None,
            None,
        );
    }
    normalize(
        next,
        Some(kept),
        arrow.start_is_special,
        arrow.end_is_special,
    )
}

/// `handleSegmentRelease`: route the stretch between the released segment's neighbours
/// afresh, keep everything outside it, and re-index what follows.
pub(super) fn release(arrow: &Arrow, fixed: &[FixedSegment], board: &Board) -> Routed {
    let unchanged = Routed {
        points: Some(arrow.points.clone()),
        ..Routed::default()
    };
    let old = arrow.fixed_segments.as_deref().unwrap_or(&[]);
    let Some(deleted) = old
        .iter()
        .position(|o| !fixed.iter().any(|s| s.index == o.index))
    else {
        return unchanged;
    };
    let deleted_index = old[deleted].index;
    let previous = deleted.checked_sub(1).and_then(|k| old.get(k));
    let following = old.get(deleted + 1);
    let last = *arrow.points.last().unwrap_or(&[0.0, 0.0]);
    let x = arrow.x + previous.map_or(0.0, |s| s.end[0]);
    let y = arrow.y + previous.map_or(0.0, |s| s.end[1]);
    let sub = Arrow {
        x,
        y,
        start_binding: if previous.is_some() {
            None
        } else {
            arrow.start_binding.clone()
        },
        end_binding: if following.is_some() {
            None
        } else {
            arrow.end_binding.clone()
        },
        start_arrowhead: false,
        end_arrowhead: false,
        ..arrow.clone()
    };
    let target = [
        arrow.x + following.map_or(last[0], |s| s.start[0]) - x,
        arrow.y + following.map_or(last[1], |s| s.start[1]) - y,
    ];
    let e = ends(&sub, board, &[[0.0, 0.0], target], &Options::default());
    let restored = normalize(
        corners_of(arrow.start_binding.is_some(), &e),
        Some(fixed.to_vec()),
        None,
        None,
    )
    .points
    .unwrap_or_default();
    if restored.len() < 2 {
        return Routed::default();
    }

    let mut points: Vec<Pt> = Vec::new();
    if let Some(p) = previous {
        points.extend(arrow.points.iter().take(p.index).map(|&q| global(arrow, q)));
    }
    let (ox, oy) = previous.map_or((0.0, 0.0), |s| (s.end[0], s.end[1]));
    points.extend(
        restored
            .iter()
            .map(|p| [arrow.x + ox + p[0], arrow.y + oy + p[1]]),
    );
    if let Some(f) = following {
        points.extend(arrow.points.iter().skip(f.index).map(|&q| global(arrow, q)));
    }

    let replaced = following.map_or(arrow.points.len(), |s| s.index) as i64
        - previous.map_or(0, |s| s.index) as i64
        - 1;
    let mut segments: Vec<FixedSegment> = fixed
        .iter()
        .map(|s| {
            if s.index > deleted_index {
                let index = s.index as i64 - replaced + (restored.len() as i64 - 1);
                FixedSegment {
                    index: index.max(0) as usize,
                    ..s.clone()
                }
            } else {
                s.clone()
            }
        })
        .collect();

    // Merge the joins that ended up straight, and double the ones that turned back.
    let mut simplified: Vec<Pt> = Vec::with_capacity(points.len());
    for (i, &p) in points.iter().enumerate() {
        if i > 0 && i + 1 < points.len() {
            let previous = heading_for_point(p, points[i - 1]);
            let next = heading_for_point(points[i + 1], p);
            if previous == next {
                for s in &mut segments {
                    if s.index > i {
                        s.index -= 1;
                    }
                }
                continue;
            }
            if previous == next.flip() {
                for s in &mut segments {
                    if s.index > i {
                        s.index += 1;
                    }
                }
                simplified.extend([p, p]);
                continue;
            }
        }
        simplified.push(p);
    }
    normalize(simplified, Some(segments), Some(false), Some(false))
}

/// The padding a first or last segment keeps from its shape when it is moved: 40, or
/// half the segment if that is shorter than 45, toward the side the heading points to.
fn padding(heading: Heading, length: f64) -> f64 {
    let short = length < BASE_PADDING + 5.0;
    let size = if short { length / 2.0 } else { BASE_PADDING };
    if heading.is_positive() {
        size
    } else {
        -size
    }
}

/// `handleSegmentMove`: the segment the person is dragging moves, its neighbours stretch
/// to meet it, and a first or last segment grows a new one so the end stays put.
pub(super) fn move_segment(
    arrow: &Arrow,
    fixed: &[FixedSegment],
    start_heading: Heading,
    end_heading: Heading,
    start_target: Option<&Target>,
    end_target: Option<&Target>,
) -> Routed {
    let old = arrow.fixed_segments.as_deref();
    let active = fixed
        .iter()
        .enumerate()
        .find_map(|(i, s)| match old.and_then(|o| o.get(i)) {
            Some(o) if o.index == s.index => {
                let x_moved = s.start[0] != o.start[0] && s.end[0] != o.end[0];
                let y_moved = s.start[1] != o.start[1] && s.end[1] != o.end[1];
                (x_moved != y_moved).then_some(i)
            }
            _ => Some(i),
        });
    let Some(active) = active else {
        return Routed {
            points: Some(arrow.points.clone()),
            ..Routed::default()
        };
    };
    let first_fixed = old.is_some_and(|o| o.iter().any(|s| s.index == 1));
    let last_fixed = old.is_some_and(|o| o.iter().any(|s| s.index == arrow.points.len() - 1));

    let mut fs = fixed.to_vec();
    let length = distance(fs[active].start, fs[active].end);
    if !first_fixed && fs[active].index == 1 && start_target.is_some() {
        let pad = padding(start_heading, length);
        let horizontal = start_heading.is_horizontal();
        let s = fs[active].start;
        fs[active].start = [
            s[0] + if horizontal { pad } else { 0.0 },
            s[1] + if horizontal { 0.0 } else { pad },
        ];
    }
    if !last_fixed && fs[active].index == arrow.points.len() - 1 && end_target.is_some() {
        let pad = padding(end_heading, length);
        let horizontal = end_heading.is_horizontal();
        let e = fs[active].end;
        fs[active].end = [
            e[0] + if horizontal { pad } else { 0.0 },
            e[1] + if horizontal { 0.0 } else { pad },
        ];
    }

    let mut segments: Vec<FixedSegment> = fs
        .iter()
        .map(|s| FixedSegment {
            index: s.index,
            start: global(arrow, s.start),
            end: global(arrow, s.end),
        })
        .collect();
    let mut points: Vec<Pt> = arrow.points.iter().map(|&p| global(arrow, p)).collect();
    let end_idx = segments[active].index;
    let start_idx = end_idx.wrapping_sub(1);
    let (start, end) = (segments[active].start, segments[active].end);
    let previous_horizontal = start_idx
        .checked_sub(1)
        .and_then(|k| points.get(k))
        .filter(|&&q| points.get(start_idx).is_some_and(|&p| !points_equal(p, q)))
        .map(|&q| heading_for_point_is_horizontal(q, points[start_idx]));
    let next_horizontal = points
        .get(end_idx + 1)
        .filter(|&&q| points.get(end_idx).is_some_and(|&p| !points_equal(p, q)))
        .map(|&q| heading_for_point_is_horizontal(q, points[end_idx]));
    if let Some(horizontal) = previous_horizontal {
        let dir = usize::from(horizontal);
        points[start_idx - 1][dir] = start[dir];
    }
    if let Some(p) = points.get_mut(start_idx) {
        *p = start;
    }
    if let Some(p) = points.get_mut(end_idx) {
        *p = end;
    }
    if let Some(horizontal) = next_horizontal {
        let dir = usize::from(horizontal);
        points[end_idx + 1][dir] = end[dir];
    }

    if let Some(k) = segments.iter().position(|s| s.index == start_idx) {
        let dir = usize::from(heading_for_point_is_horizontal(
            segments[k].end,
            segments[k].start,
        ));
        segments[k].start[dir] = start[dir];
        segments[k].end = start;
    }
    if let Some(k) = segments.iter().position(|s| s.index == end_idx + 1) {
        let dir = usize::from(heading_for_point_is_horizontal(
            segments[k].end,
            segments[k].start,
        ));
        segments[k].end[dir] = end[dir];
        segments[k].start = end;
    }

    let first = global(arrow, arrow.points[0]);
    if !first_fixed && start_idx == 0 {
        let horizontal = match start_target {
            Some(_) => start_heading.is_horizontal(),
            None => heading_for_point_is_horizontal(points[1], points[0]),
        };
        points.insert(
            0,
            [
                if horizontal { start[0] } else { first[0] },
                if horizontal { first[1] } else { start[1] },
            ],
        );
        if start_target.is_some() {
            points.insert(0, first);
        }
        for s in &mut segments {
            s.index += if start_target.is_some() { 2 } else { 1 };
        }
    }
    let last = global(arrow, arrow.points[arrow.points.len() - 1]);
    if !last_fixed && end_idx == arrow.points.len() - 1 {
        let horizontal = end_heading.is_horizontal();
        points.push([
            if horizontal { end[0] } else { last[0] },
            if horizontal { last[1] } else { end[1] },
        ]);
        if end_target.is_some() {
            points.push(last);
        }
    }

    let local = |p: Pt| [p[0] - arrow.x, p[1] - arrow.y];
    let segments = segments
        .into_iter()
        .map(|s| FixedSegment {
            index: s.index,
            start: local(s.start),
            end: local(s.end),
        })
        .collect();
    normalize(points, Some(segments), Some(false), Some(false))
}

/// `handleEndpointDrag`: an end moves with segments fixed. The fixed middle is kept as
/// it is; only the segments next to each end are redrawn, with a dongle (an extra corner
/// a padding out) when the end's shape would otherwise be left along its own side.
#[allow(clippy::too_many_arguments)]
pub(super) fn drag_endpoint(
    arrow: &Arrow,
    updated: &[Pt],
    fixed: &[FixedSegment],
    start_heading: Heading,
    end_heading: Heading,
    start_global: Pt,
    end_global: Pt,
    start_target: Option<&Target>,
    end_target: Option<&Target>,
) -> Routed {
    let mut start_special = arrow.start_is_special;
    let mut end_special = arrow.end_is_special;
    let n = updated.len();
    let globals: Vec<Pt> = updated
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            if i == 0 || i == n - 1 {
                global(arrow, p)
            } else {
                global(arrow, arrow.points.get(i).copied().unwrap_or(p))
            }
        })
        .collect();
    let mut segments: Vec<FixedSegment> = fixed
        .iter()
        .map(|s| FixedSegment {
            index: s.index,
            start: [
                arrow.x + (s.start[0] - updated[0][0]),
                arrow.y + (s.start[1] - updated[0][1]),
            ],
            end: [
                arrow.x + (s.end[0] - updated[0][0]),
                arrow.y + (s.end[1] - updated[0][1]),
            ],
        })
        .collect();

    let special = |v: Option<bool>| v == Some(true);
    let offset = 2 + usize::from(special(start_special));
    let end_offset = 2 + usize::from(special(end_special)) as isize;
    let mut points: Vec<Pt> = Vec::new();
    while ((points.len() + offset) as isize) < globals.len() as isize - end_offset {
        points.push(globals[points.len() + offset]);
    }

    let pad = |heading: Heading| {
        let positive = if heading.is_horizontal() {
            heading == Heading::Right
        } else {
            heading == Heading::Down
        };
        if positive {
            BASE_PADDING
        } else {
            -BASE_PADDING
        }
    };

    {
        let (Some(second), Some(third)) = (
            at(&globals, if special(start_special) { 2 } else { 1 }),
            at(&globals, if special(start_special) { 3 } else { 2 }),
        ) else {
            return Routed::default();
        };
        let start_horizontal = start_heading.is_horizontal();
        let second_horizontal =
            vector_to_heading([second[0] - third[0], second[1] - third[1]]).is_horizontal();
        if start_target.is_some() && start_horizontal == second_horizontal {
            let p = pad(start_heading);
            points.insert(
                0,
                [
                    if second_horizontal {
                        start_global[0] + p
                    } else {
                        third[0]
                    },
                    if second_horizontal {
                        third[1]
                    } else {
                        start_global[1] + p
                    },
                ],
            );
            points.insert(
                0,
                [
                    if start_horizontal {
                        start_global[0] + p
                    } else {
                        start_global[0]
                    },
                    if start_horizontal {
                        start_global[1]
                    } else {
                        start_global[1] + p
                    },
                ],
            );
            if !special(start_special) {
                start_special = Some(true);
                for s in &mut segments {
                    if s.index > 1 {
                        s.index += 1;
                    }
                }
            }
        } else {
            points.insert(
                0,
                [
                    if second_horizontal {
                        start_global[0]
                    } else {
                        second[0]
                    },
                    if second_horizontal {
                        second[1]
                    } else {
                        start_global[1]
                    },
                ],
            );
            if special(start_special) {
                start_special = Some(false);
                for s in &mut segments {
                    if s.index > 1 {
                        s.index -= 1;
                    }
                }
            }
        }
        points.insert(0, start_global);
    }

    {
        let len = globals.len() as isize;
        let (Some(second_to_last), Some(third_to_last)) = (
            at(&globals, len - if special(end_special) { 3 } else { 2 }),
            at(&globals, len - if special(end_special) { 4 } else { 3 }),
        ) else {
            return Routed::default();
        };
        let end_horizontal = end_heading.is_horizontal();
        let second_horizontal = heading_for_point_is_horizontal(third_to_last, second_to_last);
        if end_target.is_some() && end_horizontal == second_horizontal {
            let p = pad(end_heading);
            points.push([
                if second_horizontal {
                    end_global[0] + p
                } else {
                    third_to_last[0]
                },
                if second_horizontal {
                    third_to_last[1]
                } else {
                    end_global[1] + p
                },
            ]);
            points.push([
                if end_horizontal {
                    end_global[0] + p
                } else {
                    end_global[0]
                },
                if end_horizontal {
                    end_global[1]
                } else {
                    end_global[1] + p
                },
            ]);
            if !special(end_special) {
                end_special = Some(true);
            }
        } else {
            points.push([
                if second_horizontal {
                    end_global[0]
                } else {
                    second_to_last[0]
                },
                if second_horizontal {
                    second_to_last[1]
                } else {
                    end_global[1]
                },
            ]);
            if special(end_special) {
                end_special = Some(false);
            }
        }
    }
    points.push(end_global);

    let local = |p: Pt| [p[0] - start_global[0], p[1] - start_global[1]];
    let segments: Option<Vec<FixedSegment>> = segments
        .iter()
        .map(|s| {
            let start = *points.get(s.index.checked_sub(1)?)?;
            let end = *points.get(s.index)?;
            Some(FixedSegment {
                index: s.index,
                start: local(start),
                end: local(end),
            })
        })
        .collect();
    let Some(segments) = segments else {
        return Routed::default();
    };
    normalize(points, Some(segments), start_special, end_special)
}
