//! The properties panel's view of the selection, and the style edits it makes.
//!
//! One pass answers every row of the panel: for each property, the value the selection
//! shares or `None` for "mixed" — the oracle's `getFormValue` + `reduceToCommonValue`
//! (`packages/excalidraw/actions/actionProperties.tsx@1118751f:229-270`). The host used
//! to read each value on its own, from the first selected element or from a snapshot
//! taken at the last selection event, so a mixed selection showed one element's values
//! as everyone's and an undo or a peer's edit left the panel stale.
//!
//! `style_revision` is what makes the answer cheap: it moves on everything that can
//! change it, and the host asks again only then — once per revision, not once per row
//! and not once per frame.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use super::DrawEngine;
use crate::render::default_arrowhead;
use crate::scene::figure::{self, FigureKind};
use crate::scene::{
    apply_style_patch, is_auto_resize, is_transparent, resolved_font_family, resolved_text_align,
    resolved_vertical_align, Arrowhead, DrawElement, DrawElementStyle, DrawElementStylePatch,
    DrawElementType, FillStyle, StrokeStyle, TextAlign, VerticalAlign,
};

/// Excalidraw's `reduceToCommonValue`: the one value everything shares, or nothing.
enum Common<T> {
    Empty,
    One(T),
    Mixed,
}

impl<T: PartialEq> Common<T> {
    fn add(&mut self, value: T) {
        match self {
            Common::Empty => *self = Common::One(value),
            Common::One(held) if *held == value => {}
            Common::One(_) => *self = Common::Mixed,
            Common::Mixed => {}
        }
    }

    fn get(self) -> Option<T> {
        match self {
            Common::One(value) => Some(value),
            Common::Empty | Common::Mixed => None,
        }
    }
}

/// The Edges row: Excalidraw reads roundness as a two-way choice (`:1807-1816`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Edges {
    Sharp,
    Round,
}

impl Edges {
    fn of(roundness: Option<f64>) -> Self {
        if roundness.is_some() {
            Edges::Round
        } else {
            Edges::Sharp
        }
    }
}

/// The Arrow type row: `ARROW_TYPE` (`packages/common/src/constants.ts@1118751f`) less
/// `elbow`, which this engine does not route — a recorded gap (`docs/reference/console.md`).
/// An arrow is curved when it has a roundness at all, as the oracle's form value reads it
/// (`actionProperties.tsx@1118751f:2275-2296`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ArrowType {
    Sharp,
    Round,
}

impl ArrowType {
    pub(super) fn of(roundness: Option<f64>) -> Self {
        if roundness.is_some() {
            ArrowType::Round
        } else {
            ArrowType::Sharp
        }
    }

    /// The roundness an arrow of this type is drawn with. A curve takes the default
    /// style's: the painter reads an arrow's roundness only as "curved" (`PROPORTIONAL_RADIUS`
    /// carries no value either).
    pub(super) fn roundness(self) -> Option<f64> {
        match self {
            ArrowType::Round => DrawElementStyle::default().roundness,
            ArrowType::Sharp => None,
        }
    }

    /// `"sharp"` or `"round"`; anything else — `"elbow"` included — is `None`.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "sharp" => Some(ArrowType::Sharp),
            "round" => Some(ArrowType::Round),
            _ => None,
        }
    }
}

/// What the properties panel shows. Every value is `None` when the selection disagrees
/// on it — the panel then marks nothing as current — or when nothing selected has it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionStyle {
    /// Live selected elements. Zero means the values are the next element's style.
    /// Locked ones count — the layer, flip and group rows act on them — but everything
    /// below reads only what a style would change ([`DrawEngine::restylable`]).
    pub count: usize,
    /// Types of the oracle's target elements: the selection plus the labels its shapes
    /// carry (`getTargetElements`), distinct. A label is what makes a labelled shape
    /// offer the text rows.
    pub kinds: Vec<DrawElementType>,
    /// The subset of `kinds` with a background that paints something — what the fill
    /// style row asks about (`shapeActionPredicates.ts@1118751f:130-139`).
    pub filled_kinds: Vec<DrawElementType>,
    /// `suppportsHorizontalAlign` (`packages/element/src/textElement.ts@1118751f:462-477`).
    pub text_alignable: bool,
    /// `shouldAllowVerticalAlign` (`textElement.ts@1118751f:446-460`).
    pub vertical_alignable: bool,
    pub can_align: bool,
    pub can_distribute: bool,
    /// Whether the polygon toggle would do anything: every selected element a line with
    /// four points or more, and at least one selected (`DrawEngine::can_toggle_polygon`).
    pub can_toggle_polygon: bool,
    /// Only asked for a single element: from two up the group row shows regardless.
    pub is_group: bool,
    pub stroke_color: Option<String>,
    pub background_color: Option<String>,
    pub fill_style: Option<FillStyle>,
    pub stroke_width: Option<f64>,
    pub stroke_style: Option<StrokeStyle>,
    pub roughness: Option<f64>,
    pub opacity: Option<f64>,
    pub edges: Option<Edges>,
    pub start_arrowhead: Option<Arrowhead>,
    pub end_arrowhead: Option<Arrowhead>,
    /// Read off arrows alone; the Edges row reads everything else.
    pub arrow_type: Option<ArrowType>,
    /// Whether the selected lines are already closed polygons — `None` for a mixed
    /// selection, or one holding nothing the polygon toggle applies to. Read off lines
    /// alone, the same way `arrow_type` reads off arrows alone.
    pub is_polygon: Option<bool>,
    pub font_size: Option<f64>,
    /// The family the texts share — 0 for the system stack a text with none is drawn in.
    pub font_family: Option<u8>,
    pub text_align: Option<TextAlign>,
    pub vertical_align: Option<VerticalAlign>,
    /// A free text is selected: the auto-resize row means something, and "Wrap text in
    /// a container" is offered (`actionWrapTextInContainer`'s predicate,
    /// `actions/actionBoundText.tsx@1118751f:262-268`).
    pub has_free_text: bool,
    /// Whether the free texts size themselves to their text ([`is_auto_resize`]).
    pub auto_resize: Option<bool>,
    /// A label is reached — selected, or carried by a selected shape.
    pub has_label: bool,
    /// Whether the labels wrap inside their shapes (`wrap` other than `false`).
    pub label_wrap: Option<bool>,
    /// "Bind text to the container" (`actionBoundText.tsx@1118751f:128-154`); see
    /// `bound_text.rs`.
    pub can_bind_text: bool,
    /// "Unbind text": a selected shape carries a label a style may change (`:64-68`).
    pub can_unbind_text: bool,
    /// The figure kind shared by the selection, or the next figure's with nothing
    /// selected. `None` for a mixed selection, or one holding no figure at all.
    pub figure_kind: Option<FigureKind>,
    /// Resolved — a figure whose own `sides`/`ratio` is absent reads back as the kind's
    /// default, the way it is actually drawn ([`figure::resolved_sides`]).
    pub figure_sides: Option<u8>,
    pub figure_ratio: Option<f64>,
    /// Whether `figure_kind` — resolved to one kind, not mixed or absent — has a sides
    /// stepper or a ratio slider at all ([`figure::has_sides`] / [`figure::has_ratio`]).
    pub figure_has_sides: bool,
    pub figure_has_ratio: bool,
    /// Whose colours a stroke pick sets: the notes', the other shapes', or both — which
    /// palette the panel offers and what it calls the row (`resolveColorTarget`,
    /// `actions/colorTargets.ts@1118751f:89-176`).
    pub stroke_domain: ColorDomain,
    /// As `stroke_domain`, for a background pick.
    pub background_domain: ColorDomain,
}

