//! The engine side of the right-button pan session: starting one on a press, feeding it
//! the moves, and ending it on the release. The rules themselves are in
//! [`crate::pan`]; what is here is the gesture they hang off, and the two flags that
//! survive the gesture, because a `contextmenu` can arrive with either end of it.
//!
//! Excalidraw keeps this in `AppPan` (`App.pan.ts`@1118751f) and calls it from two places:
//! the pointer down (`App.tsx@1118751f:8698`, which returns before the press becomes a
//! scene gesture) and the context menu (`App.tsx@1118751f:13234`).

use crate::camera::Point;
use crate::engine::{DrawEngine, Interaction};
use crate::pan::{SecondaryPan, SecondaryPanEnd, SecondaryPanStart, SecondaryStep};

impl DrawEngine {
    /// Starts a right-button session, or declines it.
    ///
    /// `pointers_down` is the host's count of pointers on the canvas, which is the only
    /// thing here the engine cannot know for itself: more than one means a two-finger
    /// gesture, and a pan must not fight it (`getPointerCount() <= 1`, `App.pan.ts:95`).
    ///
    /// Whatever the last session left pending is dropped first (`:90-91`), or the
    /// right-click after a right-drag would open no menu at all.
    pub fn begin_secondary_pan(
        &mut self,
        sx: f64,
        sy: f64,
        pointers_down: u32,
    ) -> SecondaryPanStart {
        self.suppress_next_context_menu = false;
        if pointers_down > 1 {
            return SecondaryPanStart::Declined;
        }
        self.secondary_pan = Some(SecondaryPan::start(Point { x: sx, y: sy }));
        // A pan is not about the path a press finished (`types.rs:195`), and it takes the
        // gesture over: a right-button press is never a scene gesture in the oracle, which
        // returns from the pointer down before creating one (`App.tsx:8698-8700`).
        self.finished_by_press = None;
        self.interaction = Some(Interaction::SecondaryPan);
        if self.is_editing_text() {
            SecondaryPanStart::StartedWhileEditingText
        } else {
            SecondaryPanStart::Started
        }
    }

    /// Ends a right-button session and says what the host owes the menu
    /// (`App.pan.ts:209-264`).
    pub fn end_secondary_pan(&mut self) -> SecondaryPanEnd {
        let end = match self.secondary_pan.take() {
            Some(mut session) => session.on_release(),
            None => SecondaryPanEnd::None,
        };
        if end == SecondaryPanEnd::Drag {
            // The platform's `contextmenu` is still to come, and it is not a click.
            self.suppress_next_context_menu = true;
        }
        // Only the gesture this session started. Two buttons at once is turned away by
        // the pointer count, and a release that ended somebody else's drag would drop the
        // shapes it was carrying.
        if matches!(
            self.interaction,
            Some(Interaction::SecondaryPan | Interaction::Pan { .. })
        ) {
            self.end_pointer();
        }
        end
    }

    /// Whether a `contextmenu` event belongs to a session and must not open the menu: it
    /// came with the press, so the session decides on release, or it follows a release
    /// that turned out to be a drag (`App.pan.ts:74-84`).
    pub fn consumes_context_menu(&mut self) -> bool {
        if let Some(session) = self.secondary_pan.as_mut() {
            session.note_context_menu();
            return true;
        }
        if self.suppress_next_context_menu {
            self.suppress_next_context_menu = false;
            return true;
        }
        false
    }

    /// Whether the host should forward pointer moves that arrive with no button of its own
    /// down. A path being placed point by point follows the cursor between its clicks, and
    /// so does a right-button session deciding whether it is a click or a pan: both are
    /// gestures the host cannot see from the button alone.
    pub fn wants_pointer_moves(&self) -> bool {
        self.multi_linear.is_some() || self.secondary_pan.is_some()
    }

    /// Whether a text is open for typing, which is what decides whether a press may
    /// prevent its own default (`App.pan.ts:118-125`).
    ///
    /// Not on the WASM surface: the host is told in [`Self::begin_secondary_pan`]'s answer,
    /// because a host with this as a method of its own would be deciding it per press.
    pub fn is_editing_text(&self) -> bool {
        self.editing_text_id().is_some()
    }

    /// A move of a live session (`App.pan.ts:135-207`).
    ///
    /// Both arms return a gesture, and the difference between them is the whole design:
    /// one keeps the session as it was, the other hands the gesture to
    /// [`Interaction::Pan`] with its origin **at this move** — so the distance travelled to
    /// reach the threshold is never caught up (`:146-148`, "pans from here on; the
    /// threshold distance is not caught up"). Losing that origin is the one line a port
    /// loses, and the board jumps by up to the threshold on the first move after.
    pub(crate) fn advance_secondary_pan(&mut self, sx: f64, sy: f64) -> Option<Interaction> {
        let at = Point { x: sx, y: sy };
        let step = self.secondary_pan.as_mut()?.on_move(at);
        match step {
            // A right-click so far: still the session, and nothing consumed.
            SecondaryStep::Hold => Some(Interaction::SecondaryPan),
            // The threshold is behind us. The pan starts from here.
            SecondaryStep::Engaged => Some(Interaction::Pan {
                last_x: sx,
                last_y: sy,
            }),
        }
    }
}
