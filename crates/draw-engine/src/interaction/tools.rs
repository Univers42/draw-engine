use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DrawTool {
    Select,
    /// Free-form selection: draw a loop, take what it encloses.
    Lasso,
    Hand,
    Rectangle,
    Diamond,
    Ellipse,
    Line,
    Arrow,
    Freedraw,
    Text,
    Eraser,
    /// A trail that follows the cursor and fades. Draws nothing into the scene.
    Laser,
}

impl DrawTool {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Lasso => "lasso",
            Self::Hand => "hand",
            Self::Rectangle => "rectangle",
            Self::Diamond => "diamond",
            Self::Ellipse => "ellipse",
            Self::Line => "line",
            Self::Arrow => "arrow",
            Self::Freedraw => "freedraw",
            Self::Text => "text",
            Self::Eraser => "eraser",
            Self::Laser => "laser",
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

/// Excalidraw's tool shortcuts, which are the numeral and the initial of each tool.
///
/// Every variant of [`DrawTool`] must appear here — `ci_tools.rs` asserts it — because a
/// tool nothing can reach is a tool that does not exist on a keyboard-driven board, and
/// because the host toolbars print these keys as badges. A badge advertising a key the
/// engine ignores is worse than no badge at all.
///
/// `s` is the lasso and `k` the laser, matching Excalidraw: `l` was already the line.
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
        "0" | "e" => Some(DrawTool::Eraser),
        "s" => Some(DrawTool::Lasso),
        "k" => Some(DrawTool::Laser),
        "h" => Some(DrawTool::Hand),
        _ => None,
    }
}

/// Every tool, in toolbar order. The one place that has to be updated when a tool is
/// added, so a test can walk it and hold the rest of the engine to account.
pub const ALL_TOOLS: [DrawTool; 12] = [
    DrawTool::Select,
    DrawTool::Lasso,
    DrawTool::Hand,
    DrawTool::Rectangle,
    DrawTool::Diamond,
    DrawTool::Ellipse,
    DrawTool::Line,
    DrawTool::Arrow,
    DrawTool::Freedraw,
    DrawTool::Text,
    DrawTool::Eraser,
    DrawTool::Laser,
];

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
