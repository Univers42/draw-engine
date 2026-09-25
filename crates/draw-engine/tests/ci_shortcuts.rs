//! Tool shortcuts, held to Excalidraw's own table.
//!
//! The table is `TOOLS` in `packages/excalidraw/components/Tools.tsx` at the SHA in
//! `scripts/oracle-sha.txt`, and it is transcribed below rather than described, because a
//! shortcut is not something to be approximately right about: it is muscle memory, and a
//! key that does the wrong thing is worse than a key that does nothing.
//!
//! Three things here are easy to get wrong and are each pinned by name:
//!
//! - **Freedraw answers to two letters**, `P` and `X`. Every other tool has one.
//! - **Autoshape is `Shift+X`**, which means the keymap cannot be a function of the key
//!   alone — and ours was, so autoshape had an invented key (`G`) instead.
//! - **Hand and eraser toggle.** Pressing their key while they are already active goes
//!   back to the tool you were using, rather than doing nothing. They are the two tools
//!   you reach for *during* something else, which is what makes the return trip matter.

use draw_engine::*;

/// Excalidraw's `TOOLS`, transcribed: tool, letter keys, numeric key.
///
/// `None` for a numeric key means the tool has none — image is the reverse case, a tool
/// with a digit and no letter, and both are deliberate in the oracle.
struct Shortcut {
    tool: DrawTool,
    letters: &'static [&'static str],
    shift: bool,
    digit: Option<&'static str>,
}

const ORACLE: &[Shortcut] = &[
    Shortcut {
        tool: DrawTool::Hand,
        letters: &["h"],
        shift: false,
        digit: None,
    },
    Shortcut {
        tool: DrawTool::Select,
        letters: &["v"],
        shift: false,
        digit: Some("1"),
    },
    Shortcut {
        tool: DrawTool::Rectangle,
        letters: &["r"],
        shift: false,
        digit: Some("2"),
    },
    Shortcut {
        tool: DrawTool::Diamond,
        letters: &["d"],
        shift: false,
        digit: Some("3"),
    },
    Shortcut {
        tool: DrawTool::Ellipse,
        letters: &["o"],
        shift: false,
        digit: Some("4"),
    },
    Shortcut {
        tool: DrawTool::Arrow,
        letters: &["a"],
        shift: false,
        digit: Some("5"),
    },
    Shortcut {
        tool: DrawTool::Line,
        letters: &["l"],
        shift: false,
        digit: Some("6"),
    },
    // Two letters, and the only tool with two.
    Shortcut {
        tool: DrawTool::Freedraw,
        letters: &["p", "x"],
        shift: false,
        digit: Some("7"),
    },
    Shortcut {
        tool: DrawTool::Text,
        letters: &["t"],
        shift: false,
        digit: Some("8"),
    },
    // A digit and no letter. `shortkey.md` claims `I`; the oracle gives image no letter
    // at all, and the oracle is what this project is held to.
    Shortcut {
        tool: DrawTool::Image,
        letters: &[],
        shift: false,
        digit: Some("9"),
    },
    Shortcut {
        tool: DrawTool::Eraser,
        letters: &["e"],
        shift: false,
        digit: Some("0"),
    },
    Shortcut {
        tool: DrawTool::Frame,
        letters: &["f"],
        shift: false,
        digit: None,
    },
    // The one chord in the table.
    Shortcut {
        tool: DrawTool::AutoShape,
        letters: &["x"],
        shift: true,
        digit: None,
    },
    Shortcut {
        tool: DrawTool::Laser,
        letters: &["k"],
        shift: false,
        digit: None,
    },
    Shortcut {
        tool: DrawTool::BucketFill,
        letters: &["b"],
        shift: false,
        digit: None,
    },
    // A letter and no digit: `stickynote` in `Tools.tsx@1118751f:121-124`.
    Shortcut {
        tool: DrawTool::StickyNote,
        letters: &["n"],
        shift: false,
        digit: None,
    },
];

#[test]
fn every_shortcut_in_the_oracle_selects_the_tool_it_names() {
    for entry in ORACLE {
        for letter in entry.letters {
            assert_eq!(
                tool_for_chord(letter, entry.shift),
                Some(entry.tool),
                "{letter} (shift: {}) should be {:?}",
                entry.shift,
                entry.tool
            );
        }
        if let Some(digit) = entry.digit {
            assert_eq!(
                tool_for_chord(digit, false),
                Some(entry.tool),
                "{digit} should be {:?}",
                entry.tool
            );
        }
    }
}

