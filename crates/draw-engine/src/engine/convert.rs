//! Tab and Shift+Tab: the selected shapes switch between rectangle, diamond and ellipse,
//! and the selected lines and arrows between line, sharp, curved and elbow — both branches
//! of Excalidraw's shape switch
//! (`packages/excalidraw/components/ConvertElementTypePopup.tsx@1118751f`).
//!
//! The first Tab only opens the switch and the ones after it switch
//! (`App.tsx@1118751f:5543-5572`). The switch is the host's to show, so the host holds
//! whether it is open. It tells the engine when it opens and closes, because a label shrunk
//! to fit a diamond grows back to its own size when the shape comes round to a rectangle
//! again, and a line switched to an elbow and back is the line it was — but both only while
//! that switch stays open (`FONT_SIZE_CONVERSION_CACHE`, `LINEAR_ELEMENT_CONVERSION_CACHE`).
//!
//! The generic branch has preference: a selection holding a closed shape switches the
//! closed shapes and leaves any line in it alone
//! (`ConvertElementTypePopup.tsx@1118751f:648-653`).

use std::collections::HashMap;

use crate::engine::{ArrowType, DrawEngine};
use crate::scene::binding::reanchor_to_outline;
use crate::scene::element::{Arrowhead, DrawElement, DrawElementType};
use crate::text::layout::fitting_font_size;
use crate::DrawTool;

/// `GENERIC_TYPES`, in the order Tab walks them.
const GENERIC_TYPES: [DrawElementType; 3] = [
    DrawElementType::Rectangle,
    DrawElementType::Diamond,
    DrawElementType::Ellipse,
];

/// `LINEAR_TYPES`, in the order Tab walks them
/// (`ConvertElementTypePopup.tsx@1118751f:113-120`).
const LINEAR_TYPES: [LinearType; 4] = [
    LinearType::Line,
    LinearType::SharpArrow,
    LinearType::CurvedArrow,
    LinearType::ElbowArrow,
];

fn is_generic(element: &DrawElement) -> bool {
    GENERIC_TYPES.contains(&element.kind)
}

/// One of the four a line or an arrow switches between, as the oracle's `LINEAR_TYPES`
/// names them.
///
/// Three of the four are the same element type — an arrow — and differ only in `roundness`
/// and `elbowed`, which is how the oracle reads them too
/// (`packages/element/src/typeChecks.ts@1118751f:375-389`). So this is the switch's own
/// vocabulary, layered over [`ArrowType`] rather than a second reading of those two fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinearType {
    Line,
    SharpArrow,
    CurvedArrow,
    ElbowArrow,
}

impl LinearType {
    /// Which of the four `element` is now — `getLinearElementSubType`. A line is a line
    /// whatever its roundness: the oracle's three checks all ask `isArrowElement` first
    /// (`packages/element/src/typeChecks.ts@1118751f:376-387`), so a curved arrow switched
    /// to a line stays a *curved line* rather than reading back as a curved arrow.
    pub fn of(element: &DrawElement) -> Self {
        if element.kind == DrawElementType::Line {
            return Self::Line;
        }
        match ArrowType::of(element) {
            ArrowType::Sharp => Self::SharpArrow,
            ArrowType::Round => Self::CurvedArrow,
            ArrowType::Elbow => Self::ElbowArrow,
        }
    }

    /// The name the oracle's panel and the host's strip use for it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::SharpArrow => "sharpArrow",
            Self::CurvedArrow => "curvedArrow",
            Self::ElbowArrow => "elbowArrow",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        LINEAR_TYPES.into_iter().find(|kind| kind.name() == name)
    }

    /// The element type it becomes. Only `line` is not an arrow.
    fn element_type(self) -> DrawElementType {
        match self {
            Self::Line => DrawElementType::Line,
            Self::SharpArrow | Self::CurvedArrow | Self::ElbowArrow => DrawElementType::Arrow,
        }
    }

    /// The roundness it is drawn with, `None` to leave the element's alone.
    ///
    /// The `line` case is the oracle's `newLinearElement` keeping whatever the spread
    /// carried (`packages/element/src/newElement.ts@1118751f:572-599`), against
    /// `newArrowElement` naming the roundness outright (`:617-634`,
    /// `ConvertElementTypePopup.tsx@1118751f:884-919`).
    fn roundness(self) -> Option<Option<f64>> {
        match self {
            Self::Line => None,
            Self::CurvedArrow => Some(ArrowType::Round.roundness()),
            Self::SharpArrow | Self::ElbowArrow => Some(ArrowType::Sharp.roundness()),
        }
    }
}

