use crate::engine::multi_linear::is_double_tap;
use crate::engine::{DrawEngine, TextEditRequest};
use crate::interaction::DrawTool;
use crate::scene::binding::linear_endpoints;
use crate::scene::{
    bindable_at, create_element, default_element_style, is_bindable_element, is_linear_element,
    merge_style, DrawElement, DrawElementType, Geometry,
};
use crate::text::layout::{self, Laid, Measure};
use crate::text::FontKey;

impl DrawEngine {
    pub fn edit_selected_text(&mut self) -> bool {
        let Some(single) = self.single_selected() else {
            return false;
        };
        if single.is_deleted || self.untouchable(&single) || self.in_untouchable_shape(&single) {
            return false;
        }
        if single.kind == DrawElementType::Text {
            self.request_text_edit(&single);
            return true;
        }
        if !is_bindable_element(&single) && !is_linear_element(&single) {
            return false;
        }
        self.edit_label(&single);
        true
    }

    pub(crate) fn request_text_edit(&mut self, element: &DrawElement) {
        let text_align = crate::scene::resolved_text_align(element);
        // The editor is sent the box the lines are wrapped in, so it wraps where the
        // canvas does. It sits around the anchor the painter puts the lines on: a
        // label's box is as wide as its shape allows and the label sits in it by its
        // alignment, so the box's left edge is the label's anchor less the box's.
        let wrap = self.wrap_width(element);
        let x = wrap.map_or(element.x, |wrap| {
            element.x + crate::render::text_anchor_x(text_align, element.width)
                - crate::render::text_anchor_x(text_align, wrap)
        });
        let screen = crate::world_to_screen(self.camera, x, element.y);
        let family = crate::scene::resolved_font_family(element);
        self.events.text_edit = Some(TextEditRequest {
            id: element.id.clone(),
            x: screen.x,
            y: screen.y,
            font_size: layout::font_size_of(element),
            color: element.stroke_color.clone(),
            // What was typed, as the oracle's editor opens on `originalText`
            // (`packages/excalidraw/wysiwyg/textWysiwyg.tsx:488`): opened on the drawn
            // text, every soft break would come back from the edit a hard one.
            text: crate::scene::source_text(element).to_owned(),
            width: wrap.map(|wrap| wrap * self.camera.scale),
            text_align,
            container_id: element.container_id.clone(),
            font_family: family.unwrap_or(crate::text::FontKey::LEGACY),
            line_height: crate::scene::resolved_line_height(element),
        });
        self.open_text_session(element);
    }

    /// A new, empty text in the next style: the family chosen last, Excalifont until one
    /// is, with that family's line height (`newTextElement`,
    /// `packages/element/src/newElement.ts@1118751f:330-390`).
    pub(crate) fn new_text_element(&self, geometry: Geometry) -> DrawElement {
        let style = merge_style(&default_element_style(), &self.next_style);
        let mut element = create_element(DrawElementType::Text, geometry, style, self.now_ms);
        element.text = Some(String::new());
        element.original_text = Some(String::new());
        element.font_size = Some(self.next_font_size);
        element.font_family = Some(self.next_font_family);
        element.line_height =
            crate::text::font::family(self.next_font_family).map(|family| family.line_height);
        element.text_align = self.next_text_align;
        element.vertical_align = self.next_vertical_align;
        element
    }

    fn create_label(&mut self, container: &DrawElement) -> DrawElement {
        let mut label = self.new_text_element(Geometry {
            x: container.x,
            y: container.y,
            width: 0.0,
            height: 0.0,
        });
        // As big as an empty line, and placed by `apply_bindings` below. Not laid out:
        // that could grow the shape for a label that is then abandoned untyped.
        let (width, height) = self.with_measure(|measure| {
            measure.size(
                "",
                layout::font_of(&label),
                crate::scene::resolved_line_height(&label),
            )
        });
        label.width = width;
        label.height = height;
        label.container_id = Some(container.id.clone());
        // The smallest a shape holding one line of it may be, which a resize stops at too.
        let min_line = self.with_measure(|measure| layout::min_container_size(&label, measure));
        // In its shape's groups and directly above it, as the oracle makes one
        // (`packages/excalidraw/components/App.tsx:7081`, `:7103-7108`). On top of the
        // board instead, it was drawn over whatever covers its shape, and split the
        // shape's group in the stack.
        label.group_ids = container.group_ids.clone();
        let mut container = container.clone();
        container.bound_text_id = Some(label.id.clone());
        if !is_linear_element(&container) {
            grow_to_one_line(&mut container, min_line);
        }
        self.scene.add(label.clone());
        self.scene
            .place_above(std::slice::from_ref(&label.id), &container.id);
        self.scene.put(container);
        self.apply_bindings();
        self.scene.get(&label.id).cloned().unwrap_or(label)
    }

