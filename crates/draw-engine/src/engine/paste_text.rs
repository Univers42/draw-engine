//! Plain text pasted as text elements.
//!
//! The oracle's second branch of a paste: `insertClipboardContent` hands anything the
//! clipboard holds that is not its own elements to `addTextFromPaste`
//! (`packages/excalidraw/clipboard.ts@1118751f:538-553` → `App.tsx@1118751f:4757`), which
//! makes one text element per line, wraps whatever is wider than half the visible scene,
//! and centres the stack on the pointer (`App.tsx@1118751f:4979-5096`).
//!
//! Every decision is here, in the engine: the host reads the clipboard and calls
//! [`DrawEngine::paste_text`], the way it calls `pasteJson` for the other branch. Nothing
//! about a pasted text is computed in the front.

use crate::camera::Point;
use crate::engine::DrawEngine;
use crate::interaction::DrawTool;
use crate::scene::{DrawElement, Geometry};
use crate::text::layout;

/// The gap between two pasted lines, and between a pasted line and a blank one
/// (`LINE_GAP`, `App.tsx@1118751f:5014`).
const LINE_GAP: f64 = 10.0;

/// A pasted line is never wider than this, nor narrower than [`MIN_PASTE_TEXT_WIDTH`]
/// (`App.tsx@1118751f:5012-5013`).
const MAX_PASTE_TEXT_WIDTH: f64 = 800.0;
const MIN_PASTE_TEXT_WIDTH: f64 = 200.0;

/// `normalizeText(line).trim()` (`App.tsx@1118751f:5020`, `textMeasurements.ts@1118751f:64-70`):
/// the line is trimmed, so a line that is nothing but whitespace is no line at all — and the
/// `\r` a `\r\n` clipboard leaves on each of them goes with it.
///
/// ponytail: the tab the oracle also expands to eight spaces is left as the one character it
/// is — our measurer has no tab width, and the text editor leaves a pasted tab as it is too,
/// so expanding it here alone would measure the same character two ways.
fn pasted_line(source: &str) -> &str {
    source.trim()
}

impl DrawEngine {
    /// The text elements a plain-text paste makes, or nothing — the elements are not in the
    /// scene yet, so a caller can ask first.
    ///
    /// A blank line is a paragraph, not an element: it moves the line after it down by one
    /// line and one gap, and only when the line before *it* was not blank, so three blank
    /// lines are one paragraph (`App.tsx@1118751f:5053-5062`).
    fn pasted_text_elements(&self, text: &str, at: Point) -> Vec<DrawElement> {
        let max_width = self.pasted_text_width();
        let mut made: Vec<DrawElement> = Vec::new();
        let mut current_y = at.y;
        // A line's own height, for a blank line to be worth one of. Every pasted line is
        // written in the same style, so the line just made is the one to read it off —
        // and a blank line before any line is worth nothing, so it is never read at all.
        let mut line_px = 0.0;
        let mut previous_blank = true;
        for line in text.split('\n') {
            let source = pasted_line(line);
            if source.is_empty() {
                if !previous_blank {
                    current_y += line_px + LINE_GAP;
                }
                previous_blank = true;
                continue;
            }
            let element = self.pasted_text_element(source, at.x, current_y, max_width);
            line_px = element.height;
            current_y += line_px + LINE_GAP;
            made.push(element);
            previous_blank = false;
        }
        made
    }

    /// Half the visible width in scene units, capped and floored as the oracle's is
    /// (`App.tsx@1118751f:5011-5013`): the viewport is in screen pixels, the text is in
    /// scene units, and at 2× zoom a viewport twice as wide holds twice as much text.
    fn pasted_text_width(&self) -> f64 {
        let visible = self.width / self.camera.scale;
        (visible * 0.5).clamp(MIN_PASTE_TEXT_WIDTH, MAX_PASTE_TEXT_WIDTH)
    }

    /// One pasted line, as the engine mints every other text: the next style, a fresh id
    /// and seed, `version` 1, wrapped to `max_width` and sized to what it measures.
    ///
    /// A line too wide to wrap keeps its own width and re-wraps to it on any later edit,
    /// the way a fixed-width text does (`textElement.ts@1118751f:90-97`); one that fits
    /// sizes itself to its glyphs, as `autoResize: !isTextUnwrapped` asks
    /// (`App.tsx@1118751f:5048`).
    fn pasted_text_element(&self, source: &str, x: f64, y: f64, max_width: f64) -> DrawElement {
        let mut element = self.new_text_element(Geometry {
            x,
            y,
            width: 0.0,
            height: 0.0,
        });
        let font = layout::font_of(&element);
        let line_height = crate::scene::resolved_line_height(&element);
        let (drawn, width, height, unwrapped) = self.with_measure(|measure| {
            let (width, height) = measure.size(source, font, line_height);
            if width <= max_width {
                return (source.to_owned(), width, height, true);
            }
            let drawn = measure.wrap(source, max_width, font);
            let (width, height) = measure.size(&drawn, font, line_height);
            (drawn, width, height, false)
        });
        element.text = Some(drawn);
        element.original_text = Some(source.to_owned());
        // Off the *source*'s width, not the drawn one's: a wrapped line that came out
        // narrower than the width it was wrapped at is still a fixed-width text, and
        // `autoResize: !isTextUnwrapped` is the oracle's answer to the source
        // (`App.tsx@1118751f:5028`, `:5048`).
        element.auto_resize = Some(unwrapped);
        element.width = width;
        element.height = height;
        // Centred on the pointer's `x` and on the running line: the oracle's `startX` /
        // `startY` (`App.tsx@1118751f:5038-5039`).
        element.x = x - width / 2.0;
        element.y = y - height / 2.0;
        element
    }

    /// Plain text pasted as text elements, one per line, the way the oracle's
    /// `addTextFromPaste` makes them (`App.tsx@1118751f:4979-5096`).
    ///
    /// `at` is a **scene** point, the pointer converted by the host — the same convention
    /// `pasteJson` takes, so a host that already has a pointer for one has it for both.
    /// With no pointer the text lands where the pointer would have been at the top left of
    /// the canvas (`viewport.lastPosition` starts there, `App.viewport.ts@1118751f:438`).
    ///
    /// The other branch of a paste: `paste_json` for this app's own elements, this for
    /// everything else, as `parseClipboard` decides which is which
    /// (`clipboard.ts@1118751f:538-553`). The clipboard buffer is left alone — nothing was
    /// copied. One step of undo, the new texts selected, and the tool back on the selection
    /// (`App.tsx@1118751f:5073-5080`, `:4809-4812`).
    pub fn paste_text(&mut self, text: &str, at: Option<(f64, f64)>) -> bool {
        let at = at.map_or_else(
            || crate::screen_to_world(self.camera, 0.0, 0.0),
            |(x, y)| Point { x, y },
        );
        let elements = self.pasted_text_elements(text, at);
        if elements.is_empty() {
            return false;
        }
        let ids: Vec<String> = elements.iter().map(|el| el.id.clone()).collect();
        for element in elements {
            self.scene.add(element);
        }
        self.set_tool(DrawTool::Select);
        self.set_selection(ids);
        // One step, stamped like any other local edit: a pasted element that reached the
        // host unstamped was never saved, and one that reused an id was an edit of
        // something else.
        self.push_history();
        true
    }
}
