use wasm_bindgen::prelude::*;

use super::WasmEngine;
use crate::engine::LinkDirection;
use crate::scene::DrawElementType;

/// `"rectangle"`, `"diamond"` or `"ellipse"` — the three the digit-key choice offers.
/// Anything else, including `"stickynote"`, is `None`: a node started from a sticky note
/// keeps cloning sticky notes unless the person explicitly picks one of the three.
fn parse_shape(value: &str) -> Option<DrawElementType> {
    match value {
        "rectangle" => Some(DrawElementType::Rectangle),
        "diamond" => Some(DrawElementType::Diamond),
        "ellipse" => Some(DrawElementType::Ellipse),
        _ => None,
    }
}

#[wasm_bindgen(js_class = DrawEngine)]
impl WasmEngine {
    /// Ctrl/Cmd+Arrow: preview a cluster of new nodes off the selected flowchart node.
    #[wasm_bindgen(js_name = flowchartCreate)]
    pub fn flowchart_create(&self, direction: &str) {
        if let Some(direction) = LinkDirection::parse(direction) {
            self.cell.borrow_mut().engine.flowchart_create(direction);
            self.flush();
        }
    }

    /// While Ctrl/Cmd is held, 1/2/3 as `"rectangle"` / `"diamond"` / `"ellipse"`.
    #[wasm_bindgen(js_name = flowchartSetShape)]
    pub fn flowchart_set_shape(&self, shape: &str) {
        if let Some(shape) = parse_shape(shape) {
            self.cell.borrow_mut().engine.flowchart_set_shape(shape);
            self.flush();
        }
    }

    /// Releasing Ctrl/Cmd: commits the pending cluster as one step of history.
    #[wasm_bindgen(js_name = flowchartCommit)]
    pub fn flowchart_commit(&self) {
        self.cell.borrow_mut().engine.flowchart_commit();
        self.flush();
    }

    /// Escape while creating.
    #[wasm_bindgen(js_name = flowchartCancel)]
    pub fn flowchart_cancel(&self) {
        self.cell.borrow_mut().engine.flowchart_cancel();
        self.flush();
    }

    #[wasm_bindgen(js_name = isCreatingFlowchart)]
    pub fn is_creating_flowchart(&self) -> bool {
        self.cell.borrow().engine.is_creating_flowchart()
    }

    /// The pending cluster, for a host that wants to show it outside the canvas painter
    /// (the shape-chooser strip's position, an inspector, a test).
    #[wasm_bindgen(js_name = pendingFlowchartElementsJson)]
    pub fn pending_flowchart_elements_json(&self) -> String {
        serde_json::to_string(&self.cell.borrow().engine.pending_flowchart_elements())
            .unwrap_or_else(|_| "[]".into())
    }

    /// Alt+Arrow: selects the connected node in that direction. Returns its id, or `null`.
    #[wasm_bindgen(js_name = flowchartNavigate)]
    pub fn flowchart_navigate(&self, direction: &str) -> Option<String> {
        let direction = LinkDirection::parse(direction)?;
        let id = self.cell.borrow_mut().engine.flowchart_navigate(direction);
        self.flush();
        id
    }

    /// Alt released: ends the exploration.
    #[wasm_bindgen(js_name = flowchartNavigationEnd)]
    pub fn flowchart_navigation_end(&self) {
        self.cell.borrow_mut().engine.flowchart_navigation_end();
        self.flush();
    }
}