    /// Where a new free text's top goes for a press at `y`: its first line centred on the
    /// pointer, as the oracle starts one from a point cursor (`App.tsx@1118751f:7019-7027`).
    /// With the grid snapping, the oracle puts it on the nearest grid point instead
    /// (`getTextCreationGridPoint`); ponytail: left on the point here, until text
    /// creation snaps.
    pub(crate) fn first_line_top(&self, text: &DrawElement, y: f64) -> f64 {
        if self.grid.enabled && self.grid.snap {
            return y;
        }
        y - layout::font_size_of(text) * crate::scene::resolved_line_height(text) / 2.0
    }

    fn label_target_at(&self, wx: f64, wy: f64) -> Option<DrawElement> {
        let selectable = self.selectable();
        if let Some(shape) = bindable_at(&selectable, wx, wy, 0.0, None) {
            return Some(shape.clone());
        }
        crate::hit_test(&selectable, wx, wy, self.collision_tolerance())
            .filter(|hit| is_linear_element(hit))
            .cloned()
    }

    /// Descends one level into the group under the pointer, if there is one.
    ///
    /// `editing_group_id` becomes the group the click resolves to *now*, and the
    /// selection becomes what the next level down holds. Because
    /// `selected_group_for` truncates at the edited group and takes the outermost of
    /// what remains, repeating this walks inward exactly one level per double click —
    /// to any depth, with no special case for how deep the nesting goes.
    ///
    /// Returns whether it descended, so the caller knows not to fall through to text. A
    /// label stands for its shape, as it does for a click ([`Self::element_at`]): stepped
    /// into from its label, a group held the label on its own, which moves only with its
    /// shape.
    fn step_into_group(&mut self, sx: f64, sy: f64) -> bool {
        let tolerance = self.collision_tolerance();
        let Some(hit) = self
            .element_at(sx, sy, tolerance, |element| !self.untouchable(element))
            .cloned()
        else {
            return false;
        };
        let editing = self.editing_group_id.clone();
        let Some(group) = crate::edit::selected_group_for(&hit, editing.as_deref()) else {
            // Nothing above this element inside the group being edited: the leaf is
            // already reachable, so there is nowhere further to go.
            return false;
        };
        // A group of one — its other members deleted — has nothing inside to show. The
        // oracle only steps into a group the click selected (`App.tsx:7310-7330`), which
        // a group of one never is (`groups.ts:134-141`), so the double click does what it
        // does on any lone shape.
        if !crate::edit::is_live_group(self.scene.iter_ordered(), group) {
            return false;
        }
        let group = group.clone();
        self.editing_group_id = Some(group);
        let ids = crate::edit::expand_within(
            self.scene.iter_ordered(),
            [hit.id.clone()],
            self.editing_group_id.as_deref(),
        );
        self.set_selection(ids);
        self.request_draw();
        true
    }

