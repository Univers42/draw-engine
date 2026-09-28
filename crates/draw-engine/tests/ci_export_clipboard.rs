//! Copy as PNG / as SVG: **what** goes on the clipboard, and **whether** anything does.
//!
//! The oracle's two copy actions are `actionCopyAsPng` (`actions/actionClipboard.tsx@1118751f:192`)
//! and `actionCopyAsSvg` (`:124`). Both call the same function, and the function is not a
//! clipboard function at all:
//!
//! ```text
//! actionCopyAsPng  actionClipboard.tsx@1118751f:209-213
//! actionCopyAsSvg  actionClipboard.tsx@1118751f:136-140
//!   const { exportedElements, exportingFrame } = prepareElementsForExport(
//!     elements, appState, true);
//! ```
//!
//! `exportSelectionOnly` is the literal `true` in both — **there is no third mode and no
//! flag to forget**. The selection is therefore not something the copy asks for; it is
//! what `prepareElementsForExport` finds, and an empty selection is the whole scene
//! (`data/index.ts@1118751f:56-58, 61-69`), exactly as for a file.
//!
//! ## What is new here, and why it is not the scope
//!
//! 4.2 owns the scope ([`draw_engine::export_scope`]) and this file reuses it rather than
//! deciding anything a second time. What a *clipboard* needs on top of a file is three
//! things, and each one is a decision a host is not allowed to make (BUNNY.md §2):
//!
//! - **the MIME type** — `image/png` for the raster, `text/plain` for the vector. The
//!   oracle names both (`clipboard.ts@1118751f:568-570`, `:596-598`); a host that picks
//!   its own produces a clipboard nothing will paste.
//! - **whether to write at all** — the oracle's `predicate`
//!   (`actionClipboard.tsx@1118751f:247-249, 186-188`): the browser can take the payload
//!   *and* there is something to take. The browser's answer is a fact the host has to
//!   report (`probablySupportsClipboardBlob`, `clipboard.ts@1118751f:68-72`); the verdict
//!   is the motor's, the same way `ExportOptions`'s default scale is decided from a
//!   device pixel ratio the engine cannot see (`export/png.rs@1118751f:51-55`).
//! - **what was copied** — the toast says "selection" or "canvas" and nothing else
//!   (`locales/en.json@1118751f:579-580`, interpolated at `actionClipboard.tsx:165-167`
//!   and `:225-227`). Which of the two is a fact about the scope, so it rides out with it
//!   rather than being re-derived by the front from its own selection count.
//!
//! ## The two paths are not symmetric, and it is not in the write
//!
//! The brief expected one path to fail quietly. It does not, and the code is why:
//! `exportCanvas`'s `clipboard-svg` arm rethrows one message
//! (`data/index.ts@1118751f:151-159`) and its `clipboard` arm *classifies* the failure
//! into three — too big, a Firefox `ClipboardItem` that is not defined, and the generic
//! case (`:194-213`). Both land in the action's `catch`, and both return
//! `appState.errorMessage` (`actionClipboard.tsx@176-184`, `:236-245`). In English the two
//! generic messages are the **same string** (`locales/en.json@1118751f:278` and `:320`).
//!
//! So the asymmetry is *how much* is reported, not *whether*: the SVG path says "couldn't
//! copy", and the PNG path may also say why. The part that would have been silent — a
//! `ClipboardItem` built from a promise in Safari, which must be constructed in the same
//! tick or the browser complains about user intent (`clipboard.ts@1118751f:558-563`) — is a
//! *success* path, and it is the host's because it is a browser rule.

mod common;
use common::*;
use draw_engine::*;

/// A stroke colour per element, so a test can ask what reached the picture. See
/// [`svg_uses`].
const LEFT: &str = "#111111";
const RIGHT: &str = "#222222";
const FRAME: &str = "#333333";
const CHILD: &str = "#444444";

