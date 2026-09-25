//! The native sticky note: Excalidraw's `stickynote` (`packages/element/src/stickyNote.ts`,
//! its tests `packages/element/src/__tests__/stickyNote.test.ts` and
//! `packages/excalidraw/tests/stickyNotes.test.tsx`, all `@1118751f`).
//!
//! One element with a painted shadow and date, holding an ordinary label whose font is
//! fitted under a ceiling (`baseFontSize`) before the note grows past its base height.
//! The cases below are the oracle's, driven through the engine the way a person drives
//! it — the tool, the typing session, the handles, the panel — wherever that is possible.
//!
//! Measured with the test hook: a char is `size / 2` wide plus 4 per line, and a line is
//! `size × 1.25` tall. So at the 28px ceiling "short" is 74 × 35, and a note's 250 leaves
//! 218 to wrap in and 198 of body under the footer.

mod common;
use common::*;
use draw_engine::engine::ColorDomain;
use draw_engine::scene::sticky::*;
use draw_engine::text::layout::{bound_text_max_height, sticky_layout, Measure};
use draw_engine::text::{FontKey, MeasureCache};
use draw_engine::*;

const FONT: f64 = 28.0;
const SIZE: f64 = DEFAULT_STICKY_NOTE_SIZE;

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .unwrap_or_else(|| panic!("{id} is in the scene"))
}

fn live(engine: &DrawEngine) -> Vec<DrawElement> {
    engine
        .get_scene()
        .into_iter()
        .filter(|el| !el.is_deleted)
        .collect()
}

fn notes(engine: &DrawEngine) -> Vec<DrawElement> {
    live(engine)
        .into_iter()
        .filter(|el| el.kind == DrawElementType::StickyNote)
        .collect()
}

/// Forty lines of 24 chars: at the 16px floor each is one line of 196, 800 tall — more
/// than any 250 note holds, so the note grows to 852.
fn overflowing() -> String {
    vec!["abcdefghijklmnopqrstuvwx"; 40].join("\n")
}

/// A click with the sticky note tool at `at`, at the next text size `font`: the note and
/// the label the click opened for typing.
fn place(engine: &mut DrawEngine, at: (f64, f64)) -> (String, String) {
    engine.set_tool(DrawTool::StickyNote);
    engine.begin_pointer(at.0, at.1, false, false);
    engine.end_pointer();
    let request = engine
        .drain_events()
        .text_edit
        .expect("a new note opens its label for typing");
    let note = request
        .container_id
        .clone()
        .expect("the text opened is the note's label");
    (note, request.id)
}

/// A 250 note at (100, 100) with `text` typed into it at a 28px ceiling, as a person
/// makes one: the tool, a click, the typing session. Returns the engine, the note's id
/// and its label's.
fn note_with(text: &str) -> (DrawEngine, String, String) {
    let mut engine = engine_with_measure(vec![]);
    engine.set_font_size(FONT);
    let (note, label) = place(&mut engine, (225.0, 225.0));
    engine.update_text_edit(text);
    engine.commit_text_edit(text, true);
    engine.drain_events();
    (engine, note, label)
}

/// The engine's own handle ring at 1:1.
fn handle(note: &DrawElement, kind: HandleKind) -> (f64, f64) {
    let handle = selection_handles(note, HandleLayout::screen(8.0, 26.0, 1.0))
        .into_iter()
        .find(|h| h.kind == kind)
        .unwrap_or_else(|| panic!("the note offers {kind:?}"));
    (handle.x, handle.y)
}

/// Grabs `kind` of the selected note and drags it by `by`, in steps, Shift as given.
fn resize(engine: &mut DrawEngine, note: &str, kind: HandleKind, by: (f64, f64), shift: bool) {
    let from = handle(&element(engine, note), kind);
    engine.begin_pointer(from.0, from.1, false, false);
    for step in 1..=4 {
        let t = f64::from(step) / 4.0;
        engine.move_pointer(from.0 + by.0 * t, from.1 + by.1 * t, shift, false);
    }
    engine.end_pointer();
}

/// The test hook's measure, for the layout on its own.
fn measured<T>(body: impl FnOnce(&Measure) -> T) -> T {
    let cache = MeasureCache::new();
    let line_width = |line: &str, font: FontKey| line.len() as f64 * font.size() * 0.5 + 4.0;
    body(&Measure {
        cache: &cache,
        line_width: &line_width,
    })
}

fn centre(el: &DrawElement) -> Point {
    Point {
        x: el.x + el.width / 2.0,
        y: el.y + el.height / 2.0,
    }
}

/// `p` turned by `angle` about `about`.
fn turned(p: Point, about: Point, angle: f64) -> Point {
    let (sin, cos) = angle.sin_cos();
    let (dx, dy) = (p.x - about.x, p.y - about.y);
    Point {
        x: about.x + dx * cos - dy * sin,
        y: about.y + dx * sin + dy * cos,
    }
}

fn assert_near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "expected {a} ~= {b}");
}

mod the_tool {
    use super::*;

    /// `App.tsx@1118751f:11820-11836`: a click is the default square, centred on the press,
    /// in the notes' own colours, dated, and the tool gives way to typing into it.
    #[test]
    fn a_click_places_the_default_note_centred_on_it_and_opens_its_label() {
        let mut engine = engine_with_measure(vec![]);
        let (note_id, label_id) = place(&mut engine, (400.0, 300.0));

        let note = element(&engine, &note_id);
        assert_eq!(note.kind, DrawElementType::StickyNote);
        assert_near(note.x, 275.0);
        assert_near(note.y, 175.0);
        assert_near(note.width, SIZE);
        assert_near(note.height, SIZE);
        assert_eq!(note.base_height, Some(SIZE));
        assert_eq!(note.background_color, DEFAULT_STICKY_NOTE_BG);
        assert_eq!(note.stroke_color, DEFAULT_STICKY_NOTE_STROKE);
        assert_eq!(note.fill_style, FillStyle::Solid);
        assert!(
            note.created.is_some_and(|at| at > 1.6e12),
            "dated by the wall clock"
        );
        assert_eq!(engine.get_tool(), DrawTool::Select);

        let label = element(&engine, &label_id);
        assert_eq!(label.container_id.as_deref(), Some(note_id.as_str()));
        assert_eq!(
            label.stroke_color, note.stroke_color,
            "the label takes the note's ink"
        );
        assert_eq!(
            label.base_font_size, label.font_size,
            "its size is its ceiling"
        );
    }

