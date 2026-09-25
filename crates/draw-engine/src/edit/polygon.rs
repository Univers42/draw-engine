//! Converting a line to a closed, fillable polygon and back.
//!
//! Excalidraw's `togglePolygon` action
//! (`packages/excalidraw/actions/actionLinearEditor.tsx@1118751f:106-212`): a panel button
//! and a keyboard action that closes the selected line's loop (or opens it) without
//! having to redraw it, ported the same way `flip.rs` ports its action.

use crate::scene::element::{DrawElement, DrawElementType};
use crate::scene::geometry::can_become_polygon;

/// How close an unclosed line's ends already have to be for the toggle to merge them into
/// one point, rather than appending a new point that duplicates the first.
///
/// Excalidraw's `LINE_POLYGON_POINT_MERGE_DISTANCE`
/// (`packages/common/src/constants.ts@1118751f:607`).
pub const LINE_POLYGON_POINT_MERGE_DISTANCE: f64 = 20.0;

/// Whether the toggle would do anything: at least one line selected, and every selected
/// element a line with four points or more.
///
/// Three points are not enough here even though [`can_become_polygon`] accepts them
/// unclosed: the toggle's own job is closing the loop, and the panel button that offers it
/// has to agree with `canBecomePolygon` once it has — the oracle's predicate asks for four
/// outright (`actionLinearEditor.tsx@1118751f:127-138`) rather than special-casing three.
pub fn can_toggle_polygon(elements: &[&DrawElement]) -> bool {
    !elements.is_empty()
        && elements
            .iter()
            .all(|el| el.kind == DrawElementType::Line && point_count(el) >= 4)
}

/// The patches that toggle `targets` to polygons, or back to open lines.
///
/// If any target is not already a polygon, every target becomes one; only when they all
/// already are does the toggle open them back up — `actionTogglePolygon.perform`
/// (`actionLinearEditor.tsx@1118751f:139-169`). Opening a polygon also clears its
/// background to transparent, so a line never carries a fill the panel no longer offers a
/// way to reach.
pub fn toggle_polygon(targets: &[DrawElement]) -> Vec<DrawElement> {
    let next_state = targets.iter().any(|el| !el.is_polygon());
    targets
        .iter()
        .filter(|el| el.kind == DrawElementType::Line)
        .map(|el| apply_polygon_state(el, next_state))
        .collect()
}

fn apply_polygon_state(element: &DrawElement, next_state: bool) -> DrawElement {
    let mut next = element.clone();
    if next_state {
        if let Some(points) = close_points(element.points.as_deref().unwrap_or(&[])) {
            next.points = Some(points);
        }
    } else {
        next.background_color = "transparent".into();
    }
    next.polygon = Some(next_state);
    next
}

/// Closes `points` into a loop: merges the last point into the first when they already
/// sit within [`LINE_POLYGON_POINT_MERGE_DISTANCE`], otherwise appends a new point at the
/// first's position — `toggleLinePolygonState`
/// (`packages/element/src/shape.ts@1118751f:1138-1180`). `None` when the points cannot
/// become a polygon at all ([`can_become_polygon`]).
fn close_points(points: &[[f64; 2]]) -> Option<Vec<[f64; 2]>> {
    if !can_become_polygon(points) {
        return None;
    }
    let mut points = points.to_vec();
    let first = points[0];
    let last = points[points.len() - 1];
    let distance = (first[0] - last[0]).hypot(first[1] - last[1]);
    if distance > LINE_POLYGON_POINT_MERGE_DISTANCE || points.len() < 4 {
        points.push(first);
    } else {
        let last_index = points.len() - 1;
        points[last_index] = first;
    }
    Some(points)
}

fn point_count(element: &DrawElement) -> usize {
    element.points.as_deref().map_or(0, <[_]>::len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::element::{create_element, DrawElementStyle, Geometry};

    fn line(points: Vec<[f64; 2]>) -> DrawElement {
        let mut el = create_element(
            DrawElementType::Line,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
            DrawElementStyle::default(),
            0.0,
        );
        el.points = Some(points);
        el
    }

    #[test]
    fn predicate_needs_four_points_on_every_selected_line() {
        let short = line(vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
        let long = line(vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 0.0]]);
        assert!(!can_toggle_polygon(&[&short]));
        assert!(can_toggle_polygon(&[&long]));
        assert!(!can_toggle_polygon(&[&long, &short]));
        assert!(!can_toggle_polygon(&[]));
    }

    #[test]
    fn closing_an_open_triangle_appends_the_first_point_and_flags_it() {
        let open = line(vec![[0.0, 0.0], [50.0, 0.0], [25.0, 50.0], [40.0, 45.0]]);
        let [patch] = toggle_polygon(&[open]).try_into().unwrap();
        assert!(patch.is_polygon());
        let points = patch.points.unwrap();
        assert_eq!(points.len(), 5);
        assert_eq!(*points.first().unwrap(), *points.last().unwrap());
    }

    #[test]
    fn closing_merges_a_near_first_last_point_instead_of_appending() {
        let almost_closed = line(vec![[0.0, 0.0], [50.0, 0.0], [25.0, 50.0], [1.0, 1.0]]);
        let [patch] = toggle_polygon(&[almost_closed]).try_into().unwrap();
        let points = patch.points.unwrap();
        assert_eq!(points.len(), 4, "merged onto the existing last point");
        assert_eq!(*points.last().unwrap(), [0.0, 0.0]);
    }

    #[test]
    fn toggling_a_polygon_back_opens_it_and_clears_the_background() {
        let mut closed = line(vec![[0.0, 0.0], [50.0, 0.0], [25.0, 50.0], [0.0, 0.0]]);
        closed.polygon = Some(true);
        closed.background_color = "#ffab00".into();
        let [patch] = toggle_polygon(&[closed]).try_into().unwrap();
        assert!(!patch.is_polygon());
        assert_eq!(patch.background_color, "transparent");
    }

    #[test]
    fn one_open_selected_among_polygons_closes_every_target() {
        let mut closed = line(vec![[0.0, 0.0], [50.0, 0.0], [25.0, 50.0], [0.0, 0.0]]);
        closed.polygon = Some(true);
        let open = line(vec![[0.0, 0.0], [50.0, 0.0], [25.0, 50.0], [40.0, 45.0]]);
        let patches = toggle_polygon(&[closed, open]);
        assert!(patches.iter().all(DrawElement::is_polygon));
    }
}
