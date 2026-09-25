use std::cell::OnceCell;
use std::collections::HashSet;

use super::ArrowType;
use crate::camera::{Camera, WorldBounds};
use crate::engine::DrawEngine;
use crate::interaction::ease_out;
use crate::math::lerp;
use crate::scene::{is_linear_element, Arrowhead, DrawElement, DrawElementType};

/// The font sizes the contract stores (`fontSize`, `packages/contract/src/element.ts`).
/// The oracle has no ceiling; a size above this one would never save.
const FONT_SIZES: std::ops::RangeInclusive<f64> = 1.0..=1000.0;

/// `FONT_SIZE_RELATIVE_INCREASE_STEP` (`actions/actionProperties.tsx@1118751f:182`).
const FONT_SIZE_STEP: f64 = 0.1;

/// The oracle's hover preview skips a selection holding more texts, or more characters,
/// than this: every hover lays all of it out again (`actionProperties.tsx@1118751f:1203-1225`).
const PREVIEW_MAX_TEXTS: usize = 200;
const PREVIEW_MAX_CHARS: usize = 5000;

/// A size as the contract would store it, or `None` for one that is not a number.
fn storable_font_size(size: f64) -> Option<f64> {
    size.is_finite()
        .then(|| size.clamp(*FONT_SIZES.start(), *FONT_SIZES.end()))
}

/// How long a reveal takes to ease in, matching the oracle's own
/// `animation: { duration: 300 }` (`App.tsx@1118751f`, `revealIfHidden`).
const CAMERA_REVEAL_MS: f64 = 300.0;

/// An in-flight camera move: linear in `x`/`y`/`scale`, eased in time. `DrawEngine::set_now`
/// ticks it; [`DrawEngine::reveal`] is the one place that starts one.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CameraAnim {
    from: Camera,
    to: Camera,
    start_ms: f64,
    end_ms: f64,
}

impl DrawEngine {
    /// The live label of `container` that a style reaching the shape may change too
    /// ([`Self::restylable`]): not one a peer holds — typing into a label holds the label
    /// alone, and its shape stays selectable — nor a locked one the selection does not
    /// carry. Every write that reaches a label through its shape goes through here: the
    /// style panel's (`selection_style.rs`) and the text rows' below.
    pub(super) fn label_of(
        &self,
        container: &DrawElement,
        carried: &OnceCell<HashSet<String>>,
    ) -> Option<&DrawElement> {
        container
            .bound_text_id
            .as_deref()
            .and_then(|id| self.scene.get(id))
            .filter(|label| !label.is_deleted && self.restylable(label, carried))
    }

    /// Whether a style may change `element`, one of the selection or the label of one.
    /// `carried` is [`Self::carried_selection`], worked out the first time a locked
    /// element needs it — and so, with none selected, never: worked out for every read,
    /// it cost the panel's read of a whole selected board 80% more.
    ///
    /// Not a locked element the selection holds without carrying it. This engine's Select
    /// All takes a loose locked element, so it can be unlocked from the menu, where the
    /// oracle's skips it (`actions/actionSelectAll.ts@1118751f:32-38`); a style chosen
    /// then passes it by. A locked member of a selected group is carried, and is
    /// restyled: the oracle selects every member of a clicked group with no lock filter
    /// (`selectGroupsForSelectedElements`, `packages/element/src/groups.ts@1118751f:66-140`)
    /// and restyles every selected element (`changeProperty`,
    /// `actions/actionProperties.tsx@1118751f:193-223`) — its lock filter,
    /// `shouldIgnoreElementFromSelection` (`packages/element/src/selection.ts@1118751f:33-34`),
    /// is the marquee's alone.
    ///
    /// Nor anything a peer holds, nor the label of a shape that is not ours to change —
    /// locked and not carried, or held by a peer: a label is laid out in its shape and
    /// grows it, so restyling the words restyles the shape.
    pub(super) fn restylable(
        &self,
        element: &DrawElement,
        carried: &OnceCell<HashSet<String>>,
    ) -> bool {
        let ours = |el: &DrawElement| {
            !self.untouchable(el)
                || carried
                    .get_or_init(|| self.carried_selection())
                    .contains(&el.id)
        };
        ours(element) && self.container_of(element).is_none_or(ours)
    }

