use std::collections::HashMap;

use draw_rough::ops::{Op, OpSetKind};
use draw_rough::Drawable;
use wasm_bindgen::{JsCast, JsValue};
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

    /// Decoded images, by the id of the element that shows them, each with the length
    /// of the `data:` URL it was decoded from.
    ///
    /// Decoding happens once and is reused for every frame afterwards. Without this the
    /// painter would hand the canvas a fresh `HTMLImageElement` sixty times a second and
    /// the browser would decode the same megabyte over and over.
    ///
    /// By id, not by the URL itself: that hashed the whole URL — megabytes — for every
    /// image in every frame that painted it, which is every frame while anything near it
    /// moves. An element's picture does not change once it has one; the length is there
    /// to notice if it ever did.
    static IMAGES: std::cell::RefCell<HashMap<String, (usize, web_sys::HtmlImageElement)>> =
        std::cell::RefCell::new(HashMap::new());
}

/// The decoded picture of an image element, if it is ready yet.
///
/// Decoding is asynchronous even for a `data:` URL, so the first frame after an image is
/// inserted usually has nothing to draw. Rather than leave a hole until something else
/// happens to repaint, loading throws away the cached static layer and asks for another
/// frame.
///
/// Both halves are needed. Asking for a frame alone stopped working when the static
/// layer started being reused between frames: that layer is keyed by the scene revision,
/// which a decode does not change, so the "repaint" was served from the layer painted
/// *before* the image existed and the placeholder stayed on screen until something
/// unrelated edited the scene. Loading a board full of images showed only empty boxes.
fn decoded_image(element: &DrawElement) -> Option<web_sys::HtmlImageElement> {
    IMAGES.with(|cache| {
        let mut cache = cache.borrow_mut();
        let cached = cache.get(&element.id);
        let Some(data_url) = element.data_url.as_deref() else {
            // A peer's gesture, sent without the picture it cannot change: the one
            // already decoded for this element.
            return cached.and_then(|(_, image)| image.complete().then(|| image.clone()));
        };
        if let Some((len, image)) = cached {
            if *len == data_url.len() {
                return image.complete().then(|| image.clone());
            }
        }
        let Ok(image) = web_sys::HtmlImageElement::new() else {
            return None;
        };
        let on_load = wasm_bindgen::closure::Closure::once_into_js(move || {
            invalidate_layer();
            super::request_repaint();
        });
        image.set_onload(Some(on_load.unchecked_ref()));
        image.set_src(data_url);
        let ready = image.complete();
        cache.insert(element.id.clone(), (data_url.len(), image.clone()));
        ready.then_some(image)
    })
}

/// How many decoded images to keep before dropping the ones that are off screen.
const MAX_CACHED_IMAGES: usize = 32;

/// Forget decoded images once there are too many of them.
///
/// Deliberately not "forget everything not on screen". The painter is handed the
/// **culled** element list, so an image scrolled just out of view would be evicted and
/// re-decoded the moment it came back — which is visible, as a blank where the picture
/// should be. Waiting until there are enough of them to matter trades a little memory
/// for a board that does not flicker while you pan.
fn evict_images(live: &[&DrawElement]) {
    IMAGES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() <= MAX_CACHED_IMAGES {
            return;
        }
        let visible: std::collections::HashSet<&str> =
            live.iter().map(|element| element.id.as_str()).collect();
        cache.retain(|id, _| visible.contains(id.as_str()));
    });
}

/// Draws an image element, or a placeholder while it is still decoding.
///
/// The placeholder is the element's own box, faint: it shows that something is arriving
/// and where it will land, so the board does not appear to have swallowed the file.
///
/// **Drawn in element-local space**, at `(0, 0, |w|, |h|)`, like every other element.
/// `with_element_transform` has already moved the origin to the element and applied its
/// mirror and rotation. This used to draw at the element's *world* position inside that
/// transform, so the translation was applied twice: the picture appeared at double the
/// element's coordinates, away from the element itself. Clicking the picture hit
/// nothing and the real, grabbable image sat in empty-looking space. That was the whole
/// of "an image cannot be dragged". Excalidraw draws it the same local way
/// (`renderElement.ts:606-616`).
fn paint_image(ctx: &CanvasRenderingContext2d, view: [f64; 6], element: &DrawElement) {
    let (w, h) = (element.width.abs(), element.height.abs());
    let decoded = decoded_image(element);

    with_element_transform(ctx, view, element, || match decoded {
        Some(image) => {
            let _ = ctx.draw_image_with_html_image_element_and_dw_and_dh(&image, 0.0, 0.0, w, h);
        }
        None => {
            ctx.save();
            set_stroke(ctx, "#bbbbbb");
            ctx.set_line_width(1.0);
            ctx.begin_path();
            ctx.rect(0.0, 0.0, w, h);
            ctx.stroke();
            ctx.restore();
        }
    });
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
    /// Keyed by the shape fingerprint, not by element id, so every copy of a duplicated
    /// shape shares one `Path2D`. A board made by holding Ctrl+D is hundreds of elements
    /// with identical geometry, and one path each was both the build cost and the memory.
    static PATHS: std::cell::RefCell<HashMap<u64, CachedPaths>> =
        std::cell::RefCell::new(HashMap::new());
    /// Fingerprints drawn since the last eviction.
    static PATHS_USED: std::cell::RefCell<std::collections::HashSet<u64>> =
        std::cell::RefCell::new(std::collections::HashSet::new());

    /// One `Path2D` per freehand stroke per detail level, keyed by its geometry.
    ///
    /// See `paint_freedraw`: before this, every stroke's outline was computed and
    /// replayed call by call on every frame.
    static FREEHAND: std::cell::RefCell<HashMap<u64, Option<Path2d>>> =
        std::cell::RefCell::new(HashMap::new());
    static FREEHAND_USED: std::cell::RefCell<std::collections::HashSet<u64>> =
        std::cell::RefCell::new(std::collections::HashSet::new());

    /// The detail level of the frame being painted. See `PaintView::detail_scale`.
    static DETAIL_LEVEL: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
}

