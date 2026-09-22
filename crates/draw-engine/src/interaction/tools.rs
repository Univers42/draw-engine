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
    /// A named region that owns whatever is drawn inside it.
    Frame,
    /// Waiting for a file. Nothing is drawn with the pointer: the host opens a picker,
    /// decodes what it gets, and calls `insert_image`. The tool exists so that the
    /// shortcut, the toolbar state and the cursor all live where every other tool's do.
    Image,
    /// Waiting for a URL. Like the image tool, the pointer draws nothing: the host asks
    /// for a link and calls `insert_embed`.
    Embed,
    /// Draws freehand and converts the stroke into the shape it was meant to be.
    AutoShape,
    /// Fills the region under the pointer. Nothing is dragged: the click *is* the
    /// gesture, and what it leaves behind is a polygon derived from the strokes around it.
    BucketFill,
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
            Self::Frame => "frame",
            Self::Image => "image",
            Self::Embed => "embed",
            Self::AutoShape => "autoshape",
            Self::BucketFill => "bucketfill",
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

/// Tools that switch back when their own key is pressed again.
///
/// The hand and the eraser, and Excalidraw marks exactly these two. They are the tools
/// you reach for *during* something else — pan to see the rest of the diagram, rub out a
/// stray line — so the return trip is the point. Without it, leaving the eraser costs a
/// second keystroke and you have to remember what you were holding.
pub fn is_toggle_tool(tool: DrawTool) -> bool {
    matches!(tool, DrawTool::Hand | DrawTool::Eraser)
}

/// Excalidraw's tool shortcuts: `TOOLS` in `components/Tools.tsx`, pinned by
/// `tests/ci_shortcuts.rs`.
///
/// Every variant of [`DrawTool`] must be reachable from here, because a tool nothing can
/// reach is a tool that does not exist on a keyboard-driven board — and because the host
/// toolbars print these keys as badges, and a badge advertising a key the engine ignores
/// is worse than no badge at all.
///
/// `shift` is a parameter rather than something the caller folds into `key`, because
/// Shift+X is autoshape while X is freedraw: the two are different tools, so the keymap
/// cannot be a function of the key alone. It used to be, and autoshape had an invented
/// key as a result.
///
/// Holding shift is otherwise inert — Excalidraw dropped Shift+letter as a separate
/// binding and made tool keys case-insensitive, so `R` and `r` are the same shortcut.
pub fn tool_for_chord(key: &str, shift: bool) -> Option<DrawTool> {
    let key = key.to_ascii_lowercase();
    if shift && key == "x" {
        return Some(DrawTool::AutoShape);
    }
    match key.as_str() {
        "1" | "v" => Some(DrawTool::Select),
        "2" | "r" => Some(DrawTool::Rectangle),
        "3" | "d" => Some(DrawTool::Diamond),
        "4" | "o" => Some(DrawTool::Ellipse),
        "5" | "a" => Some(DrawTool::Arrow),
        "6" | "l" => Some(DrawTool::Line),
        // Two letters, and the only tool with two.
        "7" | "p" | "x" => Some(DrawTool::Freedraw),
        "8" | "t" => Some(DrawTool::Text),
        "0" | "e" => Some(DrawTool::Eraser),
        "k" => Some(DrawTool::Laser),
        "f" => Some(DrawTool::Frame),
        // A digit and no letter, which is the oracle's choice rather than an oversight.
        "9" => Some(DrawTool::Image),
        "b" => Some(DrawTool::BucketFill),
        "h" => Some(DrawTool::Hand),
        // Two tools Excalidraw gives no key at all: it reaches the lasso through a mode
        // of the selection tool and the embed through a menu. Ours are toolbar entries in
        // their own right, so they need keys, and these are the ones left — `l` was
        // already the line.
        "s" => Some(DrawTool::Lasso),
        "w" => Some(DrawTool::Embed),
        _ => None,
    }
}

/// The tool for a key with no modifier held.
pub fn tool_for_key(key: &str) -> Option<DrawTool> {
    tool_for_chord(key, false)
}

/// Every tool, in toolbar order. The one place that has to be updated when a tool is
/// added, so a test can walk it and hold the rest of the engine to account.
pub const ALL_TOOLS: [DrawTool; 17] = [
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
    DrawTool::Frame,
    DrawTool::Image,
    DrawTool::Embed,
    DrawTool::AutoShape,
    DrawTool::BucketFill,
];

pub fn tool_to_element_type(tool: DrawTool) -> Option<crate::scene::DrawElementType> {
    use crate::scene::DrawElementType;
    match tool {
        // Draws as a freehand stroke and is recognised on release; until then it is one.
        DrawTool::AutoShape => Some(DrawElementType::Freedraw),
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