    /// The label is not made in advance: one left empty goes with no trace, and the note
    /// stays, alone, at its base height (`stickyNotes.test.tsx`, "empty").
    #[test]
    fn a_note_left_empty_keeps_no_label() {
        let mut engine = engine_with_measure(vec![]);
        let (note, _) = place(&mut engine, (400.0, 300.0));
        engine.commit_text_edit("", true);

        let scene = engine.get_scene();
        assert_eq!(scene.len(), 1, "no label, not even deleted: {scene:?}");
        assert_eq!(element(&engine, &note).bound_text_id, None);
        assert_near(element(&engine, &note).height, SIZE);
    }

    /// Typing makes the note's one label — never a second note or a second text.
    #[test]
    fn typing_fills_the_notes_one_label() {
        let (engine, note, label) = note_with("hello");
        assert_eq!(notes(&engine).len(), 1);
        let texts: Vec<DrawElement> = live(&engine)
            .into_iter()
            .filter(|el| el.kind == DrawElementType::Text)
            .collect();
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0].id, label);
        assert_eq!(
            element(&engine, &note).bound_text_id.as_deref(),
            Some(label.as_str())
        );
        assert_eq!(texts[0].text.as_deref(), Some("hello"));
    }

    /// The note and what was typed into it are two steps of undo, as the oracle's
    /// creation capture and the editor's commit are.
    #[test]
    fn undo_takes_back_the_typing_then_the_note() {
        let (mut engine, note, label) = note_with("hello");
        engine.undo();
        assert!(
            live(&engine).iter().all(|el| el.id != label),
            "the typing is undone"
        );
        assert!(
            live(&engine).iter().any(|el| el.id == note),
            "the note is not"
        );
        engine.undo();
        assert!(notes(&engine).is_empty());
        engine.redo();
        engine.redo();
        assert_eq!(element(&engine, &label).text.as_deref(), Some("hello"));
    }

    /// A drag is square unless Shift frees it, where a shape is the other way round
    /// (`App.tsx@1118751f:13419-13425`, `:11839-11850`).
    #[test]
    fn a_drag_is_square_unless_shift_is_held() {
        let mut engine = engine_with_measure(vec![]);
        engine.set_tool(DrawTool::StickyNote);
        engine.begin_pointer(100.0, 100.0, false, false);
        engine.move_pointer(300.0, 200.0, false, false);
        engine.end_pointer();
        let square = notes(&engine).pop().expect("a note");
        assert_near(square.width, 200.0);
        assert_near(square.height, 200.0);
        assert_eq!(square.base_height, Some(200.0));

        let mut engine = engine_with_measure(vec![]);
        engine.set_tool(DrawTool::StickyNote);
        engine.begin_pointer(100.0, 100.0, false, false);
        engine.move_pointer(300.0, 200.0, true, false);
        engine.end_pointer();
        let free = notes(&engine).pop().expect("a note");
        assert_near(free.width, 200.0);
        assert_near(free.height, 100.0);
    }

    /// A small drag still holds one line at the next text's size (`getStickyNoteMinSize`):
    /// at 20px a 25 line, so 75 wide and 77 tall, squared to 77.
    #[test]
    fn a_small_drag_holds_one_line() {
        let mut engine = engine_with_measure(vec![]);
        engine.set_tool(DrawTool::StickyNote);
        engine.begin_pointer(100.0, 100.0, false, false);
        engine.move_pointer(130.0, 130.0, false, false);
        engine.end_pointer();
        let note = notes(&engine).pop().expect("a note");
        assert_near(note.width, 77.0);
        assert_near(note.height, 77.0);
        assert_near(note.x, 100.0);
    }

    /// Dragged up and left, the note grows away from the edge the drag left fixed.
    #[test]
    fn a_drag_up_and_left_keeps_its_far_edge() {
        let mut engine = engine_with_measure(vec![]);
        engine.set_tool(DrawTool::StickyNote);
        engine.begin_pointer(400.0, 400.0, false, false);
        engine.move_pointer(380.0, 390.0, true, false);
        engine.end_pointer();
        let note = notes(&engine).pop().expect("a note");
        assert_near(note.width, 75.0);
        assert_near(note.height, 77.0);
        assert_near(note.x + note.width, 400.0);
        assert_near(note.y + note.height, 400.0);
    }

    /// With the tool locked the next click makes another note, and nothing is selected.
    #[test]
    fn a_locked_tool_keeps_making_notes() {
        let mut engine = engine_with_measure(vec![]);
        engine.set_tool_locked(true);
        engine.set_tool(DrawTool::StickyNote);
        engine.begin_pointer(200.0, 200.0, false, false);
        engine.end_pointer();
        assert!(engine.drain_events().text_edit.is_none());
        assert_eq!(engine.get_tool(), DrawTool::StickyNote);
        assert!(engine.get_selection().is_empty());
        engine.begin_pointer(600.0, 200.0, false, false);
        engine.end_pointer();
        assert_eq!(notes(&engine).len(), 2);
    }
}

mod layout {
    use super::*;

    /// "downscales font to fit the base size before growing height": seven lines hold at
    /// 22 (192.5 of 198), the first size down the grid from 28 that fits.
    #[test]
    fn the_font_shrinks_before_the_note_grows() {
        let (engine, note, label) = note_with(&["abcdefghij"; 7].join("\n"));
        let note = element(&engine, &note);
        let label = element(&engine, &label);
        assert_near(note.height, SIZE);
        assert_eq!(note.base_height, Some(SIZE));
        assert_eq!(label.font_size, Some(22.0));
        assert_eq!(label.base_font_size, Some(FONT));
        assert!(label.x - note.x >= STICKY_NOTE_PADDING);
        assert!(label.y - note.y >= STICKY_NOTE_PADDING);
    }

    /// "grows downward at min font and shrinks back to base when text is deleted".
    #[test]
    fn at_the_floor_the_note_grows_down_and_comes_back() {
        let (mut engine, note, label) = note_with(&overflowing());
        let grown = element(&engine, &note);
        assert_eq!(
            element(&engine, &label).font_size,
            Some(STICKY_NOTE_MIN_FONT_SIZE)
        );
        assert_near(grown.height, 800.0 + STICKY_NOTE_BODY_INSET_Y);
        assert_near(grown.y, 100.0);

        engine.set_element_text(&label, "short");
        let back = element(&engine, &note);
        assert_near(back.height, SIZE);
        assert_near(back.y, 100.0);
        assert_eq!(element(&engine, &label).font_size, Some(FONT));
    }

