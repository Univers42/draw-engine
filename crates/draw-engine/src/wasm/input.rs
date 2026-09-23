use serde::Serialize;
use wasm_bindgen::prelude::*;

use super::WasmEngine;
use crate::engine::{DebugState, DrawEngine};
use crate::scene::Scene;

/// The engine's own state, plus the render timings only the frame loop can see.
///
/// Flattened rather than nested so the JSON reads as one object with four sections —
/// `scene`, `viewport`, `interaction`, `rendering` — which is the shape an inspector
/// wants, rather than `state.scene` alongside `rendering`.
#[derive(Serialize)]
struct DebugSnapshot {
    #[serde(flatten)]
    state: DebugState,
    rendering: DebugRendering,
}

/// What a frame costs, and what it cost over the last two seconds.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DebugRendering {
    /// Every frame since the engine started.
    frames: u64,
    /// Wall time between frames — display cadence plus everything else on the page.
    median_frame_ms: f64,
    p95_frame_ms: f64,
    /// Our own CPU, split into assembling the display list and issuing the drawing.
    /// Kept apart from the interval: a slow interval with a fast paint means the time is
    /// going somewhere that is not us.
    median_build_ms: f64,
    median_paint_ms: f64,
    p95_paint_ms: f64,
    last_build_ms: f64,
    last_paint_ms: f64,
    /// After viewport culling. Compare against `scene.elementCount` to see whether
    /// culling is doing anything.
    elements_rendered: usize,
    /// Rough geometry reused vs regenerated. A miss count climbing during a pan or a
    /// drag means the cache fingerprint covers something it should not.
    shape_cache_hits: u64,
    shape_cache_misses: u64,
    shape_cache_len: usize,
    /// Device pixels, not CSS pixels. This over the viewport size is the dpr in force.
    canvas_width: u32,
    canvas_height: u32,
    /// Whether another frame is already owed. There are no dirty *regions* here — the
    /// whole canvas repaints — so this is a boolean and that is the whole truth.
    dirty: bool,
}

/// The name a small enum travels under, which is the one serde writes into a scene.
///
/// Through serde rather than a second `match` in this layer, so the names have one
/// definition: renaming a variant in `element.rs` reaches the host automatically instead
/// of leaving the binding quietly disagreeing with the file format.
fn wire_name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .map(|json| json.trim_matches('"').to_string())
        .unwrap_or_default()
}