/// A browser that can take both payloads, which is the case the menu entries assume.
fn browser() -> ClipboardHost {
    ClipboardHost {
        can_write_blob: true,
        can_write_text: true,
    }
}

/// A browser that can take neither: no `navigator.clipboard`, so neither entry is offered.
fn blind_browser() -> ClipboardHost {
    ClipboardHost {
        can_write_blob: false,
        can_write_text: false,
    }
}

/// A frame holding `child`, at the origin.
fn frame_holding(child: DrawElement) -> (DrawElement, DrawElement) {
    let frame = tinted(frame_at_origin(300.0), FRAME);
    let mut child = tinted(child, CHILD);
    child.frame_id = Some(frame.id.clone());
    (frame, child)
}

// ── The type is the engine's ───────────────────────────────────────────────────

/// The clipboard is written under the type the **engine** names.
///
/// This is the smallest thing 4.3 has to add and the easiest to get wrong later, because
/// a host that picks its own type still pastes — into some other application, or nowhere.
/// The oracle writes `image/png` through a `ClipboardItem` and the vector as `text/plain`
/// (never `image/svg+xml`: `copyTextToSystemClipboard`'s own comment says
/// `navigator.clipboard.write` "doesn't work with non-standard mime types",
/// `clipboard.ts@1118751f:625`).
#[test]
fn the_clipboard_is_written_under_the_type_the_engine_names() {
    let engine = engine_with_scene(vec![box_at(0.0, 0.0, 40.0, 40.0)]);

    let png = engine.clipboard_copy(ClipboardFormat::Png, &browser(), &ExportOptions::default());
    let svg = engine.clipboard_copy(ClipboardFormat::Svg, &browser(), &ExportOptions::default());

    assert_eq!(png.mime, "image/png");
    assert_eq!(svg.mime, "text/plain");
}

// ── Whether a copy is offered at all ───────────────────────────────────────────

/// No copy is offered when the browser cannot take the payload.
///
/// The oracle's `predicate` is `probablySupportsClipboardBlob && elements.length > 0`
/// for the raster and `probablySupportsClipboardWriteText && elements.length > 0` for
/// the vector (`actionClipboard.tsx@1118751f:247-249, 186-188`). The capability is a
/// fact about the browser the host reports (`clipboard.ts@1118751f:65-72`); **the verdict
/// is the motor's**, because a host that decides it can only get it wrong in the
/// permissive direction — offering an entry that then does nothing, which is the failure
/// this whole task exists to make impossible.
#[test]
fn no_copy_is_offered_when_the_browser_cannot_take_it() {
    let engine = engine_with_scene(vec![box_at(0.0, 0.0, 40.0, 40.0)]);
    let options = ExportOptions::default();

    for format in [ClipboardFormat::Png, ClipboardFormat::Svg] {
        let copy = engine.clipboard_copy(format, &blind_browser(), &options);
        assert!(
            !copy.supported,
            "{format:?} should not be offered to a blind browser"
        );
        assert_eq!(
            copy.text, None,
            "{format:?} must build no payload it cannot deliver"
        );
    }
}

/// The oracle's second half: `elements.length > 0`. An empty board offers nothing.
///
/// Reached from the *canvas* branch of the context menu with the whole scene's elements
/// (`ContextMenu.tsx@1118751f:38-48` calls `predicate(elements, …)` with
/// `useExcalidrawElements()`), so this is a scene-emptiness test and not a selection one:
/// a board with five elements and nothing selected still offers both entries, and what it
/// copies is all five.
#[test]
fn a_board_with_nothing_on_it_offers_no_copy() {
    let engine = engine_with_scene(vec![]);
    let options = ExportOptions::default();

    for format in [ClipboardFormat::Png, ClipboardFormat::Svg] {
        assert!(
            !engine
                .clipboard_copy(format, &browser(), &options)
                .supported,
            "{format:?} should not be offered on an empty board"
        );
    }
}