    /// Whether a double click at `world` types into `arrow`, the one container on offer:
    /// when it is on the arrow, or within `TEXT_TO_CENTER_SNAP_THRESHOLD` (30 scene
    /// units, `packages/common/src/constants.ts@1118751f:30`) of where its label sits —
    /// `handleCanvasDoubleClick` and `getTextWysiwygSnappedToCenterPosition`
    /// (`App.tsx@1118751f:7356-7392`, `:13800-13832`).
    fn takes_double_click_text(&self, arrow: &DrawElement, world: crate::camera::Point) -> bool {
        let (start, end) = linear_endpoints(arrow);
        let middle = ((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
        crate::hit_test_element(arrow, world.x, world.y, self.collision_tolerance())
            || (world.x - middle.0).hypot(world.y - middle.1) < 30.0
    }

    /// Opens `container`'s label for editing, making it first if it has none.
    fn edit_label(&mut self, container: &DrawElement) {
        let bound = container
            .bound_text_id
            .as_ref()
            .and_then(|id| self.scene.get(id).cloned());
        let label = if let Some(bound) = bound.filter(|el| !el.is_deleted) {
            bound
        } else {
            self.create_label(container)
        };
        self.type_in(&label);
    }

    /// Opens `element` for typing, and selects it while it is typed — after opening it,
    /// so the change pending from then on keeps that selection from being the one the
    /// edit's step of history begins with: that is what the press or the key left
    /// selected, a label's shape and not the label on its own (`stamp.rs`, "Undo puts the
    /// selection back").
    fn type_in(&mut self, element: &DrawElement) {
        self.request_text_edit(element);
        self.set_selection(vec![element.id.clone()]);
    }

    pub fn handle_double_click(&mut self, sx: f64, sy: f64) {
        // Any double click ends what the press that finished a path could be half of.
        let finished = self
            .finished_by_press
            .take()
            .filter(|(_, at)| is_double_tap(*at, sx, sy));
        // Only with the selection tools, as Excalidraw's `handleCanvasDoubleClick` has it
        // (`App.tsx:7200-7209`): a double click with the eraser, say, put an empty text
        // box on the board and opened an editor on it.
        if !matches!(
            self.tool,
            DrawTool::Select | DrawTool::Lasso | DrawTool::AutoShape
        ) {
            return;
        }
        let world = self.screen_to_world(sx, sy);
        // The double click one of whose presses ended a path. Excalidraw's finished path
        // is then its one selected element (checked on excalidraw.com), so it is the only
        // container the double click can type into
        // (`getTextBindableContainerAtPosition`, `App.tsx@1118751f:6831-6838`) — never the
        // shape under the pointer. A line opens its line editor instead
        // (`App.tsx@1118751f:7222-7234`). An arrow takes a label when the double click
        // is on it or near its middle (`:7356-7392`); otherwise the text is a free one at
        // the pointer — which is where the second press of a double click that ended the
        // arrow lands, but not always the first's: a click that binds in orbit moves the
        // end onto the shape's outline, toward its centre. A path too short to keep was
        // thrown away, and Excalidraw's double click does nothing while a path is open
        // (`App.tsx@1118751f:7199-7201`), so neither does this one.
        let finished = match finished {
            Some((id, _)) => match self.scene.get(&id).filter(|el| !el.is_deleted).cloned() {
                Some(path) => Some(path),
                None => return,
            },
            None => None,
        };
        if let Some(path) = &finished {
            if path.kind != DrawElementType::Arrow {
                self.open_linear_points(path);
                return;
            }
            if self.takes_double_click_text(path, world) {
                self.edit_label(path);
                return;
            }
            // Past it, nothing under the pointer is on offer but a text to edit
            // (`startTextEditing`, `App.tsx@1118751f:6960-6965`): no group, no line's
            // points, no container — hence the `finished.is_none()` below.
        }
        // Stepping into a group comes first. A double click inside one means "show me
        // what is in here", and letting the text branch run first would put a label on
        // the shape instead — which is what happened, and why a group could not be
        // opened at all.
        if finished.is_none() && self.step_into_group(sx, sy) {
            return;
        }
        if let Some(hit) = crate::hit_test(
            &self.selectable(),
            world.x,
            world.y,
            self.collision_tolerance(),
        )
        .cloned()
        {
            if hit.kind == DrawElementType::Text && !self.in_untouchable_shape(&hit) {
                self.type_in(&hit);
                return;
            }
            // A path of more than two points selects to a box, because it is a shape.
            // Its corners are still there, behind this gesture — the same door
            // Excalidraw puts its line editor behind.
            if finished.is_none() && self.open_linear_points(&hit) {
                return;
            }
        }
        if finished.is_none() {
            if let Some(container) = self.label_target_at(world.x, world.y) {
                self.edit_label(&container);
                return;
            }
        }
        let font_size = self.next_font_size;
        let mut element = self.new_text_element(Geometry {
            x: world.x,
            y: world.y,
            width: 4.0,
            height: font_size,
        });
        element.y = self.first_line_top(&element, world.y);
        self.scene.add(element.clone());
        self.type_in(&element);
    }

    /// Gives a text column a new width and re-wraps it to fit.
    ///
    /// Re-wrapping is the whole job. Without it the text keeps the line breaks it had
    /// when the box was wider and simply spills out of the box it is supposedly inside —
    /// and the box is the only thing the person moved.
    ///
    /// Does nothing to auto-sizing text, which has no width of its own to impose. A
    /// resize handle must not silently change what an element *is*.
    pub fn set_text_box_width(&mut self, id: &str, width: f64) {
        let Some(element) = self.scene.get(id).cloned() else {
            return;
        };
        if element.kind != DrawElementType::Text
            || element.container_id.is_some()
            || crate::scene::is_auto_resize(&element)
        {
            return;
        }
        // What was typed, not what was drawn at the old width.
        let text = crate::scene::source_text(&element).to_owned();
        let mut next = element;
        next.width = width.abs().max(8.0);
        self.scene.put(next);
        // Through `set_element_text` rather than re-wrapping here, so a column resized by
        // a handle and one retyped into end up with exactly the same lines.
        self.set_element_text(id, &text);
    }

    /// Writes `text` into the text `id` and commits it, in one call — the edit before
    /// the typing session (`text_session.rs`), kept for hosts that write text whole. It
    /// ends a session open on the same text, with no layout of its own.
    pub fn set_element_text(&mut self, id: &str, text: &str) {
        self.drop_text_session(Some(id));
        self.set_element_text_step(id, text);
        self.refresh_live();
    }

    /// The one-shot write: what the session's commit does, with no floor for the shape to
    /// shrink to and the selection let go of when the text goes.
    fn set_element_text_step(&mut self, id: &str, text: &str) {
        let Some(element) = self.scene.get(id).cloned() else {
            return;
        };
        if text.trim().is_empty() {
            self.clear_selection();
            self.remove_emptied_text(&element);
        } else {
            self.type_into(&element, text, None);
            self.push_history();
        }
        self.request_draw();
    }

    /// A text element as it would be with `text` in it, uncommitted — for a host on the
    /// one-shot [`Self::set_element_text`] to show peers while it is typed. A host on the
    /// typing session (`text_session.rs`) streams [`Self::gesture_elements`] instead,
    /// which carries the shape the text grows too. `None` for an id that is not a text in
    /// the scene.
    ///
    /// ponytail: the label only, the shape it grows arriving with the one-shot commit;
    /// the session is the upgrade.
    pub fn text_preview(&self, id: &str, text: &str) -> Option<DrawElement> {
        let element = self.scene.get(id)?;
        if element.kind != DrawElementType::Text {
            return None;
        }
        Some(self.with_text(element, text).text)
    }

    /// The width a text's lines wrap at, or `None` when the glyphs decide — see
    /// [`layout::wrap_width`].
    fn wrap_width(&self, element: &DrawElement) -> Option<f64> {
        layout::wrap_width(element, self.container_of(element))
    }

    /// Whether `text` is the label of a shape that is not ours to change — locked, or held
    /// by a peer. Its words are that shape's, and laying them out grows it, so it is not
    /// typed into: the oracle locks a label with its shape (`actionToggleElementLock`,
    /// `actions/actionElementLock.ts@1118751f:49-54`), and a double click does not hit a
    /// locked one (`getTextElementAtPosition`, `App.tsx@1118751f:6588-6599`, `:6654-6677`).
    fn in_untouchable_shape(&self, text: &DrawElement) -> bool {
        self.container_of(text)
            .is_some_and(|container| self.untouchable(container))
    }

    /// The live container of a label, if it is one.
    pub(crate) fn container_of(&self, text: &DrawElement) -> Option<&DrawElement> {
        text.container_id
            .as_deref()
            .and_then(|id| self.scene.get(id))
            .filter(|container| !container.is_deleted)
    }

    /// Runs `body` with the measurer layout uses: the host's per-font line measure, or
    /// the per-size hook standing in for it.
    pub(crate) fn with_measure<T>(&self, body: impl FnOnce(&Measure) -> T) -> T {
        let (line, text) = (self.measure_line, self.measure_text);
        let line_width = move |line_text: &str, font: FontKey| match line {
            Some(measure) => measure(line_text, font),
            None => text(line_text, font.size()).0,
        };
        body(&Measure {
            cache: &self.text_cache,
            line_width: &line_width,
        })
    }

    /// `text` laid out where it stands ([`layout::layout_text`]): its own lines from its
    /// source, and its container grown if it is a label that no longer fits.
    pub(crate) fn laid_out(&self, text: &DrawElement) -> Laid {
        self.with_measure(|measure| layout::layout_text(text, self.container_of(text), measure))
    }

    /// `element` with `text` typed into it: laid out, and — a free text — kept on the
    /// edge its alignment anchors (`getAdjustedDimensions`).
    pub(crate) fn with_text(&self, element: &DrawElement, text: &str) -> Laid {
        let mut typed = element.clone();
        typed.original_text = Some(text.to_owned());
        let mut laid = self.laid_out(&typed);
        if element.container_id.is_none() {
            let prev = self.with_measure(|measure| {
                measure.size(
                    element.text.as_deref().unwrap_or_default(),
                    layout::font_of(element),
                    crate::scene::resolved_line_height(element),
                )
            });
            let at = layout::edit_anchor(element, prev, laid.text.width, laid.text.height);
            laid.text.x = at.x;
            laid.text.y = at.y;
        }
        laid
    }

    /// A font has finished loading: every text drawn in a family is measured again and
    /// laid out anew, so the boxes fit the glyphs now on screen rather than a fallback's.
    ///
    /// Not an edit. Nothing is stamped, pending, sent or undoable: the words are the
    /// same, every client lays them out for itself, and a board opened only to read
    /// must not save itself. Texts with no family use the system stack, which was
    /// never loading, and are left alone.
    ///
    /// Divergence: Excalidraw only drops its caches and repaints here
    /// (`Fonts.onLoaded`, `packages/excalidraw/fonts/Fonts.ts@1118751f:106-148`), so a box
    /// sized in a fallback's widths keeps them until the text is next edited.
    ///
    /// ponytail: an arrow bound to a shape that grew here is re-routed at the next edit
    /// of either — re-routing now would be an unstamped change to the arrow.
    pub fn fonts_loaded(&mut self) {
        self.text_cache.clear();
        let texts: Vec<DrawElement> = self
            .scene
            .iter_ordered()
            .filter(|el| {
                el.kind == DrawElementType::Text && crate::scene::resolved_font_family(el).is_some()
            })
            .cloned()
            .collect();
        let mut changed = false;
        for text in texts {
            let mut laid = self.laid_out(&text);
            if text.container_id.is_none() {
                // A centred text stays centred, a right-aligned one keeps its right edge.
                let at = layout::edit_anchor(
                    &text,
                    (text.width, text.height),
                    laid.text.width,
                    laid.text.height,
                );
                laid.text.x = at.x;
                laid.text.y = at.y;
            }
            if let Some(container) = laid.container {
                changed |= self.scene.replace_unrecorded(container);
            }
            if laid.text != text {
                changed |= self.scene.replace_unrecorded(laid.text);
            }
        }
        if changed {
            self.request_draw();
        }
    }
}

/// A shape too small for one line of the label it is being given grows to hold one, from
/// its top-left corner (`startTextEditing`, `App.tsx@1118751f:6974-7006`).
fn grow_to_one_line(container: &mut DrawElement, (min_width, min_height): (f64, f64)) {
    let rect =
        crate::scene::normalize_rect(container.x, container.y, container.width, container.height);
    if rect.width >= min_width && rect.height >= min_height {
        return;
    }
    container.x = rect.x;
    container.y = rect.y;
    container.width = rect.width.max(min_width);
    container.height = rect.height.max(min_height);
}
