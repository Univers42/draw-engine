#![allow(clippy::cloned_ref_to_slice_refs)]

use std::collections::HashSet;

use draw_engine::*;

fn box_at(x: f64, y: f64, w: f64, h: f64) -> DrawElement {
    create_element_default(
        DrawElementType::Rectangle,
        Geometry {
            x,
            y,
            width: w,
            height: h,
        },
    )
}

fn arrow(x1: f64, y1: f64, x2: f64, y2: f64) -> DrawElement {
    let mut el = create_element_default(
        DrawElementType::Arrow,
        Geometry {
            x: x1,
            y: y1,
            width: x2 - x1,
            height: y2 - y1,
        },
    );
    el.points = Some(vec![[0.0, 0.0], [x2 - x1, y2 - y1]]);
    el
}

fn ids(elements: &[DrawElement]) -> HashSet<String> {
    elements.iter().map(|e| e.id.clone()).collect()
}

#[test]
fn clipboard_remaps_ids_and_keeps_bindings() {
    let a = box_at(0.0, 0.0, 100.0, 60.0);
    let b = box_at(300.0, 0.0, 100.0, 60.0);
    let mut link = arrow(100.0, 30.0, 300.0, 30.0);
    link.start_binding = Some(a.id.clone());
    link.end_binding = Some(b.id.clone());
    let mut label = create_element_default(
        DrawElementType::Text,
        Geometry {
            x: 10.0,
            y: 10.0,
            width: 40.0,
            height: 20.0,
        },
    );
    label.text = Some("hi".into());
    label.container_id = Some(a.id.clone());
    let mut shape = a;
    shape.bound_text_id = Some(label.id.clone());
    let scene = vec![shape.clone(), b.clone(), link.clone(), label.clone()];
    let json = serialize_selection(&scene, &ids(&[shape.clone(), b, link])).unwrap();
    let copies = materialize_elements(&json, 20.0, 10.0, 5.0).unwrap();
    assert_eq!(copies.len(), 4);
    let old: HashSet<_> = scene.iter().map(|e| e.id.clone()).collect();
    for copy in &copies {
        assert!(!old.contains(&copy.id));
        assert_eq!(copy.version, 1);
        assert_eq!(copy.updated, 5.0);
    }
    let copy_a = copies.iter().find(|e| e.bound_text_id.is_some()).unwrap();
    let copy_label = copies
        .iter()
        .find(|e| e.kind == DrawElementType::Text)
        .unwrap();
    let copy_link = copies
        .iter()
        .find(|e| e.kind == DrawElementType::Arrow)
        .unwrap();
    assert_eq!(copy_a.x, shape.x + 20.0);
    assert_eq!(copy_label.container_id.as_deref(), Some(copy_a.id.as_str()));
    assert_eq!(
        copy_a.bound_text_id.as_deref(),
        Some(copy_label.id.as_str())
    );
    assert_eq!(copy_link.start_binding.as_deref(), Some(copy_a.id.as_str()));
    assert!(copy_link.end_binding.is_some());
}

#[test]
fn clipboard_strips_dangling_bindings() {
    let a = box_at(0.0, 0.0, 100.0, 60.0);
    let b = box_at(300.0, 0.0, 100.0, 60.0);
    let mut link = arrow(100.0, 30.0, 300.0, 30.0);
    link.start_binding = Some(a.id.clone());
    link.end_binding = Some(b.id.clone());
    let json = serialize_selection(&[a, b, link.clone()], &ids(&[link])).unwrap();
    let copies = materialize_elements(&json, 0.0, 0.0, 0.0).unwrap();
    assert!(copies[0].start_binding.is_none());
    assert!(copies[0].end_binding.is_none());
}

#[test]
fn clipboard_remaps_group_ids() {
    let g = "group-1";
    let mut a = box_at(0.0, 0.0, 100.0, 60.0);
    let mut b = box_at(200.0, 0.0, 100.0, 60.0);
    a.group_id = Some(g.into());
    b.group_id = Some(g.into());
    let json = serialize_selection(&[a.clone(), b.clone()], &ids(&[a, b])).unwrap();
    let copies = materialize_elements(&json, 0.0, 0.0, 0.0).unwrap();
    assert!(copies[0].group_id.is_some());
    assert_eq!(copies[0].group_id, copies[1].group_id);
    assert_ne!(copies[0].group_id.as_deref(), Some(g));
}