    /// "respects user font size below the sticky note minimum font size".
    #[test]
    fn a_ceiling_under_the_floor_is_kept() {
        let (engine, note, label) = note_with("short");
        let laid = measured(|measure| {
            sticky_layout(
                &element(&engine, &note),
                Some(&element(&engine, &label)),
                &StickyLayoutOpts {
                    base_font_size: Some(15.0),
                    ..StickyLayoutOpts::default()
                },
                measure,
            )
        });
        assert_eq!(laid.text.expect("a label").font_size, Some(15.0));
    }

    /// "normalizes non-finite and out-of-range font ceilings".
    #[test]
    fn a_ceiling_is_normalized() {
        assert_eq!(
            normalize_sticky_font_size(f64::NAN),
            STICKY_NOTE_FALLBACK_FONT_SIZE
        );
        assert_eq!(
            normalize_sticky_font_size(f64::INFINITY),
            STICKY_NOTE_FALLBACK_FONT_SIZE
        );
        assert_eq!(
            normalize_sticky_font_size(f64::NEG_INFINITY),
            STICKY_NOTE_FALLBACK_FONT_SIZE
        );
        assert_eq!(normalize_sticky_font_size(1e20), STICKY_NOTE_MAX_FONT_SIZE);
        assert_eq!(normalize_sticky_font_size(0.0), 1.0);
        assert_eq!(normalize_sticky_font_size(24.0), 24.0);
    }

    /// "terminates the font fit for pathological font ceilings".
    #[test]
    fn a_pathological_ceiling_still_ends() {
        let (engine, note, label) = note_with("some text that must wrap");
        for ceiling in [1e20, f64::INFINITY, f64::NAN] {
            let mut label = element(&engine, &label);
            label.base_font_size = Some(ceiling);
            let laid = measured(|measure| {
                sticky_layout(
                    &element(&engine, &note),
                    Some(&label),
                    &StickyLayoutOpts::default(),
                    measure,
                )
            });
            let size = laid.text.expect("a label").font_size.expect("a size");
            assert!(
                size.is_finite() && size <= STICKY_NOTE_MAX_FONT_SIZE,
                "{size}"
            );
            assert!(laid.container.height.is_finite());
        }
    }

    /// "honors a lowered ceiling even when the old fitted size still fits".
    #[test]
    fn a_lowered_ceiling_is_honored() {
        let (engine, note, label) = note_with("short");
        let laid = measured(|measure| {
            sticky_layout(
                &element(&engine, &note),
                Some(&element(&engine, &label)),
                &StickyLayoutOpts {
                    base_font_size: Some(20.0),
                    ..StickyLayoutOpts::default()
                },
                measure,
            )
        });
        let text = laid.text.expect("a label");
        assert_eq!(text.font_size, Some(20.0));
        assert_eq!(text.base_font_size, Some(20.0));
    }

    /// "keeps odd and fractional ceilings reachable and stays on the ceiling-anchored grid".
    #[test]
    fn the_grid_is_anchored_at_the_ceiling() {
        let (engine, note, label) = note_with("short");
        let (note, label) = (element(&engine, &note), element(&engine, &label));
        let fit = |text: Option<&str>, ceiling: f64| {
            measured(|measure| {
                sticky_layout(
                    &note,
                    Some(&label),
                    &StickyLayoutOpts {
                        original_text: text.map(str::to_owned),
                        base_font_size: Some(ceiling),
                        ..StickyLayoutOpts::default()
                    },
                    measure,
                )
            })
            .text
            .and_then(|text| text.font_size)
            .expect("a size")
        };
        assert_eq!(fit(None, 27.0), 27.0);
        assert_eq!(fit(None, 27.5), 27.5);
        let fitted = fit(Some(&["abcdefghij"; 7].join("\n")), 27.0);
        assert!(fitted < 27.0);
        assert!(fitted == STICKY_NOTE_MIN_FONT_SIZE || (27.0 - fitted) % 2.0 == 0.0);
    }

    /// "fits in a handful of measurements": a keystroke near the last size tries at most
    /// two sizes, a cold search from the 512 ceiling at most twelve.
    #[test]
    fn the_fit_tries_few_sizes() {
        let (engine, note, label) = note_with("hello world");
        let (note, label) = (element(&engine, &note), element(&engine, &label));
        let sizes = |label: &DrawElement, opts: StickyLayoutOpts| {
            let tried = std::cell::RefCell::new(std::collections::HashSet::new());
            let cache = MeasureCache::new();
            let line_width = |line: &str, font: FontKey| {
                tried.borrow_mut().insert(font.size().to_bits());
                line.len() as f64 * font.size() * 0.5 + 4.0
            };
            sticky_layout(
                &note,
                Some(label),
                &opts,
                &Measure {
                    cache: &cache,
                    line_width: &line_width,
                },
            );
            tried.into_inner().len()
        };
        let warm = StickyLayoutOpts {
            original_text: Some("hello world!".into()),
            ..StickyLayoutOpts::default()
        };
        assert!(sizes(&label, warm) <= 2);
        let mut hot = label.clone();
        hot.font_size = Some(STICKY_NOTE_MAX_FONT_SIZE);
        let cold = StickyLayoutOpts {
            original_text: Some(overflowing()),
            base_font_size: Some(STICKY_NOTE_MAX_FONT_SIZE),
            ..StickyLayoutOpts::default()
        };
        assert!(sizes(&hot, cold) <= 12);
    }

    /// "sizes the minimum note to fit one line at the ceiling".
    #[test]
    fn the_least_note_holds_one_line() {
        assert_eq!(
            sticky_min_size(20.0, 1.25),
            (STICKY_NOTE_MIN_SIZE, 25.0 + 52.0)
        );
        assert_eq!(sticky_min_size(48.0, 1.25), (60.0 + 32.0, 60.0 + 52.0));
    }

    /// "reserves the footer below the label body".
    #[test]
    fn the_footer_is_kept_out_of_the_body() {
        let (engine, note, label) = note_with("A");
        let note = element(&engine, &note);
        assert_near(
            bound_text_max_height(&note, element(&engine, &label).height),
            note.height - STICKY_NOTE_BODY_INSET_Y,
        );
    }

