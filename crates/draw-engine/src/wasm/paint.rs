use std::collections::HashMap;

use draw_rough::ops::{Op, OpSetKind};
use draw_rough::Drawable;
use wasm_bindgen::JsValue;
use web_sys::{CanvasRenderingContext2d, Path2d};

use crate::engine::{PaintView, Painter};
use crate::interaction::Axis;
use crate::render::arrowheads::ArrowheadGeometry;
use crate::render::cache::{shape_fingerprint, ShapeCache};
use crate::render::default_arrowhead;
use crate::render::opts::dash_array;
use crate::scene::{DrawElement, DrawElementType};
use crate::selection::{selection_corners_padded, selection_handles, HandleKind};

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

thread_local! {
    /// One `Path2D` per op set, rebuilt only when the shape's geometry changes.
    ///
    /// This is the single biggest win in the render path. Replaying a drawable op by op
    /// means a `move_to` / `line_to` / `bezier_curve_to` call **per op, per frame**, and
    /// every one of those crosses the WASM-to-JS boundary. A hachure-filled rectangle is
    /// a few thousand ops on its own; measured in the browser, a modest scene was
    /// issuing ~9,700 canvas calls per frame at roughly 2.7us each — about 26ms, which
    /// is a missed frame before any pixels are touched.
    ///
    /// A `Path2D` is built once and then drawn with a single `stroke`/`fill` call per op
    /// set per frame. Because the geometry is in element-local space and position, zoom
    /// and rotation are applied to the *context*, the same path object stays valid
    /// through panning, zooming and dragging — it is only rebuilt when the element
    /// genuinely changes shape.
    static PATHS: std::cell::RefCell<HashMap<String, CachedPaths>> =
        std::cell::RefCell::new(HashMap::new());
}

/// One element's built paths, tagged with the shape fingerprint they were built from.
///
/// The fingerprint is what makes a translate free: it covers geometry only, so panning,
/// zooming and dragging leave it untouched and the cached paths stay valid.
type CachedPaths = (u64, Vec<(OpSetKind, Path2d)>);

/// Builds the `Path2D` objects for one drawable.
fn build_paths(drawable: &Drawable) -> Vec<(OpSetKind, Path2d)> {
    drawable
        .sets
        .iter()
        .filter_map(|set| {
            let path = Path2d::new().ok()?;
            for op in &set.ops {
                match *op {
                    Op::Move([x, y]) => path.move_to(x, y),
                    Op::LineTo([x, y]) => path.line_to(x, y),
                    Op::BCurveTo([x1, y1, x2, y2, x, y]) => {
                        path.bezier_curve_to(x1, y1, x2, y2, x, y)
                    }
                }
            }
            Some((set.kind, path))
        })
        .collect()
}

/// Draws one element's cached paths.
///
/// Mirrors rough's own `RoughCanvas.draw`: a `path` is stroked with the element's
/// stroke, a `fillPath` is filled with its background, and a `fillSketch` — which is how
/// every pattern fill arrives — is *stroked* with the background colour at `fillWeight`,
/// not filled. Treating a fillSketch as a fill turns a delicate hachure into a solid
/// block.
fn replay(ctx: &CanvasRenderingContext2d, element: &DrawElement) {
    let fingerprint = shape_fingerprint(element);
    let fill_weight = element.stroke_width / 2.0;

    PATHS.with(|cache| {
        let mut cache = cache.borrow_mut();

        let stale = match cache.get(&element.id) {
            Some((cached, _)) => *cached != fingerprint,
            None => true,
        };
        if stale {
            let built = SHAPES.with(|shapes| {
                shapes
                    .borrow_mut()
                    .get(element)
                    .map(build_paths)
                    .unwrap_or_default()
            });
            cache.insert(element.id.clone(), (fingerprint, built));
        }

        let Some((_, paths)) = cache.get(&element.id) else {
            return;
        };

        for (kind, path) in paths {
            match kind {
                OpSetKind::Path => {
                    set_stroke(ctx, &element.stroke_color);
                    set_line_width_cached(ctx, element.stroke_width);
                    set_dash_cached(ctx, dash_array(element));
                    ctx.stroke_with_path(path);
                }
                OpSetKind::FillPath => {
                    set_fill(ctx, &element.background_color);
                    set_dash_cached(ctx, None);
                    ctx.fill_with_path_2d(path);
                }
                OpSetKind::FillSketch => {
                    set_stroke(ctx, &element.background_color);
                    set_line_width_cached(ctx, fill_weight);
                    set_dash_cached(ctx, None);
                    ctx.stroke_with_path(path);
                }
            }
        }
    });
}

