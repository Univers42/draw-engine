use crate::engine::DrawEngine;
use crate::scene::{bump_version, is_linear_element, DrawElement};

impl DrawEngine {
    /// The live label of `container` that is ours to change: not one a peer holds —
    /// typing into a label holds the label alone, and its shape stays selectable — nor a
    /// locked one (`untouchable`). Every write that reaches a label through its shape goes
    /// through here: the style panel's (`selection_style.rs`) and the text rows' below.
    pub(super) fn label_of(&self, container: &DrawElement) -> Option<&DrawElement> {
        container
            .bound_text_id
            .as_deref()
            .and_then(|id| self.scene.get(id))
            .filter(|label| !label.is_deleted && !self.untouchable(label))
    }

    /// Whether a style may change `element`: not a locked one, nor the label of a locked
    /// shape. The oracle cannot select a locked element (`shouldIgnoreElementFromSelection`,
    /// `packages/element/src/selection.ts@1118751f:33-34`), so none of its styles reaches
    /// one. This engine's Select All takes locked elements, so they can be unlocked from
    /// the menu; a style chosen then passes them by.
    pub(super) fn restylable(&self, element: &DrawElement) -> bool {
        !element.locked()
            && self
                .container_of(element)
                .is_none_or(|container| !container.locked())
    }

    pub fn set_arrowheads(
        &mut self,
        start: Option<crate::scene::Arrowhead>,
        end: Option<crate::scene::Arrowhead>,
    ) {
        let linears: Vec<_> = self
            .get_selected_elements()
            .into_iter()
            .filter(is_linear_element)
            .collect();
        if linears.is_empty() {
            return;
        }
        let now = self.now_ms;
        for mut element in linears {
            if let Some(kind) = start {
                element.start_arrowhead = Some(kind);
            }
            if let Some(kind) = end {
                element.end_arrowhead = Some(kind);
            }
            self.scene.put(bump_version(element, now));
        }
        self.push_history();
        self.request_draw();
    }

    pub fn set_font_size(&mut self, size: f64) {
        self.next_font_size = size;
        self.touch_style();
        // Through `selected_texts` rather than the raw selection, so resizing works with
        // a labelled shape selected — which is the only thing you *can* select once a
        // shape has a label.
        self.relayout_selected_texts(|text| text.font_size = Some(size), true);
    }

    /// Writes `change` into every selected text, lays each out again from its source
    /// (`redrawTextBoundingBox`), grows the shapes that no longer hold their labels, and
    /// commits — which stamps only what changed. With `anchor_font_resize`, a free
    /// auto-sizing text keeps its aligned edge and its vertical middle
    /// (`offsetElementAfterFontResize`); otherwise a free text stays where it is, as the
    /// oracle's family and alignment changes leave it.
    fn relayout_selected_texts(
        &mut self,
        change: impl Fn(&mut DrawElement),
        anchor_font_resize: bool,
    ) {
        let texts = self.selected_texts();
        if texts.is_empty() {
            return;
        }
        for prev in texts {
            let mut next = prev.clone();
            change(&mut next);
            let mut laid = self.laid_out(&next);
            if anchor_font_resize {
                if let Some(at) = crate::text::layout::font_resize_anchor(&prev, &laid.text) {
                    laid.text.x = at.x;
                    laid.text.y = at.y;
                }
            }
            if let Some(container) = laid.container {
                self.scene.put(container);
            }
            self.scene.put(laid.text);
        }
        self.apply_bindings();
        self.push_history();
        self.request_draw();
    }

    /// The family for the selected text, or for the next text written when nothing is
    /// selected. The family's own line height comes with it (`changeFontFamily`,
    /// `actionProperties.tsx@1118751f:1285-1290`), and the text is laid out again where
    /// it stands. An id the engine does not draw with is ignored.
    pub fn set_font_family(&mut self, family: u8) {
        let Some(line_height) = crate::text::font::family(family).map(|f| f.line_height) else {
            return;
        };
        self.next_font_family = family;
        self.relayout_selected_texts(
            |text| {
                text.font_family = Some(family);
                text.line_height = Some(line_height);
            },
            false,
        );
    }

    /// The family of the first selected text — `0` for the system stack a text with none
    /// is drawn in — or the next text's when nothing is selected.
    pub fn get_font_family(&self) -> u8 {
        self.selected_texts()
            .first()
            .map_or(self.next_font_family, |text| {
                crate::scene::resolved_font_family(text).unwrap_or(crate::text::FontKey::LEGACY)
            })
    }

