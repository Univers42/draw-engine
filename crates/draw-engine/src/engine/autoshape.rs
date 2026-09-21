use crate::engine::DrawEngine;
use crate::scene::{
    arrow_endpoints, bump_version, recognize_shape, DrawElementType, RecognizedShape,
};

impl DrawEngine {
    /// Turn a freehand stroke into the shape it was meant to be.
    ///
    /// Replaces the element in place, keeping its id, so the conversion is one step in
    /// the history and anything referring to the stroke — a binding, a selection — still
    /// refers to it afterwards. A stroke that is not enough like any shape is left
    /// exactly as drawn, which is the behaviour that makes the tool safe to leave on:
    /// the worst it does is nothing.
    ///
    /// Returns what the stroke was taken for.
    pub fn convert_to_shape(&mut self, id: &str) -> RecognizedShape {
        let Some(element) = self.scene.get(id).cloned() else {
            return RecognizedShape::Freedraw;
        };
        if element.kind != DrawElementType::Freedraw {
            return RecognizedShape::Freedraw;
        }

        let points = crate::selection::linear::world_points(&element);
        let shape = recognize_shape(&points, self.camera.scale);
        if shape == RecognizedShape::Freedraw {
            return shape;
        }

        let mut next = element.clone();
        // The stroke's own style carries over: someone who set a colour before drawing
        // meant it for whatever the stroke became.
        match shape {
            RecognizedShape::Rectangle | RecognizedShape::Diamond | RecognizedShape::Ellipse => {
                let min_x = points.iter().fold(f64::MAX, |a, p| a.min(p.x));
                let max_x = points.iter().fold(f64::MIN, |a, p| a.max(p.x));
                let min_y = points.iter().fold(f64::MAX, |a, p| a.min(p.y));
                let max_y = points.iter().fold(f64::MIN, |a, p| a.max(p.y));
                next.kind = match shape {
                    RecognizedShape::Rectangle => DrawElementType::Rectangle,
                    RecognizedShape::Diamond => DrawElementType::Diamond,
                    _ => DrawElementType::Ellipse,
                };
                next.x = min_x;
                next.y = min_y;
                next.width = (max_x - min_x).max(1.0);
                next.height = (max_y - min_y).max(1.0);
                // A shape is described by its box; leaving the path behind would make
                // every later hit test and resize read points that no longer mean
                // anything.
                next.points = None;
            }
            RecognizedShape::Line | RecognizedShape::Arrow => {
                let Some((start, tip)) = arrow_endpoints(&points) else {
                    return RecognizedShape::Freedraw;
                };
                next.kind = if shape == RecognizedShape::Arrow {
                    DrawElementType::Arrow
                } else {
                    DrawElementType::Line
                };
                next.x = start.x;
                next.y = start.y;
                next.width = tip.x - start.x;
                next.height = tip.y - start.y;
                next.points = Some(vec![[0.0, 0.0], [tip.x - start.x, tip.y - start.y]]);
                // Deliberately not set here: an arrow's head comes from its type, via
                // `render::default_arrowhead`. Baking one in would freeze it against a
                // later change to the default.
            }
            RecognizedShape::Freedraw => unreachable!("handled above"),
        }

        self.scene.put(bump_version(next, self.now_ms));
        // A line or arrow that now ends inside a shape should attach to it, exactly as
        // one drawn with the arrow tool would.
        self.apply_bindings();
        self.request_draw();
        shape
    }
}