/// Drops cached paths for elements that are no longer in the scene.
///
/// Without this the cache is a leak, and a long editing session churns through a lot of
/// elements. Run once per frame against what was actually drawn.
fn evict_paths(live: &[&DrawElement]) {
    PATHS.with(|cache| {
        let mut cache = cache.borrow_mut();
        // Only worth the sweep once the cache has outgrown the scene by a clear margin;
        // doing it every frame would cost more than it reclaims.
        if cache.len() <= live.len().saturating_mul(2).max(64) {
            return;
        }
        let ids: std::collections::HashSet<&str> = live.iter().map(|e| e.id.as_str()).collect();
        cache.retain(|id, _| ids.contains(id.as_str()));
    });
}

thread_local! {
    /// The empty dash pattern, allocated once rather than per shape per frame.
    static EMPTY_DASH: js_sys::Array = js_sys::Array::new();
}

fn dash_js(pattern: Option<[f64; 2]>) -> js_sys::Array {
    match pattern {
        None => js_sys::Array::new(),
        Some([a, b]) => [a, b].into_iter().map(JsValue::from_f64).collect(),
    }
}

/// Positions an element-local shape in world space.
///
/// This used to be `save` / `translate` / `rotate` / `restore` around every element —
/// four to six boundary crossings each, which at 8,000 elements is tens of thousands of
/// calls a frame doing nothing but bookkeeping. The whole chain is one 2x3 matrix, so it
/// is multiplied out in Rust and applied with a single `setTransform`.
///
/// `view_transform` is the device-pixel-ratio and camera part, computed once per frame.
///
/// # Mirroring
///
/// A negative `width` or `height` means the element is mirrored on that axis, and the
/// mirror is applied **here**, as a sign in the matrix. The geometry is generated from
/// the absolute size, so a flip regenerates nothing at all: the cached rough ops and the
/// cached `Path2D` both stay valid and the whole operation is one sign change per axis.
/// Baking the mirror into the geometry instead would rebuild every op — and, because
/// rough's jitter depends on the coordinates it is handed, would produce a *different*
/// hand-drawn stroke rather than the same one reversed.
///
/// `element.x` is the anchor corner, which is the left edge when the width is positive
/// and the right edge when it is negative. That is the same convention
/// [`crate::scene::geometry::normalize_rect`] already implements, so bounds, hit testing
/// and binding need no special case.
fn with_element_transform(
    ctx: &CanvasRenderingContext2d,
    view: [f64; 6],
    element: &DrawElement,
    body: impl FnOnce(),
) {
    set_alpha_cached(ctx, (element.opacity / 100.0).clamp(0.0, 1.0));

    // Shapes only: a line or arrow carries its mirror in its points, so the sign of its
    // width means nothing and applying it would reverse the element a second time.
    let (sx, sy) = crate::scene::geometry::mirror_signs(element);

    // Element-local: mirror, then translate to the origin, and rotate about the centre
    // if turned.
    let (a, b, c, d, e, f) = if element.angle == 0.0 {
        (sx, 0.0, 0.0, sy, element.x, element.y)
    } else {
        // The pivot in the element's own coordinates, and the same pivot in world space.
        // For a shape this is the middle of its box, as before. For a line or arrow it is
        // the middle of its *points*: reading it as `x + width / 2` put the pivot outside
        // a leftward arrow altogether, so turning one swung it around a point past its
        // own tip.
        let (lcx, lcy) = crate::scene::geometry::local_center(element);
        let centre = crate::scene::geometry::rotation_center(element);
        let (sin, cos) = element.angle.sin_cos();
        // T(centre) * R * S(sx, sy) * T(-local centre)
        (
            sx * cos,
            sx * sin,
            -sy * sin,
            sy * cos,
            centre.x - sx * cos * lcx + sy * sin * lcy,
            centre.y - sx * sin * lcx - sy * cos * lcy,
        )
    };

    let m = mul(view, [a, b, c, d, e, f]);
    let _ = ctx.set_transform(m[0], m[1], m[2], m[3], m[4], m[5]);

    body();
}