/// The inverse. An unknown name yields `None` and the caller does nothing — the host
/// must not be able to put a value into the scene that the engine cannot read back.
fn from_wire<T: serde::de::DeserializeOwned>(name: &str) -> Option<T> {
    serde_json::from_str(&format!("\"{name}\"")).ok()
}

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

    /// Choose a tool the way a keyboard shortcut does, with the toggle tools' return trip.
    ///
    /// Separate from `setTool` because a toolbar button must not toggle: it shows the
    /// tool as active, so switching away on a second click would contradict the screen.
    #[wasm_bindgen(js_name = activateTool)]
    pub fn activate_tool(&self, tool: &str) {
        if let Ok(tool) = serde_json::from_str(&format!("\"{tool}\"")) {
            self.cell.borrow_mut().engine.activate_tool(tool);
            self.flush();
        }
    }

    /// Put a decoded image on the board.
    ///
    /// The host decodes the file — only a browser can — and having done so already knows
    /// the natural size. Everything after that is arithmetic and happens in the engine,
    /// so two frontends dropping the same file in the same place produce the same
    /// element. Returns the new element's id, or `undefined` if the image could not be
    /// measured.
    #[wasm_bindgen(js_name = insertImage)]
    pub fn insert_image(
        &self,
        data_url: &str,
        natural_width: f64,
        natural_height: f64,
        sx: f64,
        sy: f64,
    ) -> Option<String> {
        let id = self.cell.borrow_mut().engine.insert_image(
            data_url,
            natural_width,
            natural_height,
            sx,
            sy,
        );
        self.flush();
        id
    }

    /// Put an embed on the board. `raw_url` is whatever the person pasted.
    ///
    /// Whether the host may be framed at all, and what the pasted page rewrites to, are
    /// the engine's to decide — a host that resolved links itself could frame something
    /// the rules would have refused. Returns the new element's id, or `undefined`.
    #[wasm_bindgen(js_name = insertEmbed)]
    pub fn insert_embed(&self, raw_url: &str, sx: f64, sy: f64) -> Option<String> {
        let id = self.cell.borrow_mut().engine.insert_embed(raw_url, sx, sy);
        self.flush();
        id
    }

    /// Whether a pasted link can be embedded, and what it resolves to, as JSON.
    ///
    /// Lets a host tell someone their link will not work *before* it puts an empty box
    /// on the board for them.
    #[wasm_bindgen(js_name = resolveEmbed)]
    pub fn resolve_embed(&self, raw_url: &str) -> Option<String> {
        let resolved = crate::scene::embed_link(raw_url)?;
        serde_json::to_string(&serde_json::json!({
            "url": resolved.url,
            "intrinsicWidth": resolved.intrinsic_width,
            "intrinsicHeight": resolved.intrinsic_height,
            "kind": match resolved.kind {
                crate::scene::EmbedKind::Video => "video",
                crate::scene::EmbedKind::Generic => "generic",
            },
            "allowSameOrigin": resolved.allow_same_origin,
        }))
        .ok()
    }

    /// The embeds on screen and where their frames go, as JSON.
    ///
    /// Screen pixels, because the host positions real `<iframe>` elements over the
    /// canvas and the camera is the engine's. A host doing this conversion itself would
    /// drift away from the rectangle drawn under it as soon as anyone panned.
    #[wasm_bindgen(js_name = embedFrames)]
    pub fn embed_frames(&self) -> String {
        let frames = self.cell.borrow().engine.embed_frames();
        serde_json::to_string(&frames).unwrap_or_else(|_| "[]".into())
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

    /// Whether a line or arrow is being placed point by point right now.
    ///
    /// The host needs this every pointer move, because a path is the one thing in the
    /// engine that follows the cursor with **no button held** — so while it is true the
    /// canvas has to keep forwarding moves it would otherwise drop.
    ///
    /// `try_borrow` for the same reason [`Self::hover_cursor`] uses it: this is on the
    /// pointer-move path, and a panic there aborts the WASM instance for good.
    #[wasm_bindgen(js_name = linearInProgress)]
    pub fn linear_in_progress(&self) -> bool {
        match self.cell.try_borrow() {
            Ok(state) => state.engine.linear_in_progress().is_some(),
            Err(_) => false,
        }
    }

    /// Ends a path being placed, keeping what has been placed so far.
    ///
    /// Enter's binding and Escape's. Not a discard — see [`DrawEngine::finish_linear`].
    #[wasm_bindgen(js_name = finishLinear)]
    pub fn finish_linear(&self) {
        self.with_now(DrawEngine::finish_linear);
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

    /// The size of a run of text, in world units, using the font the canvas draws with.
    ///
    /// Exposed so the host's editing overlay can size itself from the same measurement
    /// the element gets, instead of estimating. It had its own guess —
    /// `maxLineLength * fontSize * 0.65` — so the textarea, the painted glyphs and the
    /// element's own box were three different widths.
    ///
    /// Returns `[width, height]`.
    #[wasm_bindgen(js_name = measureText)]
    pub fn measure_text(&self, text: &str, font_size: f64) -> Vec<f64> {
        let lines: Vec<&str> = text.split('\n').collect();
        let width = lines
            .iter()
            .map(|line| super::measure_line(line, font_size))
            .fold(0.0_f64, f64::max);
        vec![
            width.max(4.0),
            (lines.len() as f64 * font_size * crate::TEXT_LINE_HEIGHT).max(font_size),
        ]
    }

    /// The CSS font family every piece of text is drawn with.
    ///
    /// The host needs it so its editing overlay renders in the same face; a textarea in
    /// a different font shifts the text visibly the moment an edit is committed.
    #[wasm_bindgen(js_name = fontFamily)]
    pub fn font_family(&self) -> String {
        crate::FONT_FAMILY.to_string()
    }

    /// Replaces the grid settings.
    ///
    /// Takes JSON so the shape can gain fields without breaking the binding — the grid
    /// gained `snap` separately from `enabled` for exactly that reason.
    #[wasm_bindgen(js_name = setGridJson)]
    pub fn set_grid_json(&self, json: &str) {
        if let Ok(grid) = serde_json::from_str(json) {
            self.cell.borrow_mut().engine.set_grid(grid);
            self.flush();
        }
    }

    #[wasm_bindgen(js_name = getGridJson)]
    pub fn get_grid_json(&self) -> String {
        serde_json::to_string(&self.cell.borrow().engine.grid()).unwrap_or_default()
    }

    #[wasm_bindgen(js_name = zoomAt)]
    pub fn zoom_at(&self, sx: f64, sy: f64, factor: f64) {
        self.cell.borrow_mut().engine.zoom_at(sx, sy, factor);
        self.flush();
    }

    /// One wheel event's worth of zoom, anchored at the cursor.
    ///
    /// Takes the raw `WheelEvent.deltaY` rather than a factor the host worked out: what a
    /// wheel delta is worth differs per browser and per device, and it is arithmetic, so
    /// it belongs here where every host shares one answer.
    #[wasm_bindgen(js_name = wheelZoom)]
    pub fn wheel_zoom(&self, sx: f64, sy: f64, delta_y: f64) {
        self.cell.borrow_mut().engine.wheel_zoom(sx, sy, delta_y);
        self.flush();
    }

    /// Everything the engine knows about itself, in one call.
    ///
    /// The scene, geometry, hit testing and selection all live in WASM and none of it is
    /// reachable from the DOM, so without this anyone debugging from outside is reduced
    /// to inferring state from pixels. This is what `tools/editor-inspector` reads.
    ///
    /// Merges two sources that cannot be merged anywhere else: the engine's own state,
    /// which is runtime-agnostic and knows nothing about clocks, and the render timings,
    /// which only exist here because a frame is only visible from the frame loop.
    ///
    /// `try_borrow` like the rest of the re-entrant surface — a snapshot taken while a
    /// frame is in flight returns nothing rather than panicking, and a panic in WASM
    /// aborts the instance for the rest of the session.
    #[wasm_bindgen(js_name = debugSnapshotJson)]
    pub fn debug_snapshot_json(&self) -> String {
        let Ok(cell) = self.cell.try_borrow() else {
            return "{}".into();
        };
        let (hits, misses) = crate::wasm::paint::shape_cache_stats();
        let snapshot = DebugSnapshot {
            state: cell.engine.debug_state(),
            rendering: DebugRendering {
                frames: cell.frames.total,
                // A distribution, not a number: frame costs are a floor with spikes, and
                // one sample cannot tell a slow frame from a slow session.
                median_frame_ms: cell.frames.quantile(|f| f.interval_ms, 0.5),
                p95_frame_ms: cell.frames.quantile(|f| f.interval_ms, 0.95),
                median_build_ms: cell.frames.quantile(|f| f.build_ms, 0.5),
                median_paint_ms: cell.frames.quantile(|f| f.paint_ms, 0.5),
                p95_paint_ms: cell.frames.quantile(|f| f.paint_ms, 0.95),
                last_build_ms: cell.stats.build_ms,
                last_paint_ms: cell.stats.paint_ms,
                elements_rendered: cell.stats.visible,
                shape_cache_hits: hits,
                shape_cache_misses: misses,
                shape_cache_len: crate::wasm::paint::shape_cache_len(),
                canvas_width: cell.canvas.width(),
                canvas_height: cell.canvas.height(),
                dirty: cell.engine.needs_frame(),
            },
        };
        serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".into())
    }

    /// How the last frames were served: `{redraws, scrolls, reuses}`, then reset.
    ///
    /// A diagnostic, not an API. The static layer is either being reused or it is not,
    /// and from outside the engine those two look identical until something is measured
    /// against the wrong assumption.
    #[wasm_bindgen(js_name = paintStatsJson)]
    pub fn paint_stats_json(&self) -> String {
        let (redraws, scrolls, reuses) = crate::wasm::paint::take_plan_counts();
        format!("{{\"redraws\":{redraws},\"scrolls\":{scrolls},\"reuses\":{reuses}}}")
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

    #[wasm_bindgen(js_name = zoomToSelection)]
    pub fn zoom_to_selection(&self, padding: Option<f64>) {
        self.cell
            .borrow_mut()
            .engine
            .zoom_to_selection(padding.unwrap_or(96.0));
        self.flush();
    }

    /// Move by a screenful. `pages_x`/`pages_y` are counts, so -1 is one page back.
    #[wasm_bindgen(js_name = pageBy)]
    pub fn page_by(&self, pages_x: f64, pages_y: f64) {
        self.cell.borrow_mut().engine.page_by(pages_x, pages_y);
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

    /// Re-widths a dragged-out text column and re-wraps it. Ignored for auto-sizing
    /// text, which has no width of its own to impose.
    #[wasm_bindgen(js_name = setTextBoxWidth)]
    pub fn set_text_box_width(&self, id: &str, width: f64) {
        self.cell.borrow_mut().engine.set_text_box_width(id, width);
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

    /// `"left"`, `"center"` or `"right"`. Anything else is ignored rather than coerced,
    /// so a typo in the host is a control that does nothing instead of a scene holding a
    /// value the engine will not read back.
    #[wasm_bindgen(js_name = setTextAlign)]
    pub fn set_text_align(&self, align: &str) {
        if let Some(align) = from_wire(align) {
            self.cell.borrow_mut().engine.set_text_align(align);
            self.flush();
        }
    }

    #[wasm_bindgen(js_name = getTextAlign)]
    pub fn get_text_align(&self) -> String {
        wire_name(&self.cell.borrow().engine.get_text_align())
    }

    /// `"top"`, `"middle"` or `"bottom"`.
    #[wasm_bindgen(js_name = setVerticalAlign)]
    pub fn set_vertical_align(&self, align: &str) {
        if let Some(align) = from_wire(align) {
            self.cell.borrow_mut().engine.set_vertical_align(align);
            self.flush();
        }
    }

    #[wasm_bindgen(js_name = getVerticalAlign)]
    pub fn get_vertical_align(&self) -> String {
        wire_name(&self.cell.borrow().engine.get_vertical_align())
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