#[test]
fn zorder_front_back_step() {
    let a = box_at(0.0, 0.0, 100.0, 60.0);
    let b = box_at(10.0, 0.0, 100.0, 60.0);
    let c = box_at(20.0, 0.0, 100.0, 60.0);
    let live = vec![a.clone(), b.clone(), c.clone()];
    let idx = |els: Vec<DrawElement>| {
        els.into_iter()
            .map(|e| [&a, &b, &c].iter().position(|x| x.id == e.id).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        idx(reorder_elements(
            &live,
            &ids(&[a.clone()]),
            ZOrderMode::Front
        )),
        vec![1, 2, 0]
    );
    assert_eq!(
        idx(reorder_elements(
            &live,
            &ids(&[c.clone()]),
            ZOrderMode::Back
        )),
        vec![2, 0, 1]
    );
    assert_eq!(
        idx(reorder_elements(
            &live,
            &ids(&[a.clone()]),
            ZOrderMode::Forward
        )),
        vec![1, 0, 2]
    );
    assert_eq!(
        idx(reorder_elements(
            &live,
            &ids(&[c.clone()]),
            ZOrderMode::Backward
        )),
        vec![0, 2, 1]
    );
    assert_eq!(
        idx(reorder_elements(
            &live,
            &ids(&[a.clone(), b.clone()]),
            ZOrderMode::Forward
        )),
        vec![2, 0, 1]
    );
    assert_eq!(
        idx(reorder_elements(
            &live,
            &ids(&[c.clone()]),
            ZOrderMode::Forward
        )),
        vec![0, 1, 2]
    );
}

#[test]
fn align_on_selection_bounds() {
    let a = box_at(0.0, 0.0, 100.0, 60.0);
    let b = box_at(300.0, 200.0, 50.0, 20.0);
    let left = align_elements(
        &[a.clone(), b.clone()],
        &ids(&[a.clone(), b.clone()]),
        AlignMode::Left,
    );
    assert_eq!(left.iter().map(|e| e.x).collect::<Vec<_>>(), vec![0.0, 0.0]);
    let centred = align_elements(
        &[a.clone(), b.clone()],
        &ids(&[a.clone(), b.clone()]),
        AlignMode::CenterY,
    );
    let cy = |e: &DrawElement| e.y + e.height / 2.0;
    assert_eq!(cy(&centred[0]), cy(&centred[1]));
    assert!(align_elements(&[a.clone(), b], &ids(&[a]), AlignMode::Left).is_empty());
}

#[test]
fn distribute_centres() {
    let a = box_at(0.0, 0.0, 20.0, 20.0);
    let b = box_at(30.0, 0.0, 20.0, 20.0);
    let c = box_at(200.0, 0.0, 20.0, 20.0);
    let spread = distribute_elements(
        &[a.clone(), b.clone(), c.clone()],
        &ids(&[a.clone(), b.clone(), c.clone()]),
        'x',
    );
    let mut centres: Vec<f64> = spread.iter().map(|e| e.x + e.width / 2.0).collect();
    centres.sort_by(|p, q| p.partial_cmp(q).unwrap());
    assert_eq!(centres, vec![10.0, 110.0, 210.0]);
    assert!(distribute_elements(&[a.clone(), b.clone()], &ids(&[a, b]), 'x').is_empty());
}

#[test]
fn flip_horizontal() {
    let a = box_at(0.0, 0.0, 100.0, 60.0);
    let b = box_at(300.0, 0.0, 100.0, 60.0);
    let flipped = flip_elements(&[a.clone(), b.clone()], &ids(&[a, b]), FlipAxis::Horizontal);

    // The two swap places. Asserted through the bounds rather than through `x`, because
    // a mirrored element's `x` is its *right* edge: the sign of the extent is what the
    // painter reads as the mirror, so a flip has to change it.
    assert_eq!(element_bounds(&flipped[0]).min_x, 300.0);
    assert_eq!(element_bounds(&flipped[0]).max_x, 400.0);
    assert_eq!(element_bounds(&flipped[1]).min_x, 0.0);
    assert!(flipped[0].width < 0.0, "mirrored on the horizontal axis");
    assert_eq!(flipped[0].height, 60.0, "and untouched on the vertical one");

    let link = arrow(0.0, 0.0, 400.0, 100.0);
    let flipped = flip_elements(&[link.clone()], &ids(&[link]), FlipAxis::Horizontal);
    let (start, end) = linear_endpoints(&flipped[0]);
    assert_eq!(start.x, 400.0);
    assert_eq!(end.x, 0.0);
    assert_eq!(start.y, 0.0);
}

/// A reflection is its own inverse, so flipping twice has to land on the original
/// numbers exactly — not merely in the same place. Repositioning without changing the
/// extent looked like an involution while doing nothing at all.
#[test]
fn flipping_twice_restores_the_original() {
    for axis in [FlipAxis::Horizontal, FlipAxis::Vertical] {
        let shape = box_at(120.0, 40.0, 200.0, 90.0);
        let once = flip_elements(&[shape.clone()], &ids(&[shape.clone()]), axis);
        let twice = flip_elements(&once, &ids(&once), axis);

        assert_eq!(twice[0].x, shape.x, "{axis:?}");
        assert_eq!(twice[0].y, shape.y, "{axis:?}");
        assert_eq!(twice[0].width, shape.width, "{axis:?}");
        assert_eq!(twice[0].height, shape.height, "{axis:?}");

        // And one flip genuinely reverses the shape rather than leaving it be.
        let mirrored = if axis == FlipAxis::Horizontal {
            once[0].width
        } else {
            once[0].height
        };
        assert!(mirrored < 0.0, "{axis:?} must actually mirror the shape");
    }
}

/// Text is repositioned but never mirrored — reversed glyphs are a rendering bug, not a
/// drawing operation.
#[test]
fn flipping_text_moves_it_without_reversing_it() {
    let mut label = create_element_default(
        DrawElementType::Text,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 25.0,
        },
    );
    label.text = Some("hello".into());
    let other = box_at(300.0, 0.0, 100.0, 60.0);

    let flipped = flip_elements(
        &[label.clone(), other.clone()],
        &ids(&[label.clone(), other]),
        FlipAxis::Horizontal,
    );
    let moved = flipped.iter().find(|e| e.id == label.id).unwrap();
    assert!(moved.width > 0.0, "glyphs must not be reversed");
    assert_eq!(moved.x, 320.0);
}