/// The paths built for one piece of geometry.
///
/// The fingerprint covers geometry only, which is what makes a translate free: panning,
/// zooming and dragging leave it untouched and the cached paths stay valid.
type CachedPaths = Vec<(OpSetKind, Path2d)>;

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

    PATHS_USED.with(|used| used.borrow_mut().insert(fingerprint));

    PATHS.with(|cache| {
        let mut cache = cache.borrow_mut();

        if let std::collections::hash_map::Entry::Vacant(slot) = cache.entry(fingerprint) {
            count_path_lookup(false);
            let built = SHAPES.with(|shapes| {
                shapes
                    .borrow_mut()
                    .get(element)
                    .map(build_paths)
                    .unwrap_or_default()
            });
            slot.insert(built);
        } else {
            count_path_lookup(true);
        }

        let Some(paths) = cache.get(&fingerprint) else {
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

/// Drops cached geometry and paths the frame that just ran did not draw.
///
/// Without this both caches leak, and a long editing session churns through a lot of
/// shapes. Run once per frame, after painting.
///
/// Swept by *use* rather than by element id: a shape now belongs to no single element —
/// that is the point of keying on geometry — so the only thing that can be asked about it
/// is whether anything still draws it.
fn evict_paths(live: &[&DrawElement]) {
    // Both caches keep a budget's worth before evicting anything. Sweeping eagerly costs
    // far more than it reclaims: during a pan the visible set changes every frame, so
    // dropping what the last frame missed regenerates each shape as it crosses the edge.
    let budget = live.len().saturating_mul(2).max(64);
    SHAPES.with(|shapes| shapes.borrow_mut().sweep(budget));
    PATHS.with(|cache| {
        let mut cache = cache.borrow_mut();
        let used = PATHS_USED.with(|used| std::mem::take(&mut *used.borrow_mut()));
        if cache.len() <= budget {
            return;
        }
        cache.retain(|fingerprint, _| used.contains(fingerprint));
    });
    FREEHAND.with(|cache| {
        let mut cache = cache.borrow_mut();
        let used = FREEHAND_USED.with(|used| std::mem::take(&mut *used.borrow_mut()));
        if cache.len() <= budget {
            return;
        }
        cache.retain(|fingerprint, _| used.contains(fingerprint));
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
    let fade = ERASE_FADE.with(std::cell::Cell::get);
    set_alpha_cached(ctx, (element.opacity / 100.0).clamp(0.0, 1.0) * fade);

    let m = mul(view, element_matrix(element));
    let _ = ctx.set_transform(m[0], m[1], m[2], m[3], m[4], m[5]);

    body();
}

/// An element's own transform — mirror, position, rotation — from its local space to the
/// world.
fn element_matrix(element: &DrawElement) -> [f64; 6] {
    // Shapes only: a line or arrow carries its mirror in its points, so the sign of its
    // width means nothing and applying it would reverse the element a second time.
    let (sx, sy) = crate::scene::geometry::mirror_signs(element);

    // Element-local: mirror, then translate to the origin, and rotate about the centre
    // if turned.
    if element.angle == 0.0 {
        [sx, 0.0, 0.0, sy, element.x, element.y]
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
        [
            sx * cos,
            sx * sin,
            -sy * sin,
            sy * cos,
            centre.x - sx * cos * lcx + sy * sin * lcy,
            centre.y - sx * sin * lcx - sy * cos * lcy,
        ]
    }
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

    /// The factor the element being painted is faded by: 1, or
    /// [`READY_TO_ERASE_OPACITY`] while the eraser has it marked.
    ///
    /// Set per element by the scene loop and read where every element sets its alpha, so
    /// no painting function has to take the eraser as a parameter.
    static ERASE_FADE: std::cell::Cell<f64> = const { std::cell::Cell::new(1.0) };
}

/// How visible an element marked by the eraser stays: Excalidraw's
/// `ELEMENT_READY_TO_ERASE_OPACITY` (20), multiplied into the element's own.
const READY_TO_ERASE_OPACITY: f64 = 0.2;

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

/// The offscreen layer the static scene is kept in.
///
/// One canvas, because the strip path that needed a second one to scroll through is not
/// taken — see the note on `LayerPlan::Scroll` in `paint`. When it is, this gains a back
/// buffer: a scroll copies the layer onto itself offset, and a self-blit with overlap is
/// a copy whose result depends on the order pixels happen to be read in.
struct Layers {
    front: web_sys::HtmlCanvasElement,
    front_ctx: CanvasRenderingContext2d,
    /// The frame this layer actually holds, or `None` when nothing has been painted into
    /// it yet.
    ///
    /// An `Option` rather than a bare key, because an empty canvas that reports a key is
    /// indistinguishable to `plan_layer` from one that has the frame: it compares the key
    /// to itself, answers `Reuse`, nothing is painted, and the empty layer is blitted over
    /// the board. Everything vanishes and stays vanished, because the frame loop then has
    /// no reason to run again.
    ///
    /// That is not hypothetical — it is what a fresh layer used to be stamped with, and a
    /// layer is rebuilt whenever the device size changes, which is twice per pan on any
    /// screen above dpr 1. Making the empty state representable is what stops it coming
    /// back: there is no longer a value to write here that claims content the canvas does
    /// not have.
    painted: Option<(crate::render::scroll::LayerKey, crate::camera::Camera)>,
    device: (u32, u32),
    /// Whether the frame on the target canvas has any chrome drawn over the layer.
    ///
    /// When it has not, and nothing has changed, the canvas already shows the right
    /// picture and the frame can be skipped entirely.
    overlay_drawn: bool,
    /// While a gesture runs: what sits above the live elements, cached like `front`,
    /// which then holds what sits below them. Made the first time it is needed.
    above: Option<(web_sys::HtmlCanvasElement, CanvasRenderingContext2d)>,
    above_painted: Option<(crate::render::scroll::LayerKey, crate::camera::Camera)>,
    /// What `front` holds in terms of the scene, for bringing it up to date by painting
    /// on top of it — with the chrome digest it was painted under. `None` when it holds
    /// something that cannot be: what is below a live element with more above it.
    base: Option<(crate::render::append::PictureBase, u64)>,
}

thread_local! {
    static LAYERS: std::cell::RefCell<Option<Layers>> = const { std::cell::RefCell::new(None) };
    /// How each frame was served, since the counters were last read: redraws, scrolls,
    /// reuses. Exposed through `paintStatsJson` because the difference between "the
    /// scroll path is working" and "it is quietly never taken" is invisible from the
    /// outside and is exactly the thing a benchmark result hangs on.
    static PLAN_COUNTS: std::cell::Cell<(u32, u32, u32)> = const { std::cell::Cell::new((0, 0, 0)) };
    /// Path2D lookups: hits, then misses.
    ///
    /// **This is the cache that decides per-frame work**, not `SHAPES`. A `Path2D` is
    /// built once per piece of geometry and stroked with a single canvas call
    /// thereafter, so a hit here is the difference between one call and thousands.
    static PATH_COUNTS: std::cell::Cell<(u64, u64)> = const { std::cell::Cell::new((0, 0)) };
}

fn count_path_lookup(hit: bool) {
    PATH_COUNTS.with(|c| {
        let (hits, misses) = c.get();
        if hit {
            c.set((hits + 1, misses));
        } else {
            c.set((hits, misses + 1));
        }
    });
}

/// Path2D cache hits and misses, cumulative.
///
/// Read this to answer "is geometry being rebuilt". `shape_cache_stats` cannot answer it:
/// `SHAPES` sits *behind* this cache and is consulted only when this one misses, so its
/// hit count is normally zero however well everything is working — which reads exactly
/// backwards. The two are reported side by side and documented so nobody has to
/// rediscover the layering from a confusing number.
pub fn path_cache_stats() -> (u64, u64) {
    PATH_COUNTS.with(|c| c.get())
}

/// How many Path2D objects are being kept alive.
pub fn path_cache_len() -> usize {
    PATHS.with(|cache| cache.borrow().len())
}

/// Forgets the static layer, so the next frame redraws the scene instead of reusing it.
///
/// For changes to the picture that the layer's key cannot see. The key is the scene
/// revision plus theme and grid, and an image finishing its decode changes none of
/// them — so without this the frame after a decode was served from a layer painted
/// before the image existed.
fn invalidate_layer() {
    LAYERS.with(|cell| {
        if let Some(layers) = cell.borrow_mut().as_mut() {
            layers.painted = None;
            layers.base = None;
        }
    });
}

/// A picture of the whole scene as it is.
fn whole_picture(view: &PaintView) -> crate::render::append::PictureBase {
    crate::render::append::PictureBase {
        revision: view.scene_revision,
        left_out: Vec::new(),
    }
}

/// Brings `front` up to the scene as it is by painting what is new on top of it, when
/// that is all that changed — see [`crate::render::append`]. Returns whether it did.
///
/// Only for a picture of the same view: same camera, same size and resolution, same
/// chrome. And never with frames on the board: their names are painted over everything
/// in a whole picture and are missing from the one a gesture leaves behind.
fn paint_on_top(
    layers: &mut Layers,
    view: &PaintView,
    key: crate::render::scroll::LayerKey,
    digest: u64,
) -> bool {
    let Some((painted_key, painted_camera)) = layers.painted else {
        return false;
    };
    let Some((base, base_digest)) = &layers.base else {
        return false;
    };
    let same_view = painted_camera == view.camera
        && painted_key.scale == key.scale
        && painted_key.dpr == key.dpr
        && painted_key.width == key.width
        && painted_key.height == key.height
        && *base_digest == digest
        && view.frame_names.is_empty();
    if !same_view || (base.revision == view.scene_revision && base.left_out.is_empty()) {
        return false;
    }
    let Some(top) = crate::render::append::plan_append(
        base,
        view.scene.changes_since(base.revision),
        &view.elements,
    ) else {
        return false;
    };
    STATE.with(|s| s.borrow_mut().reset());
    FONT.with(|f| *f.borrow_mut() = None);
    layers.front_ctx.save();
    layers.front_ctx.set_global_alpha(1.0);
    paint_elements(&layers.front_ctx, view, &top);
    layers.front_ctx.restore();
    layers.painted = Some((key, view.camera));
    layers.base = Some((whole_picture(view), digest));
    true
}

/// Redraws, scrolls and reuses since the last call, then resets.
pub fn take_plan_counts() -> (u32, u32, u32) {
    PLAN_COUNTS.with(|c| c.replace((0, 0, 0)))
}

/// The same counts, without resetting them.
///
/// **The top-line render number.** There are three caches stacked here, and this is the
/// outermost: on a `Reuse` frame the static layer's bitmap is kept and no element is
/// replayed at all, so neither the path cache nor the rough cache is even consulted.
/// A high `reuses` against `frames` is the renderer working; it also means the two
/// numbers below it will look frozen, which is correct rather than broken.
pub fn plan_counts() -> (u32, u32, u32) {
    PLAN_COUNTS.with(|c| c.get())
}

/// Cumulative rough-geometry cache hits and misses.
///
/// **Not the headline number.** `SHAPES` sits behind the `Path2D` cache and is consulted
/// only when *that* one misses, so its hit count stays at zero however well the renderer
/// is doing, and its miss count settles at the number of distinct pieces of geometry ever
/// drawn. Reading "0 hits" here as "the cache is broken" is exactly backwards — it means
/// the cache in front of it is never letting anything through.
///
/// Use [`path_cache_stats`] to ask whether geometry is being rebuilt per frame.
///
/// Read, not taken: a running total, so two reads either side of a gesture give that
/// gesture's cost without disturbing anyone else's measurement.
pub fn shape_cache_stats() -> (u64, u64) {
    SHAPES.with(|shapes| shapes.borrow().stats())
}

/// How many pieces of geometry are being kept alive.
pub fn shape_cache_len() -> usize {
    SHAPES.with(|shapes| shapes.borrow().len())
}

fn count_plan(redraw: u32, scroll: u32, reuse: u32) {
    PLAN_COUNTS.with(|c| {
        let (r, s, u) = c.get();
        c.set((r + redraw, s + scroll, u + reuse));
    });
}

/// A canvas of exactly `w` x `h` device pixels, with a 2D context.
fn make_layer(w: u32, h: u32) -> Option<(web_sys::HtmlCanvasElement, CanvasRenderingContext2d)> {
    let document = web_sys::window()?.document()?;
    let canvas: web_sys::HtmlCanvasElement =
        document.create_element("canvas").ok()?.dyn_into().ok()?;
    canvas.set_width(w);
    canvas.set_height(h);
    let ctx = canvas
        .get_context("2d")
        .ok()??
        .dyn_into::<CanvasRenderingContext2d>()
        .ok()?;
    Some((canvas, ctx))
}

/// Everything painted into the layer that is not an element.
fn chrome_digest(view: &PaintView) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
    };
    eat(view.theme.background.as_bytes());
    eat(view.theme.grid.as_bytes());
    eat(&[u8::from(view.grid.enabled)]);
    eat(&view.grid.size.to_bits().to_le_bytes());
    eat(&(view.grid.step as u64).to_le_bytes());
    // The eraser's marks are painted into the layer, faded, so the layer is only good
    // for the marks it was painted with.
    eat(&view.erasing_revision.to_le_bytes());
    hash
}

/// The box an element can paint into, in device pixels.
fn device_bounds(view: &PaintView, element: &DrawElement) -> crate::scene::geometry::Rect {
    let world = crate::render::bounds::render_bounds(element);
    let top_left = crate::world_to_screen(view.camera, world.min_x, world.min_y);
    let bottom_right = crate::world_to_screen(view.camera, world.max_x, world.max_y);
    crate::scene::geometry::Rect {
        x: top_left.x * view.dpr,
        y: top_left.y * view.dpr,
        width: (bottom_right.x - top_left.x) * view.dpr,
        height: (bottom_right.y - top_left.y) * view.dpr,
    }
}

/// Marks a layer key with what it holds, so a picture of one part of the scene can never
/// be taken for a picture of another that happens to share the numbers.
const WHOLE_SCENE: u64 = 0x5ce3_e000;
const BELOW_LIVE: u64 = 0xb3_10e0;
const ABOVE_LIVE: u64 = 0xab_0e00;

/// Paints a frame while a gesture runs: cached pictures of everything the gesture does
/// not touch, and only the live elements drawn fresh.
///
/// Everything below the first live element in stacking order is one cached layer, and
/// everything above it another; the live elements are drawn between the two, so stacking
/// order holds. Both layers are keyed on the scene's `static_revision`, which stays put
/// while only live elements change — so a frame of a drag or a stroke copies two
/// pictures and draws what moved, where it used to repaint the whole board. Returns
/// whether both layers had to be drawn from scratch.
fn paint_live(
    ctx: &CanvasRenderingContext2d,
    layers: &mut Layers,
    view: &PaintView,
    key: crate::render::scroll::LayerKey,
    device: (u32, u32),
) -> bool {
    use crate::render::scroll::{plan_layer, LayerKey, LayerPlan};

    let split = view
        .elements
        .iter()
        .position(|element| view.live.contains(&element.id))
        .unwrap_or(view.elements.len());
    let below = &view.elements[..split];
    let (live, above): (Vec<&DrawElement>, Vec<&DrawElement>) = view.elements[split..]
        .iter()
        .partition(|element| view.live.contains(&element.id));

    let keyed = |part: u64| LayerKey {
        content: view.static_revision,
        chrome: key.chrome ^ part,
        ..key
    };

    let below_key = keyed(BELOW_LIVE);
    let mut drew_below = false;
    if !matches!(
        plan_layer(layers.painted, below_key, view.camera),
        LayerPlan::Reuse
    ) && !adopt_as_below(layers, view, &live, key, below_key)
    {
        paint_layer(&layers.front_ctx, view, below, true);
        layers.painted = Some((below_key, view.camera));
        // Everything but the live elements, when they are the top of the stack: then the
        // whole scene is this picture with them painted over it, which is how the frame
        // after the gesture gets it.
        layers.base = above.is_empty().then(|| {
            (
                crate::render::append::PictureBase {
                    revision: view.scene_revision,
                    left_out: live.iter().map(|element| element.id.clone()).collect(),
                },
                chrome_digest(view),
            )
        });
        drew_below = true;
    }

    let mut drew_above = above.is_empty();
    if !above.is_empty() {
        if layers.above.is_none() {
            layers.above = make_layer(device.0, device.1);
            layers.above_painted = None;
        }
        if let Some((_, above_ctx)) = &layers.above {
            let above_key = keyed(ABOVE_LIVE);
            if !matches!(
                plan_layer(layers.above_painted, above_key, view.camera),
                LayerPlan::Reuse
            ) {
                paint_layer(above_ctx, view, &above, false);
                layers.above_painted = Some((above_key, view.camera));
                drew_above = true;
            }
        }
    }
    // A redraw is a layer actually painted. `drew_above` also stands for "there is no
    // above", which is not one: counted as one, every frame of every gesture on the top
    // of the stack reported a redraw of the board that never happened.
    let redrew = drew_below || (drew_above && !above.is_empty());
    count_plan(u32::from(redrew), 0, u32::from(!redrew));

    // Composed on the target: what is below, what is live, what is above.
    STATE.with(|s| s.borrow_mut().reset());
    FONT.with(|f| *f.borrow_mut() = None);
    ctx.set_global_alpha(1.0);
    let _ = ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    ctx.clear_rect(0.0, 0.0, f64::from(device.0), f64::from(device.1));
    let _ = ctx.draw_image_with_html_canvas_element(&layers.front, 0.0, 0.0);
    paint_elements(ctx, view, &live);
    if !above.is_empty() {
        if let Some((above_canvas, _)) = &layers.above {
            STATE.with(|s| s.borrow_mut().reset());
            ctx.set_global_alpha(1.0);
            let _ = ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
            let _ = ctx.draw_image_with_html_canvas_element(above_canvas, 0.0, 0.0);
        }
    }
    let _ = ctx.set_transform(view.dpr, 0.0, 0.0, view.dpr, 0.0, 0.0);
    paint_frame_names(ctx, view);
    // The chrome is always redrawn over a live frame; nothing here is ever "already on
    // the canvas".
    layers.overlay_drawn = true;
    drew_below && drew_above
}

/// Takes the picture in `front` as the layer below a gesture when it already is that —
/// see [`crate::render::append::holds_all_but`]. Returns whether it did.
fn adopt_as_below(
    layers: &mut Layers,
    view: &PaintView,
    live: &[&DrawElement],
    key: crate::render::scroll::LayerKey,
    below_key: crate::render::scroll::LayerKey,
) -> bool {
    let Some((painted_key, painted_camera)) = layers.painted else {
        return false;
    };
    let Some((base, digest)) = &layers.base else {
        return false;
    };
    let same_view = painted_camera == view.camera
        && painted_key.scale == key.scale
        && painted_key.dpr == key.dpr
        && painted_key.width == key.width
        && painted_key.height == key.height
        && *digest == chrome_digest(view)
        // A whole picture has the frame names over everything; the layer below a gesture
        // has none.
        && view.frame_names.is_empty();
    if !same_view
        || !crate::render::append::holds_all_but(
            base,
            view.scene.changes_since(base.revision),
            &view.elements,
            live,
        )
    {
        return false;
    }
    let mut left_out = base.left_out.clone();
    left_out.extend(live.iter().map(|element| element.id.clone()));
    layers.base = Some((
        crate::render::append::PictureBase {
            revision: base.revision,
            left_out,
        },
        *digest,
    ));
    layers.painted = Some((below_key, view.camera));
    true
}

/// Draws a cached layer from scratch: the paper and the grid when it is the bottom one,
/// then `elements`.
fn paint_layer(
    ctx: &CanvasRenderingContext2d,
    view: &PaintView,
    elements: &[&DrawElement],
    backdrop: bool,
) {
    STATE.with(|s| s.borrow_mut().reset());
    FONT.with(|f| *f.borrow_mut() = None);
    ctx.save();
    ctx.set_global_alpha(1.0);
    let _ = ctx.set_transform(view.dpr, 0.0, 0.0, view.dpr, 0.0, 0.0);
    ctx.clear_rect(0.0, 0.0, view.width, view.height);
    if backdrop {
        set_fill(ctx, &view.theme.background);
        ctx.fill_rect(0.0, 0.0, view.width, view.height);
        paint_grid(ctx, view);
    }
    paint_elements(ctx, view, elements);
    ctx.restore();
}

/// Draws `elements` in order, each with its own transform, frame clip and eraser fade.
///
/// The context's transform is left as the last element set it.
fn paint_elements(ctx: &CanvasRenderingContext2d, view: &PaintView, elements: &[&DrawElement]) {
    let dpr = view.dpr;
    DETAIL_LEVEL.with(|level| {
        level.set(crate::render::path_data::lod_level(view.detail_scale));
    });
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

    for element in elements {
        // A child that pokes out of its frame is cut off at the frame's edge — that
        // is what makes a frame read as a window onto a region rather than as a
        // rectangle drawn behind things. The engine decides which children need it.
        let clip = view.frame_clips.get(&element.id);
        if let Some(bounds) = clip {
            ctx.save();
            // Set under the plain device transform, because the previous element
            // left its own matrix on the context. A clip region is fixed in device
            // space once applied, so the element is free to set its own transform
            // afterwards without escaping it.
            let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
            let tl = crate::world_to_screen(view.camera, bounds.min_x, bounds.min_y);
            let br = crate::world_to_screen(view.camera, bounds.max_x, bounds.max_y);
            ctx.begin_path();
            ctx.rect(tl.x, tl.y, br.x - tl.x, br.y - tl.y);
            ctx.clip();
        }
        // Faded while the eraser has it marked — or has marked the frame it is in, which
        // takes it too — as Excalidraw's `resolveElementRenderState` does.
        let marked = view.erasing.contains(&element.id)
            || element
                .frame_id
                .as_ref()
                .is_some_and(|frame| view.erasing.contains(frame));
        ERASE_FADE.with(|fade| fade.set(if marked { READY_TO_ERASE_OPACITY } else { 1.0 }));
        paint_element(
            ctx,
            view_transform,
            element,
            &view.theme.background,
            &view.elements,
        );
        if clip.is_some() {
            ctx.restore();
            // `restore` put the context's alpha, stroke, fill, dash and font back to what
            // they were before the clip, and the caches still hold what was set inside
            // it. Left alone, the next element with the same values skipped setting them
            // and drew with the restored ones — a marked element at full strength, or an
            // unmarked one faded.
            STATE.with(|s| s.borrow_mut().reset());
            FONT.with(|f| *f.borrow_mut() = None);
        }
    }
    ERASE_FADE.with(|fade| fade.set(1.0));
}

/// Draws the static scene — background, grid, elements, frame names — into `ctx`.
///
/// `only` limits it to the strips that have just come into view — elements outside them
/// are skipped rather than clipped, because a clipped draw still builds every path before
/// the rasteriser discards it. Only the frames in motion pass strips, over a moved
/// picture that the frame after the motion replaces: see the note on
/// `LayerPlan::Scroll` in `paint` for why the persistent layer is never patched this way.
fn paint_static(
    ctx: &CanvasRenderingContext2d,
    view: &PaintView,
    only: Option<&[crate::scene::geometry::Rect]>,
) {
    let dpr = view.dpr;
    // A fresh context has none of the state the cache believes it set.
    STATE.with(|s| s.borrow_mut().reset());
    FONT.with(|f| *f.borrow_mut() = None);

    ctx.save();
    if let Some(rects) = only {
        let _ = ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        // Cleared before the clip is set, not after. A clipped clear antialiases against
        // the clip edge, so the boundary pixel ends up a blend of the shifted old content
        // and the new — a seam one pixel wide, which then travels with the picture on
        // every later scroll. Clearing the exact integer rectangle first means those
        // pixels are fully replaced rather than blended into.
        for rect in rects {
            ctx.clear_rect(rect.x, rect.y, rect.width, rect.height);
        }
        ctx.begin_path();
        for rect in rects {
            ctx.rect(rect.x, rect.y, rect.width, rect.height);
        }
        ctx.clip();
    }

    let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
    ctx.clear_rect(0.0, 0.0, view.width, view.height);
    set_fill(ctx, &view.theme.background);
    ctx.fill_rect(0.0, 0.0, view.width, view.height);
    paint_grid(ctx, view);

    let elements: Vec<&DrawElement> = match only {
        Some(rects) => view
            .elements
            .iter()
            .filter(|element| {
                crate::render::scroll::intersects_any(device_bounds(view, element), rects)
            })
            .copied()
            .collect(),
        None => view.elements.clone(),
    };
    paint_elements(ctx, view, &elements);
    paint_frame_names(ctx, view);
    ctx.restore();
}

impl Painter for CanvasPainter<'_> {
    fn paint(&mut self, view: &PaintView) {
        use crate::render::scroll::{plan_layer, LayerKey, LayerPlan};

        let ctx = self.ctx;
        let dpr = view.dpr;
        let device = (
            (view.width * dpr).round().max(1.0) as u32,
            (view.height * dpr).round().max(1.0) as u32,
        );
        let key = LayerKey {
            scale: view.camera.scale,
            dpr,
            width: view.width,
            height: view.height,
            // The whole scene's revision, not a digest of the visible elements: the
            // culled set changes as the camera moves, so a digest of it invalidated the
            // layer on every pan — which is precisely the frame the layer exists for.
            // Measured before the change: 120 redraws and zero scrolls across a pan.
            content: view.scene_revision,
            chrome: chrome_digest(view) ^ WHOLE_SCENE,
        };

        let drew_everything = LAYERS.with(|cell| {
            let mut slot = cell.borrow_mut();
            // A layer of the wrong size is no layer at all.
            if slot.as_ref().is_none_or(|l| l.device != device) {
                let (front, front_ctx) = make_layer(device.0, device.1)?;
                *slot = Some(Layers {
                    front,
                    front_ctx,
                    // Nothing has been drawn into it, so it holds no frame. Saying so is
                    // what makes the plan below a `Redraw` instead of a `Reuse` of an
                    // empty canvas.
                    painted: None,
                    device,
                    overlay_drawn: true,
                    above: None,
                    above_painted: None,
                    base: None,
                });
            }
            let layers = slot.as_mut().expect("just built");

            if !view.live.is_empty() {
                return Some(paint_live(ctx, layers, view, key, device));
            }

            // While the camera is moving, the last picture moved or scaled to where the
            // camera is now, instead of the whole board painted again — see
            // `plan_motion`. Only for the same picture: anything else about the frame
            // changing means painting it.
            if view.in_motion {
                if let Some((painted_key, painted_camera)) = layers.painted {
                    let same_picture = painted_key.content == key.content
                        && painted_key.chrome == key.chrome
                        && painted_key.dpr == key.dpr
                        && painted_key.width == key.width
                        && painted_key.height == key.height;
                    let blit = same_picture
                        .then(|| {
                            crate::render::scroll::plan_motion(
                                painted_camera,
                                view.camera,
                                dpr,
                                f64::from(device.0),
                                f64::from(device.1),
                            )
                        })
                        .flatten();
                    if let Some(blit) = blit {
                        count_plan(0, 1, 0);
                        STATE.with(|s| s.borrow_mut().reset());
                        FONT.with(|f| *f.borrow_mut() = None);
                        ctx.set_global_alpha(1.0);
                        let _ = ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
                        let (w, h) = (f64::from(device.0), f64::from(device.1));
                        // The paper first: a scaled picture's edge is a fraction of a
                        // pixel, and what shows through it must be the paper.
                        set_fill(ctx, &view.theme.background);
                        ctx.fill_rect(0.0, 0.0, w, h);
                        let _ =
                            ctx.set_transform(blit.scale, 0.0, 0.0, blit.scale, blit.dx, blit.dy);
                        let _ = ctx.draw_image_with_html_canvas_element(&layers.front, 0.0, 0.0);
                        // Then what the picture does not reach, painted for real.
                        let exposed = blit.exposed(w, h);
                        if !exposed.is_empty() {
                            paint_static(ctx, view, Some(&exposed));
                        }
                        layers.overlay_drawn = true;
                        return Some(false);
                    }
                }
            }

            let digest = chrome_digest(view);
            let appended = paint_on_top(layers, view, key, digest);
            let plan = plan_layer(layers.painted, key, view.camera);
            let bare = crate::render::scroll::overlay_is_empty(
                view.selected.len() + view.peer_marks.len(),
                view.marquee.is_some(),
                view.lasso.len(),
                view.laser.len() + view.peer_lasers.len(),
                view.snap_guides.len(),
                view.binding_highlight.is_some(),
                view.linear_handles.len(),
            );

            let full = match plan {
                LayerPlan::Reuse => {
                    count_plan(0, 0, 1);
                    // Nothing changed and nothing is drawn over it: the canvas already
                    // holds this exact frame. Copying it onto itself would be a
                    // full-canvas memcpy to produce the picture that is already there.
                    // Unless the layer was just painted on: then the canvas does not
                    // have what is new yet.
                    if bare && !layers.overlay_drawn && !appended {
                        return Some(false);
                    }
                    false
                }
                LayerPlan::Redraw => {
                    count_plan(1, 0, 0);
                    paint_static(&layers.front_ctx, view, None);
                    layers.base = Some((whole_picture(view), digest));
                    true
                }
                // Not taken yet, deliberately. Redrawing only the strip that has come
                // into view is the right idea and `plan_layer` works out the strips
                // correctly, but the painting of it does not: a clipped redraw leaves a
                // seam along the clip edge — about seventy pixels at a max channel
                // difference of 30 — and because each scroll builds on the last, the seam
                // travels with the picture and accumulates. `e2e/layer.spec.ts` catches
                // it by comparing a panned frame against a freshly drawn one, exactly.
                //
                // It is also worth less than it looks. A strip runs the full width or
                // height of the viewport, so on a board of large overlapping shapes it
                // crosses nearly all of them: measured on 300 screen-sized shapes, the
                // strip path came out at 1592ms against 1006ms for a plain redraw.
                //
                // So the frame is redrawn, and the saving that matters — skipping the
                // frames where nothing changed at all — is untouched by this.
                LayerPlan::Scroll { .. } => {
                    count_plan(1, 0, 0);
                    paint_static(&layers.front_ctx, view, None);
                    layers.base = Some((whole_picture(view), digest));
                    true
                }
            };
            // Recorded only here, after the arm above has actually painted into it.
            layers.painted = Some((key, view.camera));
            layers.overlay_drawn = !bare;

            // The layer, one device pixel to one device pixel.
            STATE.with(|s| s.borrow_mut().reset());
            FONT.with(|f| *f.borrow_mut() = None);
            ctx.set_global_alpha(1.0);
            let _ = ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
            ctx.clear_rect(0.0, 0.0, f64::from(device.0), f64::from(device.1));
            let _ = ctx.draw_image_with_html_canvas_element(&layers.front, 0.0, 0.0);
            Some(full)
        });

        // No layer — no document, no context, a browser that refused the canvas. Draw
        // straight to the target rather than showing nothing.
        let drew_everything = match drew_everything {
            Some(full) => full,
            None => {
                paint_static(ctx, view, None);
                true
            }
        };

        // Only after a full redraw. On a reuse nothing was asked of the caches, and on a
        // scroll only the strip was — so "what this frame used" is not the set to keep.
        if drew_everything {
            evict_paths(&view.elements);
            evict_images(&view.elements);
        }

        STATE.with(|s| s.borrow_mut().reset());
        FONT.with(|f| *f.borrow_mut() = None);
        let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
        paint_overlay(ctx, view);
        // Last, so the beam is over the selection chrome as well as the drawing. It is
        // pointing at the board, so nothing on the board should cover it.
        paint_laser(ctx, view);
    }
}

/// Frame names, written above each frame.
///
/// At a fixed size on screen rather than in world units, because a name is a label on the
/// board rather than something drawn on it — zooming out to see the whole layout is
/// exactly when you most need to read which frame is which.
fn paint_frame_names(ctx: &CanvasRenderingContext2d, view: &PaintView) {
    if view.frame_names.is_empty() {
        return;
    }
    ctx.save();
    let _ = ctx.set_transform(view.dpr, 0.0, 0.0, view.dpr, 0.0, 0.0);
    set_fill(ctx, &view.theme.frame_name);
    ctx.set_font(&crate::render::font_string(
        crate::scene::FRAME_NAME_FONT_SIZE,
    ));
    ctx.set_text_baseline("alphabetic");
    for (anchor, name, frame) in &view.frame_names {
        // Set for each name, not inherited: the last element painted leaves its own
        // alpha behind, which faded every name whenever the eraser had marked it. A
        // name fades with its own frame, as Excalidraw's `renderFrameNames` does.
        ctx.set_global_alpha(if view.erasing.contains(frame) {
            READY_TO_ERASE_OPACITY
        } else {
            1.0
        });
        let at = crate::world_to_screen(view.camera, anchor.x, anchor.y);
        let _ = ctx.fill_text(name, at.x, at.y);
    }
    ctx.restore();
    // The cached font no longer matches what the context holds.
    FONT.with(|f| *f.borrow_mut() = None);
}

/// Laser trails, filled.
///
/// Each outline arrives already shaped by the engine — a closed loop whose width varies
/// along its length — so there is nothing to compute here and nothing a second host could
/// get differently. Filled rather than stroked: a stroke has one width, and the whole
/// point of the trail is that it tapers.
fn paint_laser(ctx: &CanvasRenderingContext2d, view: &PaintView) {
    // The others' first, so a trail of one's own stays on top of theirs.
    for (color, outlines) in &view.peer_lasers {
        fill_outlines(ctx, view, color, outlines);
    }
    fill_outlines(ctx, view, &view.laser_color, &view.laser);
}

fn fill_outlines(
    ctx: &CanvasRenderingContext2d,
    view: &PaintView,
    color: &str,
    outlines: &[Vec<crate::interaction::LaserPoint>],
) {
    if outlines.is_empty() {
        return;
    }
    ctx.save();
    set_fill(ctx, color);
    for outline in outlines {
        let Some(first) = outline.first() else {
            continue;
        };
        let start = crate::world_to_screen(view.camera, first.x, first.y);
        ctx.begin_path();
        ctx.move_to(start.x, start.y);
        for point in &outline[1..] {
            let screen = crate::world_to_screen(view.camera, point.x, point.y);
            ctx.line_to(screen.x, screen.y);
        }
        ctx.close_path();
        ctx.fill();
    }
    ctx.restore();
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
    elements: &[&DrawElement],
) {
    match element.kind {
        DrawElementType::Line | DrawElementType::Arrow => paint_linear(ctx, view, element),
        DrawElementType::Freedraw => paint_freedraw(ctx, view, element),
        DrawElementType::Image => paint_image(ctx, view, element),
        DrawElementType::Text => {
            let is_linear_label = element
                .container_id
                .as_deref()
                .and_then(|cid| elements.iter().find(|e| e.id == cid))
                .is_some_and(|c| crate::is_linear_element(c));
            paint_text(
                ctx,
                view,
                element,
                if is_linear_label {
                    Some(background)
                } else {
                    None
                },
            )
        }
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
    let level = DETAIL_LEVEL.with(std::cell::Cell::get);
    let key = crate::render::path_data::freehand_fingerprint(points, element.stroke_width, level);
    FREEHAND_USED.with(|used| used.borrow_mut().insert(key));
    with_element_transform(ctx, view, element, || {
        // Filled, not stroked. The width varies along the stroke — faster means thinner,
        // the way a real nib behaves — and `lineWidth` is one number for a whole path,
        // so the only way to draw a varying width is to fill the region between the two
        // sides of it.
        set_fill(ctx, &element.stroke_color);
        FREEHAND.with(|cache| {
            let mut cache = cache.borrow_mut();
            let path = cache.entry(key).or_insert_with(|| {
                count_path_lookup(false);
                freehand_path(points, element.stroke_width, level)
            });
            if let Some(path) = path {
                ctx.fill_with_path_2d(path);
            }
        });
    });
}

/// The `Path2D` of a freehand stroke at one detail level, or `None` for a stroke with no
/// outline to fill.
///
/// Built once and kept: it depends on the samples and the width, not on where the stroke
/// is or how it is turned — those are the context's transform — so panning, zooming
/// within a level and dragging all reuse it. It used to be recomputed and replayed call
/// by call on every frame, for every stroke on the board.
fn freehand_path(points: &[[f64; 2]], stroke_width: f64, level: i32) -> Option<Path2d> {
    let outline = crate::freehand::stroke_outline(points, stroke_width, crate::freehand::THINNING);
    if outline.len() < 3 {
        return None;
    }
    let simplified = crate::render::path_data::simplify(
        &outline,
        crate::render::path_data::lod_tolerance(level),
    );
    // A stroke so small it simplifies away entirely still has to show up as something.
    let shape = if simplified.len() >= 3 {
        simplified
    } else {
        outline
    };
    Path2d::new_with_path_string(&crate::render::path_data::smooth_closed_outline(&shape)).ok()
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
        // The two used to be a hard-coded pair chosen by `container_id`: centre for a
        // label, left for anything else. They are element properties now, and the anchor
        // and the canvas setting have to be derived from the same value — `fill_text`
        // places the line's anchor at x, and which end that is depends on `textAlign`.
        let align = crate::scene::resolved_text_align(element);
        ctx.set_text_align(crate::render::canvas_text_align(align));
        let anchor_x = crate::render::text_anchor_x(align, element.width);
        for (i, line) in text.split('\n').enumerate() {
            let _ = ctx.fill_text(line, anchor_x, i as f64 * line_height);
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

/// Fill for the point being moved right now.
///
/// Excalidraw's `POINT_HANDLE_SELECTED_FILL` — `rgba(134, 131, 226, 0.9)`, read off
/// `interactiveScene.ts` at the pinned SHA. A resting joint is white and a moving one is
/// violet, which is the whole of what the colour is for: on a path whose points are a few
/// pixels apart, knowing which one is under the pointer is otherwise guesswork, and
/// letting go of the wrong one puts a bend somewhere nobody meant.
const POINT_HANDLE_ACTIVE_FILL: &str = "rgba(134, 131, 226, 0.9)";

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
    // Under this engine's own chrome: what someone else holds is context, what you hold
    // is what you are working on.
    paint_peer_marks(ctx, view);

    set_stroke(ctx, &view.theme.accent);
    ctx.set_line_width(1.0);
    // Solid, said rather than assumed. The dash is sticky canvas state and the element
    // pass leaves it wherever the last element wanted it, so selecting a dashed or dotted
    // shape drew its own selection frame in that shape's dash — a frame made of widely
    // spaced dots, which reads as nothing at all. Every piece of chrome below that wants
    // something other than solid sets it and puts it back.
    set_dash_cached(ctx, None);

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

    paint_lasso(ctx, view);
    paint_binding_highlight(ctx, view);

    // A linear element is edited by its points; it gets no frame.
    if !view.linear_handles.is_empty() {
        paint_linear_handles(ctx, view);
        return;
    }

    if view.selected.len() == 1 {
        paint_shape_selection(ctx, view, view.selected[0]);
        paint_radius_handles(ctx, view);
    } else if view.selected.len() > 1 {
        paint_group_selection(ctx, view);
    }
}

/// The free-form selection loop, while one is being drawn.
///
/// Drawn closed — the segment back to the start is shown dashed — because that is the
/// loop the engine will actually test against. Leaving it open would let someone aim a
/// gap that does not exist.
fn paint_lasso(ctx: &CanvasRenderingContext2d, view: &PaintView) {
    if view.lasso.len() < 2 {
        return;
    }
    ctx.save();
    set_stroke(ctx, &view.theme.accent);
    ctx.set_line_width(1.0);

    ctx.begin_path();
    let first = crate::world_to_screen(view.camera, view.lasso[0].x, view.lasso[0].y);
    ctx.move_to(first.x, first.y);
    for p in &view.lasso[1..] {
        let s = crate::world_to_screen(view.camera, p.x, p.y);
        ctx.line_to(s.x, s.y);
    }
    ctx.stroke();

    // The closing segment, dashed so it reads as implied rather than drawn.
    let last = view.lasso[view.lasso.len() - 1];
    let end = crate::world_to_screen(view.camera, last.x, last.y);
    let _ = ctx.set_line_dash(&dash_js(Some([4.0, 4.0])));
    ctx.begin_path();
    ctx.move_to(end.x, end.y);
    ctx.line_to(first.x, first.y);
    ctx.stroke();

    // Shade the enclosed area, as the marquee does, so what is caught is visible.
    let _ = ctx.set_line_dash(&dash_js(None));
    ctx.set_global_alpha(0.1);
    set_fill(ctx, &view.theme.accent);
    ctx.begin_path();
    ctx.move_to(first.x, first.y);
    for p in &view.lasso[1..] {
        let s = crate::world_to_screen(view.camera, p.x, p.y);
        ctx.line_to(s.x, s.y);
    }
    ctx.close_path();
    ctx.fill();
    ctx.restore();
}

/// The frame and its handles for a single shape.
///
/// The frame sits `frame_pad` outside the element, not on it, so the element's own
/// outline is still a move target — and every handle drawn here comes from the same
/// [`selection_handles`] call the pointer code hit-tests against, so the two cannot
/// drift. They did: the cardinal handles were hit-testable but never painted, which made
/// grabbing the middle of an edge resize a shape you were only trying to move.
/// The outline around one element, turned with it.
///
/// Pulled out of [`paint_shape_selection`] because a multi-selection needs exactly this
/// and nothing else: Excalidraw gives every selected element its own border and *then*
/// draws the group's box around all of them (`interactiveScene.ts:1922-1948` and
/// `:2027-2052`). Only the single-element case adds handles on top.
fn paint_element_outline(ctx: &CanvasRenderingContext2d, view: &PaintView, element: &DrawElement) {
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
}

fn paint_shape_selection(ctx: &CanvasRenderingContext2d, view: &PaintView, element: &DrawElement) {
    paint_element_outline(ctx, view, element);

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

/// Corner-radius handles: a small circle inside each corner of a selected rectangle.
///
/// Circles, where the resize handles are squares, so the two read as different tools at
/// a glance — one changes the size, the other the shape of the corner. Smaller than the
/// resize handles because they sit *inside* the shape, over the drawing. The one being
/// dragged takes the same focus fill as a moving line point.
fn paint_radius_handles(ctx: &CanvasRenderingContext2d, view: &PaintView) {
    if view.radius_handles.is_empty() {
        return;
    }
    set_dash_cached(ctx, None);
    for (corner, handle) in view.radius_handles.iter().enumerate() {
        let s = crate::world_to_screen(view.camera, handle.x, handle.y);
        ctx.begin_path();
        let _ = ctx.arc(s.x, s.y, RADIUS_HANDLE_R, 0.0, std::f64::consts::PI * 2.0);
        if view.active_radius_handle == Some(corner) {
            set_fill(ctx, POINT_HANDLE_ACTIVE_FILL);
        } else {
            set_fill(ctx, &view.theme.background);
        }
        ctx.fill();
        ctx.stroke();
    }
}

/// Radius of a corner-radius handle, in screen pixels.
const RADIUS_HANDLE_R: f64 = 4.0;

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

/// How wide a member's trace is, in CSS pixels.
const MEMBER_OUTLINE_PX: f64 = 1.5;

/// Traces each member of a multi-selection along its own shape, in the selection colour.
///
/// It used to be a padded box around each member — Excalidraw's look — which on a board of
/// neighbouring shapes was a lattice of rectangles over everything, the same for a circle
/// as for a scribble, hiding what it was meant to point at. The trace is the shape itself:
/// the edge of a rectangle, the curve of an ellipse or a line, the ink of a stroke. See
/// [`crate::render::outline`].
fn paint_member_outlines(ctx: &CanvasRenderingContext2d, view: &PaintView) {
    trace_outlines(ctx, view, &view.selected, MEMBER_OUTLINE_PX);
}

/// How wide the trace around what a peer holds is, in CSS pixels: wider than a member's,
/// because it has to read against whatever colour that peer was given.
const PEER_OUTLINE_PX: f64 = 2.0;

/// What other people hold, each traced in their colour with their name above it — the
/// way Figma shows who is working where. See `engine/peers.rs`.
fn paint_peer_marks(ctx: &CanvasRenderingContext2d, view: &PaintView) {
    if view.peer_marks.is_empty() {
        return;
    }
    set_dash_cached(ctx, None);
    for mark in &view.peer_marks {
        // Set outside the trace's save/restore, so the cache and the context agree after.
        set_stroke(ctx, mark.color);
        trace_outlines(ctx, view, &mark.elements, PEER_OUTLINE_PX);
    }
    for mark in &view.peer_marks {
        paint_name_tag(ctx, view, mark);
    }
    // The tags set the font behind the cache's back.
    FONT.with(|f| *f.borrow_mut() = None);
}

/// Height of a peer's name tag, and the size of the name in it, in CSS pixels.
const NAME_TAG_PX: f64 = 18.0;
const NAME_TAG_FONT: &str = "600 11px system-ui, -apple-system, 'Segoe UI', sans-serif";

/// A peer's name on a tag of their colour, sitting on the top-left corner of what they
/// hold. At a fixed size on screen, like a frame's name: it labels the board rather than
/// being drawn on it.
fn paint_name_tag(
    ctx: &CanvasRenderingContext2d,
    view: &PaintView,
    mark: &crate::engine::PeerMark,
) {
    let Some(bounds) = crate::scene_bounds(mark.elements.iter().copied()) else {
        return;
    };
    let pad = view.handle_layout.frame_pad;
    let corner = crate::world_to_screen(view.camera, bounds.min_x - pad, bounds.min_y - pad);
    ctx.set_font(NAME_TAG_FONT);
    let text_w = ctx
        .measure_text(mark.name)
        .map(|metrics| metrics.width())
        .unwrap_or(0.0);
    let (w, h) = (text_w + 12.0, NAME_TAG_PX);
    // Above the corner, or inside the top of the screen when that is off it.
    let x = corner.x.max(0.0);
    let y = (corner.y - h - 2.0).max(0.0);
    set_fill(ctx, mark.color);
    ctx.begin_path();
    let _ = ctx.round_rect_with_f64(x, y, w, h, 4.0);
    ctx.fill();
    set_fill(ctx, PEER_TAG_TEXT);
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text(mark.name, x + 6.0, y + h / 2.0);
}

/// The name on a peer's tag. White reads on every colour the host hands out.
const PEER_TAG_TEXT: &str = "#ffffff";

/// Traces `elements` along their own shapes in the current stroke colour, `width_px` CSS
/// pixels wide. See [`paint_member_outlines`].
fn trace_outlines(
    ctx: &CanvasRenderingContext2d,
    view: &PaintView,
    elements: &[&DrawElement],
    width_px: f64,
) {
    use crate::render::outline::{element_outline, Outline};
    use draw_rough::renderer::Segment;

    let dpr = view.dpr;
    let s = view.camera.scale;
    let view_transform = [
        dpr * s,
        0.0,
        0.0,
        dpr * s,
        dpr * view.camera.x,
        dpr * view.camera.y,
    ];
    let level = crate::render::path_data::lod_level(view.detail_scale);
    let selected: std::collections::HashSet<&str> =
        elements.iter().map(|e| e.id.as_str()).collect();

    ctx.save();
    // In the element's own units, where one CSS pixel is `1 / scale`.
    ctx.set_line_width(width_px / s.max(f64::MIN_POSITIVE));
    ctx.set_line_join("round");
    ctx.set_line_cap("round");
    for element in elements.iter().copied() {
        // A label is traced by its container, which is the shape someone sees.
        if element
            .container_id
            .as_deref()
            .is_some_and(|id| selected.contains(id))
        {
            continue;
        }
        let Some(outline) = element_outline(element) else {
            continue;
        };
        let m = mul(view_transform, element_matrix(element));
        let _ = ctx.set_transform(m[0], m[1], m[2], m[3], m[4], m[5]);
        match outline {
            Outline::Path(segments) => {
                ctx.begin_path();
                for segment in segments {
                    match segment {
                        Segment::MoveTo([x, y]) => ctx.move_to(x, y),
                        Segment::LineTo([x, y]) => ctx.line_to(x, y),
                        Segment::CurveTo([x1, y1, x2, y2, x, y]) => {
                            ctx.bezier_curve_to(x1, y1, x2, y2, x, y)
                        }
                        Segment::Close => ctx.close_path(),
                    }
                }
                ctx.stroke();
            }
            Outline::Ellipse { w, h } => {
                ctx.begin_path();
                let _ = ctx.ellipse(
                    w / 2.0,
                    h / 2.0,
                    w / 2.0,
                    h / 2.0,
                    0.0,
                    0.0,
                    std::f64::consts::PI * 2.0,
                );
                ctx.stroke();
            }
            Outline::Freehand => {
                // The same `Path2D` the ink is filled from, so the trace hugs the stroke
                // exactly and costs no new geometry.
                let points = element.points.as_deref().unwrap_or(&[]);
                let key = crate::render::path_data::freehand_fingerprint(
                    points,
                    element.stroke_width,
                    level,
                );
                FREEHAND_USED.with(|used| used.borrow_mut().insert(key));
                FREEHAND.with(|cache| {
                    let mut cache = cache.borrow_mut();
                    let path = cache
                        .entry(key)
                        .or_insert_with(|| freehand_path(points, element.stroke_width, level));
                    if let Some(path) = path {
                        ctx.stroke_with_path(path);
                    }
                });
            }
        }
    }
    ctx.restore();
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

    // Every member is traced first. Without it a multi-selection showed only the box
    // around the whole lot, so you could see *that* a region was held but not *which*
    // shapes in it were — and an unselected shape sitting inside those bounds was
    // indistinguishable from a selected one.
    paint_member_outlines(ctx, view);

    let pad = view.handle_layout.frame_pad;
    let tl = crate::world_to_screen(view.camera, bounds.min_x - pad, bounds.min_y - pad);
    let br = crate::world_to_screen(view.camera, bounds.max_x + pad, bounds.max_y + pad);
    // Dotted, so the group's box reads as chrome around the outlines rather than as a
    // sixth rectangle someone drew. Excalidraw dots this one and only this one
    // (`setLineDash([2 / zoom])`, `interactiveScene.ts:2037`) for the same reason.
    set_dash_cached(ctx, Some([2.0, 2.0]));
    ctx.stroke_rect(tl.x, tl.y, br.x - tl.x, br.y - tl.y);
    set_dash_cached(ctx, None);

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
        } else if view.active_handle == Some(handle.handle) {
            set_fill(ctx, POINT_HANDLE_ACTIVE_FILL);
            ctx.fill();
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
