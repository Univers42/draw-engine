#![allow(clippy::cloned_ref_to_slice_refs)]
mod common;
use common::*;
use draw_engine::*;
use std::collections::HashSet;

fn make_three_elements() -> (DrawElement, DrawElement, DrawElement, Vec<DrawElement>) {
    let a = box_at(0.0, 0.0, 20.0, 20.0);
    let b = box_at(30.0, 0.0, 20.0, 20.0);
    let c = box_at(60.0, 0.0, 20.0, 20.0);
    let scene = vec![a.clone(), b.clone(), c.clone()];
    (a, b, c, scene)
}

#[test]
fn zorder_bring_to_front() {
    let (a, b, c, scene) = make_three_elements();
    let res = reorder_elements(&scene, &ids(&[a.clone()]), ZOrderMode::Front);
    assert_eq!(
        res.iter().map(|e| &e.id).collect::<Vec<_>>(),
        vec![&b.id, &c.id, &a.id]
    );
}

#[test]
fn zorder_send_to_back() {
    let (a, b, c, scene) = make_three_elements();
    let res = reorder_elements(&scene, &ids(&[c.clone()]), ZOrderMode::Back);
    assert_eq!(
        res.iter().map(|e| &e.id).collect::<Vec<_>>(),
        vec![&c.id, &a.id, &b.id]
    );
}

#[test]
fn zorder_step_forward() {
    let (a, b, c, scene) = make_three_elements();
    let res = reorder_elements(&scene, &ids(&[a.clone()]), ZOrderMode::Forward);
    assert_eq!(
        res.iter().map(|e| &e.id).collect::<Vec<_>>(),
        vec![&b.id, &a.id, &c.id]
    );
}

#[test]
fn zorder_step_backward() {
    let (a, b, c, scene) = make_three_elements();
    let res = reorder_elements(&scene, &ids(&[c.clone()]), ZOrderMode::Backward);
    assert_eq!(
        res.iter().map(|e| &e.id).collect::<Vec<_>>(),
        vec![&a.id, &c.id, &b.id]
    );
}

#[test]
fn zorder_already_at_front_is_noop() {
    let (a, b, c, scene) = make_three_elements();
    let res = reorder_elements(&scene, &ids(&[c.clone()]), ZOrderMode::Front);
    assert_eq!(
        res.iter().map(|e| &e.id).collect::<Vec<_>>(),
        vec![&a.id, &b.id, &c.id]
    );
}

#[test]
fn zorder_already_at_back_is_noop() {
    let (a, b, c, scene) = make_three_elements();
    let res = reorder_elements(&scene, &ids(&[a.clone()]), ZOrderMode::Back);
    assert_eq!(
        res.iter().map(|e| &e.id).collect::<Vec<_>>(),
        vec![&a.id, &b.id, &c.id]
    );
}

#[test]
fn zorder_empty_selection_is_noop() {
    let (_, _, _, scene) = make_three_elements();
    let res = reorder_elements(&scene, &HashSet::new(), ZOrderMode::Front);
    assert_eq!(res.len(), 3);
}

#[test]
fn align_left_multiple_elements() {
    let (a, b, c, scene) = make_three_elements();
    let res = align_elements(&scene, &ids(&[a, b, c]), None, AlignMode::Left);
    assert!(res.iter().all(|e| (e.x - 0.0).abs() < EPS));
}

#[test]
fn align_right_multiple_elements() {
    let (a, b, c, scene) = make_three_elements();
    let res = align_elements(&scene, &ids(&[a, b, c]), None, AlignMode::Right);
    assert!(res.iter().all(|e| (e.x + e.width - 80.0).abs() < EPS));
}

#[test]
fn align_center_x_multiple_elements() {
    let (a, b, c, scene) = make_three_elements();
    let res = align_elements(&scene, &ids(&[a, b, c]), None, AlignMode::CenterX);
    assert_close(res[0].x + res[0].width / 2.0, res[1].x + res[1].width / 2.0);
    assert_close(res[1].x + res[1].width / 2.0, res[2].x + res[2].width / 2.0);
}

