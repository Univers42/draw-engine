//! Selecting a line editor's individual points, and deleting them.
//!
//! Backspace and Delete on a line or arrow whose points are held removes **points**, not
//! the element — but only when points are held, and that is the whole of the behaviour.
//! `BACKSPACE` appears in exactly three places in Excalidraw at `1118751f`:
//! `packages/common/src/keys.ts:37` (the constant),
//! `packages/excalidraw/actions/actionDeleteSelected.tsx:306` (`keyTest`), and
//! `App.tsx:5954` (⌘/Ctrl+Backspace, clear canvas). `linearElementEditor.ts` contains no
//! `Backspace` at all. So there is no key handler on the line editor: deleting a point is
//! the **first branch of the ordinary delete action**, and a line editor with no point
//! held falls straight through to deleting whole elements.
//!
//! `actionDeleteSelected.perform` (`actionDeleteSelected.tsx@1118751f:213-273`) tries
//! three things **in this order**, and the order is the spec:
//!
//! 1. `selectedPointsIndices == null` → `return false` (`:229-231`) — this action deletes
//!    nothing and the action below it takes the element instead. The comment above the
//!    branch says why that is the right default: if you meant a point and missed, taking
//!    the whole element is "most likely a mistake" — but with no point held, deleting
//!    what is held is what was asked for.
//! 2. `selectedPointsIndices.length >= linearElement.points.length` → delete the
//!    **element** (`:234-251`), `selectedLinearElement: null`,
//!    `CaptureUpdateAction.IMMEDIATELY`.
//! 3. otherwise [`DrawEngine::delete_points`], and re-map the selection to
//!    `[selectedPointsIndices[0] - 1]`, or `[0]` when the first index was `0`
//!    (`:253-272`).
//!
//! # The gesture that holds a point
//!
//! A **click on a point**, and a **Shift+drag** that box-selects several at once.
//! `LinearElementEditor.handlePointerDown` (`linearElementEditor.ts@1118751f:1204-1216`)
//! and `handleBoxSelection` (`:248-309`). The reachability of the second is not obvious and
//! is written out in full beside [`DrawEngine::select_points_in_box`].

use crate::camera::WorldBounds;
use crate::engine::types::PointSelection;
use crate::engine::DrawEngine;
use crate::scene::{bump_version, DrawElementType};

impl DrawEngine {
    /// Records that the press landed on point `index` of `id`, as
    /// `LinearElementEditor.handlePointerDown` does
    /// (`linearElementEditor.ts@1118751f:1204-1216`).
    ///
    /// ```ts
    /// const nextSelectedPointsIndices =
    ///   clickedPointIndex > -1 || event.shiftKey
    ///     ? event.shiftKey ||
    ///       linearElementEditor.selectedPointsIndices?.includes(clickedPointIndex)
    ///       ? normalizeSelectedPoints([...(linearElementEditor.selectedPointsIndices || []), clickedPointIndex])
    ///       : [clickedPointIndex]
    ///     : null;
    /// ```
    ///
    /// Read in three parts, because each is a different rule:
    ///
    /// - a press that named **no** point leaves the list exactly as it was (`:1204`) — the
    ///   `clickedPointIndex > -1` test is outside the ternary, so the whole assignment is
    ///   skipped. A press on empty canvas beside a line therefore neither selects a point
    ///   nor forgets the ones already held;
    /// - shift **accumulates**, de-duplicated and sorted by
    ///   [`crate::selection::linear::normalize_selected_points`], so shift-clicking a
    ///   point already held is idempotent and cannot toggle it off — the `Set` at `:2371`
    ///   throws the duplicate away. The same accumulation happens without shift when the
    ///   point is already held, which is the `||` in the middle of `:1207`;
    /// - a plain click on a point not already held **replaces** the list with that one
    ///   index (`:1212`).
    ///
    /// `accumulate` is the press's shift flag, which is where the host puts
    /// `event.shiftKey`.
    pub(crate) fn select_point(&mut self, id: &str, index: Option<usize>, accumulate: bool) {
        if index.is_none() && !accumulate {
            return;
        }
        let same_element = self
            .selected_points
            .as_ref()
            .is_some_and(|held| held.id == id);
        let current: Vec<i64> = if same_element {
            self.selected_points
                .as_ref()
                .map(|held| held.indices.iter().map(|i| *i as i64).collect())
                .unwrap_or_default()
        } else {
            // A press on a different element starts afresh. The oracle cannot reach here
            // with a stale list — a new `LinearElementEditor` is constructed on every
            // selection change, with `selectedPointsIndices: null`
            // (`linearElementEditor.ts@1118751f:199`, `_getLinearElementEditor`,
            // `selection.ts@1118751f:248-266`) — and so does this, on `set_selection`.
            Vec::new()
        };
        let clicked = index.map_or(-1, |i| i as i64);
        let next = if accumulate || current.contains(&clicked) {
            let mut both = current;
            both.push(clicked);
            both
        } else {
            vec![clicked]
        };
        let Some(indices) = crate::selection::linear::normalize_selected_points(&next) else {
            return;
        };
        if same_element && self.selected_points.as_ref().map(|h| &h.indices) == Some(&indices) {
            return;
        }
        self.selected_points = Some(PointSelection {
            id: id.to_string(),
            indices,
        });
        self.request_draw();
    }