/// What the switch was asked for, as one of the names the oracle's panel offers
/// (`ConvertElementTypePopup.tsx@1118751f:113-120`). `None` means a step round, which is
/// what Tab and Shift+Tab ask for.
///
/// A conversion never crosses the two families — `isValidConversion`
/// (`ConvertElementTypePopup.tsx@1118751f:929-950`) — so the two spellings cannot be
/// confused for one another, and a [`LinearType`] that is not one of the four above is not
/// a type the switch can reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvertTo {
    Generic(DrawElementType),
    Linear(LinearType),
}

impl ConvertTo {
    /// The name the oracle's panel and the host's strip use for it, the inverse of
    /// [`Self::parse`]. The closed shapes spell theirs as their element type's own serde
    /// name (`ConvertElementTypePopup.tsx@1118751f:113-115`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Generic(DrawElementType::Rectangle) => "rectangle",
            Self::Generic(DrawElementType::Diamond) => "diamond",
            Self::Generic(DrawElementType::Ellipse) => "ellipse",
            Self::Generic(_) => "",
            Self::Linear(kind) => kind.name(),
        }
    }

    /// Every type the switch can be asked for, in the order the two families are offered.
    pub fn all() -> impl Iterator<Item = Self> {
        GENERIC_TYPES
            .into_iter()
            .map(Self::Generic)
            .chain(LINEAR_TYPES.into_iter().map(Self::Linear))
    }

    /// The oracle's two names, the closed shapes and the four linear types.
    pub fn parse(name: &str) -> Option<Self> {
        Self::all().find(|kind| kind.name() == name)
    }
}

/// `isEligibleLinearElement` (`ConvertElementTypePopup.tsx@1118751f:666-672`): a line
/// always, an arrow only while it is bound to nothing and carries no label.
///
/// **Differs from the oracle in one case:** the oracle lets a *line* that carries bindings
/// through, because nothing here can give a line one — `drop_binding` answers `None` for
/// anything that is not an arrow (`engine/pointer_move.rs@1118751f:461`) — so its
/// constructor's `startBinding: null` is writing `null` over `null`. A board can still
/// hold one, loaded from a file, and switching it would drop a binding no shape would hear
/// about and leave a `boundElements` entry pointing at an arrow that is not bound to
/// anything. So we refuse it, as we refuse a bound arrow, rather than orphan it.
fn is_eligible_linear(element: &DrawElement) -> bool {
    let unbound = element.start_binding.is_none()
        && element.end_binding.is_none()
        && element.bound_text_id.is_none();
    match element.kind {
        DrawElementType::Line => unbound,
        DrawElementType::Arrow => unbound,
        _ => false,
    }
}

/// The type Tab (`forward`) or Shift+Tab switches `shapes` to: the next one round from the
/// type they share, or the first one either way when they differ.
fn next_type(shapes: &[DrawElement], forward: bool) -> DrawElementType {
    let shared = shapes
        .first()
        .filter(|first| shapes.iter().all(|shape| shape.kind == first.kind))
        .and_then(|first| GENERIC_TYPES.iter().position(|kind| *kind == first.kind));
    let index = shared.map_or(-1, |index| index as isize);
    let count = GENERIC_TYPES.len() as isize;
    let step = if forward { 1 } else { -1 };
    GENERIC_TYPES[((index + count + step) % count) as usize]
}

impl DrawEngine {
    /// Whether Tab has anything to switch: a closed shape, or a line or an arrow nothing
    /// is bound to — `getConversionTypeFromElements` and `isEligibleLinearElement`
    /// (`ConvertElementTypePopup.tsx@1118751f:641-672`).
    pub fn can_convert_selection(&self) -> bool {
        let selected = self.get_selected_elements();
        selected.iter().any(is_generic) || selected.iter().any(is_eligible_linear)
    }

