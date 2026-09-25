//! The sticky note: a filled pad with a shadow, the date it was made, and a label that
//! shrinks to fit before the note grows.
//!
//! A port of Excalidraw's `packages/element/src/stickyNote.ts@1118751f` — everything in it
//! that needs no text measurer: the constants (`packages/common/src/constants.ts@1118751f:
//! 221-264`, `colors.ts@1118751f:267-281`), the colour rules, the painted outline with its
//! jitter and lifted corner, the footer's date, the smallest a note may be, and what a
//! resize asks the layout for. The layout itself — the font fit and the growth — lives
//! with every other text layout, in `text/layout.rs` (`sticky_layout`).
//!
//! Painting needs no measurer either: the footer's text is chosen by width bucket, never
//! measured, so the canvas, the SVG and a server would all draw the same one.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::camera::Point;
use crate::scene::element::{DrawElement, DrawElementType};
use crate::scene::geometry::is_transparent;

pub const STICKY_NOTE_PADDING: f64 = 16.0;
/// `STICKY_NOTE_FOOTER.height`: the date's row under the label body.
pub const STICKY_NOTE_FOOTER_HEIGHT: f64 = 20.0;
pub const STICKY_NOTE_FOOTER_FONT_SIZE: f64 = 12.0;
pub const STICKY_NOTE_FOOTER_FONT_FAMILY: &str = "Helvetica, Arial, sans-serif";
/// The date's baseline, above the note's bottom edge.
pub const STICKY_NOTE_FOOTER_BASELINE_FROM_BOTTOM: f64 = 14.0;
pub const STICKY_NOTE_FOOTER_OPACITY: f64 = 1.0;
/// The body width below which the date drops its year: the worst case, "30 May 2026", at
/// 12px in the system sans stack, with margin.
pub const STICKY_NOTE_FOOTER_MIN_BODY_WIDTH_FOR_YEAR: f64 = 80.0;
/// Outer height to label body height: both paddings and the footer.
pub const STICKY_NOTE_BODY_INSET_Y: f64 = STICKY_NOTE_PADDING * 2.0 + STICKY_NOTE_FOOTER_HEIGHT;
pub const DEFAULT_STICKY_NOTE_SIZE: f64 = 250.0;
/// The floor for a note's width and base height; the UI's floor is font-aware on top of
/// it ([`sticky_min_size`]).
pub const STICKY_NOTE_MIN_SIZE: f64 = 75.0;
pub const STICKY_NOTE_SHADOW_OFFSET: f64 = 3.0;
pub const STICKY_NOTE_SHADOW_OPACITY: f64 = 0.16;
pub const STICKY_NOTE_EDGE_SHADOW_WIDTH: f64 = 0.5;
pub const STICKY_NOTE_EDGE_SHADOW_OPACITY: f64 = 0.08;
pub const STICKY_NOTE_MIN_FONT_SIZE: f64 = 16.0;
pub const STICKY_NOTE_MAX_FONT_SIZE: f64 = 512.0;
pub const STICKY_NOTE_FALLBACK_FONT_SIZE: f64 = 28.0;
pub const STICKY_NOTE_FONT_STEP: f64 = 2.0;
pub const DEFAULT_STICKY_NOTE_BG: &str = "#ffdf6b";
/// `DEFAULT_ELEMENT_PROPS.strokeColor`: a note's ink when it has none.
pub const DEFAULT_STICKY_NOTE_STROKE: &str = "#1e1e1e";
/// The quick picks for a note's background, in order; the first is the default. Never
/// transparent.
pub const STICKY_NOTE_BACKGROUND_PICKS: [&str; 5] = [
    DEFAULT_STICKY_NOTE_BG,
    "#fcc2d7",
    "#b2f2bb",
    "#a5d8ff",
    "#ffd8a8",
];

