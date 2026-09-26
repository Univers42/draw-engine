use serde::{Deserialize, Serialize};

use crate::scene::figure::{FigureKind, FigureParams};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DrawElementType {
    Rectangle,
    Diamond,
    Ellipse,
    Line,
    Arrow,
    Freedraw,
    Text,
    Image,
    Frame,
    Embed,
    /// A sticky note (Excalidraw's `stickynote`): a filled pad painted with its shadow and
    /// the date it was made, holding a label that shrinks to fit before the note grows.
    /// See `scene/sticky.rs`.
    #[serde(rename = "stickynote")]
    StickyNote,
    /// A parametric shape generated inside its box from a small parameter set — a
    /// polygon, a star, a parallelogram, a trapezoid, a cylinder or a document. Not a
    /// free path: its outline comes from exactly one place, `scene::figure::outline`, and
    /// its params live in [`DrawElement::figure`]. See `docs/reference/figure.md`.
    Figure,
}

impl DrawElementType {
    /// Whether this kind is a box to the geometry: hit, bounded, bound to, snapped,
    /// flipped and exported as a rectangle is. A sticky note is one with its own paint
    /// (`_isRectanguloidElement`, `packages/element/src/utils.ts@1118751f:253-266`).
    pub fn is_rect_like(self) -> bool {
        matches!(self, Self::Rectangle | Self::StickyNote)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FillStyle {
    Hachure,
    CrossHatch,
    Solid,
    Zigzag,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StrokeStyle {
    Solid,
    Dashed,
    Dotted,
}

/// Excalidraw's `Arrowhead` union (`packages/element/src/types.ts@1118751f:350-364`), plus
/// `None` for "no head" — the outer `Option<Arrowhead>` on [`DrawElement`] is `None` for
/// "unset, fall back to the type's default" (Excalidraw's `undefined`), so this variant is
/// needed for "explicitly no head" (Excalidraw's `null`).
///
/// Wire names are `snake_case` to match the oracle's strings exactly
/// (`triangle_outline`, `cardinality_one_or_many`, ...). Four legacy spellings are
/// accepted on the way in and folded into their modern equivalent, exactly as the
/// oracle's own `normalizeArrowhead` does (`packages/element/src/arrowheads.ts@1118751f:3-21`):
/// `dot` was `circle` before it existed, and `crowfoot_one` / `crowfoot_many` /
/// `crowfoot_one_or_many` were the cardinality markers before ER diagrams got their own
/// name for them. Serialization never emits the legacy spelling — a board loads it once
/// and is only ever saved back in the modern shape, the same contract `legacy_group_id`
/// keeps for groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arrowhead {
    None,
    Arrow,
    Triangle,
    TriangleOutline,
    #[serde(alias = "dot")]
    Circle,
    CircleOutline,
    Diamond,
    DiamondOutline,
    Bar,
    /// "Exactly one" and "many" in an ER diagram's crow's-foot notation.
    #[serde(alias = "crowfoot_one")]
    CardinalityOne,
    #[serde(alias = "crowfoot_many")]
    CardinalityMany,
    #[serde(alias = "crowfoot_one_or_many")]
    CardinalityOneOrMany,
    CardinalityExactlyOne,
    CardinalityZeroOrOne,
    CardinalityZeroOrMany,
}

/// How a bound arrow end sits against the shape it is bound to.
///
/// Excalidraw's `BindMode` (`packages/element/src/types.ts@1118751f:316-333`), less the `skip` mode
/// only its feature-flagged strategy produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindMode {
    /// The end sits exactly on its anchor, wherever inside the shape that is.
    Inside,
    /// The end aims at its anchor but stops a gap clear of the outline.
    Orbit,
}

/// Picker order: the oracle's `getArrowheadOptions` (`actionProperties.tsx@1118751f:1830-1941`)
/// — its visible section (none, arrow, triangle, triangle outline), then its hidden
/// section (circle, circle outline, diamond, diamond outline, bar), then its cardinality
/// section (one, many, one-or-many, exactly-one, zero-or-one, zero-or-many).
pub const ARROWHEADS: [Arrowhead; 15] = [
    Arrowhead::None,
    Arrowhead::Arrow,
    Arrowhead::Triangle,
    Arrowhead::TriangleOutline,
    Arrowhead::Circle,
    Arrowhead::CircleOutline,
    Arrowhead::Diamond,
    Arrowhead::DiamondOutline,
    Arrowhead::Bar,
    Arrowhead::CardinalityOne,
    Arrowhead::CardinalityMany,
    Arrowhead::CardinalityOneOrMany,
    Arrowhead::CardinalityExactlyOne,
    Arrowhead::CardinalityZeroOrOne,
    Arrowhead::CardinalityZeroOrMany,
];

/// Where a line of text sits across the width of its own box.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

pub const TEXT_ALIGNS: [TextAlign; 3] = [TextAlign::Left, TextAlign::Center, TextAlign::Right];

/// Where a label sits down the height of the shape holding it.
///
/// Free-standing text has no second box to sit in, so this only has visible meaning for
/// a label — but it is stored on every text so that binding one to a shape later does
/// not have to invent a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerticalAlign {
    Top,
    Middle,
    Bottom,
}