#[test]
fn align_center_y_multiple_elements() {
    let (a, b, c, scene) = make_three_elements();
    let res = align_elements(&scene, &ids(&[a, b, c]), None, AlignMode::CenterY);
    assert_close(
        res[0].y + res[0].height / 2.0,
        res[1].y + res[1].height / 2.0,
    );
}

#[test]
fn align_single_element_returns_empty() {
    let (a, _, _, scene) = make_three_elements();
    let res = align_elements(&scene, &ids(&[a.clone()]), None, AlignMode::Left);
    assert!(res.is_empty());
}

#[test]
fn distribute_equidistant_x_axis() {
    let (a, b, c, scene) = make_three_elements();
    let res = distribute_elements(&scene, &ids(&[a, b, c]), None, 'x');
    let mut centers: Vec<_> = res.iter().map(|e| e.x + e.width / 2.0).collect();
    centers.sort_by(|l, r| l.partial_cmp(r).unwrap());
    assert_close(centers[1] - centers[0], centers[2] - centers[1]);
}

#[test]
fn distribute_equidistant_y_axis() {
    let a = box_at(0.0, 0.0, 20.0, 20.0);
    let b = box_at(0.0, 40.0, 20.0, 20.0);
    let c = box_at(0.0, 100.0, 20.0, 20.0);
    let scene = vec![a.clone(), b.clone(), c.clone()];
    let res = distribute_elements(&scene, &ids(&[a, b, c]), None, 'y');
    let mut centers: Vec<_> = res.iter().map(|e| e.y + e.height / 2.0).collect();
    centers.sort_by(|l, r| l.partial_cmp(r).unwrap());
    assert_close(centers[1] - centers[0], centers[2] - centers[1]);
}

#[test]
fn flip_horizontal_preserves_bounds() {
    let a = box_at(10.0, 20.0, 30.0, 40.0);
    let b = box_at(50.0, 30.0, 20.0, 20.0);
    let scene = vec![a.clone(), b.clone()];
    let flipped = flip_elements(&scene, &ids(&[a, b]), FlipAxis::Horizontal);
    let b_new = scene_bounds(&flipped).unwrap();
    assert_close(b_new.min_x, 10.0);
    assert_close(b_new.max_x, 70.0);
}

#[test]
fn flip_vertical_preserves_bounds() {
    let a = box_at(10.0, 20.0, 30.0, 40.0);
    let b = box_at(50.0, 30.0, 20.0, 20.0);
    let scene = vec![a.clone(), b.clone()];
    let flipped = flip_elements(&scene, &ids(&[a, b]), FlipAxis::Vertical);
    let b_new = scene_bounds(&flipped).unwrap();
    assert_close(b_new.min_y, 20.0);
    assert_close(b_new.max_y, 60.0);
}

/// The three group primitives against one another. `None` for the editing group
/// throughout: this is the top-level case, and nesting has its own file
/// (`ci_groups_nested.rs`).
#[test]
fn grouping_expand_and_ungroup() {
    let (a, b, c, _) = make_three_elements();
    let grouped = group_patches(
        &[a.clone(), b.clone(), c.clone()],
        &ids(&[a.clone(), b.clone()]),
        "group_1",
        None,
    );
    assert!(is_single_group(
        &grouped,
        &ids(&[grouped[0].clone(), grouped[1].clone()]),
        None,
    ));

    let expanded = expand_to_groups(&grouped, [a.id.clone()]);
    assert_eq!(expanded, HashSet::from([a.id, b.id]));

    let ungrouped = ungroup_patches(
        &grouped,
        &ids(&[grouped[0].clone(), grouped[1].clone()]),
        None,
    );
    assert!(ungrouped[0].group_ids.is_empty());
    assert!(ungrouped[1].group_ids.is_empty());
}