    /// The type the panel shows pressed: the one the switchable elements among `selected`
    /// share, and `None` when they differ or nothing is switchable —
    /// `getConversionTypeFromElements` plus the panel's `sameType`
    /// (`ConvertElementTypePopup.tsx@1118751f:204-215`, `:641-664`).
    ///
    /// The engine's to answer, not the host's: for a line or an arrow it is a reading of
    /// `roundness` and `elbowed`, which is [`LinearType::of`]'s and nobody else's.
    pub fn shared_conversion_type(&self) -> Option<ConvertTo> {
        let selected = self.get_selected_elements();
        let generic: Vec<DrawElement> =
            selected.iter().filter(|e| is_generic(e)).cloned().collect();
        if !generic.is_empty() {
            let first = generic[0].kind;
            return generic
                .iter()
                .all(|shape| shape.kind == first)
                .then_some(ConvertTo::Generic(first));
        }
        let linear: Vec<LinearType> = selected
            .iter()
            .filter(|e| is_eligible_linear(e))
            .map(LinearType::of)
            .collect();
        let first = *linear.first()?;
        linear
            .iter()
            .all(|kind| *kind == first)
            .then_some(ConvertTo::Linear(first))
    }

    /// The switch opened: from here until [`Self::end_conversion`], each switched shape's
    /// label comes back to the size it had now, and each selected line or arrow is
    /// remembered under the type it has now.
    pub fn begin_conversion(&mut self) {
        self.conversion_font_sizes.get_or_insert_with(HashMap::new);
        self.remember_label_sizes();
        self.conversion_lines.get_or_insert_with(HashMap::new);
        self.remember_lines();
    }

    /// The switch closed: what it remembered is forgotten.
    pub fn end_conversion(&mut self) {
        self.conversion_font_sizes = None;
        self.conversion_lines = None;
    }

    /// Switches the selection to `to`, or to the next type round (`forward`) or the one
    /// before — `convertElementTypes`. One step of undo. Says whether anything changed.
    ///
    /// A closed shape, a line or an unbound arrow, whichever the selection holds first:
    /// the generic branch has preference (`ConvertElementTypePopup.tsx@1118751f:648-653`).
    /// A conversion never crosses the two families, so asking for one inside the other
    /// changes nothing at all (`:929-950`).
    ///
    /// Each shape keeps its id, style, place and size, and its roundness on or off; an
    /// explicit corner radius goes, as the oracle's `roundness.value` does. Arrows bound to
    /// it are aimed at the new outline and routed again. A label is laid out again at the
    /// size it had when the switch opened, shrunk until its lines fit.
    ///
    /// A line or an arrow keeps its id, its points and its place; it becomes one of the
    /// four [`LinearType`]s, and a type it has been in since the switch opened is handed
    /// back whole rather than rebuilt.
    pub fn convert_selection(&mut self, to: Option<ConvertTo>, forward: bool) -> bool {
        let selected: Vec<DrawElement> = self.get_selected_elements();
        let shapes: Vec<DrawElement> = selected.iter().filter(|e| is_generic(e)).cloned().collect();
        if !shapes.is_empty() {
            return self.convert_generic(&shapes, to, forward);
        }
        self.convert_linear(&selected, to, forward)
    }

    /// The generic branch of [`Self::convert_selection`], split out so the linear one
    /// beside it is the same shape of thing and neither is the tail of the other.
    fn convert_generic(
        &mut self,
        shapes: &[DrawElement],
        to: Option<ConvertTo>,
        forward: bool,
    ) -> bool {
        let next = match to {
            Some(ConvertTo::Generic(kind)) => kind,
            Some(ConvertTo::Linear(_)) => return false,
            None => next_type(shapes, forward),
        };
        // The popup remembers what it sees before a click switches anything.
        self.remember_label_sizes();

        let switched: Vec<DrawElement> = shapes
            .iter()
            .filter(|shape| shape.kind != next)
            .map(|shape| {
                let mut shape = shape.clone();
                shape.kind = next;
                shape.corner_radius = None;
                shape
            })
            .collect();
        if switched.is_empty() {
            return false;
        }
        for shape in &switched {
            self.scene.put(shape.clone());
        }
        self.refit_switched_shapes(&switched);
        self.apply_bindings();
        self.set_tool(DrawTool::Select);
        self.push_history();
        self.request_draw();
        true
    }