pub const VERTICAL_ALIGNS: [VerticalAlign; 3] = [
    VerticalAlign::Top,
    VerticalAlign::Middle,
    VerticalAlign::Bottom,
];

/// The horizontal alignment to draw with, for an element that may not name one.
///
/// Both alignment fields are `Option` rather than plain values with a `Default`, and the
/// reason is every board saved before they existed. Such a scene carries neither, and
/// the two roles want opposite answers: free text reads from the left like a paragraph,
/// a label centres inside its shape. One blanket default would re-align every label ever
/// saved, so the board would come back looking different — the one thing a format change
/// must not do. Deriving the fallback from `container_id` reproduces exactly what the
/// two hard-coded constants used to do.
pub fn resolved_text_align(element: &DrawElement) -> TextAlign {
    element
        .text_align
        .unwrap_or(if element.container_id.is_some() {
            TextAlign::Center
        } else {
            TextAlign::Left
        })
}

/// Whether a text element sizes itself to its glyphs, or keeps the width it was given.
///
/// `None` means auto, because every text saved before the field existed did. Only
/// `false` is ever written out, so an old scene round-trips byte-identically.
pub fn is_auto_resize(element: &DrawElement) -> bool {
    element.auto_resize.unwrap_or(true)
}

/// The vertical alignment to lay out with. See [`resolved_text_align`] for why it is not
/// a plain `Default`.
pub fn resolved_vertical_align(element: &DrawElement) -> VerticalAlign {
    element
        .vertical_align
        .unwrap_or(if element.container_id.is_some() {
            VerticalAlign::Middle
        } else {
            VerticalAlign::Top
        })
}

/// The text as it was typed, before any wrapping — Excalidraw's `originalText`.
///
/// `text` is what is drawn: the source with its soft line breaks baked in. Every text
/// saved before `original_text` existed has only that, so it stands in as the source,
/// and so does it for an empty source, as the oracle's `originalText || text` has it
/// (`packages/excalidraw/data/restore.ts@1118751f:570`).
pub fn source_text(element: &DrawElement) -> &str {
    element
        .original_text
        .as_deref()
        .filter(|source| !source.is_empty())
        .or(element.text.as_deref())
        .unwrap_or_default()
}

/// The family ids a text may carry, the contract's (`MAX_FONT_FAMILY` in
/// `packages/contract/src/limits.ts`). Wider than [`crate::text::font::FAMILIES`], so a newer client's
/// font survives a trip through this one.
const FONT_FAMILY_IDS: std::ops::RangeInclusive<f64> = 1.0..=64.0;