/// Multiplies two 2x3 affine transforms in Canvas2D's `[a, b, c, d, e, f]` order.
fn mul(p: [f64; 6], q: [f64; 6]) -> [f64; 6] {
    [
        p[0] * q[0] + p[2] * q[1],
        p[1] * q[0] + p[3] * q[1],
        p[0] * q[2] + p[2] * q[3],
        p[1] * q[2] + p[3] * q[3],
        p[0] * q[4] + p[2] * q[5] + p[4],
        p[1] * q[4] + p[3] * q[5] + p[5],
    ]
}

/// The canvas state the painter believes is currently set.
///
/// Every `strokeStyle`, `lineWidth` and `setLineDash` is a call across the WASM
/// boundary, and a scene overwhelmingly uses a handful of styles — 8,000 elements
/// sharing one stroke colour were setting it 8,000 times a frame. Tracking what is
/// already set turns that into one call.
#[derive(Default)]
struct PaintState {
    stroke: Option<String>,
    fill: Option<String>,
    line_width: Option<f64>,
    dash: Option<Option<[f64; 2]>>,
    alpha: Option<f64>,
}

impl PaintState {
    /// Forgets everything. Called once per frame, because the context's state is not
    /// ours to assume across frames.
    fn reset(&mut self) {
        *self = Self::default();
    }
}

thread_local! {
    static STATE: std::cell::RefCell<PaintState> =
        std::cell::RefCell::new(PaintState::default());
}

fn set_line_width_cached(ctx: &CanvasRenderingContext2d, width: f64) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.line_width != Some(width) {
            ctx.set_line_width(width);
            s.line_width = Some(width);
        }
    });
}

fn set_alpha_cached(ctx: &CanvasRenderingContext2d, alpha: f64) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.alpha != Some(alpha) {
            ctx.set_global_alpha(alpha);
            s.alpha = Some(alpha);
        }
    });
}

fn set_dash_cached(ctx: &CanvasRenderingContext2d, dash: Option<[f64; 2]>) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.dash != Some(dash) {
            let _ = ctx.set_line_dash(&dash_js(dash));
            s.dash = Some(dash);
        }
    });
}

thread_local! {
    /// Interned colour strings.
    ///
    /// `fillStyle` and `strokeStyle` are a union type in web-sys, so they can only be
    /// set reflectively — and that used to allocate a fresh `JsValue` from a Rust `&str`
    /// for every colour, on every element, on every frame. A scene has a handful of
    /// distinct colours and they almost never change, so they are allocated once.
    static COLORS: std::cell::RefCell<HashMap<String, JsValue>> =
        std::cell::RefCell::new(HashMap::new());

    /// The property-name keys, which were also being rebuilt per call.
    static FILL_KEY: JsValue = JsValue::from_str("fillStyle");
    static STROKE_KEY: JsValue = JsValue::from_str("strokeStyle");
}

fn color_value(color: &str) -> JsValue {
    COLORS.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache
            .entry(color.to_string())
            .or_insert_with(|| JsValue::from_str(color))
            .clone()
    })
}

fn set_fill(ctx: &CanvasRenderingContext2d, color: &str) {
    let changed = STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.fill.as_deref() == Some(color) {
            return false;
        }
        s.fill = Some(color.to_string());
        true
    });
    if !changed {
        return;
    }
    let value = color_value(color);
    FILL_KEY.with(|key| {
        let _ = js_sys::Reflect::set(ctx.as_ref(), key, &value);
    });
}

fn set_stroke(ctx: &CanvasRenderingContext2d, color: &str) {
    let changed = STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.stroke.as_deref() == Some(color) {
            return false;
        }
        s.stroke = Some(color.to_string());
        true
    });
    if !changed {
        return;
    }
    let value = color_value(color);
    STROKE_KEY.with(|key| {
        let _ = js_sys::Reflect::set(ctx.as_ref(), key, &value);
    });
}

