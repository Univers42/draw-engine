use crate::engine::multi_linear::is_double_tap;
use crate::engine::{DrawEngine, TextEditRequest};
use crate::interaction::DrawTool;
use crate::scene::binding::linear_endpoints;
use crate::scene::{
    bindable_at, bump_version, create_element, default_element_style, is_bindable_element,
    is_linear_element, merge_style, DrawElement, DrawElementType, Geometry,
};
use crate::TEXT_LINE_HEIGHT;

impl DrawEngine {
    pub fn edit_selected_text(&mut self) -> bool {
        let Some(single) = self.single_selected() else {
            return false;
        };
        if single.is_deleted || self.untouchable(&single) {
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

    pub(crate) fn font_size_of(&self, element: &DrawElement) -> f64 {
        element.font_size.unwrap_or(super::DEFAULT_FONT_SIZE)
    }

    pub(crate) fn request_text_edit(&mut self, element: &DrawElement) {
        let text_align = crate::scene::resolved_text_align(element);
        // The editor is sent the box the lines are wrapped in, so it wraps where the
        // canvas does. It sits around the anchor the painter puts the lines on, which for
        // a shape's label or a column is the element itself. An arrow's label is not: it
        // is an 8-unit placeholder on the arrow's middle, and its lines wrap far wider.
        let wrap = self.wrap_width(element);
        let x = wrap.map_or(element.x, |wrap| {
            element.x + crate::render::text_anchor_x(text_align, element.width)
                - crate::render::text_anchor_x(text_align, wrap)
        });
        let screen = crate::world_to_screen(self.camera, x, element.y);
        self.events.text_edit = Some(TextEditRequest {
            id: element.id.clone(),
            x: screen.x,
            y: screen.y,
            font_size: element.font_size.unwrap_or(super::DEFAULT_FONT_SIZE),
            color: element.stroke_color.clone(),
            // What was typed, as the oracle's editor opens on `originalText`
            // (`packages/excalidraw/wysiwyg/textWysiwyg.tsx:488`): opened on the drawn
            // text, every soft break would come back from the edit a hard one.
            text: crate::scene::source_text(element).to_owned(),
            width: wrap.map(|wrap| wrap * self.camera.scale),
            text_align,
            container_id: element.container_id.clone(),
        });
    }

    fn create_label(&mut self, container: &DrawElement) -> DrawElement {
        let width = if is_linear_element(container) {
            8.0
        } else {
            (container.width.abs() - 16.0).max(8.0)
        };
        let style = merge_style(&default_element_style(), &self.next_style);
        let mut label = create_element(
            DrawElementType::Text,
            Geometry {
                x: container.x,
                y: container.y,
                width,
                height: self.next_font_size,
            },
            style,
            self.now_ms,
        );
        label.text = Some(String::new());
        label.font_size = Some(self.next_font_size);
        label.text_align = self.next_text_align;
        label.vertical_align = self.next_vertical_align;
        label.container_id = Some(container.id.clone());
        // In its shape's groups and directly above it, as the oracle makes one
        // (`packages/excalidraw/components/App.tsx:7081`, `:7103-7108`). On top of the
        // board instead, it was drawn over whatever covers its shape, and split the
        // shape's group in the stack.
        label.group_ids = container.group_ids.clone();
        label.stroke_color = self.get_next_style().stroke_color;
        let mut container = container.clone();
        container.bound_text_id = Some(label.id.clone());
        self.scene.add(label.clone());
        self.scene
            .place_above(std::slice::from_ref(&label.id), &container.id);
        self.scene.put(container);
        self.apply_bindings();
        self.scene.get(&label.id).cloned().unwrap_or(label)
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
    /// Returns whether it descended, so the caller knows not to fall through to text.
    fn step_into_group(&mut self, sx: f64, sy: f64) -> bool {
        let Some(hit) = self.selectable_hit(sx, sy, self.collision_tolerance()) else {
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
        self.set_selection(vec![label.id.clone()]);
        self.request_text_edit(&label);
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
            if hit.kind == DrawElementType::Text {
                self.set_selection(vec![hit.id.clone()]);
                self.request_text_edit(&hit);
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
        let style = merge_style(&default_element_style(), &self.next_style);
        let mut element = create_element(
            DrawElementType::Text,
            Geometry {
                x: world.x,
                y: world.y,
                width: 4.0,
                height: self.next_font_size,
            },
            style,
            self.now_ms,
        );
        element.text = Some(String::new());
        element.font_size = Some(self.next_font_size);
        element.text_align = self.next_text_align;
        element.vertical_align = self.next_vertical_align;
        let id = element.id.clone();
        self.scene.add(element.clone());
        self.set_selection(vec![id]);
        self.request_text_edit(&element);
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

    pub fn set_element_text(&mut self, id: &str, text: &str) {
        let Some(element) = self.scene.get(id).cloned() else {
            return;
        };
        if text.trim().is_empty() {
            if let Some(container_id) = &element.container_id {
                if let Some(mut container) = self.scene.get(container_id).cloned() {
                    container.bound_text_id = None;
                    self.scene.put(container);
                }
            }
            if self.scene.created_since_commit(id) {
                self.scene.discard(id);
            } else {
                // A committed text emptied is a deletion, and a deletion is a tombstone:
                // the server and the peers have to be told, and undo has to be able to
                // stamp the text back above it.
                self.scene.remove(id, self.now_ms);
            }
            self.clear_selection();
            // Settled here, although it usually changes nothing: a label abandoned before
            // anything was typed leaves its container exactly as committed, and without
            // a commit the container stayed pending — refusing every peer's edit of it.
            // Removing a committed text records its deletion.
            self.push_history();
            self.request_draw();
            return;
        }
        let next = self.with_text(element, text);
        self.scene.put(bump_version(next, self.now_ms));
        self.apply_bindings();
        self.push_history();
        self.request_draw();
    }

    /// A text element as it would be with `text` in it, uncommitted — what peers are
    /// shown while it is being typed, so a word appears on their screens as it is
    /// written rather than all at once when the editor closes. `None` for an id that is
    /// not a text in the scene.
    pub fn text_preview(&self, id: &str, text: &str) -> Option<DrawElement> {
        let element = self.scene.get(id)?;
        if element.kind != DrawElementType::Text {
            return None;
        }
        Some(self.with_text(element.clone(), text))
    }

    /// The width a text's lines wrap at, or `None` when the glyphs decide.
    ///
    /// Three ways a text element gets its width, and only the last lets the glyphs
    /// decide. A label takes its container's; a dragged-out column keeps the one it was
    /// given; auto-sizing text grows to fit.
    fn wrap_width(&self, element: &DrawElement) -> Option<f64> {
        if let Some(container_id) = &element.container_id {
            self.scene
                .get(container_id)
                .map(|container| (container.width.abs() - crate::LABEL_PADDING * 2.0).max(8.0))
        } else if !crate::scene::is_auto_resize(element) {
            Some(element.width.abs().max(8.0))
        } else {
            None
        }
    }

    /// `element` with `text` in it, wrapped and measured as the canvas will draw it.
    fn with_text(&self, element: DrawElement, text: &str) -> DrawElement {
        let font_size = self.font_size_of(&element);
        let wrap_to = self.wrap_width(&element);
        let final_text = match wrap_to {
            Some(max_width) => self.wrap_text_to_width(text, max_width, font_size),
            None => text.to_string(),
        };
        let (width, height) = (self.measure_text)(&final_text, font_size);
        let mut next = element;
        next.text = Some(final_text.clone());
        // What was typed is the source now. Left alone it would still read what the text
        // said before the edit, and a client laying text out from it would put the old
        // words back. A text with none keeps none: its `text` is its source, and an old
        // board edited here saves exactly the fields it always did.
        if next.original_text.is_some() {
            next.original_text = Some(text.to_owned());
        }
        // A column keeps the width it was given: it is the thing the person set, and
        // shrinking it to the longest wrapped line would make the box creep inwards a
        // little on every edit.
        if crate::scene::is_auto_resize(&next) && next.container_id.is_none() {
            next.width = width;
        }
        next.height = height.max(
            font_size.max(final_text.split('\n').count() as f64 * font_size * TEXT_LINE_HEIGHT),
        );
        next
    }

    /// Soft-wraps `text` to `max_width` as Excalidraw's `wrapText` does (`crate::text`),
    /// measured with the engine's measure hook and memoised per hard line.
    ///
    /// ponytail: the hook measures whole texts and floors them at 4px (`measure_via_ctx`),
    /// so a char measured alone and narrower than that — a zero-width char, a space at a
    /// small size — wraps as 4px wide. A per-line, per-font measure replaces the hook when
    /// fonts land.
    fn wrap_text_to_width(&self, text: &str, max_width: f64, font_size: f64) -> String {
        let measure = self.measure_text;
        self.text_cache.wrap_text(
            text,
            max_width,
            crate::text::FontKey::legacy(font_size),
            &|line| measure(line, font_size).0,
        )
    }
}