    /// "centers a middle-aligned label in the whole note, footer ignored", at three angles.
    #[test]
    fn a_middle_label_is_centred_in_the_whole_note() {
        for angle in [
            0.0,
            std::f64::consts::FRAC_PI_4,
            std::f64::consts::FRAC_PI_2,
        ] {
            let (engine, note, label) = note_with("Balanced");
            let mut note = element(&engine, &note);
            note.angle = angle;
            let laid = measured(|measure| {
                sticky_layout(
                    &note,
                    Some(&element(&engine, &label)),
                    &StickyLayoutOpts::default(),
                    measure,
                )
            });
            let text = laid.text.expect("a label");
            let back = turned(centre(&text), centre(&laid.container), -angle);
            assert_near(back.x, centre(&laid.container).x);
            assert_near(back.y, centre(&laid.container).y);
            assert_near(text.angle, angle);
        }
    }

    /// "keeps bottom-aligned text above the date footer".
    #[test]
    fn a_bottom_label_sits_above_the_footer() {
        for angle in [
            0.0,
            std::f64::consts::FRAC_PI_4,
            std::f64::consts::FRAC_PI_2,
        ] {
            let (engine, note, label) = note_with("Last line");
            let mut note = element(&engine, &note);
            note.angle = angle;
            let mut label = element(&engine, &label);
            label.vertical_align = Some(VerticalAlign::Bottom);
            let laid = measured(|measure| {
                sticky_layout(&note, Some(&label), &StickyLayoutOpts::default(), measure)
            });
            let text = laid.text.expect("a label");
            let back = turned(centre(&text), centre(&laid.container), -angle);
            assert_near(
                back.y + text.height / 2.0,
                note.y + note.height - STICKY_NOTE_PADDING - STICKY_NOTE_FOOTER_HEIGHT,
            );
            assert_near(laid.container.height, SIZE);
        }
    }

    /// "keeps the rotated top edge in place while the note grows".
    #[test]
    fn a_turned_note_grows_from_its_top_edge() {
        let (engine, note, label) = note_with("short");
        let mut note = element(&engine, &note);
        note.angle = 0.6;
        let top = |el: &DrawElement| {
            turned(
                Point {
                    x: el.x + el.width / 2.0,
                    y: el.y,
                },
                centre(el),
                el.angle,
            )
        };
        let before = top(&note);
        let laid = measured(|measure| {
            sticky_layout(
                &note,
                Some(&element(&engine, &label)),
                &StickyLayoutOpts {
                    original_text: Some(overflowing()),
                    ..StickyLayoutOpts::default()
                },
                measure,
            )
        });
        assert!(laid.container.height > note.height);
        let after = top(&laid.container);
        assert_near(after.x, before.x);
        assert_near(after.y, before.y);
    }

    /// "preserves rounded corners with reduced radius": the short side's 4%, capped at 16.
    #[test]
    fn a_rounded_note_has_a_small_corner() {
        let mut note = create_element_default(
            DrawElementType::StickyNote,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: SIZE,
                height: SIZE,
            },
        );
        note.roundness = Some(0.0);
        assert_near(sticky_corner_radius(&note), 10.0);
        note.width = 1000.0;
        note.height = 1000.0;
        assert_near(sticky_corner_radius(&note), 16.0);
        note.roundness = None;
        assert_near(sticky_corner_radius(&note), 0.0);
    }
}

mod resizing {
    use super::*;

    /// A corner keeps the note's proportions by default, and its ceiling scales with it
    /// ("scales the note, its base height and its font ceiling on a proportional resize";
    /// `App.tsx@1118751f:13642-13650`).
    #[test]
    fn a_corner_scales_the_note_and_its_ceiling() {
        let (mut engine, note, label) = note_with("short");
        engine.select(vec![note.clone()]);
        resize(&mut engine, &note, HandleKind::Se, (250.0, 250.0), false);
        let after = element(&engine, &note);
        assert_near(after.width, 500.0);
        assert_near(after.height, 500.0);
        assert_eq!(after.base_height, Some(500.0));
        let label = element(&engine, &label);
        assert_eq!(label.base_font_size, Some(FONT * 2.0));
        assert_eq!(label.font_size, Some(FONT * 2.0));
    }

    /// "restores the gesture-start base height and ceiling when Shift is released
    /// mid-gesture" — here Shift is what frees a corner, so pressing it gives them back.
    #[test]
    fn shift_mid_gesture_gives_back_the_start_ceiling() {
        let (mut engine, note, label) = note_with("short");
        engine.select(vec![note.clone()]);
        let from = handle(&element(&engine, &note), HandleKind::Se);
        engine.begin_pointer(from.0, from.1, false, false);
        engine.move_pointer(from.0 + 250.0, from.1 + 250.0, false, false);
        assert_eq!(element(&engine, &label).base_font_size, Some(FONT * 2.0));
        engine.move_pointer(from.0 + 150.0, from.1 + 50.0, true, false);
        engine.end_pointer();
        let after = element(&engine, &note);
        assert_near(after.width, 400.0);
        assert_near(after.height, 300.0);
        assert_eq!(after.base_height, Some(300.0));
        assert_eq!(element(&engine, &label).base_font_size, Some(FONT));
        assert_eq!(element(&engine, &label).font_size, Some(FONT));
    }

    /// "restores everything when a proportional gesture returns to scale 1".
    #[test]
    fn back_to_scale_one_is_back_to_the_start() {
        let (mut engine, note, label) = note_with("short");
        engine.select(vec![note.clone()]);
        let from = handle(&element(&engine, &note), HandleKind::Se);
        engine.begin_pointer(from.0, from.1, false, false);
        engine.move_pointer(from.0 + 250.0, from.1 + 250.0, false, false);
        engine.move_pointer(from.0, from.1, false, false);
        engine.end_pointer();
        let after = element(&engine, &note);
        assert_near(after.width, SIZE);
        assert_eq!(after.base_height, Some(SIZE));
        assert_eq!(element(&engine, &label).base_font_size, Some(FONT));
    }