    /// Forgets the points held, leaving the element's selection alone.
    ///
    /// The oracle's `selectedLinearElement: null` (`actionDeleteSelected.tsx@1118751f:247`)
    /// is the whole editor going, which is a different thing; this is only the point list.
    pub(crate) fn forget_selected_points(&mut self) {
        if self.selected_points.take().is_some() {
            self.request_draw();
        }
    }

    /// Holds the points of the line or arrow `id` that fall inside `box`, plus the ones
    /// already held, as `LinearElementEditor.handleBoxSelection` does
    /// (`packages/element/src/linearElementEditor.ts@1118751f:255-309`).
    ///
    /// ```ts
    /// const [selectionX1, selectionY1, selectionX2, selectionY2] =
    ///   getElementAbsoluteCoords(appState.selectionElement, elementsMap);
    /// const pointsSceneCoords = LinearElementEditor.getPointsGlobalCoordinates(element, elementsMap);
    ///
    /// const nextSelectedPoints = pointsSceneCoords
    ///   .reduce((acc: number[], point, index) => {
    ///     if (
    ///       (point[0] >= selectionX1 && point[0] <= selectionX2 &&
    ///        point[1] >= selectionY1 && point[1] <= selectionY2) ||
    ///       (event.shiftKey && selectedPointsIndices?.includes(index))
    ///     ) { acc.push(index); }
    ///     return acc;
    ///   }, [])
    ///   .filter((index) => { /* elbow: index === 0 || index === points.length - 1 */ });
    ///
    /// setState({ selectedLinearElement: { ...selectedLinearElement,
    ///   selectedPointsIndices: nextSelectedPoints.length ? nextSelectedPoints : null } });
    /// ```
    ///
    /// Five things in there are easy to get wrong, so they are each named here:
    ///
    /// - **The box is axis-aligned and in global coordinates.** The oracle reads it off
    ///   `appState.selectionElement` with `getElementAbsoluteCoords` (`:268-269`), whose
    ///   return is `[x1, y1, x2, y2]` with `y1 = minY` and `y2 = maxY`
    ///   (`LinearElementEditor.getElementAbsoluteCoords`, `:2247-2251`) — so the y test at
    ///   `:281-282` is `point[1] >= y1 && point[1] <= y2`, the **same** min-then-max order as
    ///   x. Comparing them the other way round selects nothing at all.
    /// - **The candidate points are global too** (`getPointsGlobalCoordinates`, `:271-274`),
    ///   which is [`crate::selection::linear::world_points`] here: element origin plus the
    ///   local point, rotated about the element's centre. Comparing a global box against
    ///   local points would be a bug that only shows on a rotated or off-origin line.
    /// - **Shift latches, and the expression is `&&`.** A point already held stays held for
    ///   the rest of the drag, so the band can be dragged away and the selection follows it.
    ///   `shift && already-held` is *not* `!shift || !already-held`; that De Morgan slip was
    ///   made once already in this file's click path, where the oracle reads
    ///   `event.shiftKey || includes` (`:1207`).
    /// - **The whole set is rebuilt from the previous set on every move** (the reduce reads
    ///   `selectedPointsIndices`, not an accumulator that carries over), which is what makes
    ///   the latch above work across moves rather than only within one.
    /// - **An empty result is `None`, not an empty list** (`:304-306`). That is not a
    ///   detail: the delete action's first branch tests `== null` against an empty array
    ///   (`actionDeleteSelected.tsx@1118751f:229-231`), so the two must not be flattened.
    ///   [`crate::selection::linear::normalize_selected_points`] already does this, and the
    ///   click path uses it — so it is reused here rather than a second answer.
    ///
    /// The elbow filter (`:290-299`) is the same predicate as
    /// [`crate::selection::linear::is_point_handle`] (`:1424-1431`), which the click path
    /// uses, and it is applied to the **built** set rather than to the candidates: an elbow
    /// arrow's interior corners are the router's and are not selectable, but a corner that
    /// shift had latched is dropped by the filter too, because `:290-299` runs after the
    /// reduce.
    pub(crate) fn select_points_in_box(&mut self, id: &str, box_bounds: WorldBounds, shift: bool) {
        let Some(element) = self.scene.get(id).cloned() else {
            return;
        };
        let points = crate::selection::linear::world_points(&element);
        let latched: Vec<i64> = if shift {
            self.selected_points
                .as_ref()
                .filter(|held| held.id == id)
                .map(|held| held.indices.iter().map(|i| *i as i64).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let candidates: Vec<i64> = points
            .iter()
            .enumerate()
            .filter(|(_, point)| {
                // `point[0] >= x1 && point[0] <= x2 && point[1] >= y1 && point[1] <= y2`
                // — both axes min-then-max, as `getElementAbsoluteCoords` returns them.
                point.x >= box_bounds.min_x
                    && point.x <= box_bounds.max_x
                    && point.y >= box_bounds.min_y
                    && point.y <= box_bounds.max_y
            })
            .map(|(index, _)| index as i64)
            .chain(latched)
            .collect();
        // The filter runs on the built set, so a latched corner of an elbow arrow goes too.
        let candidates: Vec<i64> = candidates
            .into_iter()
            .filter(|index| crate::selection::linear::is_point_handle(&element, *index))
            .collect();
        let next = crate::selection::linear::normalize_selected_points(&candidates);
        let same = self.selected_points.as_ref().map(|held| &held.indices) == next.as_ref();
        if same {
            return;
        }
        self.selected_points = next.map(|indices| PointSelection {
            id: id.to_string(),
            indices,
        });
        self.request_draw();
    }

    /// Whether a press at `id` with `shift` held is the oracle's
    /// `isSelectingPointsInLineEditor` — the one flag that decides whether a drag is a
    /// **move** of the element or a **box** over its points
    /// (`App.tsx@1118751f:10895-10899`).
    ///
    /// ```ts
    /// const isSelectingPointsInLineEditor =
    ///   this.state.selectedLinearElement?.isEditing &&
    ///   event.shiftKey &&
    ///   this.state.selectedLinearElement.elementId === pointerDownState.hit.element?.id;
    /// ```
    ///
    /// It is `&& !isSelectingPointsInLineEditor` on the drag-the-element branch at
    /// `:10901-10904`, so when this is true the press falls past it, through `:11136`'s
    /// `if (this.state.selectionElement)` — the band the pointer-down made at
    /// `createGenericElementOnPointerDown` (`:10439`, `:10495-10498`) — and on to `:11274`'s
    /// `handleBoxSelection`. It is the **only** route to the marquee, and it is not an
    /// obvious one, which is why it is a named predicate here rather than a condition
    /// inlined at two call sites.
    ///
    /// The first two terms are already true of any press this engine routes here: the
    /// caller only reaches it for the single selected element, and it has already
    /// established that the element offers its points. What is left is the shift and the
    /// identity of the element, and `editing_linear` is this engine's `isEditing`.
    ///
    /// Note what it is *not*: it does not ask whether the press landed on a handle. The
    /// oracle compares the element id, so a press on the stroke works as well as one on a
    /// point — and the same shift-drag starting between two points, where there is no
    /// handle to grab, is the ordinary way to draw the box.
    pub(crate) fn selecting_points_in_line_editor(&self, id: &str, shift: bool) -> bool {
        shift && self.editing_linear.as_deref() == Some(id)
    }

    /// Removes the points at `point_indices` from the line or arrow `id`.
    ///
    /// `LinearElementEditor.deletePoints` (`linearElementEditor.ts@1118751f:1576-1618`),
    /// the whole of it:
    ///
    /// ```ts
    /// const isUncommittedPoint =
    ///   app.state.selectedLinearElement?.isEditing &&
    ///   app.state.selectedLinearElement?.lastUncommittedPoint === element.points[element.points.length - 1];
    ///
    /// const nextPoints = element.points.filter((_, idx) => !pointIndices.includes(idx));
    ///
    /// const isPolygon = isLineElement(element) && element.polygon;
    ///
    /// if (isPolygon && (isUncommittedPoint || pointIndices.includes(0) ||
    ///     pointIndices.includes(element.points.length - 1))) {
    ///   nextPoints[0] = pointFrom(nextPoints[nextPoints.length - 1][0], nextPoints[nextPoints.length - 1][1]);
    /// }
    ///
    /// const { points: normalizedPoints, offsetX, offsetY } = getNormalizedPoints({ points: nextPoints });
    /// LinearElementEditor._updatePoints(element, app.scene, normalizedPoints, offsetX, offsetY);
    /// ```
    ///
    /// Four things are worth naming, because each is easy to get subtly wrong:
    ///
    /// - the filter is `pointIndices.includes(idx)`, so the list is a **set**: order,
    ///   duplicates and indices naming nothing are all immaterial, and no point can be
    ///   removed twice. It walks the points and asks; it never indexes with the list;
    /// - **the polygon rule** keeps a closed line shut. `nextPoints[0]` is rewritten to
    ///   where the *new* last point is, because a polygon's first and last points are the
    ///   same place — take either away and the loop opens into a notch. The test for the
    ///   last index asks `element.points`, **not** `nextPoints` (`:1597`), which is the
    ///   difference between "the last point of the original" and "the last point left";
    /// - the oracle writes that rewrite with no empty check, because its only caller
    ///   guarantees `pointIndices.length < points.length` (`:234`) so `nextPoints` is
    ///   never empty. This is a public function that tests call directly, so it is
    ///   guarded here instead: a filter that removes everything answers with no points
    ///   rather than indexing off the end;
    /// - `isUncommittedPoint` is the preview point of a path still being placed, which
    ///   this engine holds as a *count* rather than as a point
    ///   (`engine/multi_linear.rs`'s `MultiLinear::committed`), so it is recognised as
    ///   "the point past `committed`" — the same thing by another representation.
    ///
    /// `_updatePoints` is the normalise-and-reseat step, and this engine reuses the
    /// normaliser placement already uses ([`crate::engine::multi_linear::reseat_points`])
    /// rather than writing a second one: points are stored relative to the element's
    /// origin, so removing the point that *was* the origin has to move the origin, or the
    /// element's box stops containing its own drawing and the marquee, the eraser and the
    /// exporter all go wrong at once.
    ///
    /// Nothing here requires two points to survive. The oracle has no such guard, so
    /// deleting one point of a two-point line leaves a one-point line, drawn as a dot.
    /// Adding "two points or gone" would be a divergence from `1118751f`; the tests say so
    /// in `a_two_point_line_can_be_reduced_to_one`.
    pub fn delete_points(&mut self, id: &str, point_indices: &[usize]) {
        let Some(mut element) = self.scene.get(id).cloned() else {
            return;
        };
        let Some(points) = element.points.clone() else {
            return;
        };
        // The preview point of a path still being placed: the one past the committed
        // count, if it is there. See the `isUncommittedPoint` note above.
        let is_uncommitted_point = self
            .multi_linear
            .as_ref()
            .filter(|state| state.id == id)
            .is_some_and(|state| points.len() > state.committed);

        let before = points.len();
        let mut next: Vec<[f64; 2]> = points
            .iter()
            .enumerate()
            .filter(|(idx, _)| !point_indices.contains(idx))
            .map(|(_, point)| *point)
            .collect();

        // `isLineElement(element) && element.polygon` (`:1590`).
        let is_polygon = element.kind == DrawElementType::Line && element.polygon == Some(true);
        let last_before = before.checked_sub(1);
        if is_polygon
            && (is_uncommitted_point
                || point_indices.contains(&0)
                || last_before.is_some_and(|last| point_indices.contains(&last)))
        {
            // `nextPoints[0] = nextPoints[nextPoints.length - 1]`, guarded on a non-empty
            // `next` — the oracle's own caller makes that impossible, and this one can be
            // called directly.
            if let (Some(replacement), Some(first)) = (next.last().copied(), next.first_mut()) {
                *first = replacement;
            }
        }

        element.points = Some(next);
        crate::engine::multi_linear::reseat_points(&mut element);
        self.scene.put(bump_version(element, self.now_ms));
        self.apply_bindings();
        self.push_history();
        self.request_draw();
    }
}
