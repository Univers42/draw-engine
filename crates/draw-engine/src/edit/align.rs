use std::collections::HashSet;

use crate::scene::element::DrawElement;
use crate::scene::geometry::{element_bounds, scene_bounds};

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

fn movable<'a>(elements: &'a [DrawElement], ids: &HashSet<String>) -> Vec<&'a DrawElement> {
    elements
        .iter()
        .filter(|el| {
            ids.contains(&el.id) && !el.is_deleted && !el.locked() && el.container_id.is_none()
        })
        .collect()
}

pub fn align_elements(
    elements: &[DrawElement],
    ids: &HashSet<String>,
    mode: AlignMode,
) -> Vec<DrawElement> {
    let targets = movable(elements, ids);
    if targets.len() < 2 {
        return Vec::new();
    }
    let owned: Vec<DrawElement> = targets.into_iter().cloned().collect();
    let Some(bounds) = scene_bounds(&owned) else {
        return Vec::new();
    };
    owned
        .into_iter()
        .map(|mut element| {
            let box_bounds = element_bounds(&element);
            let (dx, dy) = match mode {
                AlignMode::Left => (bounds.min_x - box_bounds.min_x, 0.0),
                AlignMode::Right => (bounds.max_x - box_bounds.max_x, 0.0),
                AlignMode::CenterX => (
                    (bounds.min_x + bounds.max_x) / 2.0
                        - (box_bounds.min_x + box_bounds.max_x) / 2.0,
                    0.0,
                ),
                AlignMode::Top => (0.0, bounds.min_y - box_bounds.min_y),
                AlignMode::Bottom => (0.0, bounds.max_y - box_bounds.max_y),
                AlignMode::CenterY => (
                    0.0,
                    (bounds.min_y + bounds.max_y) / 2.0
                        - (box_bounds.min_y + box_bounds.max_y) / 2.0,
                ),
            };
            element.x += dx;
            element.y += dy;
            element
        })
        .collect()
}

pub fn distribute_elements(
    elements: &[DrawElement],
    ids: &HashSet<String>,
    axis: char,
) -> Vec<DrawElement> {
    let mut targets: Vec<DrawElement> = movable(elements, ids).into_iter().cloned().collect();
    if targets.len() < 3 {
        return Vec::new();
    }
    let centre = |element: &DrawElement| {
        let bounds = element_bounds(element);
        if axis == 'x' {
            (bounds.min_x + bounds.max_x) / 2.0
        } else {
            (bounds.min_y + bounds.max_y) / 2.0
        }
    };
    targets.sort_by(|a, b| {
        centre(a)
            .partial_cmp(&centre(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let first = centre(&targets[0]);
    let last = centre(targets.last().expect("len >= 3"));
    let step = (last - first) / (targets.len() - 1) as f64;
    targets
        .into_iter()
        .enumerate()
        .map(|(index, mut element)| {
            let delta = first + step * index as f64 - centre(&element);
            if axis == 'x' {
                element.x += delta;
            } else {
                element.y += delta;
            }
            element
        })
        .collect()
}