#[test]
fn freedraw_answers_to_both_of_its_letters() {
    // `X` is the one people who came from other editors reach for, and it was missing.
    assert_eq!(tool_for_chord("p", false), Some(DrawTool::Freedraw));
    assert_eq!(tool_for_chord("x", false), Some(DrawTool::Freedraw));
}

#[test]
fn autoshape_is_the_shifted_freedraw_key() {
    // The reason the keymap needs the modifier at all. Shift+X is a *different tool* from
    // X, not a variation of it, so a keymap that only looks at the key cannot express it
    // — and ours could not, so autoshape was given an invented key instead.
    assert_eq!(tool_for_chord("x", true), Some(DrawTool::AutoShape));
    assert_eq!(tool_for_chord("x", false), Some(DrawTool::Freedraw));
}

#[test]
fn shift_does_not_change_any_other_tool_key() {
    // Excalidraw made tool shortcuts case-insensitive and dropped Shift+letter as a
    // separate binding, so holding shift must be inert everywhere except the one chord.
    for entry in ORACLE {
        for letter in entry.letters {
            if entry.shift || *letter == "x" {
                continue;
            }
            assert_eq!(
                tool_for_chord(letter, true),
                Some(entry.tool),
                "shift+{letter} should still be {:?}",
                entry.tool
            );
        }
    }
}

#[test]
fn shortcuts_ignore_the_shift_key_that_capitalised_them() {
    // A keyboard sends "R" when shift is down, not "r". Lower-casing is what makes the
    // shortcut work with caps lock on, which Excalidraw specifically fixed.
    assert_eq!(tool_for_chord("R", false), Some(DrawTool::Rectangle));
    assert_eq!(tool_for_chord("V", false), Some(DrawTool::Select));
    assert_eq!(tool_for_chord("X", true), Some(DrawTool::AutoShape));
}

#[test]
fn every_tool_can_be_reached_from_the_keyboard() {
    // A tool with no key is unreachable on a keyboard-driven board. Two of ours have no
    // key in the oracle — Excalidraw reaches the lasso through a selection-tool mode and
    // the embed through a menu — so ours are deliberate additions, listed here so that
    // adding a tool and forgetting its key fails instead of shipping.
    let extra: &[(DrawTool, &str)] = &[(DrawTool::Lasso, "s"), (DrawTool::Embed, "w")];
    for (tool, key) in extra {
        assert_eq!(tool_for_chord(key, false), Some(*tool));
    }

    for tool in ALL_TOOLS {
        let reachable = ORACLE.iter().any(|entry| entry.tool == tool)
            || extra.iter().any(|(extra_tool, _)| *extra_tool == tool);
        assert!(reachable, "{tool:?} has no keyboard shortcut");
    }
}

#[test]
fn no_two_tools_share_a_chord() {
    // The failure this prevents is silent: whichever arm of the match comes first wins,
    // and the other tool simply stops answering to its key.
    // A tool may hold several chords — freedraw holds three. A *chord* holding two tools
    // is the mistake, so the map is keyed by chord and a repeat is the failure.
    let mut claimed: std::collections::HashMap<(char, bool), DrawTool> =
        std::collections::HashMap::new();
    for key in "abcdefghijklmnopqrstuvwxyz0123456789".chars() {
        for shift in [false, true] {
            let Some(tool) = tool_for_chord(&key.to_string(), shift) else {
                continue;
            };
            if let Some(previous) = claimed.insert((key, shift), tool) {
                assert_eq!(
                    previous, tool,
                    "{key} (shift: {shift}) is claimed by two tools"
                );
            }
        }
    }
    assert!(!claimed.is_empty());
}

// ---------------------------------------------------------------------------
// Toggle tools
// ---------------------------------------------------------------------------

#[test]
fn the_hand_and_the_eraser_are_the_tools_that_toggle() {
    assert!(is_toggle_tool(DrawTool::Hand));
    assert!(is_toggle_tool(DrawTool::Eraser));
    for tool in ALL_TOOLS {
        if matches!(tool, DrawTool::Hand | DrawTool::Eraser) {
            continue;
        }
        assert!(!is_toggle_tool(tool), "{tool:?} should not toggle");
    }
}

#[test]
fn pressing_the_eraser_key_twice_goes_back_to_what_you_were_doing() {
    // The whole point of a toggle tool. You are drawing rectangles, you rub something
    // out, and you want to be drawing rectangles again — without that, the eraser costs
    // two keystrokes to leave and you lose your place.
    let mut engine = DrawEngine::new();
    engine.activate_tool(DrawTool::Rectangle);
    engine.activate_tool(DrawTool::Eraser);
    assert_eq!(engine.get_tool(), DrawTool::Eraser);

    engine.activate_tool(DrawTool::Eraser);
    assert_eq!(engine.get_tool(), DrawTool::Rectangle);
}