/// A colour pick's domain (`ColorTargetKind`, `actions/colorTargets.ts@1118751f:33-37`):
/// sticky notes keep colours of their own — defaults, top picks, and never transparent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorDomain {
    Regular,
    Sticky,
    Mixed,
}

/// `hasStrokeColor` (`packages/element/src/comparisons.ts@1118751f:19-29`).
fn has_stroke_color(kind: DrawElementType) -> bool {
    matches!(
        kind,
        DrawElementType::Rectangle
            | DrawElementType::StickyNote
            | DrawElementType::Ellipse
            | DrawElementType::Diamond
            | DrawElementType::Freedraw
            | DrawElementType::Arrow
            | DrawElementType::Line
            | DrawElementType::Text
            | DrawElementType::Embed
            | DrawElementType::Figure
    )
}

/// `hasBackground` (`comparisons.ts@1118751f:3-14`).
fn has_background(kind: DrawElementType) -> bool {
    matches!(
        kind,
        DrawElementType::Rectangle
            | DrawElementType::StickyNote
            | DrawElementType::Embed
            | DrawElementType::Ellipse
            | DrawElementType::Diamond
            | DrawElementType::Line
            | DrawElementType::Freedraw
            | DrawElementType::Figure
    )
}

/// `hasFillStyle` (`packages/element/src/comparisons.ts@1118751f:16-17`).
fn has_fill_style(kind: DrawElementType) -> bool {
    matches!(
        kind,
        DrawElementType::Rectangle
            | DrawElementType::Embed
            | DrawElementType::Ellipse
            | DrawElementType::Diamond
            | DrawElementType::Line
            | DrawElementType::Freedraw
            | DrawElementType::Figure
    )
}

/// Which elements take corners from a copied style: `isUsingAdaptiveRadius` plus
/// `isUsingProportionalRadius` (`packages/element/src/typeChecks.ts@1118751f:322-332`).
/// The radius rule itself follows the element's type in the painter, so only the
/// yes-or-no crosses over.
fn takes_roundness(kind: DrawElementType) -> bool {
    matches!(
        kind,
        DrawElementType::Rectangle
            | DrawElementType::StickyNote
            | DrawElementType::Embed
            | DrawElementType::Image
            | DrawElementType::Line
            | DrawElementType::Arrow
            | DrawElementType::Diamond
    )
}

fn push_kind(kinds: &mut Vec<DrawElementType>, kind: DrawElementType) {
    if !kinds.contains(&kind) {
        kinds.push(kind);
    }
}

/// The figures among `targets` a patch's `figure_kind`/`figure_sides`/`figure_ratio`
/// reaches — empty when the patch carries none of them, so a plain colour or opacity
/// pick never pays for a label relayout or a bindings pass it cannot need.
fn figure_target_ids(
    targets: &[(DrawElement, DrawElementStylePatch)],
    patch: &DrawElementStylePatch,
) -> Vec<String> {
    if patch.figure_kind.is_none() && patch.figure_sides.is_none() && patch.figure_ratio.is_none() {
        return Vec::new();
    }
    targets
        .iter()
        .filter(|(element, _)| element.kind == DrawElementType::Figure)
        .map(|(element, _)| element.id.clone())
        .collect()
}

impl DrawEngine {
    /// Moves the counter the host keys the panel to. Wrapping: only inequality matters.
    pub(super) fn touch_style(&mut self) {
        self.style_revision = self.style_revision.wrapping_add(1);
    }

    /// Ends a style write: one step of undo — or, while a text is being typed, part of
    /// that edit. The session's commit records the typing and every style written
    /// meanwhile as one step, and nothing is stamped or sent before it; the host's editor
    /// reads the text's new look back at once, through the revision this moves
    /// (`text_session.rs`).
    pub(super) fn commit_style(&mut self) {
        if self.text_session.is_none() {
            self.push_history();
            return;
        }
        // What a preview changed is the edit's now, as a commit would have made it.
        self.style_preview.clear();
        self.touch_style();
    }