    /// A side is free by default: "keeps the base height on a width-only drag of a
    /// content-grown note".
    #[test]
    fn a_side_keeps_the_base_height() {
        let (mut engine, note, _) = note_with(&overflowing());
        engine.select(vec![note.clone()]);
        resize(&mut engine, &note, HandleKind::E, (150.0, 0.0), false);
        let after = element(&engine, &note);
        assert_near(after.width, 400.0);
        assert_eq!(after.base_height, Some(SIZE));
        assert!(after.height > SIZE);
        assert_near(after.y, 100.0);
    }

    /// Shift on a side scales both, about the side's middle.
    #[test]
    fn shift_on_a_side_scales_both_about_the_middle() {
        let (mut engine, note, label) = note_with("short");
        engine.select(vec![note.clone()]);
        resize(&mut engine, &note, HandleKind::E, (250.0, 0.0), true);
        let after = element(&engine, &note);
        assert_near(after.width, 500.0);
        assert_near(after.height, 500.0);
        assert_eq!(after.base_height, Some(500.0));
        assert_near(after.y + after.height / 2.0, 225.0);
        assert_eq!(element(&engine, &label).base_font_size, Some(FONT * 2.0));
    }

    /// "keeps the bottom edge anchored when a north resize is content-pinned".
    #[test]
    fn a_pinned_north_drag_holds_the_bottom() {
        let (mut engine, note, _) = note_with(&overflowing());
        engine.select(vec![note.clone()]);
        let before = element(&engine, &note);
        resize(&mut engine, &note, HandleKind::N, (0.0, 100.0), false);
        let after = element(&engine, &note);
        assert_near(after.height, before.height);
        assert_near(after.y, before.y);
        assert_eq!(after.base_height, Some(before.height - 100.0));
    }

    /// "keeps the requested height as the base when a corner drag is content-pinned".
    #[test]
    fn a_pinned_corner_drag_remembers_the_base_asked_for() {
        let (mut engine, note, label) = note_with(&overflowing());
        engine.select(vec![note.clone()]);
        let before = element(&engine, &note);
        resize(
            &mut engine,
            &note,
            HandleKind::Se,
            (120.0 - before.width, SIZE - before.height),
            true,
        );
        let after = element(&engine, &note);
        assert_near(after.width, 120.0);
        assert_eq!(after.base_height, Some(SIZE));
        assert!(after.height > SIZE);
        assert_near(after.y, 100.0);
        let label = element(&engine, &label);
        assert_eq!(label.font_size, Some(STICKY_NOTE_MIN_FONT_SIZE));
        assert_eq!(label.base_font_size, Some(FONT));
    }

    /// "never resizes a note below one line at its ceiling plus the footer": 35 + 52.
    #[test]
    fn a_free_corner_stops_at_one_line() {
        let (mut engine, note, _) = note_with("A");
        engine.select(vec![note.clone()]);
        resize(
            &mut engine,
            &note,
            HandleKind::Se,
            (40.0 - SIZE, 40.0 - SIZE),
            true,
        );
        let after = element(&engine, &note);
        assert_near(after.width, STICKY_NOTE_MIN_SIZE);
        assert_near(after.height, 87.0);
        assert_eq!(after.base_height, Some(87.0));
    }

    /// "keeps a proportional shrink below the minimum proportional".
    #[test]
    fn a_proportional_minimum_keeps_the_square() {
        let (mut engine, note, _) = note_with("A");
        engine.select(vec![note.clone()]);
        resize(
            &mut engine,
            &note,
            HandleKind::Se,
            (60.0 - SIZE, 60.0 - SIZE),
            false,
        );
        let after = element(&engine, &note);
        assert_near(after.width, after.height);
        assert!(after.height >= 87.0 - 1e-9);
    }

    /// "flips a proportional corner resize over the far side, like a rectangle".
    #[test]
    fn a_corner_past_the_far_side_flips_the_right_way_up() {
        let (mut engine, note, label) = note_with("A");
        engine.select(vec![note.clone()]);
        resize(
            &mut engine,
            &note,
            HandleKind::Se,
            (400.0 - SIZE, -400.0 - SIZE),
            false,
        );
        let after = element(&engine, &note);
        assert_near(after.width, 400.0);
        assert_near(after.height, 400.0);
        assert_near(after.x, 100.0);
        assert_near(after.y, 100.0 - 400.0);
        let text = element(&engine, &label);
        assert_near(text.base_font_size.expect("a ceiling"), FONT * 400.0 / SIZE);
        assert_eq!(text.angle, 0.0);
        assert!(text.y > after.y && text.y + text.height < after.y + after.height);
    }

    /// "flips a free edge resize over the far side".
    #[test]
    fn a_side_past_the_far_side_flips() {
        let (mut engine, note, _) = note_with("A");
        engine.select(vec![note.clone()]);
        resize(
            &mut engine,
            &note,
            HandleKind::E,
            (-300.0 - SIZE, 0.0),
            false,
        );
        let after = element(&engine, &note);
        assert_near(after.width, 300.0);
        assert_near(after.height, SIZE);
        assert_near(after.x, 100.0 - 300.0);
    }

    /// "applies the minimum to a flipped size's magnitude".
    #[test]
    fn a_flipped_side_takes_the_minimum() {
        let (mut engine, note, _) = note_with("A");
        engine.select(vec![note.clone()]);
        resize(
            &mut engine,
            &note,
            HandleKind::E,
            (-10.0 - SIZE, 0.0),
            false,
        );
        let after = element(&engine, &note);
        assert_near(after.width, STICKY_NOTE_MIN_SIZE);
        assert_near(after.x, 100.0 - STICKY_NOTE_MIN_SIZE);
    }
}

mod group_resizing {
    use super::*;

    fn empty_note(x: f64, y: f64) -> DrawElement {
        let mut note = create_element_default(
            DrawElementType::StickyNote,
            Geometry {
                x,
                y,
                width: SIZE,
                height: SIZE,
            },
        );
        note.base_height = Some(SIZE);
        note.background_color = DEFAULT_STICKY_NOTE_BG.into();
        note
    }

    /// The south-east handle of the group's frame dragged from `from` to `to`.
    fn drag_frame(engine: &mut DrawEngine, from: (f64, f64), to: (f64, f64), shift: bool) {
        engine.begin_pointer(from.0 + 8.0, from.1 + 8.0, false, false);
        for step in 1..=4 {
            let t = f64::from(step) / 4.0;
            engine.move_pointer(
                from.0 + (to.0 - from.0) * t + 8.0,
                from.1 + (to.1 - from.1) * t + 8.0,
                shift,
                false,
            );
        }
        engine.end_pointer();
    }