#[test]
fn group_expand_and_detect() {
    let a = box_at(0.0, 0.0, 100.0, 60.0);
    let b = box_at(200.0, 0.0, 100.0, 60.0);
    let c = box_at(400.0, 0.0, 100.0, 60.0);
    let grouped = group_patches(&[a.clone(), b.clone()], &ids(&[a.clone(), b.clone()]), "g1");
    assert_eq!(
        grouped
            .iter()
            .map(|e| e.group_id.clone())
            .collect::<Vec<_>>(),
        vec![Some("g1".into()), Some("g1".into())]
    );
    let mut scene = grouped.clone();
    scene.push(c.clone());
    let mut expanded: Vec<_> = expand_to_groups(&scene, [a.id.clone()])
        .into_iter()
        .collect();
    expanded.sort();
    let mut expect = vec![a.id.clone(), b.id.clone()];
    expect.sort();
    assert_eq!(expanded, expect);
    assert_eq!(
        expand_to_groups(&scene, [c.id.clone()]),
        HashSet::from([c.id.clone()])
    );
    assert!(is_single_group(
        &scene,
        &ids(&[grouped[0].clone(), grouped[1].clone()])
    ));
    assert!(!is_single_group(&scene, &ids(&[grouped[0].clone(), c])));
    let ungrouped = ungroup_patches(&scene, &ids(&[grouped[0].clone(), grouped[1].clone()]));
    assert_eq!(
        ungrouped
            .iter()
            .map(|e| e.group_id.clone())
            .collect::<Vec<_>>(),
        vec![None, None]
    );
}

#[test]
fn snap_move_magnetises() {
    let static_bounds = WorldBounds {
        min_x: 100.0,
        min_y: 100.0,
        max_x: 200.0,
        max_y: 200.0,
    };
    let near = WorldBounds {
        min_x: 204.0,
        min_y: 300.0,
        max_x: 254.0,
        max_y: 340.0,
    };
    let hit = snap_move(near, &[static_bounds], 6.0);
    assert_eq!(hit.dx, -4.0);
    assert!(!hit.guides.is_empty() && hit.guides[0].axis == Axis::X);
    assert_eq!(hit.guides[0].at, 200.0);
    let far = WorldBounds {
        min_x: 220.0,
        min_y: 300.0,
        max_x: 270.0,
        max_y: 340.0,
    };
    let miss = snap_move(far, &[static_bounds], 6.0);
    assert_eq!((miss.dx, miss.dy, miss.guides.len()), (0.0, 0.0, 0));
    let centre = WorldBounds {
        min_x: 400.0,
        min_y: 128.0,
        max_x: 440.0,
        max_y: 168.0,
    };
    let snapped = snap_move(centre, &[static_bounds], 6.0);
    assert_eq!(snapped.dy, 2.0);
}

#[test]
fn resize_with_aspect() {
    let element = box_at(0.0, 0.0, 100.0, 50.0);
    let corner = resize_element(&element, HandleKind::Se, 300.0, 60.0, 1.0, Some(2.0));
    assert_eq!((corner.width / corner.height).round(), 2.0);
    let edge = resize_element(&element, HandleKind::E, 240.0, 25.0, 1.0, Some(2.0));
    assert_eq!(edge.width.round(), 240.0);
    assert_eq!(edge.height.round(), 120.0);
    let free = resize_element(&element, HandleKind::E, 240.0, 25.0, 1.0, None);
    assert_eq!(free.height, 50.0);
}