/// `STICKY_NOTE_RENDER_ROUGHNESS`: how far a corner may wander, per roughness.
const RENDER_ROUGHNESS: [f64; 3] = [0.0, 1.5, 8.0];
const CORNER_RADIUS_RATIO: f64 = 0.04;
const MAX_CORNER_RADIUS: f64 = 16.0;

pub fn is_sticky_note(element: &DrawElement) -> bool {
    element.kind == DrawElementType::StickyNote
}

/// A note's ink: never transparent (`normalizeStickyNoteStrokeColor`).
pub fn normalize_sticky_stroke(color: &str) -> String {
    if color.is_empty() || is_transparent(color) {
        DEFAULT_STICKY_NOTE_STROKE.to_owned()
    } else {
        color.to_owned()
    }
}

/// A note's paper: never transparent (`normalizeStickyNoteBackgroundColor`).
pub fn normalize_sticky_background(color: &str) -> String {
    if color.is_empty() || is_transparent(color) {
        DEFAULT_STICKY_NOTE_BG.to_owned()
    } else {
        color.to_owned()
    }
}

/// A label's ceiling, clamped (`normalizeStickyNoteFontSize`): the fit steps down from it,
/// and from a ceiling past ~2^56 a step of 2 would change nothing and never end.
pub fn normalize_sticky_font_size(font_size: f64) -> f64 {
    if !font_size.is_finite() {
        return STICKY_NOTE_FALLBACK_FONT_SIZE;
    }
    font_size.clamp(crate::selection::MIN_FONT_SIZE, STICKY_NOTE_MAX_FONT_SIZE)
}

/// The size a text was given (`getBaseFontSize`): a note's label's ceiling, any other
/// text's own size. Asks the container, because a label unbound from its note — or bound
/// to a shape — can carry a stale `base_font_size` that means nothing there.
pub fn label_ceiling(label: &DrawElement, container: Option<&DrawElement>) -> f64 {
    let size = crate::text::layout::font_size_of(label);
    if container.is_some_and(is_sticky_note) {
        label.base_font_size.unwrap_or(size)
    } else {
        size
    }
}

/// `getStickyNoteCornerRadius`: a small rounding, proportional to the short side and
/// capped, where every other shape's is adaptive.
pub fn sticky_corner_radius(element: &DrawElement) -> f64 {
    if element.roundness.is_none() {
        return 0.0;
    }
    (element.width.abs().min(element.height.abs()) * CORNER_RADIUS_RATIO).min(MAX_CORNER_RADIUS)
}

/// `seededRandom` (`packages/common/src/random.ts@1118751f:24-32`), mulberry32: the same
/// sequence for the same seed on every client and every frame, in `[0, 1)`. JavaScript's
/// bitwise operators wrap to 32 bits, which is what `wrapping_*` does here.
pub fn seeded_random(seed: u32) -> impl FnMut() -> f64 {
    let mut value = seed;
    move || {
        value = value.wrapping_add(0x6d2b_79f5);
        let mut next = value;
        next = (next ^ (next >> 15)).wrapping_mul(next | 1);
        next ^= next.wrapping_add((next ^ (next >> 7)).wrapping_mul(next | 61));
        f64::from(next ^ (next >> 14)) / 4_294_967_296.0
    }
}

/// One step of a note's outline, in the note's own space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StickyPathCommand {
    Move(Point),
    Line(Point),
    Quadratic { control: Point, point: Point },
}

