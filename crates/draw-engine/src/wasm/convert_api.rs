use wasm_bindgen::prelude::*;

use super::WasmEngine;
use crate::scene::element::DrawElementType;

/// `"rectangle"` / `"diamond"` / `"ellipse"`: what the shape switch can switch to.
fn parse_generic(value: &str) -> Option<DrawElementType> {
    match value {
        "rectangle" => Some(DrawElementType::Rectangle),
        "diamond" => Some(DrawElementType::Diamond),
        "ellipse" => Some(DrawElementType::Ellipse),
        _ => None,
    }
}

/// The shape switch — see `engine/convert.rs`.
#[wasm_bindgen(js_class = DrawEngine)]
impl WasmEngine {
    #[wasm_bindgen(js_name = canConvertSelection)]
    pub fn can_convert_selection(&self) -> bool {
        self.cell.borrow().engine.can_convert_selection()
    }

    #[wasm_bindgen(js_name = beginConversion)]
    pub fn begin_conversion(&self) {
        self.cell.borrow_mut().engine.begin_conversion();
    }

    #[wasm_bindgen(js_name = endConversion)]
    pub fn end_conversion(&self) {
        self.cell.borrow_mut().engine.end_conversion();
    }

    /// Switches the selection to `to` when it names a type, else a step round:
    /// forward for Tab, back for Shift+Tab. Whether anything changed.
    #[wasm_bindgen(js_name = convertSelection)]
    pub fn convert_selection(&self, to: Option<String>, forward: bool) -> bool {
        let to = match to.as_deref() {
            Some(name) => match parse_generic(name) {
                Some(kind) => Some(kind),
                None => return false,
            },
            None => None,
        };
        let changed = self.cell.borrow_mut().engine.convert_selection(to, forward);
        self.flush();
        changed
    }
}