    /// Whether the selected free texts size to their text (`true`) or keep a fixed width
    /// their lines wrap in (`false`). Back to auto-sizing, a text takes its typed lines
    /// and its measured size, and the point its alignment pins stays put
    /// (`actionTextAutoResize`, `actions/actionTextAutoResize.ts@1118751f`), and the
    /// arrows bound to it are re-routed to its new box (`updateBoundElements` there). To
    /// fixed, it keeps the width it has. Labels are left alone: their shape decides.
    pub fn set_text_auto_resize(&mut self, auto_resize: bool) {
        let texts: Vec<DrawElement> = self
            .selected_texts()
            .into_iter()
            .filter(|text| text.container_id.is_none())
            .filter(|text| crate::scene::is_auto_resize(text) != auto_resize)
            .collect();
        if texts.is_empty() {
            return;
        }
        for prev in texts {
            let mut next = prev.clone();
            next.auto_resize = Some(auto_resize);
            let mut laid = self.laid_out(&next);
            if auto_resize {
                let at = crate::text::layout::auto_resize_anchor(
                    &prev,
                    laid.text.width,
                    laid.text.height,
                );
                laid.text.x = at.x;
                laid.text.y = at.y;
            }
            self.scene.put(laid.text);
        }
        self.apply_bindings();
        self.push_history();
        self.request_draw();
    }

    /// Whether the selected labels wrap inside their shapes (`true`, the oracle's only
    /// way) or keep their typed lines and widen the shape to hold them (`false`).
    pub fn set_label_wrap(&mut self, wrap: bool) {
        let any_label = self
            .selected_texts()
            .iter()
            .any(|text| text.container_id.is_some());
        if !any_label {
            return;
        }
        self.relayout_selected_texts(
            |text| {
                if text.container_id.is_some() {
                    text.wrap = Some(wrap);
                }
            },
            false,
        );
    }

    pub fn get_font_size(&self) -> f64 {
        self.selected_texts()
            .into_iter()
            .next()
            .and_then(|el| el.font_size)
            .unwrap_or(self.next_font_size)
    }