    /// Changes whenever [`Self::selection_style`] may have. See the module comment.
    pub fn style_revision(&self) -> u32 {
        self.style_revision
    }

    /// The label a shape carries, if it is live.
    pub(super) fn live_label(&self, element: &DrawElement) -> Option<&DrawElement> {
        element
            .bound_text_id
            .as_deref()
            .and_then(|id| self.scene.get(id))
            .filter(|label| !label.is_deleted)
    }

    /// Whether `element` is the label of a shape that is selected too. Select All takes
    /// labels with their shapes, where the oracle's skips bound text
    /// (`actions/actionSelectAll.ts@1118751f:32-38`) and a marquee never takes one alone
    /// (`selection/marquee.rs`). Held that way a label is still its shape's words: the
    /// panel reads and styles it through its shape, as after a click on the shape.
    fn is_carried_label(&self, element: &DrawElement) -> bool {
        element
            .container_id
            .as_deref()
            .filter(|container| self.selected_ids.contains(*container))
            .and_then(|container| self.scene.get(container))
            .and_then(|container| self.live_label(container))
            .is_some_and(|label| label.id == element.id)
    }

    /// The selection as the panel reads it: live, less the labels its shapes carry
    /// ([`Self::is_carried_label`]).
    pub(super) fn panel_selection(&self) -> Vec<&DrawElement> {
        self.selected_ids
            .iter()
            .filter_map(|id| self.scene.get(id))
            .filter(|element| !element.is_deleted && !self.is_carried_label(element))
            .collect()
    }

    /// Everything the panel shows, in one pass over the selection.
    pub fn selection_style(&self) -> SelectionStyle {
        let selected = self.panel_selection();
        if selected.is_empty() {
            return self.next_selection_style();
        }
        let carried = std::cell::OnceCell::new();
        let styled: Vec<&DrawElement> = selected
            .iter()
            .copied()
            .filter(|element| self.restylable(element, &carried))
            .collect();

        let mut kinds = Vec::new();
        let mut filled_kinds = Vec::new();
        let mut text_alignable = false;
        let mut vertical_alignable = false;
        let mut stroke_color = Common::Empty;
        let mut background_color = Common::Empty;
        let mut fill_style = Common::Empty;
        let mut stroke_width = Common::Empty;
        let mut stroke_style = Common::Empty;
        let mut roughness = Common::Empty;
        let mut opacity = Common::Empty;
        let mut edges = Common::Empty;
        let mut start_arrowhead = Common::Empty;
        let mut end_arrowhead = Common::Empty;
        let mut arrow_type = Common::Empty;
        let mut is_polygon = Common::Empty;
        let mut font_size = Common::Empty;
        let mut font_family = Common::Empty;
        let mut text_align = Common::Empty;
        let mut vertical_align = Common::Empty;
        let mut has_free_text = false;
        let mut auto_resize = Common::Empty;
        let mut has_label = false;
        let mut label_wrap = Common::Empty;
        let mut can_unbind_text = false;
        let mut figure_kind = Common::Empty;
        let mut figure_sides = Common::Empty;
        let mut figure_ratio = Common::Empty;

        // The oracle's targets: each selected element, then the label it carries.
        let mut target = |element: &DrawElement| {
            push_kind(&mut kinds, element.kind);
            if !is_transparent(&element.background_color) {
                push_kind(&mut filled_kinds, element.kind);
            }
            if element.kind == DrawElementType::Text {
                match element.container_id.as_deref() {
                    // An orphaned label still answers yes, as `getContainerElement`
                    // returning nothing does in the oracle.
                    Some(container) => {
                        let on_arrow = self
                            .scene
                            .get(container)
                            .is_some_and(|c| c.kind == DrawElementType::Arrow);
                        text_alignable |= !on_arrow;
                        vertical_alignable |= !on_arrow;
                    }
                    None => text_alignable = true,
                }
            }
        };
        for element in &styled {
            target(element);
            if let Some(label) = self.label_of(element, &carried) {
                target(label);
            }
        }

        for element in &styled {
            stroke_color.add(element.stroke_color.as_str());
            background_color.add(element.background_color.as_str());
            if has_fill_style(element.kind) {
                fill_style.add(element.fill_style);
            }
            if element.kind == DrawElementType::Figure {
                let params = element.figure.clone().unwrap_or_default();
                figure_kind.add(params.kind);
                figure_sides.add(figure::resolved_sides(params.kind, params.sides));
                figure_ratio.add(figure::resolved_ratio(params.kind, params.ratio));
            }
            stroke_width.add(element.stroke_width);
            stroke_style.add(element.stroke_style);
            roughness.add(element.roughness);
            opacity.add(element.opacity);
            if element.kind == DrawElementType::Line {
                is_polygon.add(element.is_polygon());
            }
            if element.kind == DrawElementType::Arrow {
                start_arrowhead.add(default_arrowhead(element, "start"));
                end_arrowhead.add(default_arrowhead(element, "end"));
                arrow_type.add(ArrowType::of(element.roundness));
            } else {
                edges.add(Edges::of(element.roundness));
            }
            // The text rows read the text a selected element *is*, or the label it
            // carries (`:1051-1070`, `:1611-1630`, `:1709-1728`).
            let text = if element.kind == DrawElementType::Text {
                Some(*element)
            } else {
                let label = self.label_of(element, &carried);
                can_unbind_text |= label.is_some();
                label
            };
            if let Some(text) = text {
                // A note's label shows its ceiling, the size picked, never the size its
                // fit shrank it to (`getBaseFontSize`, `actionProperties.tsx@1118751f:1057`).
                font_size.add(if text.font_size.is_some() {
                    self.user_font_size(text)
                } else {
                    super::DEFAULT_FONT_SIZE
                });
                font_family.add(resolved_font_family(text).unwrap_or(crate::text::FontKey::LEGACY));
                if text.container_id.is_some() {
                    has_label = true;
                    label_wrap.add(text.wrap != Some(false));
                } else {
                    has_free_text = true;
                    auto_resize.add(is_auto_resize(text));
                }
                text_align.add(resolved_text_align(text));
                // Free text has no vertical alignment to report (`:1712-1714`).
                vertical_align.add(
                    text.container_id
                        .is_some()
                        .then(|| resolved_vertical_align(text)),
                );
            }
        }

        // Counted once for both answers (`can_align` is more than one unit,
        // `can_distribute` more than two): the count walks the selection's groups, and
        // is most of what this whole summary costs on a large selection.
        let units = self.arrange_units();
        kinds.sort_by_key(|kind| *kind as u8);
        filled_kinds.sort_by_key(|kind| *kind as u8);
        let figure_kind = figure_kind.get();
        SelectionStyle {
            count: selected.len(),
            kinds,
            filled_kinds,
            text_alignable,
            vertical_alignable,
            can_align: units > 1,
            can_distribute: units > 2,
            can_toggle_polygon: self.can_toggle_polygon(),
            is_group: selected.len() == 1 && self.selection_is_group(),
            stroke_color: stroke_color.get().map(str::to_owned),
            background_color: background_color.get().map(str::to_owned),
            fill_style: fill_style.get(),
            stroke_width: stroke_width.get(),
            stroke_style: stroke_style.get(),
            roughness: roughness.get(),
            opacity: opacity.get(),
            edges: edges.get(),
            start_arrowhead: start_arrowhead.get(),
            end_arrowhead: end_arrowhead.get(),
            arrow_type: arrow_type.get(),
            is_polygon: is_polygon.get(),
            font_size: font_size.get(),
            font_family: font_family.get(),
            text_align: text_align.get(),
            vertical_align: vertical_align.get().flatten(),
            has_free_text,
            auto_resize: auto_resize.get(),
            has_label,
            label_wrap: label_wrap.get(),
            can_bind_text: self.bind_pair(&selected, &carried).is_some(),
            can_unbind_text,
            figure_kind,
            figure_sides: figure_sides.get(),
            figure_ratio: figure_ratio.get(),
            figure_has_sides: figure_kind.is_some_and(figure::has_sides),
            figure_has_ratio: figure_kind.is_some_and(figure::has_ratio),
            stroke_domain: self.color_domain(false),
            background_domain: self.color_domain(true),
        }
    }