/// `getStickyNoteRenderPoints`: the four corners, each nudged by the note's seed as far as
/// its roughness allows — never more than a sliver of its short side.
pub fn sticky_render_points(element: &DrawElement, offset: f64, seed_offset: u32) -> [Point; 4] {
    let (w, h) = (element.width.abs(), element.height.abs());
    let roughness = crate::text::layout::js_round(element.roughness).clamp(0.0, 2.0) as usize;
    let amount = RENDER_ROUGHNESS[roughness].min(w.min(h) * 0.012);
    let at = |x: f64, y: f64| Point {
        x: offset + x,
        y: offset + y,
    };
    if amount == 0.0 {
        return [at(0.0, 0.0), at(w, 0.0), at(w, h), at(0.0, h)];
    }
    let mut random = seeded_random(element.seed.wrapping_add(seed_offset));
    let mut jitter = || (random() * 2.0 - 1.0) * amount;
    let mut corner = |x: f64, y: f64| {
        let dx = jitter();
        let dy = jitter();
        at(x + dx, y + dy)
    };
    [
        corner(0.0, 0.0),
        corner(w, 0.0),
        corner(w, h),
        corner(0.0, h),
    ]
}

fn point_at_distance(from: Point, to: Point, distance: f64) -> Point {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let length = dx.hypot(dy);
    if length == 0.0 {
        return from;
    }
    let ratio = (distance / length).min(1.0);
    Point {
        x: from.x + dx * ratio,
        y: from.y + dy * ratio,
    }
}

/// `getStickyNotePathCommands`: the paper's outline — or, with `shadow`, its shadow's, 3
/// down and right with its own jitter — rounded, and with one corner lifted off the board
/// at the roughest setting. The lifted corner comes from the note's seed, so the paper and
/// its shadow agree, and it stays put through redraws and resizes.
pub fn sticky_path_commands(element: &DrawElement, shadow: bool) -> Vec<StickyPathCommand> {
    let points = if shadow {
        sticky_render_points(element, STICKY_NOTE_SHADOW_OFFSET, 1)
    } else {
        sticky_render_points(element, 0.0, 0)
    };
    let radius = sticky_corner_radius(element);
    // Compared exactly, as the oracle does: only a roughness of 2 lifts a corner.
    #[allow(clippy::float_cmp)]
    let lifted = (element.roughness == 2.0)
        .then(|| (seeded_random(element.seed)() * points.len() as f64).floor() as usize);
    if radius == 0.0 && lifted.is_none() {
        let mut commands = vec![StickyPathCommand::Move(points[0])];
        commands.extend(points[1..].iter().map(|p| StickyPathCommand::Line(*p)));
        return commands;
    }
    let (w, h) = (element.width.abs(), element.height.abs());
    let count = points.len();
    let corners: Vec<Vec<StickyPathCommand>> = (0..count)
        .map(|index| {
            let point = points[index];
            let prev = points[(index + count - 1) % count];
            let next = points[(index + 1) % count];
            let corner_radius = radius
                .min((point.x - prev.x).hypot(point.y - prev.y) / 2.0)
                .min((point.x - next.x).hypot(point.y - next.y) / 2.0);
            if lifted == Some(index) {
                let size = w.min(h);
                let reach = (size * 0.18).min(40.0);
                // The shadow follows the paper in at half the bend, keeping its offset
                // down and right, away from the light at the top left.
                let lift = (size * 0.02).min(5.0) * if shadow { 0.5 } else { 1.0 };
                let tip = Point {
                    x: point.x
                        + if index == 1 || index == 2 {
                            -lift
                        } else {
                            lift
                        },
                    y: point.y + if index >= 2 { -lift } else { lift },
                };
                let start = point_at_distance(point, prev, reach);
                let end = point_at_distance(point, next, reach);
                return vec![
                    StickyPathCommand::Line(start),
                    StickyPathCommand::Quadratic {
                        control: point_at_distance(point, prev, reach / 2.0),
                        point: point_at_distance(tip, start, corner_radius),
                    },
                    StickyPathCommand::Quadratic {
                        control: tip,
                        point: point_at_distance(tip, end, corner_radius),
                    },
                    StickyPathCommand::Quadratic {
                        control: point_at_distance(point, next, reach / 2.0),
                        point: end,
                    },
                ];
            }
            vec![
                StickyPathCommand::Line(point_at_distance(point, prev, corner_radius)),
                StickyPathCommand::Quadratic {
                    control: point,
                    point: point_at_distance(point, next, corner_radius),
                },
            ]
        })
        .collect();
    let start = match corners[0].last() {
        Some(StickyPathCommand::Quadratic { point, .. } | StickyPathCommand::Line(point)) => *point,
        _ => points[0],
    };
    let mut commands = vec![StickyPathCommand::Move(start)];
    for index in 1..=count {
        commands.extend(corners[index % count].iter().copied());
    }
    commands
}