/// The line heights a text may carry, the contract's (`MIN_LINE_HEIGHT`/`MAX_LINE_HEIGHT`).
const LINE_HEIGHTS: std::ops::RangeInclusive<f64> = 0.5..=4.0;

/// A `fontFamily` as it comes in, kept only when the contract would store it.
///
/// Anything else — 0, 99, 1.5, a string — is dropped rather than refusing the document:
/// the server refuses every patch that carries one, so a scene holding it would never
/// save again. Dropped is what happened to the key before the engine knew it.
fn contract_font_family<'de, D>(deserializer: D) -> Result<Option<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value
        .as_f64()
        .filter(|id| id.fract() == 0.0 && FONT_FAMILY_IDS.contains(id))
        // A whole number in 1..=64, so exact.
        .map(|id| id as u8))
}

/// A `lineHeight` as it comes in, kept only when the contract would store it; see
/// [`contract_font_family`].
fn contract_line_height<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_f64().filter(|value| LINE_HEIGHTS.contains(value)))
}

/// The family to draw with, or `None` for the system stack every text used before
/// families existed.
///
/// An id the engine does not know is ignored rather than trusted: it may be a newer
/// client's font, so the field keeps it as it came, but it is drawn with the stack.
pub fn resolved_font_family(element: &DrawElement) -> Option<u8> {
    element
        .font_family
        .filter(|id| crate::text::font::family(*id).is_some())
}

