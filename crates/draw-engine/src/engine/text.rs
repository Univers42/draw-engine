use crate::engine::{DrawEngine, TextEditRequest};
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
        if single.is_deleted || single.locked() {
            return false;
        }
        if single.kind == DrawElementType::Text {
            self.request_text_edit(&single);
            return true;
        }
        if !is_bindable_element(&single) && !is_linear_element(&single) {
            return false;
        }
        let bound = single
            .bound_text_id
            .as_ref()
            .and_then(|id| self.scene.get(id).cloned());
        let label = if let Some(bound) = bound.filter(|el| !el.is_deleted) {
            bound
        } else {
            self.create_label(&single)
        };
        self.set_selection(vec![label.id.clone()]);
        self.request_text_edit(&label);
        true
    }

    fn request_text_edit(&mut self, element: &DrawElement) {
        let screen = crate::world_to_screen(self.camera, element.x, element.y);
        let width = if element.container_id.is_some() {
            Some(element.width * self.camera.scale)
        } else {
            None
        };
        self.events.text_edit = Some(TextEditRequest {
            id: element.id.clone(),
            x: screen.x,
            y: screen.y,
            font_size: element.font_size.unwrap_or(super::DEFAULT_FONT_SIZE),
            color: element.stroke_color.clone(),
            text: element.text.clone().unwrap_or_default(),
            width,
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
        label.container_id = Some(container.id.clone());
        label.stroke_color = self.get_next_style().stroke_color;
        let mut container = container.clone();
        container.bound_text_id = Some(label.id.clone());
        self.scene.add(label.clone());
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

    pub fn handle_double_click(&mut self, sx: f64, sy: f64) {
        let world = self.screen_to_world(sx, sy);
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
        }
        if let Some(container) = self.label_target_at(world.x, world.y) {
            let bound = container
                .bound_text_id
                .as_ref()
                .and_then(|id| self.scene.get(id).cloned());
            let label = if let Some(bound) = bound.filter(|el| !el.is_deleted) {
                bound
            } else {
                self.create_label(&container)
            };
            self.set_selection(vec![label.id.clone()]);
            self.request_text_edit(&label);
            return;
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
        let id = element.id.clone();
        self.scene.add(element.clone());
        self.set_selection(vec![id]);
        self.request_text_edit(&element);
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
            self.scene.discard(id);
            self.clear_selection();
            self.request_draw();
            return;
        }
        let font_size = element.font_size.unwrap_or(super::DEFAULT_FONT_SIZE);
        let final_text = if let Some(container_id) = &element.container_id {
            if let Some(container) = self.scene.get(container_id) {
                let max_width = (container.width.abs() - crate::LABEL_PADDING * 2.0).max(8.0);
                wrap_text_to_width(text, max_width, font_size, &self.measure_text)
            } else {
                text.to_string()
            }
        } else {
            text.to_string()
        };
        let (width, height) = (self.measure_text)(&final_text, font_size);
        let mut next = element;
        next.text = Some(final_text.clone());
        next.width = width;
        next.height = height.max(
            font_size.max(final_text.split('\n').count() as f64 * font_size * TEXT_LINE_HEIGHT),
        );
        self.scene.put(bump_version(next, self.now_ms));
        self.apply_bindings();
        self.push_history();
        self.request_draw();
    }
}

fn wrap_text_to_width<F>(text: &str, max_width: f64, font_size: f64, measure: &F) -> String
where
    F: Fn(&str, f64) -> (f64, f64),
{
    let mut wrapped_lines = Vec::new();
    for hard_line in text.split('\n') {
        if hard_line.is_empty() || measure(hard_line, font_size).0 <= max_width {
            wrapped_lines.push(hard_line.to_string());
            continue;
        }
        let words: Vec<&str> = hard_line.split(' ').collect();
        let mut current_line = String::new();
        for word in words {
            if current_line.is_empty() {
                if measure(word, font_size).0 > max_width {
                    // Break long words character by character
                    let mut chunk = String::new();
                    for ch in word.chars() {
                        let mut test = chunk.clone();
                        test.push(ch);
                        if measure(&test, font_size).0 > max_width && !chunk.is_empty() {
                            wrapped_lines.push(chunk);
                            chunk = ch.to_string();
                        } else {
                            chunk = test;
                        }
                    }
                    if !chunk.is_empty() {
                        current_line = chunk;
                    }
                } else {
                    current_line = word.to_string();
                }
            } else {
                let candidate = format!("{current_line} {word}");
                if measure(&candidate, font_size).0 <= max_width {
                    current_line = candidate;
                } else {
                    wrapped_lines.push(current_line);
                    current_line = word.to_string();
                }
            }
        }
        if !current_line.is_empty() {
            wrapped_lines.push(current_line);
        }
    }
    wrapped_lines.join("\n")
}