    /// With nothing selected the panel sets up the next element, so it shows that.
    fn next_selection_style(&self) -> SelectionStyle {
        let mut next = self.get_next_style();
        let sticky = self.tool == crate::interaction::DrawTool::StickyNote;
        if sticky {
            next.stroke_color.clone_from(&self.next_sticky_stroke);
            next.background_color
                .clone_from(&self.next_sticky_background);
        }
        let domain = if sticky {
            ColorDomain::Sticky
        } else {
            ColorDomain::Regular
        };
        SelectionStyle {
            count: 0,
            kinds: Vec::new(),
            filled_kinds: Vec::new(),
            text_alignable: false,
            vertical_alignable: false,
            can_align: false,
            can_distribute: false,
            // Nothing is selected, and the toggle only ever acts on selected lines — the
            // line tool being merely active offers no polygon to close.
            can_toggle_polygon: false,
            is_group: false,
            stroke_color: Some(next.stroke_color),
            background_color: Some(next.background_color),
            fill_style: Some(next.fill_style),
            stroke_width: Some(next.stroke_width),
            stroke_style: Some(next.stroke_style),
            roughness: Some(next.roughness),
            opacity: Some(next.opacity),
            edges: Some(Edges::of(next.roundness)),
            // Unchosen, the heads a new arrow resolves to (`render::default_arrowhead`).
            start_arrowhead: Some(self.next_start_arrowhead.unwrap_or(Arrowhead::None)),
            end_arrowhead: Some(self.next_end_arrowhead.unwrap_or(Arrowhead::Arrow)),
            arrow_type: Some(self.next_arrow_type),
            is_polygon: None,
            font_size: Some(self.next_font_size),
            font_family: Some(self.next_font_family),
            text_align: Some(self.next_text_align.unwrap_or(TextAlign::Left)),
            vertical_align: Some(self.next_vertical_align.unwrap_or(VerticalAlign::Middle)),
            has_free_text: false,
            auto_resize: None,
            has_label: false,
            label_wrap: None,
            can_bind_text: false,
            can_unbind_text: false,
            figure_kind: Some(self.next_figure.kind),
            figure_sides: Some(figure::resolved_sides(
                self.next_figure.kind,
                self.next_figure.sides,
            )),
            figure_ratio: Some(figure::resolved_ratio(
                self.next_figure.kind,
                self.next_figure.ratio,
            )),
            figure_has_sides: figure::has_sides(self.next_figure.kind),
            figure_has_ratio: figure::has_ratio(self.next_figure.kind),
            stroke_domain: domain,
            background_domain: domain,
        }
    }

    /// Whether `element` takes a note's colours: a note, or a note's label.
    fn is_sticky_color_target(&self, element: &DrawElement) -> bool {
        crate::scene::sticky::is_sticky_note(element)
            || (element.kind == DrawElementType::Text
                && self
                    .container_of(element)
                    .is_some_and(crate::scene::sticky::is_sticky_note))
    }

