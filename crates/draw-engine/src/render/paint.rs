use crate::scene::element::DrawElementType;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawTheme {
    pub background: String,
    pub grid: String,
    pub accent: String,
}

pub fn light_theme() -> DrawTheme {
    DrawTheme {
        background: "#ffffff".into(),
        grid: "rgba(17, 17, 17, 0.06)".into(),
        accent: "#4c6ef5".into(),
    }
}

pub fn dark_theme() -> DrawTheme {
    DrawTheme {
        background: "#191919".into(),
        grid: "rgba(255, 255, 255, 0.06)".into(),
        accent: "#748ffc".into(),
    }
}

pub const TEXT_LINE_HEIGHT: f64 = 1.25;

pub fn is_roughable(kind: DrawElementType) -> bool {
    matches!(
        kind,
        DrawElementType::Rectangle | DrawElementType::Diamond | DrawElementType::Ellipse
    )
}
