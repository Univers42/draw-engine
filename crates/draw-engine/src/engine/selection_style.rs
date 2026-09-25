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

use std::collections::HashSet;

use serde::Serialize;

use super::DrawEngine;
use crate::render::default_arrowhead;
use crate::scene::{
    apply_style_patch, is_transparent, resolved_text_align, resolved_vertical_align, Arrowhead,
    DrawElement, DrawElementStylePatch, DrawElementType, FillStyle, StrokeStyle, TextAlign,
    VerticalAlign,
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
    pub font_size: Option<f64>,
    pub text_align: Option<TextAlign>,
    pub vertical_align: Option<VerticalAlign>,
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

impl DrawEngine {
    /// Moves the counter the host keys the panel to. Wrapping: only inequality matters.
    pub(super) fn touch_style(&mut self) {
        self.style_revision = self.style_revision.wrapping_add(1);
    }

    /// Changes whenever [`Self::selection_style`] may have. See the module comment.
    pub fn style_revision(&self) -> u32 {
        self.style_revision
    }

    /// The label a shape carries, if it is live.
    fn live_label(&self, element: &DrawElement) -> Option<&DrawElement> {
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

    /// Everything the panel shows, in one pass over the selection.
    pub fn selection_style(&self) -> SelectionStyle {
        let selected: Vec<&DrawElement> = self
            .selected_ids
            .iter()
            .filter_map(|id| self.scene.get(id))
            .filter(|element| !element.is_deleted && !self.is_carried_label(element))
            .collect();
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
        let mut font_size = Common::Empty;
        let mut text_align = Common::Empty;
        let mut vertical_align = Common::Empty;

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
            stroke_width.add(element.stroke_width);
            stroke_style.add(element.stroke_style);
            roughness.add(element.roughness);
            opacity.add(element.opacity);
            if element.kind == DrawElementType::Arrow {
                start_arrowhead.add(default_arrowhead(element, "start"));
                end_arrowhead.add(default_arrowhead(element, "end"));
            } else {
                edges.add(Edges::of(element.roundness));
            }
            // The text rows read the text a selected element *is*, or the label it
            // carries (`:1051-1070`, `:1611-1630`, `:1709-1728`).
            let text = if element.kind == DrawElementType::Text {
                Some(*element)
            } else {
                self.label_of(element, &carried)
            };
            if let Some(text) = text {
                font_size.add(text.font_size.unwrap_or(super::DEFAULT_FONT_SIZE));
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
        SelectionStyle {
            count: selected.len(),
            kinds,
            filled_kinds,
            text_alignable,
            vertical_alignable,
            can_align: units > 1,
            can_distribute: units > 2,
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
            font_size: font_size.get(),
            text_align: text_align.get(),
            vertical_align: vertical_align.get().flatten(),
        }
    }

    /// With nothing selected the panel sets up the next element, so it shows that.
    fn next_selection_style(&self) -> SelectionStyle {
        let next = self.get_next_style();
        SelectionStyle {
            count: 0,
            kinds: Vec::new(),
            filled_kinds: Vec::new(),
            text_alignable: false,
            vertical_alignable: false,
            can_align: false,
            can_distribute: false,
            is_group: false,
            stroke_color: Some(next.stroke_color),
            background_color: Some(next.background_color),
            fill_style: Some(next.fill_style),
            stroke_width: Some(next.stroke_width),
            stroke_style: Some(next.stroke_style),
            roughness: Some(next.roughness),
            opacity: Some(next.opacity),
            edges: Some(Edges::of(next.roundness)),
            // The engine keeps no next arrowheads: a new arrow gets the defaults.
            start_arrowhead: Some(Arrowhead::None),
            end_arrowhead: Some(Arrowhead::Arrow),
            font_size: Some(self.next_font_size),
            text_align: Some(self.next_text_align.unwrap_or(TextAlign::Left)),
            vertical_align: Some(self.next_vertical_align.unwrap_or(VerticalAlign::Middle)),
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
        let reaches_labels = for_label.stroke_color.is_some() || for_label.opacity.is_some();
        let carried = std::cell::OnceCell::new();
        let mut selected = self.get_selected_elements();
        selected.retain(|element| {
            !self.is_carried_label(element) && self.restylable(element, &carried)
        });
        let ids: HashSet<&str> = selected.iter().map(|element| element.id.as_str()).collect();
        let mut labels = Vec::new();
        if reaches_labels {
            for element in &selected {
                if let Some(label) = self.label_of(element, &carried) {
                    if !ids.contains(label.id.as_str()) {
                        labels.push((label.clone(), for_label.clone()));
                    }
                }
            }
        }
        selected
            .into_iter()
            .map(|element| (element, patch.clone()))
            .chain(labels)
            .collect()
    }

    /// Puts a patched element back when the patch changed it; says whether it did.
    fn put_styled(&mut self, mut element: DrawElement, patch: &DrawElementStylePatch) -> bool {
        let before = element.clone();
        apply_style_patch(&mut element, patch);
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
        let targets = self.style_targets(&patch);
        if targets.is_empty() {
            self.set_next_style(patch);
            return;
        }
        for (element, patch) in targets {
            self.put_styled(element, &patch);
        }
        // The next element takes the style too, as the oracle's `currentItem*` do
        // (`actionProperties.tsx@1118751f:622`, `:971`; `colorTargets.ts@1118751f:178-192`).
        self.next_style = super::merge_style_patch(&self.next_style, &patch);
        self.push_history();
        self.request_draw();
    }

    /// Shows a style on the canvas without committing it: nothing stamped, nothing sent,
    /// no step of undo. The opacity slider previews on every move and commits once with
    /// [`Self::apply_style`] on release; the commit compares against what was there
    /// before the first preview, so the whole drag is one step. What a peer takes in the
    /// meantime is given back to them (`peers.rs`).
    pub fn preview_style(&mut self, patch: DrawElementStylePatch) {
        let mut changed = false;
        for (element, patch) in self.style_targets(&patch) {
            let id = element.id.clone();
            if self.put_styled(element, &patch) {
                self.style_preview.insert(id);
                changed = true;
            }
        }
        if changed {
            self.touch_style();
            self.request_draw();
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
                let size = from
                    .font_size
                    .filter(|_| from_text)
                    .unwrap_or(super::DEFAULT_FONT_SIZE);
                if element.font_size.unwrap_or(super::DEFAULT_FONT_SIZE) != size {
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
                element.line_height = from.line_height.filter(|_| from_text);
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
            if element == before {
                continue;
            }
            if is_text {
                // A new size or font is a new box, and a label that no longer fits grows
                // its shape: `redrawTextBoundingBox(newTextElement, container)` (`:174`).
                let laid = self.laid_out(&element);
                if let Some(container) = laid.container {
                    self.scene.put(container);
                }
                self.scene.put(laid.text);
            } else {
                self.scene.put(element);
            }
        }
        self.apply_bindings();
        self.push_history();
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
