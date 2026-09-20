//! wasm-bindgen facade. Scene stays in Rust; JS sees JSON, ids, and style patches.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use crate::engine::{DrawEngine, Painter};

mod edit_api;
mod input;
mod paint;

use paint::CanvasPainter;

pub(crate) struct EngineCell {
    pub(crate) engine: DrawEngine,
    pub(crate) canvas: HtmlCanvasElement,
    pub(crate) ctx: CanvasRenderingContext2d,
    pub(crate) raf: Option<i32>,
    pub(crate) on_camera: Option<js_sys::Function>,
    pub(crate) on_tool: Option<js_sys::Function>,
    pub(crate) on_selection: Option<js_sys::Function>,
    pub(crate) on_text: Option<js_sys::Function>,
    pub(crate) on_scene: Option<js_sys::Function>,
}

#[wasm_bindgen(js_name = DrawEngine)]
pub struct WasmEngine {
    pub(crate) cell: Rc<RefCell<EngineCell>>,
}

fn context_2d(canvas: &HtmlCanvasElement) -> Result<CanvasRenderingContext2d, JsValue> {
    canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("draw-engine: 2D canvas context unavailable"))?
        .dyn_into::<CanvasRenderingContext2d>()
        .map_err(|_| JsValue::from_str("draw-engine: 2D canvas context unavailable"))
}

#[wasm_bindgen(js_class = DrawEngine)]
impl WasmEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: HtmlCanvasElement) -> Result<WasmEngine, JsValue> {
        let ctx = context_2d(&canvas)?;
        let mut engine = DrawEngine::new();
        engine.set_measure_text(measure_via_ctx);
        let cell = Rc::new(RefCell::new(EngineCell {
            engine,
            canvas,
            ctx,
            raf: None,
            on_camera: None,
            on_tool: None,
            on_selection: None,
            on_text: None,
            on_scene: None,
        }));
        let wasm = WasmEngine { cell: cell.clone() };
        wasm.schedule();
        Ok(wasm)
    }

    #[wasm_bindgen(js_name = setOnCameraChange)]
    pub fn set_on_camera_change(&self, cb: Option<js_sys::Function>) {
        self.cell.borrow_mut().on_camera = cb;
    }

    #[wasm_bindgen(js_name = setOnToolChange)]
    pub fn set_on_tool_change(&self, cb: Option<js_sys::Function>) {
        self.cell.borrow_mut().on_tool = cb;
    }

    #[wasm_bindgen(js_name = setOnSelectionChange)]
    pub fn set_on_selection_change(&self, cb: Option<js_sys::Function>) {
        self.cell.borrow_mut().on_selection = cb;
    }

    #[wasm_bindgen(js_name = setOnRequestTextEdit)]
    pub fn set_on_request_text_edit(&self, cb: Option<js_sys::Function>) {
        self.cell.borrow_mut().on_text = cb;
    }

    #[wasm_bindgen(js_name = setOnSceneChange)]
    pub fn set_on_scene_change(&self, cb: Option<js_sys::Function>) {
        self.cell.borrow_mut().on_scene = cb;
    }

    pub fn destroy(&self) {
        let mut cell = self.cell.borrow_mut();
        cell.engine.destroy();
        if let Some(id) = cell.raf.take() {
            if let Some(window) = web_sys::window() {
                let _ = window.cancel_animation_frame(id);
            }
        }
        cell.canvas.set_width(0);
        cell.canvas.set_height(0);
    }
}

impl WasmEngine {
    pub(crate) fn with_now(&self, f: impl FnOnce(&mut DrawEngine)) {
        let now = now_ms();
        {
            let mut cell = self.cell.borrow_mut();
            cell.engine.set_now(now);
            f(&mut cell.engine);
        }
        self.flush();
    }

    pub(crate) fn flush(&self) {
        emit_events(&self.cell);
        self.schedule();
    }

    fn schedule(&self) {
        let cell = self.cell.clone();
        if cell.borrow().raf.is_some() || cell.borrow().engine.is_disposed() {
            return;
        }
        if !cell.borrow().engine.is_dirty() && !cell.borrow().engine.in_motion() {
            return;
        }
        let window = match web_sys::window() {
            Some(window) => window,
            None => return,
        };
        let cloned = cell.clone();
        let closure = Closure::once_into_js(move || {
            cloned.borrow_mut().raf = None;
            paint_frame(&cloned);
            let more = {
                let cell = cloned.borrow();
                !cell.engine.is_disposed() && (cell.engine.is_dirty() || cell.engine.in_motion())
            };
            if more {
                WasmEngine { cell: cloned }.schedule();
            }
        });
        if let Ok(id) = window.request_animation_frame(closure.as_ref().unchecked_ref()) {
            cell.borrow_mut().raf = Some(id);
        }
    }
}

fn paint_frame(cell: &Rc<RefCell<EngineCell>>) {
    let mut cell = cell.borrow_mut();
    if cell.engine.is_disposed() {
        return;
    }
    cell.engine.set_now(now_ms());
    let view = cell.engine.paint_view();
    let dpr = view.dpr;
    let bw = (view.width * dpr).round().max(1.0) as u32;
    let bh = (view.height * dpr).round().max(1.0) as u32;
    if cell.canvas.width() != bw {
        cell.canvas.set_width(bw);
    }
    if cell.canvas.height() != bh {
        cell.canvas.set_height(bh);
    }
    CanvasPainter { ctx: &cell.ctx }.paint(&view);
    cell.engine.take_dirty();
}

fn emit_events(cell: &Rc<RefCell<EngineCell>>) {
    let (events, cbs) = {
        let mut cell = cell.borrow_mut();
        let events = cell.engine.drain_events();
        (
            events,
            (
                cell.on_camera.clone(),
                cell.on_tool.clone(),
                cell.on_selection.clone(),
                cell.on_text.clone(),
                cell.on_scene.clone(),
            ),
        )
    };
    if let (Some(camera), Some(cb)) = (events.camera, cbs.0) {
        let json = serde_json::to_string(&camera).unwrap_or_default();
        let _ = cb.call1(&JsValue::NULL, &JsValue::from_str(&json));
    }
    if let (Some(tool), Some(cb)) = (events.tool, cbs.1) {
        let _ = cb.call1(&JsValue::NULL, &JsValue::from_str(tool.as_str()));
    }
    if let (Some(ids), Some(cb)) = (events.selection, cbs.2) {
        let json = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into());
        let _ = cb.call1(&JsValue::NULL, &JsValue::from_str(&json));
    }
    if let (Some(req), Some(cb)) = (events.text_edit, cbs.3) {
        let json = serde_json::to_string(&req).unwrap_or_default();
        let _ = cb.call1(&JsValue::NULL, &JsValue::from_str(&json));
    }
    if let (Some(json), Some(cb)) = (events.scene_json, cbs.4) {
        let _ = cb.call1(&JsValue::NULL, &JsValue::from_str(&json));
    }
}

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

fn measure_via_ctx(text: &str, font_size: f64) -> (f64, f64) {
    let lines: Vec<&str> = text.split('\n').collect();
    let width = lines
        .iter()
        .map(|line| line.len() as f64 * font_size * 0.6)
        .fold(0.0, f64::max);
    (
        width.max(4.0),
        (lines.len() as f64 * font_size * crate::TEXT_LINE_HEIGHT).max(font_size),
    )
}