/// A note's outline geometry as a cache key for the painted `Path2D`: every field
/// [`sticky_path_commands`] reads, plus `shadow` (the paper and its shadow trace
/// different points — a different offset and half the jitter), and nothing else.
///
/// A separate hash from [`crate::render::cache::shape_fingerprint`], not a reuse of it:
/// that one is keyed on rough.js's own field set (stroke width, stroke style, fill
/// style, corner radius, background transparency), none of which `sticky_path_commands`
/// reads, and reusing it would either miss a field that matters here or watch one that
/// does not — either is wrong, per that module's own note on what a left-out field costs.
pub fn sticky_fingerprint(element: &DrawElement, shadow: bool) -> u64 {
    // FNV-1a, as `ShapeKey::of` uses — this crate's usual way to turn "same geometry" into
    // one comparable, hashable number.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for byte in bytes {
            h ^= u64::from(*byte);
            h = h.wrapping_mul(0x1000_0000_01b3);
        }
    };
    eat(&[u8::from(shadow)]);
    eat(&element.width.abs().to_bits().to_le_bytes());
    eat(&element.height.abs().to_bits().to_le_bytes());
    eat(&element.roughness.to_bits().to_le_bytes());
    eat(&element.seed.to_le_bytes());
    // Only whether a note is rounded, not by how much: `sticky_corner_radius` ignores the
    // stored value entirely and derives the radius from width and height, already in the
    // key above.
    eat(&[u8::from(element.roundness.is_some())]);
    h
}

/// The outline as an SVG path's `d`, closed — as the oracle's SVG export writes it
/// (`staticSvgScene.ts@1118751f:159-171`).
pub fn sticky_path_data(commands: &[StickyPathCommand]) -> String {
    let mut parts: Vec<String> = commands
        .iter()
        .map(|command| match command {
            StickyPathCommand::Move(p) => format!("M {} {}", p.x, p.y),
            StickyPathCommand::Line(p) => format!("L {} {}", p.x, p.y),
            StickyPathCommand::Quadratic { control, point } => {
                format!("Q {} {} {} {}", control.x, control.y, point.x, point.y)
            }
        })
        .collect();
    parts.push("Z".to_owned());
    parts.join(" ")
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The largest time a JavaScript `Date` holds; past it the date is invalid.
const MAX_DATE_MS: f64 = 8.64e15;

/// The calendar day of `ms`, as `(year, month0, day)` — in the viewer's time zone in the
/// browser, as the oracle's `Date` reads it, and in UTC elsewhere (tests, a server).
fn calendar_day(ms: f64) -> (i64, usize, u32) {
    #[cfg(target_arch = "wasm32")]
    {
        let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms));
        (
            i64::from(date.get_full_year()),
            date.get_month() as usize,
            date.get_date(),
        )
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Howard Hinnant's `civil_from_days`.
        let days = (ms / 86_400_000.0).floor() as i64;
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month0 = if mp < 10 { mp + 2 } else { mp - 10 } as usize;
        let year = yoe + era * 400 + i64::from(month0 < 2);
        (year, month0, day)
    }
}

