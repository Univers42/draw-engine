use serde::{Deserialize, Serialize};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arrowhead {
    None,
    Arrow,
    Triangle,
    Dot,
    Diamond,
    Bar,
}

pub const ARROWHEADS: [Arrowhead; 6] = [
    Arrowhead::None,
    Arrowhead::Arrow,
    Arrowhead::Triangle,
    Arrowhead::Dot,
    Arrowhead::Diamond,
    Arrowhead::Bar,
];

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
    pub seed: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<[f64; 2]>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_binding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_binding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_arrowhead: Option<Arrowhead>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_arrowhead: Option<Arrowhead>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_text_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
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
    pub version: u32,
    pub version_nonce: u32,
    pub updated: f64,
    pub is_deleted: bool,
}

impl DrawElement {
    pub fn locked(&self) -> bool {
        self.locked.unwrap_or(false)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Geometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

fn rand_int() -> u32 {
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
        seed: rand_int(),
        points: None,
        start_binding: None,
        end_binding: None,
        start_arrowhead: None,
        end_arrowhead: None,
        text: None,
        font_size: None,
        container_id: None,
        bound_text_id: None,
        group_id: None,
        frame_id: None,
        name: None,
        data_url: None,
        embed_url: None,
        locked: None,
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
}
