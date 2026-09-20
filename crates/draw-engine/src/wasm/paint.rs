use wasm_bindgen::JsValue;
use web_sys::CanvasRenderingContext2d;

use crate::engine::{PaintView, Painter};
use crate::interaction::Axis;
use crate::render::default_arrowhead;
use crate::scene::binding::linear_endpoints;
use crate::scene::geometry::normalize_rect;
use crate::scene::{DrawElement, DrawElementType, StrokeStyle};
use crate::selection::{selection_corners, selection_handle_points};

pub struct CanvasPainter<'a> {
    pub ctx: &'a CanvasRenderingContext2d,
}

fn set_fill(ctx: &CanvasRenderingContext2d, color: &str) {
    let _ = js_sys::Reflect::set(ctx.as_ref(), &"fillStyle".into(), &JsValue::from_str(color));
}

fn set_stroke(ctx: &CanvasRenderingContext2d, color: &str) {
    let _ = js_sys::Reflect::set(
        ctx.as_ref(),
        &"strokeStyle".into(),
        &JsValue::from_str(color),
    );
}

fn dash(style: StrokeStyle) -> Vec<f64> {
    match style {
        StrokeStyle::Solid => vec![],
        StrokeStyle::Dashed => vec![8.0, 6.0],
        StrokeStyle::Dotted => vec![2.0, 4.0],
    }
}

impl Painter for CanvasPainter<'_> {
    fn paint(&mut self, view: &PaintView) {
        let ctx = self.ctx;
        let dpr = view.dpr;
        let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
        ctx.clear_rect(0.0, 0.0, view.width, view.height);
        set_fill(ctx, &view.theme.background);
        ctx.fill_rect(0.0, 0.0, view.width, view.height);
        paint_grid(ctx, view);
        ctx.save();
        let _ = ctx.translate(view.camera.x, view.camera.y);
        let _ = ctx.scale(view.camera.scale, view.camera.scale);
        for element in &view.elements {
            paint_element(ctx, element, &view.theme.background);
        }
        ctx.restore();
        paint_overlay(ctx, view);
    }
}

fn paint_grid(ctx: &CanvasRenderingContext2d, view: &PaintView) {
    let step = 40.0 * view.camera.scale;
    if step < 6.0 {
        return;
    }
    ctx.save();
    set_stroke(ctx, &view.theme.grid);
    ctx.set_line_width(1.0);
    ctx.begin_path();
    let mut x = view.camera.x % step;
    while x < view.width {
        let px = x.round() + 0.5;
        ctx.move_to(px, 0.0);
        ctx.line_to(px, view.height);
        x += step;
    }
    let mut y = view.camera.y % step;
    while y < view.height {
        let py = y.round() + 0.5;
        ctx.move_to(0.0, py);
        ctx.line_to(view.width, py);
        y += step;
    }
    ctx.stroke();
    ctx.restore();
}

fn paint_element(ctx: &CanvasRenderingContext2d, element: &DrawElement, background: &str) {
    match element.kind {
        DrawElementType::Line | DrawElementType::Arrow => paint_linear(ctx, element),
        DrawElementType::Freedraw => paint_freedraw(ctx, element),
        DrawElementType::Text => paint_text(
            ctx,
            element,
            element.container_id.as_ref().map(|_| background),
        ),
        _ => paint_shape(ctx, element),
    }
}

fn paint_shape(ctx: &CanvasRenderingContext2d, element: &DrawElement) {
    let rect = normalize_rect(element.x, element.y, element.width, element.height);
    if rect.width == 0.0 && rect.height == 0.0 {
        return;
    }
    ctx.save();
    ctx.set_global_alpha((element.opacity / 100.0).clamp(0.0, 1.0));
    if element.angle != 0.0 {
        let cx = rect.x + rect.width / 2.0;
        let cy = rect.y + rect.height / 2.0;
        let _ = ctx.translate(cx, cy);
        let _ = ctx.rotate(element.angle);
        let _ = ctx.translate(-cx, -cy);
    }
    ctx.begin_path();
    match element.kind {
        DrawElementType::Ellipse => {
            let _ = ctx.ellipse(
                rect.x + rect.width / 2.0,
                rect.y + rect.height / 2.0,
                rect.width / 2.0,
                rect.height / 2.0,
                0.0,
                0.0,
                std::f64::consts::PI * 2.0,
            );
        }
        DrawElementType::Diamond => {
            ctx.move_to(rect.x + rect.width / 2.0, rect.y);
            ctx.line_to(rect.x + rect.width, rect.y + rect.height / 2.0);
            ctx.line_to(rect.x + rect.width / 2.0, rect.y + rect.height);
            ctx.line_to(rect.x, rect.y + rect.height / 2.0);
            ctx.close_path();
        }
        _ => {
            ctx.rect(rect.x, rect.y, rect.width, rect.height);
        }
    }
    if !element.background_color.is_empty() && element.background_color != "transparent" {
        set_fill(ctx, &element.background_color);
        ctx.fill();
    }
    ctx.set_line_width(element.stroke_width);
    set_stroke(ctx, &element.stroke_color);
    let _ = ctx.set_line_dash(
        &dash(element.stroke_style)
            .into_iter()
            .map(JsValue::from_f64)
            .collect::<js_sys::Array>(),
    );
    ctx.stroke();
    ctx.restore();
}