thread_local! {
    /// [`calendar_day`] results already computed, by the exact millisecond.
    ///
    /// A note's `created` does not change except when it is edited, and every note
    /// painted in the same frame is compared against the same "now"
    /// (`wasm::paint::frame_now`), so after the first lookup of a given millisecond every
    /// later one this session is a plain `HashMap` read rather than a `js_sys::Date`
    /// allocation and a WASM↔JS crossing — on wasm32, where `calendar_day` is the only
    /// thing here that costs more than arithmetic; off it this only saves the lookup.
    /// Cleared rather than swept past a cap: simpler than tracking use per frame for a
    /// cache this small, and no worse than the next note paying for itself again.
    static CALENDAR_DAYS: RefCell<HashMap<u64, (i64, usize, u32)>> = RefCell::new(HashMap::new());
    static CALENDAR_DAY_COMPUTATIONS: Cell<u64> = const { Cell::new(0) };
}

/// However many distinct milliseconds a session plausibly asks about at once — notes made
/// in one sitting, plus "now". Past this a re-ask is cheaper than remembering every one.
const CALENDAR_DAY_CACHE_CAP: usize = 4096;

/// [`calendar_day`], memoized by its exact input — see `CALENDAR_DAYS`.
fn calendar_day_cached(ms: f64) -> (i64, usize, u32) {
    let key = ms.to_bits();
    CALENDAR_DAYS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(day) = cache.get(&key) {
            return *day;
        }
        if cache.len() >= CALENDAR_DAY_CACHE_CAP {
            cache.clear();
        }
        let day = calendar_day(ms);
        CALENDAR_DAY_COMPUTATIONS.with(|count| count.set(count.get() + 1));
        cache.insert(key, day);
        day
    })
}

/// How many times [`calendar_day_cached`] has actually called [`calendar_day`] rather than
/// serving a cached result.
///
/// Test-only visibility into the cache: the thing it saves — a `js_sys::Date` allocation
/// and a WASM↔JS crossing — does not exist off the browser to count directly, so this is
/// how a native test proves the cache is doing its job.
pub fn calendar_day_computations() -> u64 {
    CALENDAR_DAY_COMPUTATIONS.with(Cell::get)
}

/// The wall clock, in epoch ms: what a note's `created` is stamped with. Not the engine's
/// frame clock, which is the page's `performance.now()`.
pub fn wall_clock_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Whole milliseconds, as `Date.now()` gives: a fraction does not survive every
        // JSON writer's shortest form, and a date has no use for it.
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |elapsed| elapsed.as_millis() as f64)
    }
}

/// `getStickyNoteDateLabel`: "7 Sep", or "31 Dec 2025" when the year is not `now`'s and
/// `short` does not ask for the day alone. `None` when the date is unknown or invalid.
/// Fixed English, as the oracle's is.
pub fn sticky_date_label(created: Option<f64>, short: bool, now: f64) -> Option<String> {
    let created = created.filter(|ms| ms.is_finite() && ms.abs() <= MAX_DATE_MS)?;
    let (year, month0, day) = calendar_day_cached(created);
    let label = format!("{day} {}", MONTHS[month0]);
    if short || year == calendar_day_cached(now).0 {
        Some(label)
    } else {
        Some(format!("{label} {year}"))
    }
}

/// What the footer paints and where, in the note's own space.
#[derive(Clone, Debug, PartialEq)]
pub struct StickyFooter {
    pub text: String,
    pub x: f64,
    pub y: f64,
}

/// `getStickyNoteFooter`: the date, right-aligned in the bottom padding — `None` for a
/// note under the floor (the creation draft), where it would run into the top padding,
/// and for an unknown date. The form is chosen by the body's width, never measured.
pub fn sticky_footer(element: &DrawElement, now: f64) -> Option<StickyFooter> {
    let (w, h) = (element.width.abs(), element.height.abs());
    if w < STICKY_NOTE_MIN_SIZE || h < STICKY_NOTE_MIN_SIZE {
        return None;
    }
    let short = w - STICKY_NOTE_PADDING * 2.0 < STICKY_NOTE_FOOTER_MIN_BODY_WIDTH_FOR_YEAR;
    Some(StickyFooter {
        text: sticky_date_label(element.created, short, now)?,
        x: w - STICKY_NOTE_PADDING,
        y: h - STICKY_NOTE_FOOTER_BASELINE_FROM_BOTTOM,
    })
}

