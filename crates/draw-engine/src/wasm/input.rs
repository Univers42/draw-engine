use wasm_bindgen::prelude::*;

use super::WasmEngine;
use crate::engine::DrawEngine;
use crate::scene::Scene;

#[wasm_bindgen(js_class = DrawEngine)]
impl WasmEngine {
    #[wasm_bindgen(js_name = setViewport)]
    pub fn set_viewport(&self, width: f64, height: f64, dpr: f64) {
        self.cell
            .borrow_mut()
            .engine
            .set_viewport(width, height, dpr);
        self.flush();
    }

    #[wasm_bindgen(js_name = setTheme)]
    pub fn set_theme(&self, json: &str) {
        if let Ok(theme) = serde_json::from_str(json) {
            self.cell.borrow_mut().engine.set_theme(theme);
            self.flush();
        }
    }

    #[wasm_bindgen(js_name = setSceneJson)]
    pub fn set_scene_json(&self, json: &str) {
        if let Some(elements) = crate::elements_from_json(json) {
            self.cell
                .borrow_mut()
                .engine
                .set_scene(Scene::new(elements));
            self.flush();
        }
    }

    #[wasm_bindgen(js_name = loadScene)]
    pub fn load_scene(&self, json: &str) -> bool {
        let ok = self.cell.borrow_mut().engine.load_scene(json);
        self.flush();
        ok
    }

    pub fn clear(&self) {
        self.cell.borrow_mut().engine.clear();
        self.flush();
    }

    #[wasm_bindgen(js_name = setTool)]
    pub fn set_tool(&self, tool: &str) {
        if let Ok(tool) = serde_json::from_str(&format!("\"{tool}\"")) {
            self.cell.borrow_mut().engine.set_tool(tool);
            self.flush();
        }
    }

    #[wasm_bindgen(js_name = getTool)]
    pub fn get_tool(&self) -> String {
        self.cell.borrow().engine.get_tool().as_str().to_string()
    }

    #[wasm_bindgen(js_name = beginPointer)]
    pub fn begin_pointer(&self, sx: f64, sy: f64, additive: bool, duplicate: bool) {
        self.with_now(|eng| eng.begin_pointer(sx, sy, additive, duplicate));
    }

    #[wasm_bindgen(js_name = beginPan)]
    pub fn begin_pan(&self, sx: f64, sy: f64) {
        self.with_now(|eng| eng.begin_pan(sx, sy));
    }

    #[wasm_bindgen(js_name = movePointer)]
    pub fn move_pointer(&self, sx: f64, sy: f64, square: bool, bypass_snap: bool) {
        self.with_now(|eng| eng.move_pointer(sx, sy, square, bypass_snap));
    }

    #[wasm_bindgen(js_name = endPointer)]
    pub fn end_pointer(&self) {
        self.with_now(DrawEngine::end_pointer);
    }

    #[wasm_bindgen(js_name = cancelPointer)]
    pub fn cancel_pointer(&self) {
        self.with_now(DrawEngine::cancel_pointer);
    }

    #[wasm_bindgen(js_name = handleDoubleClick)]
    pub fn handle_double_click(&self, sx: f64, sy: f64) {
        self.with_now(|eng| eng.handle_double_click(sx, sy));
    }

    pub fn undo(&self) {
        self.cell.borrow_mut().engine.undo();
        self.flush();
    }

    pub fn redo(&self) {
        self.cell.borrow_mut().engine.redo();
        self.flush();
    }

    #[wasm_bindgen(js_name = hitTest)]
    pub fn hit_test(&self, sx: f64, sy: f64, tolerance: f64) -> Option<String> {
        self.cell
            .borrow()
            .engine
            .hit_test(sx, sy, tolerance)
            .and_then(|el| serde_json::to_string(&el).ok())
    }