    /// Whose colours a pick sets (`resolveColorTarget`, `actions/colorTargets.ts@1118751f:89-176`):
    /// the elements a style may change that take the property — for a stroke, the labels
    /// selected shapes carry too, since a note's visible text is its label; for a
    /// background, a note's label passes the pick on to its note. With none, the tool
    /// being drawn with decides.
    fn color_domain(&self, background: bool) -> ColorDomain {
        let carried = std::cell::OnceCell::new();
        let (mut sticky, mut regular) = (false, false);
        let mut count = |element: &DrawElement| {
            let supported = if background {
                has_background(element.kind)
            } else {
                has_stroke_color(element.kind)
            };
            if supported {
                if self.is_sticky_color_target(element) {
                    sticky = true;
                } else {
                    regular = true;
                }
            }
        };
        for element in self.panel_selection() {
            if !self.restylable(element, &carried) {
                continue;
            }
            match self.color_target(element, background) {
                Some(target) => count(target),
                None => count(element),
            }
            if !background {
                if let Some(label) = self.label_of(element, &carried) {
                    count(label);
                }
            }
        }
        match (sticky, regular) {
            (true, false) => ColorDomain::Sticky,
            (true, true) => ColorDomain::Mixed,
            (false, true) => ColorDomain::Regular,
            (false, false) if self.tool == crate::interaction::DrawTool::StickyNote => {
                ColorDomain::Sticky
            }
            (false, false) => ColorDomain::Regular,
        }
    }

    /// Where a colour pick on `element` lands when that is somewhere else
    /// (`getColorTargetElement`, `stickyNote.ts@1118751f:95-108`): a note's label has no
    /// fill of its own, so a background pick on it goes to its note.
    fn color_target(&self, element: &DrawElement, background: bool) -> Option<&DrawElement> {
        if !background || element.kind != DrawElementType::Text {
            return None;
        }
        self.container_of(element)
            .filter(|container| crate::scene::sticky::is_sticky_note(container))
    }

    /// Writes a pick into the next element's colours, in the domain it targets
    /// (`getColorTargetAppStateUpdates`, `colorTargets.ts@1118751f:178-192`): a note's
    /// never transparent. Returns the patch left for the regular next style.
    fn write_next_colors(
        &mut self,
        mut patch: DrawElementStylePatch,
        stroke: ColorDomain,
        background: ColorDomain,
    ) -> DrawElementStylePatch {
        use crate::scene::sticky::{normalize_sticky_background, normalize_sticky_stroke};
        if let Some(color) = patch.stroke_color.as_deref() {
            if stroke != ColorDomain::Regular {
                self.next_sticky_stroke = normalize_sticky_stroke(color);
            }
            if stroke == ColorDomain::Sticky {
                patch.stroke_color = None;
            }
        }
        if let Some(color) = patch.background_color.as_deref() {
            if background != ColorDomain::Regular {
                self.next_sticky_background = normalize_sticky_background(color);
            }
            if background == ColorDomain::Sticky {
                patch.background_color = None;
            }
        }
        patch
    }

    /// `syncStickyNoteInk` (`stickyNote.ts@1118751f:146-190`) after a write: a note and
    /// its label end with one ink. `before` is each written element's stroke as it was.
    pub(super) fn sync_sticky_ink(&mut self, before: &HashMap<String, String>) {
        let mut notes: Vec<String> = Vec::new();
        for id in before.keys() {
            let Some(element) = self.scene.get(id).filter(|el| !el.is_deleted) else {
                continue;
            };
            let note = if crate::scene::sticky::is_sticky_note(element) {
                Some(element.id.clone())
            } else {
                self.container_of(element)
                    .filter(|container| crate::scene::sticky::is_sticky_note(container))
                    .map(|container| container.id.clone())
            };
            if let Some(note) = note.filter(|note| !notes.contains(note)) {
                notes.push(note);
            }
        }
        notes.sort();
        for id in notes {
            let Some(note) = self.scene.get(&id).cloned() else {
                continue;
            };
            let Some(label) = self.live_label(&note).cloned() else {
                continue;
            };
            let Some(ink) = crate::scene::sticky::synced_ink(
                before.get(&note.id).map(String::as_str),
                &note.stroke_color,
                before.get(&label.id).map(String::as_str),
                &label.stroke_color,
            ) else {
                continue;
            };
            for mut element in [note, label] {
                if element.stroke_color != ink {
                    element.stroke_color.clone_from(&ink);
                    self.scene.put(element);
                }
            }
        }
    }

