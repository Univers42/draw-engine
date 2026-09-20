use std::collections::{HashMap, HashSet};

use crate::camera::{Camera, Point, WorldBounds};
use crate::interaction::DrawTool;
use crate::scene::{DrawElement, DrawElementStylePatch};
use crate::selection::HandleKind;

#[derive(Clone, Debug, serde::Serialize)]
pub struct TextEditRequest {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub font_size: f64,
    pub color: String,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct EngineEvents {
    pub camera: Option<Camera>,
    pub tool: Option<DrawTool>,
    pub selection: Option<Vec<String>>,
    pub text_edit: Option<TextEditRequest>,
    pub scene_json: Option<String>,
}

pub(crate) enum Interaction {
    Draft {
        id: String,
        start: Point,
    },
    Linear {
        id: String,
        start: Point,
    },
    Freedraw {
        id: String,
        start: Point,
    },
    Erase,
    Pan {
        last_x: f64,
        last_y: f64,
    },
    Move {
        ids: Vec<String>,
        start: Point,
        origins: HashMap<String, Point>,
        static_bounds: Vec<WorldBounds>,
    },
    Resize {
        id: String,
        handle: HandleKind,
        ratio: Option<f64>,
    },
    Rotate {
        id: String,
    },
    Marquee {
        start: Point,
        current: Point,
        base: HashSet<String>,
    },
}

pub(crate) fn default_measure(text: &str, font_size: f64) -> (f64, f64) {
    let lines: Vec<&str> = text.split('\n').collect();
    let width = lines
        .iter()
        .map(|line| line.len() as f64 * font_size * 0.6)
        .fold(0.0_f64, f64::max);
    (
        width.max(4.0),
        (lines.len() as f64 * font_size * crate::TEXT_LINE_HEIGHT).max(font_size),
    )
}

pub(crate) fn history_signature(elements: &[DrawElement]) -> String {
    elements
        .iter()
        .map(|el| {
            format!(
                "{}:{}:{},{},{},{},{:.3}:{}",
                el.id,
                el.version,
                el.x.round(),
                el.y.round(),
                el.width.round(),
                el.height.round(),
                el.angle,
                if el.is_deleted { 1 } else { 0 }
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

pub fn merge_style_patch(
    base: &DrawElementStylePatch,
    patch: &DrawElementStylePatch,
) -> DrawElementStylePatch {
    DrawElementStylePatch {
        stroke_color: patch
            .stroke_color
            .clone()
            .or_else(|| base.stroke_color.clone()),
        background_color: patch
            .background_color
            .clone()
            .or_else(|| base.background_color.clone()),
        fill_style: patch.fill_style.or(base.fill_style),
        stroke_width: patch.stroke_width.or(base.stroke_width),
        stroke_style: patch.stroke_style.or(base.stroke_style),
        roughness: patch.roughness.or(base.roughness),
        opacity: patch.opacity.or(base.opacity),
        roundness: patch.roundness.or(base.roundness),
    }
}