impl Painter for CanvasPainter<'_> {
    fn paint(&mut self, view: &PaintView) {
        let ctx = self.ctx;
        let dpr = view.dpr;

        // The context's state is not ours to assume across frames.
        STATE.with(|s| s.borrow_mut().reset());
        FONT.with(|f| *f.borrow_mut() = None);

        let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
        ctx.clear_rect(0.0, 0.0, view.width, view.height);
        set_fill(ctx, &view.theme.background);
        ctx.fill_rect(0.0, 0.0, view.width, view.height);
        paint_grid(ctx, view);

        // Device pixel ratio and camera, folded into one matrix and combined with each
        // element's own transform rather than pushed and popped around every element.
        let s = view.camera.scale;
        let view_transform = [
            dpr * s,
            0.0,
            0.0,
            dpr * s,
            dpr * view.camera.x,
            dpr * view.camera.y,
        ];

        for element in &view.elements {
            paint_element(ctx, view_transform, element, &view.theme.background);
        }
        evict_paths(&view.elements);

        let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
        paint_overlay(ctx, view);
    }
}

/// Draws the grid, when there is one to draw.
///
/// Two passes, minor lines then major, so every `step`-th line reads as heavier. A single
/// uniform weight is what makes a fine grid turn into a grey wash at anything but the
/// coarsest spacing — the emphasis is what you count squares against.
///
/// The spacing is in **world** units and converted here, so the grid belongs to the
/// drawing rather than to the viewport: zoom in and the squares grow with the shapes,
/// and a line stays on the same world coordinate while you pan.
fn paint_grid(ctx: &CanvasRenderingContext2d, view: &PaintView) {
    if !view.grid.enabled {
        return;
    }

    let world_size = view.grid.effective_size();
    let spacing = world_size * view.camera.scale;
    // Below a few pixels apart the lines merge into a solid field, which is worse than
    // no grid at all.
    if spacing < 4.0 {
        return;
    }

    let step = view.grid.step.max(1) as i64;
    let is_major = |n: i64| step > 1 && n.rem_euclid(step) == 0;
    // When the minor lines would be too dense to tell apart, draw only the majors —
    // those are `step` times further apart, so they stay legible.
    let draw_minors = spacing >= 8.0 || step == 1;

    let to_screen_x = |wx: f64| wx * view.camera.scale + view.camera.x;
    let to_screen_y = |wy: f64| wy * view.camera.scale + view.camera.y;

    // The inclusive index range whose lines fall inside the viewport.
    let i0 = ((-view.camera.x / view.camera.scale) / world_size).floor() as i64;
    let i1 = (((view.width - view.camera.x) / view.camera.scale) / world_size).ceil() as i64;
    let j0 = ((-view.camera.y / view.camera.scale) / world_size).floor() as i64;
    let j1 = (((view.height - view.camera.y) / view.camera.scale) / world_size).ceil() as i64;

    for major in [false, true] {
        if (!major && !draw_minors) || (major && step == 1) {
            continue;
        }

        ctx.save();
        set_stroke(ctx, &view.theme.grid);
        // The theme colour is tuned for the minor lines; a major is the same hue drawn
        // heavier rather than a second colour, so a custom grid colour stays coherent.
        ctx.set_line_width(if major { 1.6 } else { 1.0 });
        ctx.set_global_alpha(if major { 1.0 } else { 0.6 });
        ctx.begin_path();

        for i in i0..=i1 {
            if is_major(i) == major {
                let px = to_screen_x(i as f64 * world_size).round() + 0.5;
                ctx.move_to(px, 0.0);
                ctx.line_to(px, view.height);
            }
        }
        for j in j0..=j1 {
            if is_major(j) == major {
                let py = to_screen_y(j as f64 * world_size).round() + 0.5;
                ctx.move_to(0.0, py);
                ctx.line_to(view.width, py);
            }
        }

        ctx.stroke();
        ctx.restore();
    }
}

fn paint_element(
    ctx: &CanvasRenderingContext2d,
    view: [f64; 6],
    element: &DrawElement,
    background: &str,
) {
    match element.kind {
        DrawElementType::Line | DrawElementType::Arrow => paint_linear(ctx, view, element),
        DrawElementType::Freedraw => paint_freedraw(ctx, view, element),
        DrawElementType::Text => paint_text(
            ctx,
            view,
            element,
            element.container_id.as_ref().map(|_| background),
        ),
        _ => paint_shape(ctx, view, element),
    }
}

