//! What a pointer can grab, and where.
//!
//! These cover the class of bug where the painter and the hit test disagree about the
//! selection UI. That disagreement is invisible by construction — the user aims at what
//! is drawn and gets whatever is hit-tested — so it needs tests rather than review.

mod common;
use common::*;
use draw_engine::*;

/// The layout the engine actually uses, at 1:1 zoom.
///
/// Excalidraw's numbers: a 4px frame margin, 8px handles whose inner edge is on that
/// frame, so centres sit 8px out and each reaches 4px.
fn layout() -> HandleLayout {
    HandleLayout::screen(8.0, 26.0, 1.0)
}

fn kinds(points: &[HandlePoint]) -> Vec<HandleKind> {
    points.iter().map(|p| p.kind).collect()
}

/// A shape wide and tall enough gets all eight resize handles plus rotation.
#[test]
fn a_large_shape_offers_every_handle() {
    let element = box_at(0.0, 0.0, 300.0, 200.0);
    let got = kinds(&selection_handles(&element, layout()));
    for kind in RESIZE_HANDLES {
        assert!(got.contains(&kind), "{kind:?} missing on a large shape");
    }
    assert!(got.contains(&HandleKind::Rotate));
}

/// Excalidraw's `minimumSizeForEightHandles`: on a short edge the side handle sits so
/// close to both corners that it cannot be aimed at, and only steals drags meant for a
/// corner. Below the threshold it is dropped.
#[test]
fn a_short_edge_drops_its_side_handles() {
    // 30 wide is under 5 * 8 = 40, so no north/south. 200 tall keeps east/west.
    let element = box_at(0.0, 0.0, 30.0, 200.0);
    let got = kinds(&selection_handles(&element, layout()));

    assert!(!got.contains(&HandleKind::N));
    assert!(!got.contains(&HandleKind::S));
    assert!(got.contains(&HandleKind::E));
    assert!(got.contains(&HandleKind::W));
    assert!(got.contains(&HandleKind::Nw));
    assert!(got.contains(&HandleKind::Rotate));
}

#[test]
fn a_tiny_shape_keeps_only_its_corners() {
    let element = box_at(0.0, 0.0, 12.0, 12.0);
    let got = kinds(&selection_handles(&element, layout()));
    assert_eq!(
        got,
        vec![
            HandleKind::Nw,
            HandleKind::Ne,
            HandleKind::Se,
            HandleKind::Sw,
            HandleKind::Rotate,
        ]
    );
}

/// The regression that started this: a shape has to be grabbable.
///
/// With the frame sitting *on* the outline, every point inside a small shape was also
/// inside a handle, so picking one up always resized it instead. Padding the frame
/// outward is what gives the element back its own interior.
#[test]
fn a_small_shape_can_be_grabbed_without_resizing_it() {
    let element = box_at(0.0, 0.0, 16.0, 16.0);
    let handles = selection_handles(&element, layout());
    // The centre, and every point on the element's own outline.
    let probes = [
        (8.0, 8.0),
        (0.0, 0.0),
        (16.0, 0.0),
        (16.0, 16.0),
        (0.0, 16.0),
        (8.0, 0.0),
        (0.0, 8.0),
    ];
    for (x, y) in probes {
        assert_eq!(
            hit_handle(&handles, x, y, layout().hit),
            None,
            "({x}, {y}) on a 16x16 shape should move it, not resize it"
        );
    }
}

/// The corner is still reachable — just from outside the shape, where the handle is
/// drawn.
#[test]
fn the_padded_corner_is_still_grabbable() {
    let element = box_at(0.0, 0.0, 16.0, 16.0);
    let handles = selection_handles(&element, layout());
    // Centres sit 8px out, so the north-west handle is at (-8, -8).
    assert_eq!(
        hit_handle(&handles, -8.0, -8.0, layout().hit),
        Some(HandleKind::Nw)
    );
}