/// The unitless line height to lay out with.
///
/// The element's own when it is in range; otherwise its family's, and for the system
/// stack the [`TEXT_LINE_HEIGHT`](crate::render::TEXT_LINE_HEIGHT) every text has always
/// had. Out of range — which the contract refuses, and nothing coming in can carry, so
/// only code can set it — is ignored rather than clamped. The oracle has no range to
/// enforce: its restore replaces only a missing or zero value
/// (`packages/excalidraw/data/restore.ts@1118751f:555-562`).
pub fn resolved_line_height(element: &DrawElement) -> f64 {
    element
        .line_height
        .filter(|value| LINE_HEIGHTS.contains(value))
        .unwrap_or_else(|| {
            resolved_font_family(element)
                .and_then(crate::text::font::family)
                .map_or(crate::render::TEXT_LINE_HEIGHT, |family| family.line_height)
        })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawElementStyle {
    pub stroke_color: String,
    pub background_color: String,
    pub fill_style: FillStyle,
    pub stroke_width: f64,
    pub stroke_style: StrokeStyle,
    pub roughness: f64,
    pub opacity: f64,
    pub roundness: Option<f64>,
}

impl Default for DrawElementStyle {
    fn default() -> Self {
        Self {
            stroke_color: "#1e1e1e".into(),
            background_color: "transparent".into(),
            fill_style: FillStyle::Hachure,
            stroke_width: 2.0,
            stroke_style: StrokeStyle::Solid,
            roughness: 1.0,
            opacity: 100.0,
            roundness: Some(8.0),
        }
    }
}

pub fn default_element_style() -> DrawElementStyle {
    DrawElementStyle::default()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawElement {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: DrawElementType,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub angle: f64,
    pub stroke_color: String,
    pub background_color: String,
    pub fill_style: FillStyle,
    pub stroke_width: f64,
    pub stroke_style: StrokeStyle,
    pub roughness: f64,
    pub opacity: f64,
    pub roundness: Option<f64>,
    /// An explicit corner radius, set by dragging a corner-radius handle.
    ///
    /// `None` — every element saved before this existed — means the adaptive rule, exactly
    /// as before. It is a separate field rather than the number already in `roundness`
    /// because that number has always been written (`8`) and always ignored: honouring it
    /// now would sharpen every rounded rectangle on every existing board.
    ///
    /// The oracle has the same slot, `roundness.value`, read as a fixed radius by
    /// `getCornerRadius` and never set by its UI. Only applies while `roundness` is set:
    /// Sharp wins, and the radius is remembered for when it is Round again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f64>,
    pub seed: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<[f64; 2]>>,
    /// Only a line's: closes on its first point and paints as a filled shape rather than
    /// an open stroke — Excalidraw's `ExcalidrawLineElement.polygon`
    /// (`packages/element/src/types.ts@1118751f:382`).
    ///
    /// `None` — every line saved before this field existed — reads as `false`, exactly as
    /// the oracle's restore does (`packages/excalidraw/data/restore.ts@1118751f:645-651`):
    /// only an explicit `true` promotes a line, a first point that happens to equal the
    /// last never does on its own. Read it through [`DrawElement::is_polygon`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polygon: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_binding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_binding: Option<String>,
    /// Where on its shape the start is anchored, as a ratio of the shape's own unrotated
    /// width and height — Excalidraw's `fixedPoint`. Absent on every arrow bound before
    /// anchors existed, which reads as the centre: exactly where those were always aimed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_fixed_point: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_fixed_point: Option<[f64; 2]>,
    /// Absent reads as [`BindMode::Orbit`], again how every older binding was drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_bind_mode: Option<BindMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_bind_mode: Option<BindMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_arrowhead: Option<Arrowhead>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_arrowhead: Option<Arrowhead>,
    /// Only an arrow's: routed around its shapes in horizontal and vertical runs rather
    /// than drawn through its points — Excalidraw's `elbowed`
    /// (`packages/element/src/types.ts@1118751f:394`). `None`, as on every arrow saved
    /// before this existed, is a sharp or curved arrow. See `scene/elbow`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elbowed: Option<bool>,
    /// An elbow arrow's segments the person moved, which its route keeps — Excalidraw's
    /// `fixedSegments`. `None` is none (the oracle's `null`); an empty list is the fresh
    /// arrow's `[]`, which the router tells apart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_segments: Option<Vec<crate::scene::elbow::FixedSegment>>,
    /// Whether an elbow arrow's route has an extra corner beside its start (or end), put
    /// in to leave the shape square on while segments are fixed — Excalidraw's
    /// `startIsSpecial`/`endIsSpecial`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_is_special: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_is_special: Option<bool>,
    /// What is drawn: the source with its soft line breaks baked in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The text as it was typed, before wrapping — Excalidraw's `originalText`. Read it
    /// through [`source_text`]: `None`, as on every text saved before it existed, means
    /// `text` is the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    /// Excalidraw's numeric font family id. Read it through [`resolved_font_family`]:
    /// `None` is the system stack every text saved before families existed was drawn
    /// with, and an id the engine does not know is kept but not drawn with. One the
    /// contract would refuse is dropped as it comes in.
    #[serde(
        default,
        deserialize_with = "contract_font_family",
        skip_serializing_if = "Option::is_none"
    )]
    pub font_family: Option<u8>,
    /// Unitless, a multiple of the font size. Read it through [`resolved_line_height`]:
    /// `None` is the family's own. One the contract would refuse is dropped as it comes in.
    #[serde(
        default,
        deserialize_with = "contract_line_height",
        skip_serializing_if = "Option::is_none"
    )]
    pub line_height: Option<f64>,
    /// Read it through [`resolved_text_align`], never directly: `None` is "nobody has
    /// said", which is not the same as any of the three values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_align: Option<TextAlign>,
    /// Read it through [`resolved_vertical_align`], for the same reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_align: Option<VerticalAlign>,
    /// Read it through [`is_auto_resize`]: `None` is auto, which is what every text
    /// saved before this field existed did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_resize: Option<bool>,
    /// Only a label's: whether it wraps inside its shape (`None` or `true`, the oracle's
    /// only behaviour) or keeps its hard lines and the shape grows wide enough for them
    /// (`false`). Free text says the same with [`auto_resize`](Self::auto_resize).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_text_id: Option<String>,
    /// The groups this element belongs to, **innermost first**.
    ///
    /// The array *is* the nesting: there is no group entity, no tree and no parent
    /// pointer — a group is an id that several elements happen to carry, and an element
    /// inside two groups carries both. Verified against the oracle twice, by reading
    /// `packages/element/src/types.ts` and by grouping three rectangles on
    /// excalidraw.com and reading the result back; see `docs/reference/groups.md`.
    ///
    /// This was a single `Option<String>`, which made a group inside a group
    /// unrepresentable — and grouping a group silently destroyed the inner one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_ids: Vec<String>,
    /// The pre-array spelling, read and never written.
    ///
    /// Boards saved before groups could nest carry a single `groupId`. Kept as a capture
    /// field so those files still load: [`normalize_group_ids`] folds it into
    /// `group_ids`, so a scene is only ever in the new shape once in memory, and only the
    /// new shape is written back.
    #[serde(default, rename = "groupId", skip_serializing)]
    pub legacy_group_id: Option<String>,
    /// The frame that owns this element, if it is inside one.
    ///
    /// Membership is a property of the child, not a list on the frame, so moving an
    /// element between frames is one write rather than two — and an element can never be
    /// in two frames at once by construction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<String>,
    /// A frame's label. Nothing else carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The image itself, as a `data:` URL.
    ///
    /// Carried on the element rather than keyed into a separate file store, which is
    /// what Excalidraw does. Theirs keeps the scene small and is the better end state;
    /// this one makes saving, loading, undo, copy/paste, export and realtime work with
    /// no new plumbing at all, because the image travels wherever the element does. The
    /// cost is scene size, and the swap is mechanical when a blob endpoint exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_url: Option<String>,
    /// The page an embed frames.
    ///
    /// The **resolved** embed URL, not the one that was pasted: a YouTube watch page in
    /// an iframe shows a refusal rather than a video. Resolving once and storing the
    /// result means the board does not depend on the rules still agreeing later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    /// A sticky note's height as the user set it (`baseHeight`): the note grows above it
    /// to hold its label and never shrinks below it. `None` on anything else, and on a
    /// note from before the field, whose height is then its base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_height: Option<f64>,
    /// When a sticky note was made, in epoch ms — the date its footer shows. `None` is
    /// unknown (the oracle's `null`), and no footer. The oracle stamps it on every element;
    /// here only a note carries it, so no other element's JSON changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<f64>,
    /// A sticky note label's size as the user set it (`baseFontSize`): the ceiling its fit
    /// shrinks below. Read it through `scene::sticky::label_ceiling`, which asks the
    /// container: a label unbound from its note keeps a stale one, meaningless there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_font_size: Option<f64>,
    /// A figure's parameters — its kind, and the sides/ratio that shape it. `None` on
    /// every element that is not a figure, and on a figure whose kind carries no such
    /// field (there is none: `kind` is always present when this is `Some`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figure: Option<FigureParams>,
    pub version: u32,
    pub version_nonce: u32,
    pub updated: f64,
    pub is_deleted: bool,
}

