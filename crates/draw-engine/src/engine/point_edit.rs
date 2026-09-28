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
//! A **click on a point**, not a marquee:
//! `LinearElementEditor.handlePointerDown` (`linearElementEditor.ts@1118751f:1204-1216`).
//! The oracle's other writer of `selectedPointsIndices`, the marquee
//! `handleBoxSelection` (`:248-309`), cannot run at this SHA — its only call site is
//! guarded by `!isEditing` (`App.tsx:11274`) while its own guard needs a
//! `selectionElement` that only the sibling branch on the other side of that same guard
//! ever sets (`App.tsx:11282`, `:10497`).

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
