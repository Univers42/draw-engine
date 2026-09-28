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
//! flag to forget**. The selection is therefore not something a copy asks for; it is what
//! `prepareElementsForExport` finds, and an empty selection is the whole scene
//! (`data/index.ts@1118751f:56-58, 61-69`), exactly as for a file.
//!
//! ## What is new here, and why it is not the scope
//!
//! [`crate::export_scope`] owns the scope and this module reuses it rather than deciding
//! anything a second time. What a *clipboard* needs on top of a file is three things, and
//! each one is a decision a host is not allowed to make (BUNNY.md §2):
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
//!   (`locales/en.json@1118751f:579-580`, at `actionClipboard.tsx@1118751f:165-167` and
//!   `:225-227`). Which of the two is a fact about the scope, so it rides out with it
//!   rather than being re-derived by the front from its own selection count.

/// The oracle's two copy actions, which are two formats and not two features.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardFormat {
    /// `actionCopyAsPng` (`actionClipboard.tsx@1118751f:192`).
    Png,
    /// `actionCopyAsSvg` (`:124`).
    Svg,
}

/// What the host's browser can do, and nothing else.
///
/// The two halves are the oracle's two predicates' browser halves, verbatim:
/// `probablySupportsClipboardWriteText` is `"clipboard" in navigator && "writeText" in
/// navigator.clipboard`, and `probablySupportsClipboardBlob` adds `"write" in
/// navigator.clipboard && "ClipboardItem" in window && "toBlob" in
/// HTMLCanvasElement.prototype` (`clipboard.ts@1118751f:65-72`). Neither is a fact the
/// engine can see — `navigator` and `window` are the host's — so the host reports them and
/// this module decides what they mean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipboardHost {
    pub can_write_blob: bool,
    pub can_write_text: bool,
}

/// One copy, offered to a host that can deliver it.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipboardCopy {
    /// The oracle's `predicate`, decided by the motor from the host's report and the scene.
    pub supported: bool,
    /// What to write it as, or `""` when there is nothing to write. Never the host's
    /// choice: `image/png` and `text/plain` (`clipboard.ts@1118751f:568-570, 596-598`).
    pub mime: &'static str,
    /// What was copied — the toast's one word, carried by the scope itself
    /// ([`crate::ExportScopeKind`]) rather than re-derived by the front from its own
    /// selection count. One enum, not a second one: the two answers are the scope's.
    pub scope: crate::export::ExportScopeKind,
    /// The payload, for the vector format. `None` for a raster, which only a browser can
    /// encode, and `None` whenever `supported` is false.
    pub text: Option<String>,
}

impl ClipboardCopy {
    /// Nothing, for a copy the engine declines.
    ///
    /// A named constructor rather than a `Default`, because "declined" is an answer and a
    /// derived one would make it an accident of the type: a `ClipboardCopy` built with
    /// `..Default::default()` would be declined *and* carry a MIME type, which is the
    /// state this whole module exists to make impossible.
    pub fn declined() -> Self {
        Self {
            supported: false,
            mime: "",
            scope: crate::export::ExportScopeKind::Scene,
            text: None,
        }
    }
}

impl ClipboardFormat {
    /// The MIME type the oracle writes this format under.
    ///
    /// `image/png` for the raster (`clipboard.ts@1118751f:568-570`) and **`text/plain`**
    /// for the vector, not `image/svg+xml`: `copyTextToSystemClipboard` says so itself,
    /// that `navigator.clipboard.write` "doesn't work with non-standard mime types"
    /// (`clipboard.ts@1118751f:622-625`), and writes the string as the one type every
    /// target accepts.
    pub fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Svg => "text/plain",
        }
    }

    /// Whether this browser can take this format — the oracle's predicate's first half.
    pub fn host_can_take(self, host: &ClipboardHost) -> bool {
        match self {
            Self::Png => host.can_write_blob,
            Self::Svg => host.can_write_text,
        }
    }
}