    /// Who a style patch reaches, and with what.
    ///
    /// Every selected element a style may change ([`Self::restylable`]) takes the whole
    /// patch. The label of a selected shape takes the stroke colour and the opacity and
    /// nothing else — the two properties whose oracle actions pass `includeBoundText`
    /// (`actionProperties.tsx@1118751f:381-397`, `:962-970`); a label has no use for a
    /// fill, a width, a dash or a sloppiness. So does a label selected along with its
    /// shape: see [`Self::is_carried_label`]. A label a peer is typing into, or a locked
    /// one, is not reached ([`Self::label_of`]).
    fn style_targets(
        &self,
        patch: &DrawElementStylePatch,
    ) -> Vec<(DrawElement, DrawElementStylePatch)> {
        let for_label = DrawElementStylePatch {
            stroke_color: patch.stroke_color.clone(),
            opacity: patch.opacity,
            ..Default::default()
        };
        // Font fields are never carried by `style_targets`' patches at all — a shape has
        // none to apply them to, and a label's come through `apply_style`'s own font pass
        // below, which reaches the same set (the selected texts and the labels of
        // selected shapes) through `write_relayout_selected_texts` instead, because a font
        // change needs a relayout `apply_style_patch` cannot give it.
        let reaches_labels = for_label.stroke_color.is_some() || for_label.opacity.is_some();
        let carried = std::cell::OnceCell::new();
        let mut selected = self.get_selected_elements();
        selected.retain(|element| {
            !self.is_carried_label(element) && self.restylable(element, &carried)
        });
        let ids: HashSet<String> = selected.iter().map(|element| element.id.clone()).collect();
        let mut labels = Vec::new();
        if reaches_labels {
            for element in &selected {
                if let Some(label) = self.label_of(element, &carried) {
                    if !ids.contains(&label.id) {
                        labels.push((label.clone(), for_label.clone()));
                    }
                }
            }
        }
        // A background pick on a note's label is its note's (`getColorTargetElement`).
        let mut redirected: Vec<(DrawElement, DrawElementStylePatch)> = Vec::new();
        let selected: Vec<(DrawElement, DrawElementStylePatch)> = selected
            .into_iter()
            .map(|element| {
                let mut own = patch.clone();
                if let Some(note) = self.color_target(&element, own.background_color.is_some()) {
                    if !ids.contains(&note.id) && !redirected.iter().any(|(el, _)| el.id == note.id)
                    {
                        redirected.push((
                            note.clone(),
                            DrawElementStylePatch {
                                background_color: own.background_color.clone(),
                                ..Default::default()
                            },
                        ));
                    }
                    own.background_color = None;
                }
                (element, own)
            })
            .collect();
        selected
            .into_iter()
            .chain(labels)
            .chain(redirected)
            .collect()
    }

    /// Puts a patched element back when the patch changed it; says whether it did.
    fn put_styled(&mut self, mut element: DrawElement, patch: &DrawElementStylePatch) -> bool {
        let before = element.clone();
        apply_style_patch(&mut element, patch);
        // A note keeps its invariants whatever was written: never transparent, always
        // solid (`changeProperty`, `actionProperties.tsx@1118751f:218-223`).
        normalize_sticky_style(&mut element);
        if element == before {
            return false;
        }
        self.scene.put(element);
        true
    }

    /// Applies a style to the selection — and to the labels it carries, see
    /// [`Self::style_targets`] — as one step of undo, and makes it the next element's
    /// style as well. With nothing selected it is only the next element's.
    ///
    /// An element the patch does not change is left alone, stamp and all: the oracle's
    /// `newElementWith` returns it untouched (`packages/element/src/mutateElement.ts@1118751f:170-172`).
    /// The commit is still taken, because a preview may have moved the elements already
    /// — see [`Self::preview_style`].
    pub fn apply_style(&mut self, patch: DrawElementStylePatch) {
        // A preset's font, always written to the next text's style whatever is selected —
        // `set_font_size`/`set_font_family`/`set_text_align` (`style.rs`) do the same
        // unconditionally, and a preset is nothing more than several of those rows chosen
        // together.
        if let Some(size) = patch.font_size.and_then(super::style::storable_font_size) {
            self.next_font_size = size;
        }
        if let Some(family) = patch
            .font_family
            .filter(|id| crate::text::font::family(*id).is_some())
        {
            self.next_font_family = family;
        }
        if let Some(align) = patch.text_align {
            self.next_text_align = Some(align);
        }
        // The Shapes tool's own queued kind follows the pick too, exactly as
        // `set_arrow_type` moves `next_arrow_type` — see `set_next_figure`.
        if let Some(kind) = patch.figure_kind {
            figure::change_kind(&mut self.next_figure, kind);
        }
        self.touch_style();

        // Resolved against the state the pick lands on, before it changes anything.
        let (stroke, background) = (self.color_domain(false), self.color_domain(true));
        let targets = self.style_targets(&patch);
        let has_font =
            patch.font_size.is_some() || patch.font_family.is_some() || patch.text_align.is_some();
        if targets.is_empty() && !has_font {
            let rest = self.write_next_colors(patch, stroke, background);
            self.set_next_style(rest);
            return;
        }
        let before: HashMap<String, String> = targets
            .iter()
            .map(|(element, _)| (element.id.clone(), element.stroke_color.clone()))
            .collect();
        // The figures a kind, sides or ratio change reaches: `figure_text_inset_fraction`
        // (`text/layout.rs`) depends on all three, so a label's inner box moves with them,
        // and an arrow bound to the outline needs it re-resolved (`relayout_figure_labels`,
        // `apply_bindings` below).
        let figure_ids = figure_target_ids(&targets, &patch);
        for (element, patch) in targets {
            self.put_styled(element, &patch);
        }
        if patch.stroke_color.is_some() {
            self.sync_sticky_ink(&before);
        }
        // The font, laid out again: the selected texts and the labels of selected shapes,
        // as ONE step of undo together with the rest of the patch above — `commit_style`
        // below is the only commit either makes.
        self.apply_style_font(&patch);
        self.relayout_figure_labels(&figure_ids, false);
        if !figure_ids.is_empty() {
            self.apply_bindings();
        }
        // The next element takes the style too, as the oracle's `currentItem*` do
        // (`actionProperties.tsx@1118751f:622`, `:971`; `colorTargets.ts@1118751f:178-192`).
        let rest = self.write_next_colors(patch, stroke, background);
        self.next_style = super::merge_style_patch(&self.next_style, &rest);
        self.commit_style();
        self.request_draw();
    }