impl DrawElement {
    pub fn locked(&self) -> bool {
        self.locked.unwrap_or(false)
    }

    /// See [`Self::polygon`].
    pub fn is_polygon(&self) -> bool {
        self.polygon.unwrap_or(false)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Geometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub(crate) fn rand_int() -> u32 {
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Math::random() * 2_147_483_647.0) as u32
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::cell::Cell;
        thread_local! {
            static STATE: Cell<u64> = const { Cell::new(0x9e37_79b9_7f4a_7c15) };
        }
        STATE.with(|cell| {
            let mut x = cell.get();
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            cell.set(x);
            (x as u32) & 0x7fff_ffff
        })
    }
}

pub fn new_element_id() -> String {
    format!("el-{}-{}", rand_int(), rand_int())
}

pub fn create_element(
    kind: DrawElementType,
    geometry: Geometry,
    style: DrawElementStyle,
    now: f64,
) -> DrawElement {
    DrawElement {
        id: new_element_id(),
        kind,
        x: geometry.x,
        y: geometry.y,
        width: geometry.width,
        height: geometry.height,
        angle: 0.0,
        stroke_color: style.stroke_color,
        background_color: style.background_color,
        fill_style: style.fill_style,
        stroke_width: style.stroke_width,
        stroke_style: style.stroke_style,
        roughness: style.roughness,
        opacity: style.opacity,
        roundness: style.roundness,
        corner_radius: None,
        seed: rand_int(),
        points: None,
        polygon: None,
        start_binding: None,
        end_binding: None,
        start_fixed_point: None,
        end_fixed_point: None,
        start_bind_mode: None,
        end_bind_mode: None,
        start_arrowhead: None,
        end_arrowhead: None,
        elbowed: None,
        fixed_segments: None,
        start_is_special: None,
        end_is_special: None,
        text: None,
        original_text: None,
        font_size: None,
        font_family: None,
        line_height: None,
        text_align: None,
        vertical_align: None,
        auto_resize: None,
        wrap: None,
        container_id: None,
        bound_text_id: None,
        group_ids: Vec::new(),
        legacy_group_id: None,
        frame_id: None,
        name: None,
        data_url: None,
        embed_url: None,
        locked: None,
        base_height: None,
        created: None,
        base_font_size: None,
        figure: None,
        version: 1,
        version_nonce: rand_int(),
        updated: now,
        is_deleted: false,
    }
}

