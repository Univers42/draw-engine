use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DrawTool {
    Select,
    Hand,
    Rectangle,
    Diamond,
    Ellipse,
    Line,
    Arrow,
    Freedraw,
    Text,
    Eraser,
}

impl DrawTool {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Hand => "hand",
            Self::Rectangle => "rectangle",
            Self::Diamond => "diamond",
            Self::Ellipse => "ellipse",
            Self::Line => "line",
            Self::Arrow => "arrow",
            Self::Freedraw => "freedraw",
            Self::Text => "text",
            Self::Eraser => "eraser",
        }
    }
}

pub fn is_shape_tool(tool: DrawTool) -> bool {
    matches!(
        tool,
        DrawTool::Rectangle | DrawTool::Diamond | DrawTool::Ellipse
    )
}

pub fn is_linear_tool(tool: DrawTool) -> bool {
    matches!(tool, DrawTool::Line | DrawTool::Arrow)
}

pub fn tool_for_key(key: &str) -> Option<DrawTool> {
    match key.to_ascii_lowercase().as_str() {
        "1" | "v" => Some(DrawTool::Select),
        "2" | "r" => Some(DrawTool::Rectangle),
        "3" | "d" => Some(DrawTool::Diamond),
        "4" | "o" => Some(DrawTool::Ellipse),
        "5" | "a" => Some(DrawTool::Arrow),
        "6" | "l" => Some(DrawTool::Line),
        "7" | "p" => Some(DrawTool::Freedraw),
        "8" | "t" => Some(DrawTool::Text),
        "e" => Some(DrawTool::Eraser),
        "h" => Some(DrawTool::Hand),
        _ => None,
    }
}

pub fn tool_to_element_type(tool: DrawTool) -> Option<crate::scene::DrawElementType> {
    use crate::scene::DrawElementType;
    match tool {
        DrawTool::Rectangle => Some(DrawElementType::Rectangle),
        DrawTool::Diamond => Some(DrawElementType::Diamond),
        DrawTool::Ellipse => Some(DrawElementType::Ellipse),
        DrawTool::Line => Some(DrawElementType::Line),
        DrawTool::Arrow => Some(DrawElementType::Arrow),
        DrawTool::Freedraw => Some(DrawElementType::Freedraw),
        DrawTool::Text => Some(DrawElementType::Text),
        _ => None,
    }
}
