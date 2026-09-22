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

/// Timings for the most recent frame.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PaintStats {
    /// Assembling the display list: culling, and the scene walk behind it.
    pub build_ms: f64,
    /// Issuing the drawing itself.
    pub paint_ms: f64,
    pub visible: usize,
}

pub(crate) struct EngineCell {
    pub stats: PaintStats,
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

thread_local! {
    /// Every engine on this page, weakly.
    ///
    /// Exists for one job: something outside the input path can finish and need the
    /// canvas redrawn. An image decodes asynchronously, so the frame that first asks for
    /// it has nothing to draw, and without a way back the picture would stay a
    /// placeholder until the next unrelated repaint.
    ///
    /// Weak, so an engine that has been destroyed is not kept alive by this list, and a
    /// `Vec` rather than a single slot because a page may embed more than one board.
    static LIVE: RefCell<Vec<std::rc::Weak<RefCell<EngineCell>>>> = const { RefCell::new(Vec::new()) };
}

/// Ask every live engine for another frame.
///
/// Called from asynchronous callbacks — image decoding today — which have no handle on
/// the engine that wanted the work. Dead entries are swept here rather than tracked, so
/// destroying an engine costs nothing.
pub(crate) fn request_repaint() {
    let live: Vec<Rc<RefCell<EngineCell>>> = LIVE.with(|live| {
        let mut live = live.borrow_mut();
        live.retain(|weak| weak.strong_count() > 0);
        live.iter().filter_map(|weak| weak.upgrade()).collect()
    });
    for cell in live {
        {
            let Ok(mut borrowed) = cell.try_borrow_mut() else {
                continue;
            };
            if borrowed.engine.is_disposed() {
                continue;
            }
            borrowed.engine.request_draw();
        }
        WasmEngine { cell }.schedule();
    }
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
    /// Timings for the most recent frame, as JSON.
    ///
    /// Exists because a frame measured from outside is just the rAF interval — the
    /// display cadence plus everything else on the page — which cannot tell our paint
    /// apart from the browser's own work. Three separate guesses at a slow frame were
    /// wrong before this existed.
    #[wasm_bindgen(js_name = paintStats)]
    pub fn paint_stats(&self) -> String {
        match self.cell.try_borrow() {
            Ok(cell) => format!(
                r#"{{"buildMs":{:.3},"paintMs":{:.3},"visible":{}}}"#,
                cell.stats.build_ms, cell.stats.paint_ms, cell.stats.visible
            ),
            Err(_) => String::from(r#"{"buildMs":0,"paintMs":0,"visible":0}"#),
        }
    }

    #[wasm_bindgen(constructor)]
    pub fn new(canvas: HtmlCanvasElement) -> Result<WasmEngine, JsValue> {
        let ctx = context_2d(&canvas)?;
        let mut engine = DrawEngine::new();
        engine.set_measure_text(measure_via_ctx);
        let cell = Rc::new(RefCell::new(EngineCell {
            stats: PaintStats::default(),
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
        LIVE.with(|live| live.borrow_mut().push(Rc::downgrade(&cell)));
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
        {
            // A single borrow for all three checks, and a failed borrow means a frame
            // is already in flight — which is exactly when there is nothing to do.
            let Ok(c) = cell.try_borrow() else {
                return;
            };
            if c.raf.is_some() || c.engine.is_disposed() {
                return;
            }
            // One question, asked of the engine. This used to spell out `dirty ||
            // in_motion` here and again below, which meant the browser loop held its own
            // opinion about when the engine still had work — and so never learned about
            // anything that animates on its own.
            if !c.engine.needs_frame() {
                return;
            }
        }
        let window = match web_sys::window() {
            Some(window) => window,
            None => return,
        };
        let cloned = cell.clone();
        let closure = Closure::once_into_js(move || {
            if let Ok(mut c) = cloned.try_borrow_mut() {
                c.raf = None;
            }
            paint_frame(&cloned);
            let more = match cloned.try_borrow() {
                Ok(cell) => cell.engine.needs_frame(),
                // Busy: assume there is more to do rather than stalling the loop.
                Err(_) => true,
            };
            if more {
                WasmEngine { cell: cloned }.schedule();
            }
        });
        if let Ok(id) = window.request_animation_frame(closure.as_ref().unchecked_ref()) {
            if let Ok(mut c) = cell.try_borrow_mut() {
                c.raf = Some(id);
            }
        }
    }
}

/// Paints one frame.
///
/// Uses `try_borrow_mut` rather than `borrow_mut` because this holds the cell for the
/// whole paint, and the paint calls into JavaScript hundreds of times. Anything that
/// re-enters during those calls — a wrapped canvas method from an extension or an
/// analytics shim, a synchronously dispatched event, a devtools override — would hit a
/// second `borrow_mut` and **panic**, and a panic in WASM aborts the instance: the
/// canvas is dead for the rest of the session with no way back.
///
/// Skipping a frame is always the better failure. The engine stays dirty, so the next
/// animation frame repaints it and nothing is lost but one frame.
fn paint_frame(cell: &Rc<RefCell<EngineCell>>) {
    let Ok(mut cell) = cell.try_borrow_mut() else {
        return;
    };
    if cell.engine.is_disposed() {
        return;
    }
    let started = now_ms();
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
    let built = now_ms();
    let visible = view.elements.len();
    CanvasPainter { ctx: &cell.ctx }.paint(&view);
    let painted = now_ms();
    drop(view);

    cell.engine.take_dirty();

    // Recorded so a slow frame can be attributed rather than guessed at. Measuring a
    // frame from outside only gives the rAF interval — the display cadence plus
    // everything else on the page — which cannot tell our paint apart from the
    // browser's own work.
    cell.stats = PaintStats {
        build_ms: built - started,
        paint_ms: painted - built,
        visible,
    };
}

fn emit_events(cell: &Rc<RefCell<EngineCell>>) {
    let (events, cbs) = {
        // Draining events calls host callbacks, which routinely call back into the
        // engine. Borrow only long enough to take the events, and never panic if a
        // callback is already inside us.
        let Ok(mut cell) = cell.try_borrow_mut() else {
            return;
        };
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
    // One callback carries both shapes. A delta is tagged so the host can tell them
    // apart, and the full form is kept for the structural changes a delta cannot
    // express — a reorder, a hard delete, an undo.
    if let Some(cb) = cbs.4 {
        if let Some(delta) = events.scene_delta {
            #[derive(serde::Serialize)]
            #[serde(rename_all = "camelCase")]
            struct DeltaEnvelope<'a> {
                #[serde(rename = "type")]
                kind: &'a str,
                version: u32,
                updated: &'a [crate::scene::DrawElement],
                removed: &'a [String],
            }
            let envelope = DeltaEnvelope {
                kind: "osidraw-delta",
                version: 1,
                updated: &delta.updated,
                removed: &delta.removed,
            };
            if let Ok(json) = serde_json::to_string(&envelope) {
                let _ = cb.call1(&JsValue::NULL, &JsValue::from_str(&json));
            }
        } else if let Some(json) = events.scene_json {
            let _ = cb.call1(&JsValue::NULL, &JsValue::from_str(&json));
        }
    }
}

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

thread_local! {
    /// A detached 2D context kept solely for measuring text.
    ///
    /// Detached because measuring must not disturb the canvas being drawn to: setting a
    /// font on the live context would fight the painter's own font cache. One context is
    /// created lazily and reused, so measuring a string costs a `measureText` call and
    /// nothing else.
    static MEASURE: std::cell::RefCell<Option<web_sys::CanvasRenderingContext2d>> =
        const { std::cell::RefCell::new(None) };
}

/// Runs `body` with a context whose font is already set to `font_size`.
fn with_measure_ctx<T>(
    font_size: f64,
    body: impl FnOnce(&web_sys::CanvasRenderingContext2d) -> T,
) -> Option<T> {
    MEASURE.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            let canvas = web_sys::window()?
                .document()?
                .create_element("canvas")
                .ok()?
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .ok()?;
            *cell = canvas
                .get_context("2d")
                .ok()
                .flatten()
                .and_then(|c| c.dyn_into::<web_sys::CanvasRenderingContext2d>().ok());
        }
        let ctx = cell.as_ref()?;
        ctx.set_font(&crate::font_string(font_size));
        Some(body(ctx))
    })
}

/// The width of one line, as the browser will actually draw it.
pub(crate) fn measure_line(line: &str, font_size: f64) -> f64 {
    with_measure_ctx(font_size, |ctx| {
        ctx.measure_text(line).map(|m| m.width()).unwrap_or(0.0)
    })
    .unwrap_or_else(|| estimate_line(line, font_size))
}

/// The fallback when there is no document to measure against — a server-side render, or
/// a context the browser refused to hand over.
///
/// Counts **characters**, not bytes. `str::len()` is the UTF-8 byte length, so the old
/// estimate made "café" a fifth too wide, "日本語" three times too wide, and every emoji
/// four times too wide.
fn estimate_line(line: &str, font_size: f64) -> f64 {
    line.chars().count() as f64 * font_size * 0.6
}

/// Measures text with the font it will be drawn with.
///
/// This used to estimate `bytes * fontSize * 0.6` despite its name, and the error was not
/// subtle: measured against a real canvas at 20px, "iiiiiiiiii" came out 170% too wide,
/// "WWWWWWWWWW" 36% too narrow, "Ω≈ç√∫" 198% too wide, and even "Hello" 32% too wide.
/// Everything downstream inherits that error — the selection frame, the hit test, where
/// the editing overlay sits, how a bound label is laid out, and the exported SVG.
fn measure_via_ctx(text: &str, font_size: f64) -> (f64, f64) {
    let lines: Vec<&str> = text.split('\n').collect();
    let width = lines
        .iter()
        .map(|line| measure_line(line, font_size))
        .fold(0.0, f64::max);
    (
        width.max(4.0),
        (lines.len() as f64 * font_size * crate::TEXT_LINE_HEIGHT).max(font_size),
    )
}