    /// The heads of the selected lines and arrows — `None` leaves that end as it is — and
    /// always of the next arrow drawn: `actionChangeArrowhead` sets
    /// `currentItemStartArrowhead` / `currentItemEndArrowhead` whatever is selected
    /// (`actions/actionProperties.tsx@1118751f:1944-1982`), and a new arrow is drawn with
    /// them (`components/App.tsx@1118751f:10251-10254`). What a style may not change is
    /// passed by ([`Self::restylable`]), and an end that already has the head is no edit.
    pub fn set_arrowheads(&mut self, start: Option<Arrowhead>, end: Option<Arrowhead>) {
        self.next_start_arrowhead = start.or(self.next_start_arrowhead);
        self.next_end_arrowhead = end.or(self.next_end_arrowhead);
        self.touch_style();
        let carried = OnceCell::new();
        let changed: Vec<DrawElement> = self
            .get_selected_elements()
            .into_iter()
            .filter(|element| is_linear_element(element) && self.restylable(element, &carried))
            .filter_map(|mut element| {
                let before = (element.start_arrowhead, element.end_arrowhead);
                element.start_arrowhead = start.or(element.start_arrowhead);
                element.end_arrowhead = end.or(element.end_arrowhead);
                (before != (element.start_arrowhead, element.end_arrowhead)).then_some(element)
            })
            .collect();
        if changed.is_empty() {
            return;
        }
        for element in changed {
            self.scene.put(element);
        }
        self.commit_style();
        self.request_draw();
    }

    /// The Arrow type row for the selected arrows, and always the next arrow's type, as
    /// `actionChangeArrowType` sets `currentItemArrowType` (`:2057-2242`, `:2229`). An
    /// arrow already of the type keeps the roundness it has.
    pub fn set_arrow_type(&mut self, arrow_type: ArrowType) {
        self.next_arrow_type = arrow_type;
        self.touch_style();
        let carried = OnceCell::new();
        let changed: Vec<DrawElement> = self
            .get_selected_elements()
            .into_iter()
            .filter(|element| {
                element.kind == DrawElementType::Arrow
                    && ArrowType::of(element.roundness) != arrow_type
                    && self.restylable(element, &carried)
            })
            .collect();
        if changed.is_empty() {
            return;
        }
        for mut arrow in changed {
            arrow.roundness = arrow_type.roundness();
            self.scene.put(arrow);
        }
        self.apply_bindings();
        self.commit_style();
        self.request_draw();
    }

    /// What a new arrow takes from the panel rather than from the next style: its type
    /// (`currentItemArrowType`, where the Edges row's `currentItemRoundness` is only
    /// for lines and shapes) and the heads chosen, if any — `App.tsx@1118751f:10251-10276`.
    pub(super) fn style_new_arrow(&self, arrow: &mut DrawElement) {
        arrow.roundness = self.next_arrow_type.roundness();
        arrow.start_arrowhead = self.next_start_arrowhead;
        arrow.end_arrowhead = self.next_end_arrowhead;
    }

    /// A size for the selected texts and the labels their shapes carry, or for the next
    /// text when nothing is selected — within what the contract stores; a size that is
    /// not a number is ignored.
    pub fn set_font_size(&mut self, size: f64) {
        let Some(size) = storable_font_size(size) else {
            return;
        };
        self.next_font_size = size;
        self.touch_style();
        // Through `selected_texts` rather than the raw selection, so resizing works with
        // a labelled shape selected — which is the only thing you *can* select once a
        // shape has a label.
        let sticky = self.sticky_labels();
        self.relayout_selected_texts(
            |text| set_user_font_size(text, size, sticky.contains(&text.id)),
            true,
        );
        self.refloor_text_session();
    }

    /// Ctrl/Cmd+Shift+> and <: `actionIncreaseFontSize` / `actionDecreaseFontSize`
    /// (`actionProperties.tsx@1118751f:1095-1141`). Each selected text, and each label a
    /// selected shape carries, a tenth bigger — or back by the same factor — from its own
    /// size, rounded as `Math.round` rounds, and laid out again as a picked size is. The
    /// next text's size follows only when they all end at the same one, and with nothing
    /// selected nothing changes (`changeFontSize`, `:294-354`). Within what the contract
    /// stores, where the oracle has no ceiling.
    pub fn step_font_size(&mut self, increase: bool) {
        let step = |size: f64| {
            let next = if increase {
                size * (1.0 + FONT_SIZE_STEP)
            } else {
                (1.0 / (1.0 + FONT_SIZE_STEP)) * size
            };
            storable_font_size(crate::text::layout::js_round(next)).unwrap_or(size)
        };
        let sizes: Vec<f64> = self
            .selected_texts()
            .iter()
            .map(|text| step(self.user_font_size(text)))
            .collect();
        let Some(&first) = sizes.first() else {
            return;
        };
        if sizes.iter().all(|&size| size == first) {
            self.next_font_size = first;
        }
        self.touch_style();
        // Read before the loop writes: a note's label steps from its ceiling
        // (`getBaseFontSize`, `actionProperties.tsx@1118751f:1105`, `:1128`).
        let sticky = self.sticky_labels();
        self.relayout_selected_texts(
            |text| {
                let sticky = sticky.contains(&text.id);
                let from = if sticky {
                    text.base_font_size
                        .unwrap_or(crate::text::layout::font_size_of(text))
                } else {
                    crate::text::layout::font_size_of(text)
                };
                set_user_font_size(text, step(from), sticky);
            },
            true,
        );
        self.refloor_text_session();
    }