/// `getStickyNoteMinSize`: the smallest note a gesture may make — one line at the label's
/// ceiling with the padding, and the footer below it — never under the floor. Without the
/// font term a new note would grow on its first keystroke.
pub fn sticky_min_size(font_size: f64, line_height: f64) -> (f64, f64) {
    let line = (normalize_sticky_font_size(font_size) * line_height).ceil();
    (
        STICKY_NOTE_MIN_SIZE.max(line + STICKY_NOTE_PADDING * 2.0),
        STICKY_NOTE_MIN_SIZE.max(line + STICKY_NOTE_BODY_INSET_Y),
    )
}

/// The edge a note keeps when its height changes (`VerticalResizeAnchor`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StickyAnchor {
    #[default]
    Top,
    Bottom,
    Center,
}

/// `getPositionAfterHeightChange` (`sizeHelpers.ts@1118751f:28-54`): where a box turned by
/// its angle goes when it becomes `next_height` tall and keeps `anchor` where it was.
pub fn position_after_height_change(
    element: &DrawElement,
    next_height: f64,
    anchor: StickyAnchor,
) -> Point {
    let delta = (element.height - next_height) / 2.0;
    let (sin, cos) = element.angle.sin_cos();
    match anchor {
        // Turned about its centre, so holding the centre does not depend on the angle.
        StickyAnchor::Center => Point {
            x: element.x,
            y: element.y + delta,
        },
        StickyAnchor::Bottom => Point {
            x: element.x - delta * sin,
            y: element.y + delta * (1.0 + cos),
        },
        StickyAnchor::Top => Point {
            x: element.x + delta * sin,
            y: element.y + delta * (1.0 - cos),
        },
    }
}

/// A note as a resize found it: what every move of the gesture is measured from, so
/// letting go of Shift gives back the base height and the ceiling it began with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StickyOrigin {
    pub width: f64,
    pub base_height: f64,
    /// The label's ceiling, if it has a label.
    pub ceiling: Option<f64>,
}

impl StickyOrigin {
    pub fn of(note: &DrawElement, label: Option<&DrawElement>) -> Self {
        Self {
            width: note.width.abs(),
            base_height: note.base_height.unwrap_or(note.height.abs()),
            ceiling: label.map(|label| label_ceiling(label, Some(note))),
        }
    }
}

/// What a layout is asked to hold on to — each absolute, never a multiple of the live
/// value, which would compound across the moves of a drag.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StickyLayoutOpts {
    /// What to lay out, before wrapping; the label's own when `None`.
    pub original_text: Option<String>,
    /// The base height to keep or take; the note's own when `None`.
    pub base_height: Option<f64>,
    /// The label's ceiling to keep or take; its own when `None`.
    pub base_font_size: Option<f64>,
    pub anchor: StickyAnchor,
}

/// The handle a resize holds, as far as a note cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StickyHandle {
    pub north: bool,
    pub south: bool,
    /// A side handle — east or west — rather than a corner or the top or bottom.
    pub east_or_west_side: bool,
}