    /// "uses the requested height as the base on a free multi-select resize of an empty
    /// note", and "syncs the base height of an empty note".
    #[test]
    fn an_empty_note_takes_the_height_asked_for_as_its_base() {
        let note = empty_note(0.0, 0.0);
        let rect = box_at(300.0, 0.0, 100.0, SIZE);
        let ids = vec![note.id.clone(), rect.id.clone()];
        let mut engine = engine_with_measure(vec![note.clone(), rect]);
        engine.select(ids);
        drag_frame(&mut engine, (400.0, SIZE), (800.0, 375.0), false);
        let after = element(&engine, &note.id);
        assert_near(after.width, 500.0);
        assert_near(after.height, 375.0);
        assert_eq!(after.base_height, Some(375.0));
    }

    /// "scales the label's ceiling on a proportional multi-select resize".
    #[test]
    fn a_proportional_group_resize_scales_the_ceiling() {
        let (mut engine, note, label) = note_with("short");
        let rect = box_at(400.0, 100.0, 100.0, SIZE);
        let rect_id = rect.id.clone();
        let mut scene = engine.get_scene();
        scene.push(rect);
        engine.set_scene(Scene::new(scene));
        engine.select(vec![note.clone(), rect_id]);
        drag_frame(&mut engine, (500.0, 350.0), (900.0, 600.0), true);
        let after = element(&engine, &note);
        assert_near(after.width, 500.0);
        assert_near(after.height, 500.0);
        assert_eq!(after.base_height, Some(500.0));
        let label = element(&engine, &label);
        assert_eq!(label.base_font_size, Some(FONT * 2.0));
        assert_eq!(label.font_size, Some(FONT * 2.0));
    }

    /// "flips … without promoting its grown height": a flip keeps the base and the
    /// ceiling the note had.
    #[test]
    fn a_flip_keeps_the_base_and_the_ceiling() {
        let (mut engine, note, label) = note_with(&overflowing());
        let grown = element(&engine, &note).height;
        let rect = box_at(400.0, 100.0, 100.0, SIZE);
        let rect_id = rect.id.clone();
        let mut scene = engine.get_scene();
        scene.push(rect);
        engine.set_scene(Scene::new(scene));
        engine.select(vec![note.clone(), rect_id]);
        drag_frame(
            &mut engine,
            (500.0, 100.0 + grown),
            (100.0 - 400.0, 100.0 - grown),
            true,
        );
        let after = element(&engine, &note);
        assert_eq!(after.base_height, Some(SIZE));
        assert_near(after.height, grown);
        assert_eq!(element(&engine, &label).base_font_size, Some(FONT));
    }
}

mod colours {
    use super::*;

    fn bg(color: &str) -> DrawElementStylePatch {
        DrawElementStylePatch {
            background_color: Some(color.into()),
            ..Default::default()
        }
    }

    /// With the tool in hand and nothing selected, a pick is the next note's and not the
    /// next shape's (`resolveColorTarget`, `colorTargets.ts@1118751f:131-134`).
    #[test]
    fn a_pick_with_the_tool_is_the_next_notes() {
        let mut engine = engine_with_measure(vec![]);
        engine.set_tool(DrawTool::StickyNote);
        let style = engine.selection_style();
        assert_eq!(style.background_domain, ColorDomain::Sticky);
        assert_eq!(
            style.background_color.as_deref(),
            Some(DEFAULT_STICKY_NOTE_BG)
        );
        engine.apply_style(bg("#a5d8ff"));
        assert_eq!(engine.get_next_style().background_color, "transparent");
        engine.begin_pointer(300.0, 300.0, false, false);
        engine.end_pointer();
        assert_eq!(notes(&engine)[0].background_color, "#a5d8ff");
    }

    /// Transparent is not a note's colour: its paper and its ink fall back to their
    /// defaults (`normalizeStickyNoteStyle`).
    #[test]
    fn a_note_is_never_transparent() {
        let (mut engine, note, _) = note_with("hi");
        engine.select(vec![note.clone()]);
        engine.apply_style(DrawElementStylePatch {
            background_color: Some("transparent".into()),
            stroke_color: Some("transparent".into()),
            fill_style: Some(FillStyle::Hachure),
            ..Default::default()
        });
        let after = element(&engine, &note);
        assert_eq!(after.background_color, DEFAULT_STICKY_NOTE_BG);
        assert_eq!(after.stroke_color, DEFAULT_STICKY_NOTE_STROKE);
        assert_eq!(after.fill_style, FillStyle::Solid);
    }

    /// A note and its label are one ink: a stroke pick on the label while it is typed
    /// colours the note — its date — too (`syncStickyNoteInk`).
    #[test]
    fn the_label_and_the_note_share_one_ink() {
        let (mut engine, note, label) = note_with("hi");
        engine.select(vec![label.clone()]);
        assert_eq!(engine.selection_style().stroke_domain, ColorDomain::Sticky);
        engine.apply_style(stroke_patch("#1971c2"));
        assert_eq!(element(&engine, &note).stroke_color, "#1971c2");
        assert_eq!(element(&engine, &label).stroke_color, "#1971c2");
    }

    /// A background pick on a note's label is its note's: a label has no fill.
    #[test]
    fn a_background_pick_on_a_label_colours_its_note() {
        let (mut engine, note, label) = note_with("hi");
        engine.select(vec![label.clone()]);
        engine.apply_style(bg("#b2f2bb"));
        assert_eq!(element(&engine, &note).background_color, "#b2f2bb");
        assert_eq!(element(&engine, &label).background_color, "transparent");
    }

    /// A note and a rectangle together write both domains' next colours
    /// (`getColorTargetAppStateUpdates` with `kind: "mixed"`).
    #[test]
    fn a_mixed_pick_sets_both_next_colours() {
        let (mut engine, note, _) = note_with("hi");
        let rect = box_at(500.0, 100.0, 100.0, 100.0);
        let rect_id = rect.id.clone();
        let mut scene = engine.get_scene();
        scene.push(rect);
        engine.set_scene(Scene::new(scene));
        engine.select(vec![note, rect_id.clone()]);
        assert_eq!(
            engine.selection_style().background_domain,
            ColorDomain::Mixed
        );
        engine.apply_style(bg("#fcc2d7"));
        assert_eq!(element(&engine, &rect_id).background_color, "#fcc2d7");
        engine.clear_selection();
        assert_eq!(engine.get_next_style().background_color, "#fcc2d7");
        engine.set_tool(DrawTool::StickyNote);
        assert_eq!(
            engine.selection_style().background_color.as_deref(),
            Some("#fcc2d7")
        );
    }