    /// Writes `change` into every selected text, lays each out again from its source
    /// (`redrawTextBoundingBox`), grows the shapes that no longer hold their labels, and
    /// commits — which stamps only what changed. With `anchor_font_resize`, a free
    /// auto-sizing text keeps its aligned edge and its vertical middle
    /// (`offsetElementAfterFontResize`); otherwise a free text stays where it is, as the
    /// oracle's family and alignment changes leave it.
    ///
    /// A text being typed is written here too, and its editor closing commits it
    /// ([`Self::commit_style`]): committed now, a new one would leave an empty text for
    /// undo to bring back.
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
            self.put_laid(laid);
        }
        self.apply_bindings();
        self.commit_style();
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
        self.touch_style();
        self.relayout_selected_texts(
            |text| {
                text.font_family = Some(family);
                text.line_height = Some(line_height);
            },
            false,
        );
        self.refloor_text_session();
    }

    /// Shows `family` on the selected texts without committing it — the font picker's
    /// hover (`onHover`, `actionProperties.tsx@1118751f:1465-1471`) — and `None` gives
    /// back what the last one changed (`onLeave`, `resetAll`, `:1472-1478`). Each hover
    /// gives the last one back first, so none is laid out from another. Picking commits
    /// with [`Self::set_font_family`], against what was there before the first hover: one
    /// step of undo. As the oracle's, a selection of more than 200 texts or 5000
    /// characters is not previewed. What a peer takes meanwhile is given back to them
    /// (`peers.rs`), as for [`Self::preview_style`].
    ///
    /// A text being typed, and its shape, are given back as the first hover found them —
    /// what was typed, and a size stepped meanwhile — as the oracle's picker caches the
    /// editing text when it opens (`actionProperties.tsx@1118751f:1484-1499`), rather than
    /// as they were committed.
    ///
    /// The host loads the face first, as for [`Self::set_font_family`].
    pub fn preview_font_family(&mut self, family: Option<u8>) {
        let gave_back = self.give_back_style_preview();
        let line_height = family
            .and_then(crate::text::font::family)
            .map(|font| font.line_height);
        if let (Some(family), Some(line_height)) = (family, line_height) {
            let texts = self.selected_texts();
            let chars: usize = texts
                .iter()
                .map(|text| crate::scene::source_text(text).chars().count())
                .sum();
            if texts.len() <= PREVIEW_MAX_TEXTS && chars <= PREVIEW_MAX_CHARS {
                for prev in texts {
                    let mut next = prev.clone();
                    next.font_family = Some(family);
                    next.line_height = Some(line_height);
                    let laid = self.laid_out(&next);
                    if let Some(container) = laid.container {
                        self.put_previewed(container);
                    }
                    if laid.text != prev {
                        self.put_previewed(laid.text);
                    }
                }
            }
        }
        if gave_back || !self.style_preview.is_empty() {
            self.touch_style();
            self.request_draw();
        }
    }

    /// Gives back what the style preview in progress changed — a font hovered, the opacity
    /// slider mid-drag — as it was before it: a copy a font preview found as a change of
    /// ours is put back, anything else is no longer changed. Says whether there was any.
    pub(super) fn give_back_style_preview(&mut self) -> bool {
        let previewed: Vec<(String, Option<DrawElement>)> = self.style_preview.drain().collect();
        let gave_back = !previewed.is_empty();
        for (id, found) in previewed {
            match found {
                Some(found) => self.scene.put(found),
                None => self.drop_local_change(&id),
            }
        }
        gave_back
    }

    /// Puts what a font preview changed, remembering what it found when that was already
    /// a change of ours — the text being typed and its shape — to give it back as found.
    fn put_previewed(&mut self, element: DrawElement) {
        let found = self
            .scene
            .committed(&element.id)
            .and_then(|_| self.scene.get(&element.id).cloned());
        self.style_preview.insert(element.id.clone(), found);
        self.scene.put(element);
    }

    /// The families the board's texts are drawn in, each once — the font picker's "In
    /// this scene" (`Fonts.getSceneFamilies`, `fonts/Fonts.ts@1118751f:94-96`). The system
    /// stack a text with no family is drawn in is not one the picker offers.
    pub fn scene_font_families(&self) -> Vec<u8> {
        let mut families = Vec::new();
        for element in self.scene.iter_ordered() {
            if element.kind != DrawElementType::Text {
                continue;
            }
            if let Some(family) = crate::scene::resolved_font_family(element) {
                if !families.contains(&family) {
                    families.push(family);
                }
            }
        }
        families
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
        self.commit_style();
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
            .first()
            .filter(|el| el.font_size.is_some())
            .map_or(self.next_font_size, |el| self.user_font_size(el))
    }

    /// The size a text was given: a note's label's ceiling, which its fit shrinks below
    /// and the panel shows, or any other text's own size (`getBaseFontSize`,
    /// `packages/element/src/stickyNote.ts@1118751f:405-413`).
    pub(super) fn user_font_size(&self, text: &DrawElement) -> f64 {
        crate::scene::sticky::label_ceiling(text, self.container_of(text))
    }

    /// The selected texts that are notes' labels, whose picked size is their ceiling.
    fn sticky_labels(&self) -> HashSet<String> {
        self.selected_texts()
            .into_iter()
            .filter(|text| {
                self.container_of(text)
                    .is_some_and(crate::scene::sticky::is_sticky_note)
            })
            .map(|text| text.id)
            .collect()
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
        let carried = OnceCell::new();
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for element in self.get_selected_elements() {
            if !self.restylable(&element, &carried) {
                continue;
            }
            let candidate = if element.kind == crate::scene::DrawElementType::Text {
                Some(element)
            } else {
                self.label_of(&element, &carried).cloned()
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

    /// Pans (and, only if it would not otherwise fit, zooms out) so `bounds` is on screen,
    /// eased over [`CAMERA_REVEAL_MS`] rather than jumping — the flowchart's commit and its
    /// Alt+Arrow navigation both land here (`engine/flowchart.rs`). A no-op when `bounds` is
    /// already fully visible, so following a chain of nodes that are all in view never
    /// nudges the camera.
    pub(crate) fn reveal(&mut self, bounds: WorldBounds, padding: f64) {
        let visible = crate::visible_world_rect(self.camera, self.width, self.height);
        let already_visible = bounds.min_x >= visible.min_x
            && bounds.max_x <= visible.max_x
            && bounds.min_y >= visible.min_y
            && bounds.max_y <= visible.max_y;
        if already_visible {
            return;
        }
        let fit = crate::fit_bounds(bounds, self.width, self.height, padding);
        // Scale down to fit only when the bounds do not already fit at the current zoom —
        // never in, so revealing a small new node does not also zoom in on it.
        let scale = fit.scale.min(self.camera.scale);
        let center_x = (bounds.min_x + bounds.max_x) / 2.0;
        let center_y = (bounds.min_y + bounds.max_y) / 2.0;
        let target = Camera {
            scale,
            x: self.width / 2.0 - center_x * scale,
            y: self.height / 2.0 - center_y * scale,
        };
        self.animate_camera_to(target, CAMERA_REVEAL_MS);
    }

    fn animate_camera_to(&mut self, target: Camera, duration_ms: f64) {
        if target == self.camera {
            return;
        }
        self.camera_anim = Some(CameraAnim {
            from: self.camera,
            to: target,
            start_ms: self.now_ms,
            end_ms: self.now_ms + duration_ms,
        });
        self.request_draw();
    }

    /// Advances an in-flight [`reveal`](Self::reveal), if any. Called from `set_now` every
    /// frame the host owes one — `needs_frame` answers `true` for as long as this holds
    /// `Some`, so the host's loop keeps ticking until the ease finishes.
    pub(crate) fn tick_camera_anim(&mut self, now_ms: f64) {
        let Some(anim) = self.camera_anim else {
            return;
        };
        let span = (anim.end_ms - anim.start_ms).max(1.0);
        let t = ((now_ms - anim.start_ms) / span).clamp(0.0, 1.0);
        let eased = ease_out(t);
        self.set_camera(Camera {
            x: lerp(anim.from.x, anim.to.x, eased),
            y: lerp(anim.from.y, anim.to.y, eased),
            scale: lerp(anim.from.scale, anim.to.scale, eased),
        });
        if t >= 1.0 {
            self.camera_anim = None;
        }
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

/// A picked size, written where it means something (`getBaseFontSizeUpdate`,
/// `packages/element/src/stickyNote.ts@1118751f:415-423`): a note's label takes it as the
/// ceiling its fit shrinks below, clamped; any other text as its size.
fn set_user_font_size(text: &mut DrawElement, size: f64, sticky: bool) {
    if sticky {
        text.base_font_size = Some(crate::scene::sticky::normalize_sticky_font_size(size));
    } else {
        text.font_size = Some(size);
    }
}