    /// The font half of [`Self::apply_style`]: writes whichever of `patch`'s
    /// `font_family`/`font_size`/`text_align` are present into the selected texts and the
    /// labels of selected shapes, laid out again the way `set_font_size`, `set_font_family`
    /// and `set_text_align` do (`style.rs`) — reusing that same relayout write rather than
    /// a second one. No-op, and no relayout at all, when the patch carries none of them.
    fn apply_style_font(&mut self, patch: &DrawElementStylePatch) {
        let clamped_size = patch.font_size.and_then(super::style::storable_font_size);
        let resolved_family = patch.font_family.and_then(|family| {
            crate::text::font::family(family).map(|font| (family, font.line_height))
        });
        let text_align = patch.text_align;
        if clamped_size.is_none() && resolved_family.is_none() && text_align.is_none() {
            return;
        }
        let sticky = self.sticky_labels();
        let anchor_font_resize = clamped_size.is_some();
        self.write_relayout_selected_texts(
            |text| {
                if let Some(size) = clamped_size {
                    super::style::set_user_font_size(text, size, sticky.contains(&text.id));
                }
                if let Some((family, line_height)) = resolved_family {
                    text.font_family = Some(family);
                    text.line_height = Some(line_height);
                }
                if let Some(align) = text_align {
                    text.text_align = Some(align);
                }
            },
            anchor_font_resize,
        );
        self.refloor_text_session();
    }

    /// Shows a style on the canvas without committing it: nothing stamped, nothing sent,
    /// no step of undo. The opacity slider previews on every move and commits once with
    /// [`Self::apply_style`] on release; the commit compares against what was there
    /// before the first preview, so the whole drag is one step. What a peer takes in the
    /// meantime is given back to them (`peers.rs`).
    pub fn preview_style(&mut self, patch: DrawElementStylePatch) {
        let touches_figure_geometry = patch.figure_kind.is_some()
            || patch.figure_sides.is_some()
            || patch.figure_ratio.is_some();
        let mut changed = false;
        let mut figure_ids: Vec<String> = Vec::new();
        for (element, patch) in self.style_targets(&patch) {
            let id = element.id.clone();
            let is_figure = touches_figure_geometry && element.kind == DrawElementType::Figure;
            if self.put_styled(element, &patch) {
                self.style_preview.entry(id.clone()).or_insert(None);
                changed = true;
                if is_figure {
                    figure_ids.push(id);
                }
            }
        }
        // The ratio slider previews live: its label and any bound arrow follow the
        // outline mid-drag too, the same way the commit below makes them.
        if !figure_ids.is_empty() {
            self.relayout_figure_labels(&figure_ids, true);
            self.apply_bindings();
        }
        if changed {
            self.touch_style();
            self.request_draw();
        }
    }

    /// Relays out the label of each figure in `figure_ids` — [`Self::apply_style`]'s and
    /// [`Self::preview_style`]'s own figures whose kind, sides or ratio just changed — the
    /// way [`Self::bind_text`]'s own `laid_out`/`put_laid` relays one out for a container
    /// change. `preview` registers a label this touches the way the caller already
    /// registers the figure ([`Self::preview_style`]), so a cancelled preview gives it
    /// back too ([`Self::give_back_style_preview`]).
    fn relayout_figure_labels(&mut self, figure_ids: &[String], preview: bool) {
        for id in figure_ids {
            let Some(label) = self
                .scene
                .get(id)
                .and_then(|figure| self.live_label(figure))
                .cloned()
            else {
                continue;
            };
            let laid = self.laid_out(&label);
            if laid.text == label && laid.container.is_none() {
                continue;
            }
            if preview {
                self.style_preview.entry(label.id.clone()).or_insert(None);
            }
            self.put_laid(laid);
        }
    }

    /// Remembers the style of the first selected element in stacking order, and of the
    /// label it carries (`actions/actionStyles.ts@1118751f:56-77`). Says whether there
    /// was anything to copy.
    pub fn copy_styles(&mut self) -> bool {
        let Some(first) = self
            .scene
            .iter_ordered()
            .find(|element| self.selected_ids.contains(&element.id))
        else {
            return false;
        };
        let mut copied = vec![first.clone()];
        if let Some(label) = self.live_label(first) {
            copied.push(label.clone());
        }
        // A picture is not a style, and can be megabytes.
        for element in &mut copied {
            element.data_url = None;
        }
        self.copied_styles = Some(copied);
        true
    }

