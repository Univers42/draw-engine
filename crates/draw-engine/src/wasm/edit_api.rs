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

    /// Merges a peer's elements into the scene by id, last-writer-wins.
    ///
    /// Use this for anything arriving over the wire. `pasteJson` mints new ids, which
    /// is right for a paste and catastrophic for a merge. Optional `order` (live ids)
    /// rewrites z-order after the element merge.
    #[wasm_bindgen(js_name = applyRemotePatch)]
    pub fn apply_remote_patch(&self, json: &str) -> bool {
        let changed = self.cell.borrow_mut().engine.apply_remote_patch(json);
        self.flush();
        changed
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

    /// Ctrl+D: a copy ten units down and right, Excalidraw's `DEFAULT_GRID_SIZE / 2`
    /// (`packages/excalidraw/actions/actionDuplicateSelection.tsx:78-79`).
    #[wasm_bindgen(js_name = duplicateSelection)]
    pub fn duplicate_selection(&self) {
        self.cell
            .borrow_mut()
            .engine
            .duplicate_selection(10.0, 10.0);
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

    /// Whether align would move anything: two units, and no frame selected.
    #[wasm_bindgen(js_name = canAlign)]
    pub fn can_align(&self) -> bool {
        self.cell.borrow().engine.can_align()
    }

    /// Whether distribute would move anything: three units, and no frame selected.
    #[wasm_bindgen(js_name = canDistribute)]
    pub fn can_distribute(&self) -> bool {
        self.cell.borrow().engine.can_distribute()
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

    /// Ctrl+G: group what is loose, ungroup what is already exactly one group.
    ///
    /// A deliberate divergence — the oracle's Ctrl+G is a no-op on a grouped selection,
    /// which leaves the key with no inverse. See `docs/reference/groups.md`.
    #[wasm_bindgen(js_name = toggleGroupSelection)]
    pub fn toggle_group_selection(&self) {
        self.cell.borrow_mut().engine.toggle_group_selection();
        self.flush();
    }

    /// The group that has been stepped into, if any.
    #[wasm_bindgen(js_name = editingGroupId)]
    pub fn editing_group_id(&self) -> Option<String> {
        self.cell.borrow().engine.editing_group_id()
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

    /// Everything the properties panel shows, in one call. Ask once per `styleRevision`.
    #[wasm_bindgen(js_name = selectionStyleJson)]
    pub fn selection_style_json(&self) -> String {
        serde_json::to_string(&self.cell.borrow().engine.selection_style())
            .unwrap_or_else(|_| "null".into())
    }

    /// Moves whenever `selectionStyleJson` may have.
    #[wasm_bindgen(js_name = styleRevision)]
    pub fn style_revision(&self) -> u32 {
        self.cell.borrow().engine.style_revision()
    }

    /// Shows a style without committing it — a slider in motion. `applyStyleJson` commits.
    #[wasm_bindgen(js_name = previewStyleJson)]
    pub fn preview_style_json(&self, json: &str) {
        if let Ok(patch) = serde_json::from_str::<DrawElementStylePatch>(json) {
            self.cell.borrow_mut().engine.preview_style(patch);
            self.flush();
        }
    }

    #[wasm_bindgen(js_name = copyStyles)]
    pub fn copy_styles(&self) -> bool {
        self.cell.borrow_mut().engine.copy_styles()
    }

    #[wasm_bindgen(js_name = pasteStyles)]
    pub fn paste_styles(&self) {
        self.cell.borrow_mut().engine.paste_styles();
        self.flush();
    }

    /// Ctrl/Cmd+Shift+> (`true`) and < (`false`): the selected texts a tenth bigger or
    /// smaller, each from its own size.
    #[wasm_bindgen(js_name = stepFontSize)]
    pub fn step_font_size(&self, increase: bool) {
        self.cell.borrow_mut().engine.step_font_size(increase);
        self.flush();
    }

    /// `"sharp"` or `"round"` for the selected arrows and the next one. Anything else —
    /// `"elbow"`, which the engine does not route — is ignored.
    #[wasm_bindgen(js_name = setArrowType)]
    pub fn set_arrow_type(&self, arrow_type: &str) {
        if let Some(arrow_type) = crate::engine::ArrowType::parse(arrow_type) {
            self.cell.borrow_mut().engine.set_arrow_type(arrow_type);
            self.flush();
        }
    }

    /// The font picker's hover: a family shown on the selected texts uncommitted, or —
    /// with none — what the last hover changed given back. `setFontFamily` commits.
    #[wasm_bindgen(js_name = previewFontFamily)]
    pub fn preview_font_family(&self, family: Option<u8>) {
        self.cell.borrow_mut().engine.preview_font_family(family);
        self.flush();
    }

    /// The families the board's texts use, each once: the picker's "In this scene".
    #[wasm_bindgen(js_name = sceneFontFamilies)]
    pub fn scene_font_families(&self) -> Vec<u8> {
        self.cell.borrow().engine.scene_font_families()
    }

    /// "Bind text to the container": a free text and one empty shape selected.
    #[wasm_bindgen(js_name = bindText)]
    pub fn bind_text(&self) {
        self.cell.borrow_mut().engine.bind_text();
        self.flush();
    }

    /// "Unbind text": each selected shape's label becomes free text.
    #[wasm_bindgen(js_name = unbindText)]
    pub fn unbind_text(&self) {
        self.cell.borrow_mut().engine.unbind_text();
        self.flush();
    }

    /// "Wrap text in a container": a rectangle around each selected free text.
    #[wasm_bindgen(js_name = wrapTextInContainer)]
    pub fn wrap_text_in_container(&self) {
        self.cell.borrow_mut().engine.wrap_text_in_container();
        self.flush();
    }

    /// `[[colour, count], …]` over the live board, for the picker's most-used colours.
    #[wasm_bindgen(js_name = colorCountsJson)]
    pub fn color_counts_json(&self, background: bool) -> String {
        serde_json::to_string(&self.cell.borrow().engine.color_counts(background))
            .unwrap_or_else(|_| "[]".into())
    }
}