/// A browser that can take one payload is offered that one and not the other.
///
/// The two predicates are different functions of different host facts, and a host that
/// collapses them into "can we use the clipboard at all" would offer the raster to a
/// browser that can only write text.
#[test]
fn a_browser_that_takes_only_text_is_offered_only_the_vector() {
    let engine = engine_with_scene(vec![box_at(0.0, 0.0, 40.0, 40.0)]);
    let options = ExportOptions::default();
    let text_only = ClipboardHost {
        can_write_blob: false,
        can_write_text: true,
    };

    assert!(
        !engine
            .clipboard_copy(ClipboardFormat::Png, &text_only, &options)
            .supported
    );
    assert!(
        engine
            .clipboard_copy(ClipboardFormat::Svg, &text_only, &options)
            .supported
    );
}

// ── The scope, which is 4.2's, reached the oracle's way ────────────────────────

/// A copy of nothing selected is the whole scene, and says so.
///
/// `exportSelectionOnly` is hard-coded `true` by both actions, so the only thing that can
/// make a copy narrower is a live selection. With none, `isSomeElementSelected` is false
/// and the ternary at `data/index.ts@1118751f:61-69` takes its `else` arm. **What stops
/// the competing branch is that no live element is selected** — not the flag, which is
/// always true here.
#[test]
fn a_copy_of_nothing_selected_is_the_whole_scene() {
    let shape = tinted(box_at(0.0, 0.0, 200.0, 100.0), LEFT);
    let other = tinted(box_at(400.0, 200.0, 50.0, 50.0), RIGHT);
    let engine = engine_with_scene(vec![shape, other]);

    let copy = engine.clipboard_copy(ClipboardFormat::Svg, &browser(), &ExportOptions::default());

    assert_eq!(copy.scope, ExportScopeKind::Scene);
    assert!(
        copy.supported,
        "a board with two elements still offers a copy"
    );
    let svg = copy.text.as_deref().expect("a copy carries a picture");
    assert_eq!(svg_uses(svg, LEFT), 1, "the first element should be copied");
    assert_eq!(
        svg_uses(svg, RIGHT),
        1,
        "the second element should be copied"
    );
}

/// A copy of a selection is that selection, and says "selection".
#[test]
fn a_copy_of_a_selection_is_that_selection() {
    let left = tinted(box_at(0.0, 0.0, 60.0, 40.0), LEFT);
    let right = tinted(box_at(500.0, 300.0, 30.0, 30.0), RIGHT);
    let mut engine = engine_with_scene(vec![left.clone(), right.clone()]);
    engine.select(vec![left.id.clone()]);

    let copy = engine.clipboard_copy(ClipboardFormat::Svg, &browser(), &ExportOptions::default());

    assert_eq!(copy.scope, ExportScopeKind::Selection);
    let svg = copy.text.as_deref().expect("a copy carries a picture");
    assert_eq!(
        svg_uses(svg, LEFT),
        1,
        "the selected element should be copied"
    );
    assert_eq!(svg_uses(svg, RIGHT), 0, "and nothing else");
}

/// A copy of one frame is that frame's contents, and still says "selection".
///
/// The oracle's toast has no third word: `selectedElements.length ? t("toast.selection") :
/// t("toast.canvas")` (`actionClipboard.tsx@1118751f:225-227`), and a lone frame *is*
/// selected, so it says "selection". The frame's *contents* are the payload and the
/// frame's own box is the framing — `data/index.ts@1118751f:73-79`, 4.2's case.
#[test]
fn a_copy_of_one_frame_is_that_frames_contents() {
    let (frame, child) = frame_holding(box_at(20.0, 20.0, 50.0, 50.0));
    let mut engine = engine_with_scene(vec![frame.clone(), child.clone()]);
    engine.select(vec![frame.id.clone()]);

    let copy = engine.clipboard_copy(ClipboardFormat::Svg, &browser(), &ExportOptions::default());
    let svg = copy.text.as_deref().expect("a copy carries a picture");

    assert_eq!(copy.scope, ExportScopeKind::Selection);
    // The frame's outline is in the picture its contents are measured by — it overlaps
    // itself, so `getElementsOverlappingFrame` keeps it (`scope.rs`).
    assert_eq!(svg_uses(svg, FRAME), 1, "the frame's own outline");
    assert_eq!(svg_uses(svg, CHILD), 1, "and its contents");
    assert_close(svg_width(svg), 300.0);
    assert_close(svg_height(svg), 300.0);
}

