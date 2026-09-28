use wasm_bindgen::prelude::*;

use super::WasmEngine;
use crate::engine::ConvertTo;

/// The shape switch — see `engine/convert.rs`. The two branches, and the conversion never
/// crossing between them (`ConvertElementTypePopup.tsx@1118751f:929-950`).
#[wasm_bindgen(js_class = DrawEngine)]
impl WasmEngine {
    #[wasm_bindgen(js_name = canConvertSelection)]
    pub fn can_convert_selection(&self) -> bool {
        self.cell.borrow().engine.can_convert_selection()
    }

    /// The type the panel shows pressed: `"rectangle"`, `"line"`, `"sharpArrow"`,
    /// `"curvedArrow"`, `"elbowArrow"`, and nothing when the selection disagrees on it or
    /// holds nothing to switch.
    #[wasm_bindgen(js_name = sharedConversionType)]
    pub fn shared_conversion_type(&self) -> Option<String> {
        let kind = self.cell.borrow().engine.shared_conversion_type()?;
        Some(kind.name().to_string())
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
            Some(name) => match ConvertTo::parse(name) {
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