pub fn create_element_default(kind: DrawElementType, geometry: Geometry) -> DrawElement {
    create_element(kind, geometry, DrawElementStyle::default(), 0.0)
}

pub fn bump_version(mut element: DrawElement, now: f64) -> DrawElement {
    element.version += 1;
    element.version_nonce = rand_int();
    element.updated = now;
    element
}

pub fn merge_style(base: &DrawElementStyle, patch: &DrawElementStylePatch) -> DrawElementStyle {
    DrawElementStyle {
        stroke_color: patch
            .stroke_color
            .clone()
            .unwrap_or_else(|| base.stroke_color.clone()),
        background_color: patch
            .background_color
            .clone()
            .unwrap_or_else(|| base.background_color.clone()),
        fill_style: patch.fill_style.unwrap_or(base.fill_style),
        stroke_width: patch.stroke_width.unwrap_or(base.stroke_width),
        stroke_style: patch.stroke_style.unwrap_or(base.stroke_style),
        roughness: patch.roughness.unwrap_or(base.roughness),
        opacity: patch.opacity.unwrap_or(base.opacity),
        roundness: patch.roundness.unwrap_or(base.roundness),
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawElementStylePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_style: Option<FillStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_style: Option<StrokeStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roughness: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    /// Three states, not two: absent (leave it alone), `null` (make it sharp), or a
    /// value (make it round).
    ///
    /// The default serde mapping collapses the first two — an explicit `null` becomes the
    /// *outer* `None`, which reads as "not present". So a patch could turn corners on and
    /// never turn them off again: measured on the canvas, sharp gave 492 ink, round 468,
    /// and sharp again stayed 468. `double_option` keeps the two apart.
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub roundness: Option<Option<f64>>,
    /// A preset's font: the family, the size and the horizontal alignment. Reaches the
    /// selected texts and the labels of selected shapes — never a shape itself — laid out
    /// again as one step with the rest of the patch; see `engine/selection_style.rs`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_align: Option<TextAlign>,
    /// The inspector's Shape row: unlike [`figure_sides`](Self::figure_sides) and
    /// [`figure_ratio`](Self::figure_ratio), also picked with a figure selected and
    /// nothing changes it — this is what makes that visible
    /// (`engine::selection_style::apply_style`). Ignored on anything that is not a
    /// [`DrawElementType::Figure`]; see [`crate::scene::figure::change_kind`] for what a
    /// change carries over.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figure_kind: Option<FigureKind>,
    /// A figure's own two controls — the inspector's sides stepper and ratio slider.
    /// Ignored on anything that is not a [`DrawElementType::Figure`], and on a kind of
    /// figure that has no such control ([`crate::scene::figure::has_sides`] /
    /// [`crate::scene::figure::has_ratio`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figure_sides: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figure_ratio: Option<f64>,
}

/// Lets an explicit `null` mean "set this to nothing" rather than "say nothing about it".
///
/// With `#[serde(default)]` supplying `None` for an absent field, this makes the three
/// cases distinct: absent -> `None`, `null` -> `Some(None)`, value -> `Some(Some(v))`.
fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(deserializer).map(Some)
}