    /// Pastes the copied style onto the selection and its labels, as one step of undo
    /// (`actions/actionStyles.ts@1118751f:87-236`).
    ///
    /// A label takes the copied label's style and is left alone when none was copied.
    /// A text also takes the font — or the defaults, when the source has none — and is
    /// measured again. An arrow takes the heads of an arrow; a frame stays clear and
    /// square. What a style may not change keeps its own ([`Self::restylable`]): a loose
    /// locked element, what a peer holds, and their labels.
    pub fn paste_styles(&mut self) {
        let Some(copied) = self.copied_styles.clone() else {
            return;
        };
        let Some(shape_source) = copied.first() else {
            return;
        };
        let label_source = copied.get(1);

        let carried = std::cell::OnceCell::new();
        let mut targets = self.get_selected_elements();
        targets.retain(|element| self.restylable(element, &carried));
        let ids: HashSet<String> = targets.iter().map(|element| element.id.clone()).collect();
        let labels: Vec<DrawElement> = targets
            .iter()
            .filter_map(|element| self.label_of(element, &carried))
            .filter(|label| !ids.contains(&label.id))
            .cloned()
            .collect();
        targets.extend(labels);
        if targets.is_empty() {
            return;
        }
        // Shapes before labels: a label is laid out in its shape as pasted, and a shape
        // its label grew must not then be put back from the copy taken here.
        targets.sort_by_key(|element| element.container_id.is_some());
        let before: HashMap<String, String> = targets
            .iter()
            .map(|element| (element.id.clone(), element.stroke_color.clone()))
            .collect();

        for mut element in targets {
            let is_label = element.kind == DrawElementType::Text && element.container_id.is_some();
            let from = if is_label {
                match label_source {
                    Some(label) => label,
                    None => continue,
                }
            } else {
                shape_source
            };
            let before = element.clone();
            element.background_color.clone_from(&from.background_color);
            element.stroke_width = from.stroke_width;
            element.stroke_color.clone_from(&from.stroke_color);
            element.stroke_style = from.stroke_style;
            element.fill_style = from.fill_style;
            element.opacity = from.opacity;
            element.roughness = from.roughness;
            let is_text = element.kind == DrawElementType::Text;
            if is_text {
                // A text keeps its corners, which it does not paint: the engine makes one
                // with the default style's, where the oracle's has none
                // (`packages/element/src/newElement.ts@1118751f:105`). Cleared, and with
                // the size and alignment written down whenever they were unset, every
                // paste onto a text was an edit — the paste of its own style included.
                // So those two are written only when they differ as read.
                let from_text = from.kind == DrawElementType::Text;
                // A copied note's label gives its ceiling — decided by the copy, whose
                // note may be gone from the board by now (`actionStyles.ts@1118751f:100-104`,
                // `:139-142`).
                let copied_container = from
                    .container_id
                    .as_deref()
                    .and_then(|id| copied.iter().find(|el| el.id == id));
                let size = if from_text && from.font_size.is_some() {
                    crate::scene::sticky::label_ceiling(from, copied_container)
                } else {
                    super::DEFAULT_FONT_SIZE
                };
                let on_note = self
                    .container_of(&element)
                    .is_some_and(crate::scene::sticky::is_sticky_note);
                if on_note {
                    // A note's label takes the size as its ceiling (`getBaseFontSizeUpdate`),
                    // and never goes transparent: its note's ink stands in (`:160-170`).
                    let ceiling = crate::scene::sticky::normalize_sticky_font_size(size);
                    if element.base_font_size != Some(ceiling) {
                        element.base_font_size = Some(ceiling);
                    }
                    if is_transparent(&element.stroke_color) {
                        element.stroke_color = crate::scene::sticky::normalize_sticky_stroke(
                            self.container_of(&element)
                                .map_or("", |note| note.stroke_color.as_str()),
                        );
                    }
                } else if element.font_size.unwrap_or(super::DEFAULT_FONT_SIZE) != size {
                    element.font_size = Some(size);
                }
                // `sourceText.fontFamily || DEFAULT_FONT_FAMILY` (`:143`): a text's own
                // family, or the one new text is written in. Every text of the oracle's
                // has a family; one of ours with none is drawn in the system stack, which
                // is its family and crosses as it is — so the twin of a legacy text is
                // not moved to the default.
                element.font_family = if from_text {
                    from.font_family
                } else {
                    Some(crate::text::font::DEFAULT_FONT_FAMILY)
                };
                let align = if from_text {
                    resolved_text_align(from)
                } else {
                    TextAlign::Left
                };
                if resolved_text_align(&element) != align {
                    element.text_align = Some(align);
                }
                // `sourceText.lineHeight || getLineHeight(fontFamily)` (`:157`): from a
                // shape, the default family's own. Left unset, a text the text tool made
                // lost the 1.25 it was written with, and the paste of a shape's style
                // onto it was an edit that changed nothing on screen.
                element.line_height = if from_text {
                    from.line_height
                } else {
                    crate::text::font::family(crate::text::font::DEFAULT_FONT_FAMILY)
                        .map(|family| family.line_height)
                };
            } else {
                element.roundness = from.roundness.filter(|_| takes_roundness(element.kind));
            }
            if element.kind == DrawElementType::Arrow && from.kind == DrawElementType::Arrow {
                element.start_arrowhead = from.start_arrowhead;
                element.end_arrowhead = from.end_arrowhead;
            }
            if element.kind == DrawElementType::Frame {
                element.roundness = None;
                element.background_color = "transparent".into();
            }
            normalize_sticky_style(&mut element);
            if element == before {
                continue;
            }
            if is_text {
                // A new size or font is a new box, and a label that no longer fits grows
                // its shape: `redrawTextBoundingBox(newTextElement, container)` (`:174`).
                let laid = self.laid_out(&element);
                self.put_laid(laid);
            } else {
                self.scene.put(element);
            }
        }
        // A restyled note and its label end with one ink — the label's, when the copy
        // carried two (`actionStyles.ts@1118751f:106-108`).
        self.sync_sticky_ink(&before);
        self.apply_bindings();
        self.commit_style();
        self.request_draw();
    }

    /// How many live elements use each stroke (or background) colour — what the colour
    /// picker's "most used custom colours" is chosen from (`getMostUsedCustomColors`,
    /// `components/ColorPicker/colorPickerUtils.ts@1118751f:57-94`). Counted over the
    /// board, so the host asks when a picker opens, not per revision.
    pub fn color_counts(&self, background: bool) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        let mut index: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for element in self.scene.iter_ordered() {
            let color = if background {
                element.background_color.as_str()
            } else {
                element.stroke_color.as_str()
            };
            match index.get(color) {
                Some(&at) => counts[at].1 += 1,
                None => {
                    index.insert(color, counts.len());
                    counts.push((color.to_owned(), 1));
                }
            }
        }
        counts
    }
}

/// `normalizeStickyNoteStyle` (`packages/element/src/newElement.ts@1118751f:186-197`): a
/// note's colours are never transparent and its fill is always solid. Anything else is
/// left as it is.
pub(crate) fn normalize_sticky_style(element: &mut DrawElement) {
    use crate::scene::sticky::{normalize_sticky_background, normalize_sticky_stroke};
    if !crate::scene::sticky::is_sticky_note(element) {
        return;
    }
    let stroke = normalize_sticky_stroke(&element.stroke_color);
    if stroke != element.stroke_color {
        element.stroke_color = stroke;
    }
    let background = normalize_sticky_background(&element.background_color);
    if background != element.background_color {
        element.background_color = background;
    }
    element.fill_style = FillStyle::Solid;
}
