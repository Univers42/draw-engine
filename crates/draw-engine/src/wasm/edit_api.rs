use wasm_bindgen::prelude::*;

use super::WasmEngine;
use crate::scene::DrawElementStylePatch;

#[wasm_bindgen(js_class = DrawEngine)]
impl WasmEngine {
    #[wasm_bindgen(js_name = deleteSelection)]
    pub fn delete_selection(&self) {
        self.cell.borrow_mut().engine.delete_selection();
        self.flush();
    }

    #[wasm_bindgen(js_name = copySelection)]
    pub fn copy_selection(&self) -> Option<String> {
        let json = self.cell.borrow_mut().engine.copy_selection();
        self.flush();
        json
    }

    #[wasm_bindgen(js_name = cutSelection)]
    pub fn cut_selection(&self) -> Option<String> {
        let json = self.cell.borrow_mut().engine.cut_selection();
        self.flush();
        json
    }

    #[wasm_bindgen(js_name = pasteJson)]
    pub fn paste_json(&self, json: Option<String>, x: Option<f64>, y: Option<f64>) -> bool {
        let at = match (x, y) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None,
        };
        let ok = self
            .cell
            .borrow_mut()
            .engine
            .paste_json(json.as_deref(), at);
        self.flush();
        ok
    }

    #[wasm_bindgen(js_name = duplicateSelection)]
    pub fn duplicate_selection(&self) {
        self.cell
            .borrow_mut()
            .engine
            .duplicate_selection(12.0, 12.0);
        self.flush();
    }

    #[wasm_bindgen(js_name = nudgeSelection)]
    pub fn nudge_selection(&self, dx: f64, dy: f64) {
        self.cell.borrow_mut().engine.nudge_selection(dx, dy);
        self.flush();
    }

    #[wasm_bindgen(js_name = reorderSelection)]
    pub fn reorder_selection(&self, mode: &str) {
        if let Some(mode) = crate::ZOrderMode::parse(mode) {
            self.cell.borrow_mut().engine.reorder_selection(mode);
            self.flush();
        }
    }

    #[wasm_bindgen(js_name = alignSelection)]
    pub fn align_selection(&self, mode: &str) {
        if let Some(mode) = crate::AlignMode::parse(mode) {
            self.cell.borrow_mut().engine.align_selection(mode);
            self.flush();
        }
    }

    #[wasm_bindgen(js_name = distributeSelection)]
    pub fn distribute_selection(&self, axis: &str) {
        let axis = axis.chars().next().unwrap_or('x');
        self.cell.borrow_mut().engine.distribute_selection(axis);
        self.flush();
    }

    #[wasm_bindgen(js_name = flipSelection)]
    pub fn flip_selection(&self, axis: &str) {
        if let Some(axis) = crate::FlipAxis::parse(axis) {
            self.cell.borrow_mut().engine.flip_selection(axis);
            self.flush();
        }
    }

    #[wasm_bindgen(js_name = groupSelection)]
    pub fn group_selection(&self) {
        self.cell.borrow_mut().engine.group_selection();
        self.flush();
    }

    #[wasm_bindgen(js_name = ungroupSelection)]
    pub fn ungroup_selection(&self) {
        self.cell.borrow_mut().engine.ungroup_selection();
        self.flush();
    }

    #[wasm_bindgen(js_name = selectionIsGroup)]
    pub fn selection_is_group(&self) -> bool {
        self.cell.borrow().engine.selection_is_group()
    }

    #[wasm_bindgen(js_name = toggleLockSelection)]
    pub fn toggle_lock_selection(&self) {
        self.cell.borrow_mut().engine.toggle_lock_selection();
        self.flush();
    }

    #[wasm_bindgen(js_name = selectionLocked)]
    pub fn selection_locked(&self) -> bool {
        self.cell.borrow().engine.selection_locked()
    }

    #[wasm_bindgen(js_name = applyStyleJson)]
    pub fn apply_style_json(&self, json: &str) {
        if let Ok(patch) = serde_json::from_str::<DrawElementStylePatch>(json) {
            self.cell.borrow_mut().engine.apply_style(patch);
            self.flush();
        }
    }

    #[wasm_bindgen(js_name = setNextStyleJson)]
    pub fn set_next_style_json(&self, json: &str) {
        if let Ok(patch) = serde_json::from_str::<DrawElementStylePatch>(json) {
            self.cell.borrow_mut().engine.set_next_style(patch);
        }
    }

    #[wasm_bindgen(js_name = getNextStyleJson)]
    pub fn get_next_style_json(&self) -> String {
        serde_json::to_string(&self.cell.borrow().engine.get_next_style())
            .unwrap_or_else(|_| "{}".into())
    }

    #[wasm_bindgen(js_name = getSelectionJson)]
    pub fn get_selection_json(&self) -> String {
        serde_json::to_string(&self.cell.borrow().engine.get_selection())
            .unwrap_or_else(|_| "[]".into())
    }

    #[wasm_bindgen(js_name = getSelectedElementsJson)]
    pub fn get_selected_elements_json(&self) -> String {
        serde_json::to_string(&self.cell.borrow().engine.get_selected_elements())
            .unwrap_or_else(|_| "[]".into())
    }

    #[wasm_bindgen(js_name = selectJson)]
    pub fn select_json(&self, json: &str) {
        if let Ok(ids) = serde_json::from_str::<Vec<String>>(json) {
            self.cell.borrow_mut().engine.select(ids);
            self.flush();
        }
    }

    #[wasm_bindgen(js_name = clearSelection)]
    pub fn clear_selection(&self) {
        self.cell.borrow_mut().engine.clear_selection();
        self.flush();
    }

    #[wasm_bindgen(js_name = selectAll)]
    pub fn select_all(&self) {
        self.cell.borrow_mut().engine.select_all();
        self.flush();
    }
}