pub fn apply_style_patch(element: &mut DrawElement, patch: &DrawElementStylePatch) {
    if let Some(v) = &patch.stroke_color {
        element.stroke_color.clone_from(v);
    }
    if let Some(v) = &patch.background_color {
        element.background_color.clone_from(v);
    }
    if let Some(v) = patch.fill_style {
        element.fill_style = v;
    }
    if let Some(v) = patch.stroke_width {
        element.stroke_width = v;
    }
    if let Some(v) = patch.stroke_style {
        element.stroke_style = v;
    }
    if let Some(v) = patch.roughness {
        element.roughness = v;
    }
    if let Some(v) = patch.opacity {
        element.opacity = v;
    }
    if let Some(v) = patch.roundness {
        element.roundness = v;
    }
    if let Some(kind) = patch.figure_kind {
        if let Some(params) = element.figure.as_mut() {
            crate::scene::figure::change_kind(params, kind);
        }
    }
    if let Some(sides) = patch.figure_sides {
        if let Some(params) = element.figure.as_mut() {
            if crate::scene::figure::has_sides(params.kind) {
                params.sides = Some(crate::scene::figure::resolved_sides(
                    params.kind,
                    Some(sides),
                ));
            }
        }
    }
    if let Some(ratio) = patch.figure_ratio {
        if let Some(params) = element.figure.as_mut() {
            if crate::scene::figure::has_ratio(params.kind) {
                params.ratio = Some(crate::scene::figure::resolved_ratio(
                    params.kind,
                    Some(ratio),
                ));
            }
        }
    }
}

/// Folds a legacy single `groupId` into `group_ids`.
///
/// Called on every element arriving from outside — a file, the clipboard, a remote
/// patch — so nothing downstream ever has to know the old spelling existed. Idempotent:
/// an element already in the new shape is untouched.
pub fn normalize_group_ids(element: &mut DrawElement) {
    if let Some(legacy) = element.legacy_group_id.take() {
        if element.group_ids.is_empty() {
            element.group_ids.push(legacy);
        }
    }
}

/// Sanitizes a line's `polygon` flag against its own points on the way in.
///
/// The oracle's restore (`packages/excalidraw/data/restore.ts@1118751f:645-651`):
/// `polygon: isValidPolygon(points) ? element.polygon ?? false : false`. A geometrically
/// closed line does **not** get promoted to `polygon: true` on load — only an explicit
/// `true` does, and even that survives only when the points can actually support it
/// ([`crate::scene::geometry::is_valid_polygon`]: more than three points, closed exactly).
/// A `true` saved over points that no longer qualify — hand-edited, or truncated by a
/// point removed elsewhere in the same document — is corrected to `false` rather than
/// trusted, the same way an out-of-range `lineHeight` is dropped rather than clamped.
///
/// Left untouched (not stamped to `Some(false)`) when there was nothing to correct, so
/// the overwhelming common case — an ordinary open line — round-trips byte-identical.
pub fn normalize_polygon(element: &mut DrawElement) {
    if element.kind == DrawElementType::Line
        && element.polygon == Some(true)
        && !crate::scene::geometry::is_valid_polygon(element.points.as_deref().unwrap_or(&[]))
    {
        element.polygon = Some(false);
    }
}