    /// A size picked for a note's label is its ceiling (`getBaseFontSizeUpdate`), and the
    /// panel shows the ceiling, not the size the fit shrank it to.
    #[test]
    fn a_picked_size_is_the_labels_ceiling() {
        let (mut engine, note, label) = note_with(&["abcdefghij"; 7].join("\n"));
        engine.select(vec![note.clone()]);
        assert_eq!(engine.selection_style().font_size, Some(FONT));
        engine.set_font_size(40.0);
        let label = element(&engine, &label);
        assert_eq!(label.base_font_size, Some(40.0));
        assert!(
            label.font_size.expect("a size") < 40.0,
            "the fit still shrinks it"
        );
        assert_eq!(engine.selection_style().font_size, Some(40.0));
        assert_eq!(engine.get_font_size(), 40.0);
    }

    /// Pasting a style never leaves a note transparent, nor its label's ink.
    #[test]
    fn a_pasted_style_keeps_the_note_whole() {
        let (mut engine, note, label) = note_with("hi");
        let mut rect = box_at(500.0, 100.0, 100.0, 100.0);
        rect.stroke_color = "transparent".into();
        let rect_id = rect.id.clone();
        let mut scene = engine.get_scene();
        scene.push(rect);
        engine.set_scene(Scene::new(scene));
        engine.select(vec![rect_id]);
        assert!(engine.copy_styles());
        engine.select(vec![note.clone()]);
        engine.paste_styles();
        let after = element(&engine, &note);
        assert_eq!(after.background_color, DEFAULT_STICKY_NOTE_BG);
        assert_eq!(after.stroke_color, DEFAULT_STICKY_NOTE_STROKE);
        assert_eq!(
            element(&engine, &label).stroke_color,
            DEFAULT_STICKY_NOTE_STROKE
        );
    }

    /// "sticky note ink" (`stickyNote.test.ts@1118751f:1058-1129`).
    #[test]
    fn the_ink_follows_the_side_that_changed() {
        let (red, blue) = ("#e03131", "#1971c2");
        assert_eq!(synced_ink(Some(red), red, Some(red), red), None);
        assert_eq!(
            synced_ink(Some(red), blue, Some(red), red),
            Some(blue.into())
        );
        assert_eq!(
            synced_ink(Some(red), red, Some(red), blue),
            Some(blue.into())
        );
        assert_eq!(synced_ink(None, red, None, blue), Some(blue.into()));
        assert_eq!(synced_ink(None, red, None, "transparent"), Some(red.into()));
    }
}

mod dates {
    use super::*;

    /// Noon UTC on the day, so the calendar day is the same wherever this runs.
    const NOW: f64 = 1_788_868_800_000.0; // 8 Sep 2026
    const SEP_7_2026: f64 = 1_788_782_400_000.0;
    const DEC_31_2025: f64 = 1_767_182_400_000.0;
    const JAN_1_2027: f64 = 1_798_804_800_000.0;
    const MAY_30_2025: f64 = 1_748_606_400_000.0;

    /// "formats an absolute date, short while the year is the current one".
    #[test]
    fn a_date_is_absolute_and_short_this_year() {
        assert_eq!(
            sticky_date_label(Some(SEP_7_2026), false, NOW).as_deref(),
            Some("7 Sep")
        );
        assert_eq!(
            sticky_date_label(Some(DEC_31_2025), false, NOW).as_deref(),
            Some("31 Dec 2025")
        );
        assert_eq!(
            sticky_date_label(Some(DEC_31_2025), true, NOW).as_deref(),
            Some("31 Dec")
        );
        assert_eq!(
            sticky_date_label(Some(JAN_1_2027), false, NOW).as_deref(),
            Some("1 Jan 2027")
        );
    }

    /// "hides an unknown or invalid timestamp".
    #[test]
    fn an_unknown_date_is_not_shown() {
        for created in [
            None,
            Some(f64::NAN),
            Some(f64::INFINITY),
            Some(8.64e15 + 1.0),
        ] {
            assert_eq!(sticky_date_label(created, false, NOW), None, "{created:?}");
        }
    }

    /// "picks the footer form by width bucket, without measuring".
    #[test]
    fn the_footer_form_follows_the_width() {
        let footer = |width: f64, height: f64| {
            let mut note = create_element_default(
                DrawElementType::StickyNote,
                Geometry {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                },
            );
            note.created = Some(MAY_30_2025);
            sticky_footer(&note, NOW)
        };
        let full = footer(SIZE, SIZE).expect("a footer");
        assert_eq!(full.text, "30 May 2025");
        assert_near(full.x, SIZE - STICKY_NOTE_PADDING);
        assert_near(full.y, SIZE - STICKY_NOTE_FOOTER_BASELINE_FROM_BOTTOM);
        let year_width = STICKY_NOTE_PADDING * 2.0 + STICKY_NOTE_FOOTER_MIN_BODY_WIDTH_FOR_YEAR;
        assert_eq!(
            footer(year_width, SIZE).expect("a footer").text,
            "30 May 2025"
        );
        assert_eq!(
            footer(year_width - 1.0, SIZE).expect("a footer").text,
            "30 May"
        );
        assert_eq!(
            footer(STICKY_NOTE_MIN_SIZE, STICKY_NOTE_MIN_SIZE)
                .expect("a footer")
                .text,
            "30 May"
        );
        assert!(
            footer(0.0, 0.0).is_none(),
            "the creation draft has no footer"
        );
    }
}

mod around_the_note {
    use super::*;

    /// An arrow drawn into a note binds to it, like any shape.
    #[test]
    fn an_arrow_binds_to_a_note() {
        let (mut engine, note, _) = note_with("hi");
        engine.set_tool(DrawTool::Arrow);
        engine.begin_pointer(20.0, 225.0, false, false);
        engine.move_pointer(120.0, 225.0, false, false);
        engine.move_pointer(225.0, 225.0, false, false);
        engine.end_pointer();
        let arrow = live(&engine)
            .into_iter()
            .find(|el| el.kind == DrawElementType::Arrow)
            .expect("an arrow");
        assert_eq!(arrow.end_binding.as_deref(), Some(note.as_str()));
    }