#[test]
fn the_hand_returns_to_the_tool_it_interrupted() {
    let mut engine = DrawEngine::new();
    engine.activate_tool(DrawTool::Arrow);
    engine.activate_tool(DrawTool::Hand);
    engine.activate_tool(DrawTool::Hand);
    assert_eq!(engine.get_tool(), DrawTool::Arrow);
}

#[test]
fn a_toggle_tool_returns_to_select_when_there_is_nothing_to_return_to() {
    // Pressing H first thing on a fresh board. Going back to "nothing" would leave the
    // board with no tool at all.
    let mut engine = DrawEngine::new();
    engine.activate_tool(DrawTool::Hand);
    engine.activate_tool(DrawTool::Hand);
    assert_eq!(engine.get_tool(), DrawTool::Select);
}

#[test]
fn a_toggle_tool_does_not_remember_itself_as_the_way_back() {
    // Press E, press E, press E. The second press leaves the eraser, so the third has to
    // enter it again — not bounce between the eraser and the eraser.
    let mut engine = DrawEngine::new();
    engine.activate_tool(DrawTool::Ellipse);
    engine.activate_tool(DrawTool::Eraser);
    engine.activate_tool(DrawTool::Eraser);
    engine.activate_tool(DrawTool::Eraser);
    assert_eq!(engine.get_tool(), DrawTool::Eraser);
    engine.activate_tool(DrawTool::Eraser);
    assert_eq!(engine.get_tool(), DrawTool::Ellipse);
}

#[test]
fn a_tool_that_does_not_toggle_stays_put_when_its_key_is_pressed_again() {
    let mut engine = DrawEngine::new();
    engine.activate_tool(DrawTool::Rectangle);
    engine.activate_tool(DrawTool::Rectangle);
    assert_eq!(engine.get_tool(), DrawTool::Rectangle);
}

#[test]
fn set_tool_still_means_set_tool() {
    // `activate_tool` is the keyboard's door; `set_tool` is the toolbar's. Clicking the
    // eraser button twice must not turn the eraser off — the button shows it as active,
    // so turning it off would contradict what is on screen.
    let mut engine = DrawEngine::new();
    engine.set_tool(DrawTool::Rectangle);
    engine.set_tool(DrawTool::Eraser);
    engine.set_tool(DrawTool::Eraser);
    assert_eq!(engine.get_tool(), DrawTool::Eraser);
}

// ---------------------------------------------------------------------------
// The keymap is not allowed to exist twice
// ---------------------------------------------------------------------------

#[test]
fn the_typescript_keymap_says_the_same_thing_as_this_one() {
    // `engine/src/tools.ts` carries its own copy of the table because the host dispatches
    // keys before the engine sees them. Two copies of a mapping drift — this one already
    // had, on the very key this file was written to add — so the copy is read back as
    // text and held to the original. Hermetic: the file is in the repo, and no Node runs.
    let source =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/tools.ts"))
            .expect("engine/src/tools.ts");

    for entry in ORACLE {
        for letter in entry.letters {
            assert!(
                ts_maps(&source, letter, entry.shift, entry.tool),
                "engine/src/tools.ts is missing {letter} (shift: {}) -> {:?}",
                entry.shift,
                entry.tool
            );
        }
        if let Some(digit) = entry.digit {
            assert!(
                ts_maps(&source, digit, false, entry.tool),
                "engine/src/tools.ts is missing {digit} -> {:?}",
                entry.tool
            );
        }
    }
}

/// Whether `tools.ts` maps this chord to this tool, read out of the source text.
///
/// The TS table writes a plain key as `x: "freedraw"` (or `"1": "select"` when the key is
/// not an identifier) and a chord as `"shift+x": "autoshape"`, so the lookup is the same
/// shape the runtime does.
fn ts_maps(source: &str, key: &str, shift: bool, tool: DrawTool) -> bool {
    let chord = if shift {
        format!("shift+{key}")
    } else {
        key.to_string()
    };
    let quoted = format!("\"{chord}\": \"{}\"", tool.as_str());
    let bare = format!("\n  {chord}: \"{}\"", tool.as_str());
    source.contains(&quoted) || source.contains(&bare)
}

#[test]
fn tool_for_key_is_the_unshifted_chord() {
    // The old single-argument entry point still exists and still answers, so nothing that
    // never cared about modifiers had to change.
    for key in ["v", "r", "7", "b", "h"] {
        assert_eq!(tool_for_key(key), tool_for_chord(key, false));
    }
}