fn paint_linear(ctx: &CanvasRenderingContext2d, element: &DrawElement) {
    let (start, end) = linear_endpoints(element);
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = dx.hypot(dy);
    if length < 0.5 {
        return;
    }
    ctx.save();
    ctx.set_global_alpha((element.opacity / 100.0).clamp(0.0, 1.0));
    set_stroke(ctx, &element.stroke_color);
    ctx.set_line_width(element.stroke_width);
    ctx.begin_path();
    ctx.move_to(start.x, start.y);
    ctx.line_to(end.x, end.y);
    ctx.stroke();
    let _ = default_arrowhead(element, "end");
    ctx.restore();
}

fn paint_freedraw(ctx: &CanvasRenderingContext2d, element: &DrawElement) {
    let points = element.points.as_deref().unwrap_or(&[]);
    if points.len() < 2 {
        return;
    }
    ctx.save();
    ctx.set_global_alpha((element.opacity / 100.0).clamp(0.0, 1.0));
    set_stroke(ctx, &element.stroke_color);
    ctx.set_line_width(element.stroke_width);
    let _ = ctx.translate(element.x, element.y);
    ctx.begin_path();
    ctx.move_to(points[0][0], points[0][1]);
    for point in &points[1..] {
        ctx.line_to(point[0], point[1]);
    }
    ctx.stroke();
    ctx.restore();
}

fn paint_text(ctx: &CanvasRenderingContext2d, element: &DrawElement, backdrop: Option<&str>) {
    let text = element.text.as_deref().unwrap_or("");
    if text.is_empty() {
        return;
    }
    let font_size = element.font_size.unwrap_or(20.0);
    ctx.save();
    ctx.set_global_alpha((element.opacity / 100.0).clamp(0.0, 1.0));
    ctx.set_font(&format!("{font_size}px sans-serif"));
    if let Some(backdrop) = backdrop {
        set_fill(ctx, backdrop);
        ctx.fill_rect(
            element.x - 4.0,
            element.y - 2.0,
            element.width + 8.0,
            element.height + 4.0,
        );
    }
    set_fill(ctx, &element.stroke_color);
    let line_height = font_size * crate::TEXT_LINE_HEIGHT;
    for (i, line) in text.split('\n').enumerate() {
        let _ = ctx.fill_text(line, element.x, element.y + i as f64 * line_height);
    }
    ctx.restore();
}

fn paint_overlay(ctx: &CanvasRenderingContext2d, view: &PaintView) {
    set_stroke(ctx, &view.theme.accent);
    ctx.set_line_width(1.0);
    if let Some(rect) = view.marquee {
        let tl = crate::world_to_screen(view.camera, rect.min_x, rect.min_y);
        let br = crate::world_to_screen(view.camera, rect.max_x, rect.max_y);
        ctx.save();
        ctx.set_global_alpha(0.12);
        set_fill(ctx, &view.theme.accent);
        ctx.fill_rect(tl.x, tl.y, br.x - tl.x, br.y - tl.y);
        ctx.set_global_alpha(1.0);
        ctx.stroke_rect(tl.x, tl.y, br.x - tl.x, br.y - tl.y);
        ctx.restore();
    }
    for guide in &view.snap_guides {
        let (a, b) = if guide.axis == Axis::X {
            (
                crate::world_to_screen(view.camera, guide.at, guide.from),
                crate::world_to_screen(view.camera, guide.at, guide.to),
            )
        } else {
            (
                crate::world_to_screen(view.camera, guide.from, guide.at),
                crate::world_to_screen(view.camera, guide.to, guide.at),
            )
        };
        ctx.begin_path();
        ctx.move_to(a.x, a.y);
        ctx.line_to(b.x, b.y);
        ctx.stroke();
    }
    if view.selected.len() == 1 {
        let corners = selection_corners(&view.selected[0]);
        ctx.begin_path();
        let first = crate::world_to_screen(view.camera, corners[0].x, corners[0].y);
        ctx.move_to(first.x, first.y);
        for corner in &corners[1..] {
            let p = crate::world_to_screen(view.camera, corner.x, corner.y);
            ctx.line_to(p.x, p.y);
        }
        ctx.close_path();
        ctx.stroke();
        let half = view.handle_px / 2.0;
        for point in selection_handle_points(&view.selected[0], view.rotate_gap) {
            let s = crate::world_to_screen(view.camera, point.x, point.y);
            ctx.begin_path();
            ctx.rect(s.x - half, s.y - half, view.handle_px, view.handle_px);
            set_fill(ctx, &view.theme.background);
            ctx.fill();
            ctx.stroke();
        }
    } else if view.selected.len() > 1 {
        if let Some(bounds) = crate::scene_bounds(&view.selected) {
            let tl = crate::world_to_screen(view.camera, bounds.min_x, bounds.min_y);
            let br = crate::world_to_screen(view.camera, bounds.max_x, bounds.max_y);
            ctx.stroke_rect(tl.x, tl.y, br.x - tl.x, br.y - tl.y);
        }
    }
}