// ── The invariant: the clipboard carries the export's own bytes ─────────────────

/// **Whatever reaches the clipboard is what the file export of the same scope would
/// have produced.** Byte for byte.
///
/// This is the test that fails if the clipboard ever grows its own element-list decision,
/// which is the one way Law 3 can be broken here without anything looking wrong: a second
/// `if selected.is_empty() { … }` that happens to agree today and stops agreeing when a
/// case the file path handles goes missing. It compares the clipboard against
/// [`DrawEngine::export_scope`] + [`DrawEngine::export_svg_of`] — the *file* path's own
/// calls, on purpose. Comparing the clipboard to itself would be true by construction.
#[test]
fn the_clipboard_carries_exactly_what_the_file_export_would() {
    let (frame, child) = frame_holding(box_at(20.0, 20.0, 50.0, 50.0));
    let loose = box_at(700.0, 40.0, 30.0, 30.0);
    let mut turned = box_at(100.0, 100.0, 80.0, 80.0);
    turned.angle = EIGHTH_TURN;
    let elements = vec![frame.clone(), child.clone(), loose.clone(), turned.clone()];
    let options = ExportOptions::default();

    for selected in [
        vec![],
        vec![frame.id.clone()],
        vec![child.id.clone()],
        elements.iter().map(|el| el.id.clone()).collect(),
    ] {
        let mut engine = engine_with_scene(elements.clone());
        engine.select(selected.clone());

        let file = engine.export_scope(true, &options);
        let expected = engine.export_svg_of(&file);
        let copy = engine.clipboard_copy(ClipboardFormat::Svg, &browser(), &options);

        assert_eq!(copy.text, expected, "selected {selected:?}");
    }
}

// ── The failure path, which is the one nobody notices ──────────────────────────

/// A copy the host cannot deliver reports it, and says nothing was copied.
///
/// A copy the engine declines carries nothing to write, so a host cannot deliver it by
/// accident.
///
/// The oracle's two arms both `throw` rather than returning quietly
/// (`data/index.ts@1118751f:151-159, 194-213`; `actionClipboard.tsx@1118751f:176-184,
/// 236-245`), and in English their generic messages are the **same string**
/// (`locales/en.json@1118751f:278` and `:320`) — so what the motor owes the host is not a
/// sentence but a shape: a declined copy has no payload and no type, which is what keeps
/// "the engine said no" and "the browser said no" two visible states instead of one silent
/// one.
///
/// That the *host* then says something when the write itself fails is a claim about the
/// host, not the engine, and it is tested where it lives: `clipboard.test.ts` (a rejected
/// `navigator.clipboard.write`) and `e2e/copyAsImage.spec.ts`.
#[test]
fn a_copy_the_engine_declines_carries_nothing_to_write() {
    let engine = engine_with_scene(vec![box_at(0.0, 0.0, 40.0, 40.0)]);
    let options = ExportOptions::default();

    for format in [ClipboardFormat::Png, ClipboardFormat::Svg] {
        let copy = engine.clipboard_copy(format, &blind_browser(), &options);
        assert!(!copy.supported, "{format:?} should be declined");
        assert_eq!(copy.mime, "", "{format:?} should name no type to write");
        assert_eq!(copy.text, None, "{format:?} should build no payload");
    }
}