    /// Every text the alignment controls should act on, for the current selection.
    ///
    /// A bound label is not separately selectable — clicking a shape with a label in it
    /// selects the shape — so following `bound_text_id` is not a convenience here, it is
    /// the difference between the control working on labels and being dead for all of
    /// them. A label a peer holds is not followed ([`Self::label_of`]), and nothing a
    /// style may not change is taken ([`Self::restylable`]). Deduplicated by id, because
    /// selecting a shape *and* a loose text must not visit anything twice.
    fn selected_texts(&self) -> Vec<crate::scene::DrawElement> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for element in self.get_selected_elements() {
            if !self.restylable(&element) {
                continue;
            }
            let candidate = if element.kind == crate::scene::DrawElementType::Text {
                Some(element)
            } else {
                self.label_of(&element).cloned()
            };
            if let Some(text) = candidate {
                if seen.insert(text.id.clone()) {
                    out.push(text);
                }
            }
        }
        out
    }

    /// Horizontal alignment for the selected text, or for the next text drawn when
    /// nothing is selected — the same way the stroke colour behaves. Without the second
    /// half, choosing an alignment before typing would do nothing and read as a dead
    /// button.
    pub fn set_text_align(&mut self, align: crate::scene::TextAlign) {
        self.next_text_align = Some(align);
        self.touch_style();
        let texts = self.selected_texts();
        if texts.is_empty() {
            return;
        }
        self.relayout_selected_texts(|text| text.text_align = Some(align), false);
    }

    pub fn get_text_align(&self) -> crate::scene::TextAlign {
        self.selected_texts()
            .first()
            .map(crate::scene::resolved_text_align)
            .or(self.next_text_align)
            .unwrap_or(crate::scene::TextAlign::Left)
    }

    /// Vertical alignment. Unlike the horizontal one this is a *position*: the label is a
    /// real element with its own `y`, so the value has to be followed by a relayout or
    /// the label stays where it was until something unrelated moves it.
    pub fn set_vertical_align(&mut self, align: crate::scene::VerticalAlign) {
        self.next_vertical_align = Some(align);
        self.touch_style();
        let texts = self.selected_texts();
        if texts.is_empty() {
            return;
        }
        self.relayout_selected_texts(|text| text.vertical_align = Some(align), false);
    }

    pub fn get_vertical_align(&self) -> crate::scene::VerticalAlign {
        self.selected_texts()
            .first()
            .map(crate::scene::resolved_vertical_align)
            .or(self.next_vertical_align)
            .unwrap_or(crate::scene::VerticalAlign::Top)
    }

    pub fn zoom_at(&mut self, sx: f64, sy: f64, factor: f64) {
        self.set_camera(crate::zoom_at(self.camera, sx, sy, factor));
        self.bump_motion();
    }

    /// One wheel event, anchored at the cursor.
    ///
    /// The host passes the raw `deltaY` and nothing else: how far a wheel event is worth
    /// is arithmetic, it differs per browser and per device, and every platform this
    /// engine is embedded in would otherwise have to get it right independently.
    ///
    /// Returns early when the step changes nothing, which is the common case at either
    /// zoom limit — a trackpad goes on sending ticks for as long as the fingers move, and
    /// publishing a camera event for each would repaint the whole scene to produce the
    /// identical picture.
    pub fn wheel_zoom(&mut self, sx: f64, sy: f64, delta_y: f64) {
        let next = crate::camera::wheel_zoom_scale(self.camera.scale, delta_y);
        if next == self.camera.scale {
            return;
        }
        self.set_camera(crate::zoom_to(self.camera, sx, sy, next));
        self.bump_motion();
    }

    pub fn set_camera(&mut self, camera: crate::camera::Camera) {
        self.camera = camera;
        self.events.camera = Some(camera);
        self.request_draw();
    }

    pub fn pan_by(&mut self, dx_screen: f64, dy_screen: f64) {
        self.set_camera(crate::pan_by(self.camera, dx_screen, dy_screen));
        self.bump_motion();
    }

    pub fn fit(&mut self, padding: f64) {
        if let Some(bounds) = self.scene.bounds() {
            self.set_camera(crate::fit_bounds(bounds, self.width, self.height, padding));
        }
    }

    /// Frame what is selected, rather than the whole board.
    ///
    /// The command `fit` cannot stand in for: a fit has to hold everything, so the thing
    /// you are working on ends up as small as the furthest stray shape allows. Does
    /// nothing when the selection is empty — framing "nothing" has no meaning, and
    /// jumping somewhere arbitrary is the worst of the available answers.
    pub fn zoom_to_selection(&mut self, padding: f64) {
        let selected = self.get_selected_elements();
        if selected.is_empty() {
            return;
        }
        let Some(bounds) = crate::scene_bounds(selected.iter()) else {
            return;
        };
        self.set_camera(crate::fit_bounds(bounds, self.width, self.height, padding));
    }

    /// Move by a screenful, in units of pages.
    ///
    /// `PAGE_OVERLAP` is the point: moving exactly one screen leaves nothing in common
    /// between the two views, so you lose your place at every press. Keeping a strip of
    /// the old view is what makes paging better than dragging.
    ///
    /// Screen pixels, not world units, so a page is a screenful at every zoom level.
    /// Scaling it by the zoom would make paging useless exactly when it is most needed.
    pub fn page_by(&mut self, pages_x: f64, pages_y: f64) {
        /// How much of the outgoing view is still visible after a page.
        const PAGE_OVERLAP: f64 = 0.15;
        let step_x = self.width * (1.0 - PAGE_OVERLAP);
        let step_y = self.height * (1.0 - PAGE_OVERLAP);
        self.pan_by(-pages_x * step_x, -pages_y * step_y);
    }

    pub fn zoom_in(&mut self) {
        self.zoom_at(self.width / 2.0, self.height / 2.0, 1.2);
    }

    pub fn zoom_out(&mut self) {
        self.zoom_at(self.width / 2.0, self.height / 2.0, 1.0 / 1.2);
    }

    pub fn zoom_reset(&mut self) {
        self.zoom_at(self.width / 2.0, self.height / 2.0, 1.0 / self.camera.scale);
    }

    pub fn content_in_view(&self) -> bool {
        let Some(bounds) = self.scene.bounds() else {
            return true;
        };
        let view = crate::visible_world_rect(self.camera, self.width, self.height);
        bounds.min_x <= view.max_x
            && bounds.max_x >= view.min_x
            && bounds.min_y <= view.max_y
            && bounds.max_y >= view.min_y
    }
}