    /// The cursor to show for the pointer at `(sx, sy)`, as a [`crate::HoverCursor`] code.
    ///
    /// A `u8` rather than a string: this is called on every pointer move, and returning
    /// a `String` would allocate in Rust and again in JS, hundreds of times a second,
    /// almost always to say the same thing as last time.
    ///
    /// `try_borrow` rather than `borrow`, like the rest of the re-entrant surface — a
    /// pointer event that arrives while the engine is mid-frame must not panic, because a
    /// panic aborts the WASM instance and the canvas never recovers. A missed cursor
    /// update is invisible; the next mouse move fixes it.
    #[wasm_bindgen(js_name = hoverCursor)]
    pub fn hover_cursor(&self, sx: f64, sy: f64) -> u8 {
        match self.cell.try_borrow() {
            Ok(state) => state.engine.hover_cursor(sx, sy).code(),
            Err(_) => crate::HoverCursor::Default.code(),
        }
    }

    #[wasm_bindgen(js_name = zoomAt)]
    pub fn zoom_at(&self, sx: f64, sy: f64, factor: f64) {
        self.cell.borrow_mut().engine.zoom_at(sx, sy, factor);
        self.flush();
    }

    #[wasm_bindgen(js_name = panBy)]
    pub fn pan_by(&self, dx: f64, dy: f64) {
        self.cell.borrow_mut().engine.pan_by(dx, dy);
        self.flush();
    }

    pub fn fit(&self, padding: Option<f64>) {
        self.cell.borrow_mut().engine.fit(padding.unwrap_or(96.0));
        self.flush();
    }

    #[wasm_bindgen(js_name = zoomIn)]
    pub fn zoom_in(&self) {
        self.cell.borrow_mut().engine.zoom_in();
        self.flush();
    }

    #[wasm_bindgen(js_name = zoomOut)]
    pub fn zoom_out(&self) {
        self.cell.borrow_mut().engine.zoom_out();
        self.flush();
    }

    #[wasm_bindgen(js_name = zoomReset)]
    pub fn zoom_reset(&self) {
        self.cell.borrow_mut().engine.zoom_reset();
        self.flush();
    }

    #[wasm_bindgen(js_name = contentInView)]
    pub fn content_in_view(&self) -> bool {
        self.cell.borrow().engine.content_in_view()
    }

    #[wasm_bindgen(js_name = setToolLocked)]
    pub fn set_tool_locked(&self, locked: bool) {
        self.cell.borrow_mut().engine.set_tool_locked(locked);
    }

    #[wasm_bindgen(js_name = getToolLocked)]
    pub fn get_tool_locked(&self) -> bool {
        self.cell.borrow().engine.get_tool_locked()
    }

    #[wasm_bindgen(js_name = editSelectedText)]
    pub fn edit_selected_text(&self) -> bool {
        let ok = self.cell.borrow_mut().engine.edit_selected_text();
        self.flush();
        ok
    }

    #[wasm_bindgen(js_name = setElementText)]
    pub fn set_element_text(&self, id: &str, text: &str) {
        self.cell.borrow_mut().engine.set_element_text(id, text);
        self.flush();
    }

    #[wasm_bindgen(js_name = setFontSize)]
    pub fn set_font_size(&self, size: f64) {
        self.cell.borrow_mut().engine.set_font_size(size);
        self.flush();
    }

    #[wasm_bindgen(js_name = getFontSize)]
    pub fn get_font_size(&self) -> f64 {
        self.cell.borrow().engine.get_font_size()
    }

    #[wasm_bindgen(js_name = setArrowheadsJson)]
    pub fn set_arrowheads_json(&self, json: &str) {
        #[derive(serde::Deserialize)]
        struct Patch {
            start: Option<crate::scene::Arrowhead>,
            end: Option<crate::scene::Arrowhead>,
        }
        if let Ok(patch) = serde_json::from_str::<Patch>(json) {
            self.cell
                .borrow_mut()
                .engine
                .set_arrowheads(patch.start, patch.end);
            self.flush();
        }
    }

    #[wasm_bindgen(js_name = exportJson)]
    pub fn export_json(&self) -> String {
        self.cell.borrow().engine.export_json()
    }

    #[wasm_bindgen(js_name = exportSvg)]
    pub fn export_svg(&self, padding: Option<f64>) -> Option<String> {
        self.cell
            .borrow()
            .engine
            .export_svg(padding.unwrap_or(16.0))
    }

    #[wasm_bindgen(js_name = cameraJson)]
    pub fn camera_json(&self) -> String {
        serde_json::to_string(&self.cell.borrow().engine.camera).unwrap_or_else(|_| "{}".into())
    }
}