    /// Each switched shape's arrows aimed at its new outline and routed again, and its
    /// label laid out again in it — the second half of the generic branch
    /// (`ConvertElementTypePopup.tsx@1118751f:481-506`).
    fn refit_switched_shapes(&mut self, shapes: &[DrawElement]) {
        let zoom = self.camera.scale;
        for shape in shapes {
            let arrows: Vec<DrawElement> = self
                .scene
                .iter_ordered()
                .filter_map(|element| reanchor_to_outline(element, shape, zoom))
                .collect();
            for arrow in arrows {
                self.scene.put(arrow);
            }
            self.refit_label(shape);
        }
    }

    /// The linear branch of [`Self::convert_selection`] — `convertElementTypes`
    /// (`ConvertElementTypePopup.tsx@1118751f:519-639`).
    fn convert_linear(
        &mut self,
        selected: &[DrawElement],
        to: Option<ConvertTo>,
        forward: bool,
    ) -> bool {
        let lines: Vec<DrawElement> = selected
            .iter()
            .filter(|element| is_eligible_linear(element))
            .cloned()
            .collect();
        if lines.is_empty() {
            return false;
        }
        let next = match to {
            Some(ConvertTo::Linear(kind)) => kind,
            Some(ConvertTo::Generic(_)) => return false,
            None => next_linear(&lines, forward),
        };
        self.remember_lines();

        let switched: Vec<DrawElement> = lines
            .iter()
            .filter(|line| LinearType::of(line) != next)
            .map(|line| self.convert_linear_element(line, next))
            .collect();
        if switched.is_empty() {
            return false;
        }
        for line in switched {
            self.scene.put(line);
        }
        self.apply_bindings();
        self.set_tool(DrawTool::Select);
        self.push_history();
        self.request_draw();
        true
    }

    /// `element` as a `to`, by `convertElementType` and the post-normalisation that
    /// follows it (`ConvertElementTypePopup.tsx@1118751f:558-614`, `:833-927`).
    ///
    /// A type `element` has been in since the switch opened is handed back whole —
    /// `:551-556` — which is what makes the round trip a round trip rather than a rebuild.
    fn convert_linear_element(&self, element: &DrawElement, to: LinearType) -> DrawElement {
        if let Some(remembered) = self.remembered_line(&element.id, to) {
            return remembered;
        }
        let mut next = element.clone();
        next.kind = to.element_type();
        if let Some(roundness) = to.roundness() {
            next.roundness = roundness;
        }
        next.elbowed = None;
        next.fixed_segments = None;
        self.set_linear_heads(&mut next, to);
        if to == LinearType::ElbowArrow {
            let board = crate::scene::elbow::Board::new(self.scene.iter_ordered());
            crate::scene::elbow::retype(&mut next, true, &board, self.camera.scale);
        } else if crate::scene::elbow::is_elbow(element) {
            // Off an elbow and not to a type of its own: the oracle copies the points of
            // whichever of `line` / `sharpArrow` / `curvedArrow` it remembers
            // (`:600-613`), rather than un-routing the elbow's runs. With no remembered
            // one — the switch was not open when this became an elbow — the route stands.
            if let Some(plain) = self.remembered_plain_points(&element.id) {
                next.points = Some(plain);
            }
        }
        next
    }

    /// The heads a `to` is given, as `convertElementType` gives them
    /// (`ConvertElementTypePopup.tsx@1118751f:884-919`) and the post-normalisation after it
    /// (`:591-594`).
    ///
    /// A line gets neither: the oracle's `newLinearElement` writes both to null whatever
    /// the spread carried (`packages/element/src/newElement.ts@1118751f:583-586`). A sharp
    /// or a curved arrow takes them from the toolbar rather than from the element, which is
    /// `currentItemStartArrowhead` / `currentItemEndArrowhead` (`:891-892`, `:905-906`) and
    /// here the next style's. An elbow keeps the element's own and is given a head at its
    /// end whatever it had.
    fn set_linear_heads(&self, next: &mut DrawElement, to: LinearType) {
        match to {
            LinearType::Line => {
                next.start_arrowhead = None;
                next.end_arrowhead = None;
            }
            LinearType::SharpArrow | LinearType::CurvedArrow => {
                next.start_arrowhead = Some(self.next_start_arrowhead.unwrap_or(Arrowhead::None));
                next.end_arrowhead = Some(self.next_end_arrowhead.unwrap_or(Arrowhead::Arrow));
            }
            LinearType::ElbowArrow => next.end_arrowhead = Some(Arrowhead::Arrow),
        }
    }