/// The property the whole layout exists for: a handle must never reach back over the
/// element it belongs to. Excalidraw leaves a clear 4px ring between the outline and the
/// nearest thing that resizes.
#[test]
fn no_handle_reaches_back_inside_the_element() {
    let l = layout();
    assert!(
        l.hit <= l.handle_offset - l.frame_pad,
        "reach {} must not cross the {}px gap between the frame and the handle centres",
        l.hit,
        l.handle_offset - l.frame_pad
    );

    let element = box_at(0.0, 0.0, 300.0, 200.0);
    let handles = selection_handles(&element, l);
    // Every point on the element's own outline, including the edge midpoints where the
    // side handles live.
    let probes = [
        (150.0, 0.0),
        (150.0, 200.0),
        (0.0, 100.0),
        (300.0, 100.0),
        (0.0, 0.0),
        (300.0, 0.0),
        (300.0, 200.0),
        (0.0, 200.0),
    ];
    for (x, y) in probes {
        assert_eq!(
            hit_handle(&handles, x, y, l.hit),
            None,
            "({x}, {y}) is on the element's outline and must move it, not resize it"
        );
    }
}

/// Reach is radial, so a rotated element behaves the same in every direction. The old
/// box test was aligned to the world axes: it reached 1.41x as far diagonally as it did
/// along an axis, and did not turn with the shape.
#[test]
fn handle_reach_is_radial_not_axis_aligned() {
    let element = box_at(0.0, 0.0, 200.0, 200.0);
    let handles = selection_handles(&element, layout());
    let nw = handles.iter().find(|h| h.kind == HandleKind::Nw).unwrap();

    // Just inside the radius, on the diagonal.
    let d = 3.5 / std::f64::consts::SQRT_2;
    assert_eq!(
        hit_handle(&handles, nw.x + d, nw.y + d, 4.0),
        Some(HandleKind::Nw)
    );
    // Just outside it, on the same diagonal. A square test would have accepted this,
    // because each axis is still well within 4.
    let d = 4.5 / std::f64::consts::SQRT_2;
    assert_eq!(hit_handle(&handles, nw.x + d, nw.y + d, 4.0), None);
}

/// When two handles overlap, the nearer one wins rather than whichever came first in the
/// list — otherwise a corner permanently shadows the rotation handle above it.
#[test]
fn the_nearest_handle_wins() {
    let element = box_at(0.0, 0.0, 200.0, 100.0);
    let handles = selection_handles(&element, layout());
    let rotate = handles
        .iter()
        .find(|h| h.kind == HandleKind::Rotate)
        .unwrap();
    assert_eq!(
        hit_handle(&handles, rotate.x, rotate.y, 400.0),
        Some(HandleKind::Rotate),
        "a reach covering every handle must still pick the one under the pointer"
    );
}

/// A shape dragged out leftwards has negative width. Its "west" handle must stay west.
#[test]
fn negative_extent_does_not_mirror_the_handles() {
    let element = box_at(100.0, 100.0, -60.0, -40.0);
    let handles = selection_handles(&element, layout());
    let nw = handles.iter().find(|h| h.kind == HandleKind::Nw).unwrap();
    let se = handles.iter().find(|h| h.kind == HandleKind::Se).unwrap();
    assert!(nw.x < se.x, "north-west must sit left of south-east");
    assert!(nw.y < se.y, "north-west must sit above south-east");
}

/// The frame is drawn from the padded corners, and the handles are placed on them.
#[test]
fn the_frame_and_the_handles_share_their_corners() {
    let element = box_at(10.0, 20.0, 120.0, 80.0);
    let l = layout();
    // The handles are placed on the *handle* ring, which is further out than the frame.
    let corners = selection_corners_padded(&element, l.handle_offset);
    let handles = selection_handles(&element, l);

    for (kind, corner) in [
        (HandleKind::Nw, corners[0]),
        (HandleKind::Ne, corners[1]),
        (HandleKind::Se, corners[2]),
        (HandleKind::Sw, corners[3]),
    ] {
        let handle = handles.iter().find(|h| h.kind == kind).unwrap();
        assert_close(handle.x, corner.x);
        assert_close(handle.y, corner.y);
    }
}

/// Rotation carries the frame with it, so the handles stay on the shape's real corners.
#[test]
fn rotation_carries_the_handles() {
    let mut element = box_at(0.0, 0.0, 100.0, 100.0);
    element.angle = std::f64::consts::FRAC_PI_2;
    let handles = selection_handles(&element, layout());
    let nw = handles.iter().find(|h| h.kind == HandleKind::Nw).unwrap();
    // A quarter turn sends the top-left corner to the top-right.
    assert_close(nw.x, 100.0 + layout().handle_offset);
    assert_close(nw.y, -layout().handle_offset);
}
