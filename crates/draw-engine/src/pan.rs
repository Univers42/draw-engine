//! The secondary-button pan session: a right-button press that is a pan only once the
//! pointer has travelled past the drag threshold, and the two flags that let one gesture
//! be both a right-click and a pan without the platform's `contextmenu` getting in the way.
//!
//! Excalidraw's `AppPan` (`packages/excalidraw/components/App.pan.ts`@1118751f), whose
//! doc comment (`:12-25`) is the whole design: *"A secondary-button session is a pan only
//! once the pointer travels past the drag threshold; released before that, it is a
//! right-click. The platform's `contextmenu` event opens the menu for a right-click where
//! it follows the release (Windows), as it always has; where it comes with the press
//! (macOS, Linux) it is swallowed — a drag cannot be told from a click yet — and the
//! session opens the menu on release instead."*
//!
//! So the answer to "a right-drag cannot also open a context menu" is one 5px threshold and
//! one latch, and the platform difference is absorbed by remembering whether the platform
//! has already sent its event. There is no mode, no setting and no key.
//!
//! **Threshold and flag live here, not in a host.** The distance, the comparison, the
//! latch and the answer to "does this release owe the host a menu" are all control data:
//! every host that drew a board would otherwise answer them its own way.

use crate::camera::Point;

/// How far the pointer must travel from the press before a right-button session is a pan
/// rather than a right-click (`App.pan.ts@1118751f:10`, `SECONDARY_BUTTON_PAN_THRESHOLD`).
///
/// The comparison is `<=` against it, as the oracle's is (`:139-141`): a pointer exactly
/// this far from the press has not gone anywhere, and is a click.
pub const SECONDARY_BUTTON_PAN_THRESHOLD: f64 = 5.0;

/// What a right-button press did, for the host to act on (`App.pan.ts@1118751f:88-125`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SecondaryPanStart {
    /// No session: more than one pointer is down, so this press is part of a
    /// multi-finger gesture and not a pan (`getPointerCount() <= 1`, `:95`).
    Declined = 0,
    /// A session is live, and the host prevents the pointerdown's default so the browser
    /// neither scrolls the page nor steals the focus (`:114-125`, issue #4489).
    Started = 1,
    /// A session is live, and the host must **not** prevent the default: the text being
    /// typed into needs the caret and the focus, which preventing it breaks
    /// (`:118-125`, "as such, the above is broken when panning canvas while in wysiwyg").
    StartedWhileEditingText = 2,
}

/// What one move of a right-button session did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondaryStep {
    /// Not far enough from the press yet: still a right-click, and nothing is consumed.
    Hold,
    /// The threshold is behind us. The pointer is the pan's origin from here, so this move
    /// moves nothing (`:144-149`).
    Engaged,
}

/// What a right-button release owes the platform, and the host
/// (`App.pan.ts@1118751f:209-264`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SecondaryPanEnd {
    /// The host opens nothing. Either the platform's `contextmenu` was already swallowed
    /// with the press, or it is still to come and is a click of its own accord.
    None = 0,
    /// A drag whose release the platform's `contextmenu` follows. That event is not a
    /// click, so the next one is swallowed (`:216-219`).
    Drag = 1,
    /// A right-click, and the platform's event was swallowed with the press, so the menu
    /// opens here, at the release point (`:251-264`).
    OpenMenu = 2,
}

/// One live right-button session.
#[derive(Clone, Copy, Debug)]
pub struct SecondaryPan {
    press: Point,
    engaged: bool,
    /// The platform's `contextmenu` has been seen, and swallowed. Which side of the
    /// session it arrived on is the whole platform difference, and it decides whether the
    /// release has to open the menu itself.
    native_menu_seen: bool,
}

impl SecondaryPan {
    pub fn start(press: Point) -> Self {
        Self {
            press,
            engaged: false,
            native_menu_seen: false,
        }
    }

    /// Whether this session has committed to being a pan.
    pub fn engaged(&self) -> bool {
        self.engaged
    }

    /// How far the pointer is from the press, straight-line.
    pub fn travel(&self, at: Point) -> f64 {
        (at.x - self.press.x).hypot(at.y - self.press.y)
    }

    /// A move, and what it did. Nothing is consumed while the session holds: a right-click
    /// has to leave the board exactly where it found it.
    pub fn on_move(&mut self, at: Point) -> SecondaryStep {
        if self.engaged {
            return SecondaryStep::Engaged;
        }
        if self.travel(at) <= SECONDARY_BUTTON_PAN_THRESHOLD {
            return SecondaryStep::Hold;
        }
        self.engaged = true;
        SecondaryStep::Engaged
    }

    /// The platform's `contextmenu` for this session arrived. It is not a new click,
    /// whichever side of the session it landed on (`App.pan.ts@1118751f:74-78`).
    pub fn note_context_menu(&mut self) {
        self.native_menu_seen = true;
    }

    /// The release, and what the host owes the menu.
    ///
    /// The two conditions are independent and both are read: a drag whose release the
    /// platform's event follows has to swallow that event, and a click whose event was
    /// already swallowed has to open the menu itself. A drag that already swallowed one
    /// owes nothing either way.
    pub fn on_release(&mut self) -> SecondaryPanEnd {
        match (self.engaged, self.native_menu_seen) {
            (false, true) => SecondaryPanEnd::OpenMenu,
            (true, false) => SecondaryPanEnd::Drag,
            _ => SecondaryPanEnd::None,
        }
    }
}