    /// Each selected line or arrow under the type it has now, the first time the open
    /// switch sees it — `LINEAR_ELEMENT_CONVERSION_CACHE`, filled once per element
    /// (`ConvertElementTypePopup.tsx@1118751f:280-292`).
    ///
    /// Filled at [`Self::begin_conversion`] rather than on every render as the oracle's
    /// panel does, which only differs for a type reached *and* left again inside one
    /// sitting of the switch: rebuilt here, handed back whole there, and the same either
    /// way once the step is stamped.
    fn remember_lines(&mut self) {
        if self.conversion_lines.is_none() {
            return;
        }
        let lines: Vec<(String, DrawElement)> = self
            .get_selected_elements()
            .iter()
            .filter(|element| is_eligible_linear(element))
            .cloned()
            .map(|line| (line_key(&line.id, LinearType::of(&line)), line))
            .collect();
        if let Some(remembered) = self.conversion_lines.as_mut() {
            for (key, line) in lines {
                remembered.entry(key).or_insert(line);
            }
        }
    }

    /// `id`'s element as it was when the switch opened and it was a `to`, if it was one.
    fn remembered_line(&self, id: &str, to: LinearType) -> Option<DrawElement> {
        self.conversion_lines
            .as_ref()?
            .get(&line_key(id, to))
            .cloned()
    }

    /// `id`'s remembered points from whichever of the three simple types it has been, the
    /// oracle's `mapFind` over `["line", "sharpArrow", "curvedArrow"]`
    /// (`ConvertElementTypePopup.tsx@1118751f:600-607`).
    fn remembered_plain_points(&self, id: &str) -> Option<Vec<[f64; 2]>> {
        let remembered = self.conversion_lines.as_ref()?;
        [
            LinearType::Line,
            LinearType::SharpArrow,
            LinearType::CurvedArrow,
        ]
        .into_iter()
        .find_map(|kind| remembered.get(&line_key(id, kind)))
        .and_then(|line| line.points.clone())
    }

    /// Each selected shape's label size, the first time the open switch sees it.
    fn remember_label_sizes(&mut self) {
        if self.conversion_font_sizes.is_none() {
            return;
        }
        let sizes: Vec<(String, f64)> = self
            .get_selected_elements()
            .iter()
            .filter(|shape| is_generic(shape))
            .filter_map(|shape| {
                let label = self.live_label(shape)?;
                Some((shape.id.clone(), label.font_size?))
            })
            .collect();
        if let Some(remembered) = self.conversion_font_sizes.as_mut() {
            for (id, size) in sizes {
                remembered.entry(id).or_insert(size);
            }
        }
    }

    /// `shape`'s label, back at its remembered size and shrunk to fit — then laid out in
    /// the shape as any label is.
    fn refit_label(&mut self, shape: &DrawElement) {
        let Some(mut label) = self.live_label(shape).cloned() else {
            return;
        };
        let remembered = self
            .conversion_font_sizes
            .as_ref()
            .and_then(|sizes| sizes.get(&shape.id).copied());
        if let Some(size) = remembered {
            label.font_size = Some(size);
        }
        let size = self.with_measure(|measure| fitting_font_size(&label, shape, measure));
        label.font_size = Some(size);
        let laid = self.laid_out(&label);
        self.put_laid(laid);
    }
}

/// The type Tab (`forward`) or Shift+Tab switches `lines` to: the next one round from the
/// type they share, or the first one either way when they differ
/// (`ConvertElementTypePopup.tsx@1118751f:523-534`).
fn next_linear(lines: &[DrawElement], forward: bool) -> LinearType {
    let shared = lines
        .first()
        .filter(|first| {
            lines
                .iter()
                .all(|line| LinearType::of(line) == LinearType::of(first))
        })
        .map(LinearType::of)
        .and_then(|kind| LINEAR_TYPES.iter().position(|each| *each == kind));
    let index = shared.map_or(-1, |index| index as isize);
    let count = LINEAR_TYPES.len() as isize;
    let step = if forward { 1 } else { -1 };
    LINEAR_TYPES[((index + count + step) % count) as usize]
}

/// The oracle's `toCacheKey` (`ConvertElementTypePopup.tsx@1118751f:674-679`): an element
/// under a type, so the same element is remembered once per type it has been.
fn line_key(id: &str, kind: LinearType) -> String {
    format!("{id}:{}", kind.name())
}
