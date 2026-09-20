use std::collections::HashSet;

use crate::scene::binding::is_linear_element;
use crate::scene::element::DrawElement;
use crate::scene::geometry::{normalize_rect, scene_bounds};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlipAxis {
    Horizontal,
    Vertical,
}

impl FlipAxis {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "horizontal" => Some(Self::Horizontal),
            "vertical" => Some(Self::Vertical),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

fn flip_one(element: &DrawElement, axis: FlipAxis, lo: f64, hi: f64) -> DrawElement {
    let horizontal = axis == FlipAxis::Horizontal;
    let angle = if element.angle != 0.0 {
        -element.angle
    } else {
        0.0
    };
    if is_linear_element(element) {
        let points: Vec<[f64; 2]> = element
            .points
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|&[px, py]| if horizontal { [-px, py] } else { [px, -py] })
            .collect();
        let mut next = element.clone();
        next.points = Some(points);
        next.angle = angle;
        if horizontal {
            next.x = lo + hi - element.x;
            next.width = -element.width;
        } else {
            next.y = lo + hi - element.y;
            next.height = -element.height;
        }
        return next;
    }
    let rect = normalize_rect(element.x, element.y, element.width, element.height);
    let mut next = element.clone();
    next.x = rect.x;
    next.y = rect.y;
    next.width = rect.width;
    next.height = rect.height;
    next.angle = angle;
    if element.kind == crate::scene::DrawElementType::Freedraw {
        let points: Vec<[f64; 2]> = element
            .points
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|&[px, py]| {
                if horizontal {
                    [rect.width - px, py]
                } else {
                    [px, rect.height - py]
                }
            })
            .collect();
        next.points = Some(points);
    }
    if horizontal {
        next.x = lo + hi - (rect.x + rect.width);
    } else {
        next.y = lo + hi - (rect.y + rect.height);
    }
    next
}

pub fn flip_elements(
    elements: &[DrawElement],
    ids: &HashSet<String>,
    axis: FlipAxis,
) -> Vec<DrawElement> {
    let targets: Vec<&DrawElement> = elements
        .iter()
        .filter(|el| {
            ids.contains(&el.id) && !el.is_deleted && !el.locked() && el.container_id.is_none()
        })
        .collect();
    if targets.is_empty() {
        return Vec::new();
    }
    let owned: Vec<DrawElement> = targets.into_iter().cloned().collect();
    let Some(bounds) = scene_bounds(&owned) else {
        return Vec::new();
    };
    let (lo, hi) = if axis == FlipAxis::Horizontal {
        (bounds.min_x, bounds.max_x)
    } else {
        (bounds.min_y, bounds.max_y)
    };
    owned
        .iter()
        .map(|element| flip_one(element, axis, lo, hi))
        .collect()
}