/// Paints a rectangle, diamond or ellipse as rough.js would.
///
/// Previously this issued `ctx.rect()` / `ctx.ellipse()` directly, which is why every
/// shape came out with crisp CAD edges and why `seed`, `roughness` and `fillStyle` were
/// stored on the element and read by nothing.
fn paint_shape(ctx: &CanvasRenderingContext2d, view: [f64; 6], element: &DrawElement) {
    if element.width == 0.0 && element.height == 0.0 {
        return;
    }
    with_element_transform(ctx, view, element, || replay(ctx, element));
}

/// Paints a line or arrow, including every point of a multi-point path.
///
/// The previous implementation drew one `move_to`/`line_to` between the two endpoints,
/// so a multi-point line rendered as a straight segment, dashes were ignored, and
/// `default_arrowhead` was computed and discarded (`let _ = ...`) so arrows were
/// indistinguishable from lines on screen while the SVG export drew them correctly.
fn paint_linear(ctx: &CanvasRenderingContext2d, view: [f64; 6], element: &DrawElement) {
    with_element_transform(ctx, view, element, || {
        replay(ctx, element);
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
    set_line_width_cached(ctx, element.stroke_width);
    // An arrowhead is always solid, even on a dashed arrow — a dashed head reads as
    // noise at any realistic size.
    set_dash_cached(ctx, None);

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

fn paint_freedraw(ctx: &CanvasRenderingContext2d, view: [f64; 6], element: &DrawElement) {
    let points = element.points.as_deref().unwrap_or(&[]);
    if points.len() < 2 {
        return;
    }
    with_element_transform(ctx, view, element, || {
        set_stroke(ctx, &element.stroke_color);
        set_line_width_cached(ctx, element.stroke_width);
        set_dash_cached(ctx, None);
        ctx.begin_path();
        ctx.move_to(points[0][0], points[0][1]);
        for point in &points[1..] {
            ctx.line_to(point[0], point[1]);
        }
        ctx.stroke();
    });
}

/// Draws a text element.
///
/// # The baseline
///
/// `textBaseline` was never set, so Canvas2D's default of `"alphabetic"` applied and
/// the first line's *baseline* sat on the element's top edge — meaning the glyphs
/// rendered entirely **above** their own bounding box. Three things followed from that:
/// text jumped by a line height when you committed an edit, clicking on visible text
/// did not select it (the hit test uses the box, which was empty), and a bound label's
/// backdrop was painted below its glyphs instead of behind them. The SVG exporter got
/// it right, so canvas and export disagreed about where text was.
///
/// `"top"` puts the top of the line on the top of the box, which is what the box means.
/// Excalidraw reaches the same result via `alphabetic` plus a computed vertical offset;
/// matching that exactly needs real font metrics, which is tracked separately.
fn paint_text(
    ctx: &CanvasRenderingContext2d,
    view: [f64; 6],
    element: &DrawElement,
    backdrop: Option<&str>,
) {
    let text = element.text.as_deref().unwrap_or("");
    if text.is_empty() {
        return;
    }
    let font_size = element.font_size.unwrap_or(20.0);

    with_element_transform(ctx, view, element, || {
        set_font_cached(ctx, font_size);
        ctx.set_text_baseline("top");

        if let Some(backdrop) = backdrop {
            set_fill(ctx, backdrop);
            ctx.fill_rect(-4.0, -2.0, element.width + 8.0, element.height + 4.0);
        }

        set_fill(ctx, &element.stroke_color);
        let line_height = font_size * crate::TEXT_LINE_HEIGHT;
        for (i, line) in text.split('\n').enumerate() {
            let _ = ctx.fill_text(line, 0.0, i as f64 * line_height);
        }
    });
}

thread_local! {
    /// The font string currently set, so `format!` and the property write only happen
    /// when the size actually changes.
    static FONT: std::cell::RefCell<Option<f64>> = const { std::cell::RefCell::new(None) };
}

fn set_font_cached(ctx: &CanvasRenderingContext2d, size: f64) {
    FONT.with(|f| {
        let mut f = f.borrow_mut();
        if *f != Some(size) {
            // The same string the measurer uses. Drawing with `sans-serif` while
            // measuring with anything else is how a text box ends up the wrong size for
            // the glyphs inside it.
            ctx.set_font(&crate::font_string(size));
            *f = Some(size);
        }
    });
}

/// Radius of a point handle on a line or arrow.
const POINT_HANDLE_R: f64 = 5.0;

/// Draws everything that is not the document itself: the marquee, snap guides, the
/// selection frame and handles, and the binding hint.
///
/// The selection UI is modelled on Excalidraw's, observed directly rather than guessed
/// at, because the differences are not cosmetic:
///
/// - A **line or arrow gets no bounding box at all** — just a circle on each point and
///   a filled circle at each midpoint. Scaling a box cannot express "point this end
///   somewhere else", and for a dead-horizontal arrow the box is degenerate so every
///   box handle lands on the same spot.
/// - A **shape gets four corner handles and a rotation handle**, not eight. Excalidraw
///   omits the cardinal handles by default; drawing them adds four targets that mostly
///   get in the way of the corners.
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

    paint_binding_highlight(ctx, view);

    // A linear element is edited by its points; it gets no frame.
    if !view.linear_handles.is_empty() {
        paint_linear_handles(ctx, view);
        return;
    }

    if view.selected.len() == 1 {
        paint_shape_selection(ctx, view, view.selected[0]);
    } else if view.selected.len() > 1 {
        paint_group_selection(ctx, view);
    }
}

/// The frame and its handles for a single shape.
///
/// The frame sits `frame_pad` outside the element, not on it, so the element's own
/// outline is still a move target — and every handle drawn here comes from the same
/// [`selection_handles`] call the pointer code hit-tests against, so the two cannot
/// drift. They did: the cardinal handles were hit-testable but never painted, which made
/// grabbing the middle of an edge resize a shape you were only trying to move.
fn paint_shape_selection(ctx: &CanvasRenderingContext2d, view: &PaintView, element: &DrawElement) {
    let corners = selection_corners_padded(element, view.handle_layout.frame_pad);
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
    for point in selection_handles(element, view.handle_layout) {
        let s = crate::world_to_screen(view.camera, point.x, point.y);
        ctx.begin_path();
        if point.kind == HandleKind::Rotate {
            // A circle, so it reads as "turn" rather than "resize".
            let _ = ctx.arc(s.x, s.y, half, 0.0, std::f64::consts::PI * 2.0);
        } else {
            // Square to the *element*, not to the screen. Handle positions already turn
            // with the shape; leaving the boxes axis-aligned left them visibly askew
            // against a rotated frame, as if they belonged to something else.
            handle_square(ctx, s.x, s.y, half, element.angle);
        }
        set_fill(ctx, &view.theme.background);
        ctx.fill();
        ctx.stroke();
    }
}

/// A `half`-radius square centred on `(cx, cy)` and turned by `angle`.
///
/// Built as an explicit path rather than `save`/`rotate`/`rect`/`restore`, which would be
/// four boundary crossings per handle instead of five cheap ones.
fn handle_square(ctx: &CanvasRenderingContext2d, cx: f64, cy: f64, half: f64, angle: f64) {
    if angle == 0.0 {
        ctx.rect(cx - half, cy - half, half * 2.0, half * 2.0);
        return;
    }
    let (sin, cos) = angle.sin_cos();
    let corners = [(-half, -half), (half, -half), (half, half), (-half, half)];
    let mut first = true;
    for (dx, dy) in corners {
        let x = cx + dx * cos - dy * sin;
        let y = cy + dx * sin + dy * cos;
        if first {
            ctx.move_to(x, y);
            first = false;
        } else {
            ctx.line_to(x, y);
        }
    }
    ctx.close_path();
}

/// The frame, corner handles and rotation handle for a multi-element selection.
///
/// These used to be absent entirely: a multi-selection got a bare rectangle with no
/// handles, so dragging its corner fell through to the hit test and started a marquee.
/// A group could only ever be moved, never scaled or turned.
fn paint_group_selection(ctx: &CanvasRenderingContext2d, view: &PaintView) {
    let Some(bounds) = crate::scene_bounds(view.selected.iter().copied()) else {
        return;
    };

    let pad = view.handle_layout.frame_pad;
    let tl = crate::world_to_screen(view.camera, bounds.min_x - pad, bounds.min_y - pad);
    let br = crate::world_to_screen(view.camera, bounds.max_x + pad, bounds.max_y + pad);
    ctx.stroke_rect(tl.x, tl.y, br.x - tl.x, br.y - tl.y);

    // The handles sit further out than the frame, exactly as they do on a single shape.
    let off = view.handle_layout.handle_offset;
    let htl = crate::world_to_screen(view.camera, bounds.min_x - off, bounds.min_y - off);
    let hbr = crate::world_to_screen(view.camera, bounds.max_x + off, bounds.max_y + off);

    let half = view.handle_px / 2.0;
    // `rotate_gap` is in world units; the overlay paints in screen space.
    let rotate_y = htl.y - view.handle_layout.rotate_gap * view.camera.scale;

    // Corners, matching what a single shape gets, plus the rotation circle above.
    for (x, y, is_rotate) in [
        (htl.x, htl.y, false),
        (hbr.x, htl.y, false),
        (hbr.x, hbr.y, false),
        (htl.x, hbr.y, false),
        ((htl.x + hbr.x) / 2.0, rotate_y, true),
    ] {
        ctx.begin_path();
        if is_rotate {
            let _ = ctx.arc(x, y, half, 0.0, std::f64::consts::PI * 2.0);
        } else {
            ctx.rect(x - half, y - half, view.handle_px, view.handle_px);
        }
        set_fill(ctx, &view.theme.background);
        ctx.fill();
        ctx.stroke();
    }
}

/// A circle on each point of a line or arrow, filled at the midpoints.
///
/// Hollow for a real point, filled for a midpoint, which is how Excalidraw
/// distinguishes "move this" from "add one here".
fn paint_linear_handles(ctx: &CanvasRenderingContext2d, view: &PaintView) {
    for handle in &view.linear_handles {
        let s = crate::world_to_screen(view.camera, handle.x, handle.y);
        let is_midpoint = matches!(handle.handle, crate::selection::LinearHandle::Midpoint(_));

        ctx.begin_path();
        let _ = ctx.arc(s.x, s.y, POINT_HANDLE_R, 0.0, std::f64::consts::PI * 2.0);

        if is_midpoint {
            set_fill(ctx, &view.theme.accent);
            ctx.save();
            ctx.set_global_alpha(0.55);
            ctx.fill();
            ctx.restore();
        } else {
            set_fill(ctx, &view.theme.background);
            ctx.fill();
        }
        ctx.stroke();
    }
}

/// Outlines the shape a dragged arrow endpoint would attach to.
///
/// Transcribed from Excalidraw's `renderBindingHighlightForBindableElement`, after
/// sampling the real thing: the highlight is a **thin, fully opaque stroke that traces
/// the element's own outline**, not a glow and not a box. Reading their source gives
/// `lineWidth = clamp(1.75, strokeWidth, 4)` and `rgba(BINDING_HIGHLIGHT_RGB, 1)`;
/// sampling excalidraw.com's interactive canvas gives rgb(106, 189, 252), which is
/// exactly their light-theme constant.
///
/// It reuses the same segment builders the shape itself is drawn from, so the highlight
/// follows a rounded rectangle's actual corners rather than a sharp box around it —
/// they cannot drift apart.
fn paint_binding_highlight(ctx: &CanvasRenderingContext2d, view: &PaintView) {
    let Some(element) = view.binding_highlight else {
        return;
    };

    let rect =
        crate::scene::geometry::normalize_rect(element.x, element.y, element.width, element.height);
    let scale = view.camera.scale;

    ctx.save();
    set_stroke(ctx, view.theme.binding_highlight.as_str());
    // Excalidraw: clamp(1.75, strokeWidth, 4), held constant in screen pixels.
    ctx.set_line_width(element.stroke_width.clamp(1.75, 4.0));
    let _ = ctx.set_line_dash(&EMPTY_DASH.with(Clone::clone));

    // The overlay paints in screen space, so the element transform is applied here
    // rather than to the context in world units.
    let to_screen = |x: f64, y: f64| {
        let (x, y) = if element.angle == 0.0 {
            (x, y)
        } else {
            let cx = rect.x + rect.width / 2.0;
            let cy = rect.y + rect.height / 2.0;
            let (sin, cos) = element.angle.sin_cos();
            let dx = x - cx;
            let dy = y - cy;
            (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
        };
        crate::world_to_screen(view.camera, x, y)
    };

    ctx.begin_path();
    match element.kind {
        DrawElementType::Ellipse => {
            // Traced as a polyline so the element's rotation can be applied per point;
            // ctx.ellipse cannot express a rotation about a different centre.
            let steps = 64;
            for i in 0..=steps {
                let t = (i as f64 / steps as f64) * std::f64::consts::PI * 2.0;
                let p = to_screen(
                    rect.x + rect.width / 2.0 + (rect.width / 2.0) * t.cos(),
                    rect.y + rect.height / 2.0 + (rect.height / 2.0) * t.sin(),
                );
                if i == 0 {
                    ctx.move_to(p.x, p.y);
                } else {
                    ctx.line_to(p.x, p.y);
                }
            }
        }
        DrawElementType::Diamond => {
            let pts = crate::render::shape::diamond_points(rect.width, rect.height);
            let first = to_screen(rect.x + pts[0][0], rect.y + pts[0][1]);
            ctx.move_to(first.x, first.y);
            for p in &pts[1..] {
                let s = to_screen(rect.x + p[0], rect.y + p[1]);
                ctx.line_to(s.x, s.y);
            }
            ctx.close_path();
        }
        _ => {
            if element.roundness.is_some() {
                // The same path the shape itself is generated from, so the highlight
                // hugs the real rounded corners.
                let r = crate::render::shape::corner_radius(rect.width.min(rect.height), element);
                replay_segments(
                    ctx,
                    &crate::render::shape::rounded_rect_segments(rect.width, rect.height, r),
                    rect.x,
                    rect.y,
                    &to_screen,
                );
                ctx.close_path();
            } else {
                let tl = to_screen(rect.x, rect.y);
                let tr = to_screen(rect.x + rect.width, rect.y);
                let br = to_screen(rect.x + rect.width, rect.y + rect.height);
                let bl = to_screen(rect.x, rect.y + rect.height);
                ctx.move_to(tl.x, tl.y);
                ctx.line_to(tr.x, tr.y);
                ctx.line_to(br.x, br.y);
                ctx.line_to(bl.x, bl.y);
                ctx.close_path();
            }
        }
    }

    ctx.stroke();
    ctx.restore();
    let _ = scale;
}

/// Replays path segments into the context, mapped through `to_screen`.
///
/// Cubics are flattened rather than passed to `bezier_curve_to`, because each control
/// point has to go through the same world-to-screen mapping and a rotation cannot be
/// applied to a bezier by transforming its endpoints alone.
fn replay_segments(
    ctx: &CanvasRenderingContext2d,
    segments: &[draw_rough::renderer::Segment],
    ox: f64,
    oy: f64,
    to_screen: &dyn Fn(f64, f64) -> crate::camera::Point,
) {
    use draw_rough::renderer::Segment;
    let mut cur = [0.0f64, 0.0];

    for seg in segments {
        match *seg {
            Segment::MoveTo(p) => {
                let s = to_screen(ox + p[0], oy + p[1]);
                ctx.move_to(s.x, s.y);
                cur = p;
            }
            Segment::LineTo(p) => {
                let s = to_screen(ox + p[0], oy + p[1]);
                ctx.line_to(s.x, s.y);
                cur = p;
            }
            Segment::CurveTo([x1, y1, x2, y2, x, y]) => {
                const STEPS: usize = 12;
                for i in 1..=STEPS {
                    let t = i as f64 / STEPS as f64;
                    let mt = 1.0 - t;
                    let a = mt * mt * mt;
                    let b = 3.0 * mt * mt * t;
                    let c = 3.0 * mt * t * t;
                    let d = t * t * t;
                    let px = a * cur[0] + b * x1 + c * x2 + d * x;
                    let py = a * cur[1] + b * y1 + c * y2 + d * y;
                    let s = to_screen(ox + px, oy + py);
                    ctx.line_to(s.x, s.y);
                }
                cur = [x, y];
            }
            Segment::Close => ctx.close_path(),
        }
    }
}