/// `getStickyNoteResizeIntent`: what a resize of `note` — its requested box already
/// applied — asks the layout to keep.
///
/// - a width-only gesture (a free east or west side) keeps the base height;
/// - a gesture that changes the height takes the height asked for as the new base;
/// - a proportional one scales the label's ceiling with the note too;
/// - a flip keeps everything;
/// - the content correction holds the edge the gesture holds still.
pub fn sticky_resize_intent(
    note: &DrawElement,
    origin: &StickyOrigin,
    handle: StickyHandle,
    proportional: bool,
    from_center: bool,
    flip: bool,
) -> StickyLayoutOpts {
    if flip {
        return StickyLayoutOpts {
            base_height: Some(origin.base_height),
            base_font_size: origin.ceiling,
            ..StickyLayoutOpts::default()
        };
    }
    let changes_height = proportional || handle.north || handle.south;
    let scale = if proportional && origin.width != 0.0 {
        note.width.abs() / origin.width
    } else {
        1.0
    };
    StickyLayoutOpts {
        original_text: None,
        base_height: Some(if changes_height {
            note.height.abs()
        } else {
            origin.base_height
        }),
        base_font_size: origin.ceiling.map(|ceiling| ceiling * scale),
        anchor: if from_center {
            StickyAnchor::Center
        } else if handle.north {
            StickyAnchor::Bottom
        } else if proportional && handle.east_or_west_side {
            // Shift on a side holds the opposite side's midpoint: the vertical centre.
            StickyAnchor::Center
        } else {
            StickyAnchor::Top
        },
    }
}

/// `syncStickyNoteInk`: a note's ink is one colour — its own stroke, which its footer is
/// painted in, and its label's. Given the two as they were and as they are, the colour
/// both should take, or `None` when they agree. The side that changed wins; when both did,
/// or neither (data that drifted), the label does: it is the text the user styled. A
/// transparent label always takes the note's.
pub fn synced_ink(
    note_before: Option<&str>,
    note: &str,
    label_before: Option<&str>,
    label: &str,
) -> Option<String> {
    if note == label {
        return None;
    }
    let note_changed = note_before != Some(note);
    let label_changed = label_before != Some(label);
    if is_transparent(label) || (note_changed && !label_changed) {
        Some(normalize_sticky_stroke(note))
    } else {
        Some(normalize_sticky_stroke(label))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_random_matches_mulberry32() {
        // The first draws of `seededRandom(1)` and `seededRandom(0)` in the oracle.
        let mut one = seeded_random(1);
        assert!((one() - 0.627_073_940_588_161_3).abs() < 1e-15);
        let mut zero = seeded_random(0);
        assert!((zero() - 0.266_429_208_684_712_65).abs() < 1e-15);
    }

    #[test]
    fn the_wall_clock_is_whole_milliseconds() {
        // A fraction did not survive a JSON round trip: `a_note_round_trips` failed on
        // `created` by one ulp, whenever the clock read one that did not.
        assert_eq!(wall_clock_ms().fract(), 0.0);
    }

    #[test]
    fn calendar_day_reads_utc_off_the_browser() {
        // 2026-09-08T12:00:00Z
        assert_eq!(calendar_day(1_788_868_800_000.0), (2026, 8, 8));
        // 1969-12-31T12:00:00Z
        assert_eq!(calendar_day(-43_200_000.0), (1969, 11, 31));
    }

    /// `calendar_day_cached` is what stands between a sticky note's footer and a
    /// `js_sys::Date` allocation per note per paint (`wasm/paint.rs` › `paint_sticky`).
    /// Off the browser that allocation does not exist to count, so this counts the one
    /// thing that stands in for it: how many times the real, uncached `calendar_day` ran.
    #[test]
    fn a_repeated_millisecond_computes_once() {
        // A cache shared across every test on this thread: start from wherever it is.
        let before = calendar_day_computations();

        let first = calendar_day_cached(1_800_000_000_123.0);
        assert_eq!(
            calendar_day_computations(),
            before + 1,
            "a millisecond never seen before must compute"
        );

        // The same note painted again — a pan, a zoom, an unrelated element redrawing —
        // must not compute a second time.
        let second = calendar_day_cached(1_800_000_000_123.0);
        assert_eq!(second, first);
        assert_eq!(
            calendar_day_computations(),
            before + 1,
            "the same millisecond again must be a lookup, not a second allocation"
        );

        // A note with a different `created`, or "now" on a later day, is a different key
        // and does compute.
        calendar_day_cached(1_800_000_001_123.0);
        assert_eq!(calendar_day_computations(), before + 2);
    }
}