    /// The label moves with its note.
    #[test]
    fn a_dragged_note_takes_its_label() {
        let (mut engine, note, label) = note_with("hi");
        engine.set_tool(DrawTool::Select);
        let before = element(&engine, &label);
        engine.begin_pointer(130.0, 130.0, false, false);
        engine.move_pointer(180.0, 160.0, false, false);
        engine.end_pointer();
        assert_near(element(&engine, &note).x, 150.0);
        assert_near(element(&engine, &label).x, before.x + 50.0);
        assert_near(element(&engine, &label).y, before.y + 30.0);
    }

    /// Deleting the note takes its label: nothing of it is left drawn.
    #[test]
    fn a_deleted_note_takes_its_label() {
        let (mut engine, note, label) = note_with("hi");
        engine.select(vec![note.clone()]);
        engine.delete_selection();
        assert!(live(&engine).is_empty(), "{:?}", live(&engine));
        assert!(element(&engine, &note).is_deleted);
        assert!(element(&engine, &label).is_deleted);
    }

    /// Ctrl+D copies the note with its label, and the copy is dated now
    /// (`duplicate.ts@1118751f:116-117`).
    #[test]
    fn a_duplicate_is_a_new_dated_note_with_its_own_label() {
        let (mut engine, note, _) = note_with("hi");
        let mut scene = engine.get_scene();
        for el in &mut scene {
            if el.id == note {
                el.created = Some(1_000.0);
            }
        }
        engine.set_scene(Scene::new(scene));
        engine.select(vec![note.clone()]);
        engine.duplicate_selection(10.0, 10.0);
        let all = notes(&engine);
        assert_eq!(all.len(), 2);
        let copy = all.iter().find(|el| el.id != note).expect("a copy");
        assert!(
            copy.created.is_some_and(|at| at > 1.6e12),
            "{:?}",
            copy.created
        );
        let copy_label = copy.bound_text_id.clone().expect("the copy has a label");
        assert_eq!(
            element(&engine, &copy_label).container_id.as_deref(),
            Some(copy.id.as_str())
        );
    }

    /// Binding a free text to a note gives the two one ink and the text a ceiling;
    /// unbinding takes the ceiling away and puts the note back at its base height.
    #[test]
    fn a_text_bound_and_unbound() {
        let mut engine = engine_with_measure(vec![]);
        engine.set_tool_locked(true);
        engine.set_tool(DrawTool::StickyNote);
        engine.begin_pointer(225.0, 225.0, false, false);
        engine.end_pointer();
        engine.set_tool_locked(false);
        let note = notes(&engine)[0].id.clone();
        let mut text = text_at(600.0, 100.0, 50.0, 25.0);
        text.text = Some("free".into());
        text.original_text = Some("free".into());
        text.stroke_color = "#e03131".into();
        let text_id = text.id.clone();
        let mut scene = engine.get_scene();
        scene.push(text);
        engine.set_scene(Scene::new(scene));
        engine.select(vec![note.clone(), text_id.clone()]);
        engine.bind_text();
        let bound = element(&engine, &text_id);
        assert_eq!(bound.container_id.as_deref(), Some(note.as_str()));
        assert_eq!(bound.base_font_size, Some(20.0));
        assert_eq!(element(&engine, &note).stroke_color, "#e03131");

        engine.select(vec![note.clone()]);
        engine.unbind_text();
        let free = element(&engine, &text_id);
        assert_eq!(free.container_id, None);
        assert_eq!(free.base_font_size, None);
        assert_near(element(&engine, &note).height, SIZE);
    }
}

mod the_file {
    use super::*;

    /// A board with no note in it serializes exactly as it did: none of the note's three
    /// fields appears on anything else, and what is read back is written back unchanged.
    #[test]
    fn a_board_without_notes_is_byte_identical() {
        let mut label = text_at(10.0, 10.0, 40.0, 25.0);
        label.text = Some("x".into());
        let legacy = vec![box_at(0.0, 0.0, 100.0, 50.0), label];
        let json = scene_to_json(&legacy);
        for key in ["baseHeight", "created", "baseFontSize"] {
            assert!(!json.contains(key), "{key} leaked into {json}");
        }
        let read = elements_from_json(&json).expect("it reads back");
        assert_eq!(scene_to_json(&read), json);
    }

    /// A note's fields cross as they are.
    #[test]
    fn a_note_round_trips() {
        let (engine, note, label) = note_with(&overflowing());
        let scene = live(&engine);
        let json = scene_to_json(&scene);
        let read = elements_from_json(&json).expect("it reads back");
        let back = |id: &str| read.iter().find(|el| el.id == id).cloned().expect("kept");
        assert_eq!(back(&note), element(&engine, &note));
        assert_eq!(back(&label), element(&engine, &label));
        assert!(json.contains("\"type\": \"stickynote\""));
        assert!(json.contains("\"baseHeight\": 250"));
        assert!(json.contains("\"baseFontSize\": 28"));
        assert_eq!(scene_to_json(&read), json);
    }

    /// The SVG draws what the canvas draws: the shadow, the paper, the clipped edge and
    /// the date (`staticSvgScene.ts@1118751f:158-266`).
    #[test]
    fn the_svg_has_the_shadow_the_paper_and_the_date() {
        let (mut engine, note, _) = note_with("hi");
        let mut scene = engine.get_scene();
        for el in &mut scene {
            if el.id == note {
                el.created = Some(1_748_606_400_000.0);
            }
        }
        engine.set_scene(Scene::new(scene));
        let svg = engine.export_svg(10.0).expect("an svg");
        assert!(
            svg.contains(&format!("sticky-note-clipPath-{note}")),
            "{svg}"
        );
        assert!(svg.contains("fill-opacity=\"0.16\""));
        assert!(svg.contains(&format!("fill=\"{DEFAULT_STICKY_NOTE_BG}\"")));
        assert!(svg.contains("stroke-opacity=\"0.08\""));
        assert!(svg.contains(">30 May 2025</text>"), "{svg}");
        assert!(svg.contains(">hi</text>"), "the label is exported too");
    }
}
