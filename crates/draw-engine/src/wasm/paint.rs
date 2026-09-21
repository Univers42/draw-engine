use draw_rough::ops::{Op, OpSetKind};
use draw_rough::Drawable;
use wasm_bindgen::JsValue;
use web_sys::CanvasRenderingContext2d;

use crate::engine::{PaintView, Painter};
use crate::interaction::Axis;
use crate::render::arrowheads::ArrowheadGeometry;
use crate::render::cache::ShapeCache;
use crate::render::default_arrowhead;
use crate::render::opts::dash_array;
use crate::scene::{DrawElement, DrawElementType};
use crate::selection::{selection_corners, selection_handle_points};

pub struct CanvasPainter<'a> {
    pub ctx: &'a CanvasRenderingContext2d,
}

thread_local! {
    /// Rough geometry, kept between frames.
    ///
    /// A thread-local rather than a field on the engine because the painter is
    /// constructed fresh each frame behind the `Painter` trait, and the cache must not
    /// be. WASM is single-threaded, so this is safe; it is the one piece of global
    /// state in the renderer and it holds nothing but derived data, so dropping it at
    /// any moment is correct, just slower.
    static SHAPES: std::cell::RefCell<ShapeCache> = std::cell::RefCell::new(ShapeCache::new());
}

/// Replays one rough drawable into the context.
///
/// Mirrors rough's own `RoughCanvas.draw`: a `path` is stroked with the element's
/// stroke, a `fillPath` is filled with its background, and a `fillSketch` — which is
/// how every pattern fill arrives — is *stroked* with the background colour at
/// `fillWeight`, not filled. Treating a fillSketch as a fill is the classic way to turn
/// a delicate hachure into a solid block.
fn replay(ctx: &CanvasRenderingContext2d, drawable: &Drawable, element: &DrawElement) {
    let fill_weight = element.stroke_width / 2.0;

    for set in &drawable.sets {
        ctx.begin_path();
        for op in &set.ops {
            match *op {
                Op::Move([x, y]) => ctx.move_to(x, y),
                Op::LineTo([x, y]) => ctx.line_to(x, y),
                Op::BCurveTo([x1, y1, x2, y2, x, y]) => ctx.bezier_curve_to(x1, y1, x2, y2, x, y),
            }
        }

        match set.kind {
            OpSetKind::Path => {
                set_stroke(ctx, &element.stroke_color);
                ctx.set_line_width(element.stroke_width);
                let _ = ctx.set_line_dash(&dash_js(dash_array(element)));
                ctx.stroke();
            }
            OpSetKind::FillPath => {
                set_fill(ctx, &element.background_color);
                let _ = ctx.set_line_dash(&js_sys::Array::new());
                ctx.fill();
            }
            OpSetKind::FillSketch => {
                set_stroke(ctx, &element.background_color);
                ctx.set_line_width(fill_weight);
                let _ = ctx.set_line_dash(&js_sys::Array::new());
                ctx.stroke();
            }
        }
    }
}

fn dash_js(pattern: Option<[f64; 2]>) -> js_sys::Array {
    match pattern {
        None => js_sys::Array::new(),
        Some([a, b]) => [a, b].into_iter().map(JsValue::from_f64).collect(),
    }
}

/// Positions an element-local shape in world space: translate to the element's origin,
/// then rotate about its centre. Geometry is never regenerated for either.
fn with_element_transform(
    ctx: &CanvasRenderingContext2d,
    element: &DrawElement,
    body: impl FnOnce(),
) {
    ctx.save();
    ctx.set_global_alpha((element.opacity / 100.0).clamp(0.0, 1.0));

    if element.angle != 0.0 {
        let cx = element.x + element.width / 2.0;
        let cy = element.y + element.height / 2.0;
        let _ = ctx.translate(cx, cy);
        let _ = ctx.rotate(element.angle);
        let _ = ctx.translate(-cx, -cy);
    }
    let _ = ctx.translate(element.x, element.y);

    body();
    ctx.restore();
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

/// Paints a rectangle, diamond or ellipse as rough.js would.
///
/// Previously this issued `ctx.rect()` / `ctx.ellipse()` directly, which is why every
/// shape came out with crisp CAD edges and why `seed`, `roughness` and `fillStyle` were
/// stored on the element and read by nothing.
fn paint_shape(ctx: &CanvasRenderingContext2d, element: &DrawElement) {
    if element.width == 0.0 && element.height == 0.0 {
        return;
    }
    with_element_transform(ctx, element, || {
        SHAPES.with(|cache| {
            if let Some(drawable) = cache.borrow_mut().get(element) {
                replay(ctx, drawable, element);
            }
        });
    });
}

/// Paints a line or arrow, including every point of a multi-point path.
///
/// The previous implementation drew one `move_to`/`line_to` between the two endpoints,
/// so a multi-point line rendered as a straight segment, dashes were ignored, and
/// `default_arrowhead` was computed and discarded (`let _ = ...`) so arrows were
/// indistinguishable from lines on screen while the SVG export drew them correctly.
fn paint_linear(ctx: &CanvasRenderingContext2d, element: &DrawElement) {
    with_element_transform(ctx, element, || {
        SHAPES.with(|cache| {
            if let Some(drawable) = cache.borrow_mut().get(element) {
                replay(ctx, drawable, element);
            }
        });

        if element.kind == DrawElementType::Arrow {
            paint_arrowheads(ctx, element);
        }
    });
}

/// Strokes the arrowhead geometry at each end that has one.
///
/// Runs inside the element transform, so the points are element-local — the same space
/// the SVG exporter works in, which is what lets both consume one source.
fn paint_arrowheads(ctx: &CanvasRenderingContext2d, element: &DrawElement) {
    let points = element.points.as_deref().unwrap_or(&[]);
    if points.len() < 2 {
        return;
    }

    set_stroke(ctx, &element.stroke_color);
    ctx.set_line_width(element.stroke_width);
    // An arrowhead is always solid, even on a dashed arrow — a dashed head reads as
    // noise at any realistic size.
    let _ = ctx.set_line_dash(&js_sys::Array::new());

    for (end, tip_idx, from_idx) in [
        ("start", 0usize, 1usize),
        ("end", points.len() - 1, points.len() - 2),
    ] {
        let kind = default_arrowhead(element, end);
        let Some(head) = crate::render::arrowheads::arrowhead_geometry(
            kind,
            points[tip_idx],
            points[from_idx],
            element.stroke_width,
        ) else {
            continue;
        };

        match head {
            ArrowheadGeometry::Polyline(pts) => {
                ctx.begin_path();
                ctx.move_to(pts[0][0], pts[0][1]);
                for p in &pts[1..] {
                    ctx.line_to(p[0], p[1]);
                }
                ctx.stroke();
            }
            ArrowheadGeometry::Polygon(pts) => {
                set_fill(ctx, &element.stroke_color);
                ctx.begin_path();
                ctx.move_to(pts[0][0], pts[0][1]);
                for p in &pts[1..] {
                    ctx.line_to(p[0], p[1]);
                }
                ctx.close_path();
                ctx.fill();
            }
            ArrowheadGeometry::Dot { center, radius } => {
                set_fill(ctx, &element.stroke_color);
                ctx.begin_path();
                let _ = ctx.arc(
                    center[0],
                    center[1],
                    radius,
                    0.0,
                    std::f64::consts::PI * 2.0,
                );
                ctx.fill();
            }
        }
    }
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
