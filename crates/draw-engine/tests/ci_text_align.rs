//! Where text sits inside the box that holds it.
//!
//! Before this existed, both alignments were constants buried in the painter and in
//! `layout_label`: `paint.rs` chose `center` when the text had a container and `left`
//! when it did not, and a label was always vertically centred. So the picture was right
//! for exactly one case each and there was no way to ask for another — the properties
//! the spec calls "Alignment" and "Vertical alignment" simply had nowhere to live.
//!
//! The horizontal half is asserted through `text_anchor_x` and `canvas_text_align`
//! rather than through the painter, because the painter is `wasm/paint.rs` and needs a
//! `CanvasRenderingContext2d`. Pulling the arithmetic out into functions is what makes
//! it reachable from a host test at all; `e2e/textAlign.spec.ts` then checks that the
//! painter actually calls them, in pixels.

mod common;
use common::*;
use draw_engine::*;

/// The alignment an element ends up with when nothing has set one.
///
/// This is the whole reason the fields are `Option` rather than plain values with a
/// `Default`. Every scene saved before this commit has neither field, and the two roles
/// want opposite answers: free text reads from the top left like a paragraph, a label
/// centres inside the shape holding it. A single `Default` would silently re-align every
/// label ever saved — the board would look different on reload, which is the one thing a
/// format change must not do.
mod defaults {
    use super::*;

    #[test]
    fn free_text_reads_from_the_top_left() {
        let text = text_at(0.0, 0.0, 100.0, 20.0);
        assert_eq!(resolved_text_align(&text), TextAlign::Left);
        assert_eq!(resolved_vertical_align(&text), VerticalAlign::Top);
    }

    #[test]
    fn a_label_centres_in_the_shape_holding_it() {
        let mut label = text_at(0.0, 0.0, 100.0, 20.0);
        label.container_id = Some("shape".into());
        assert_eq!(resolved_text_align(&label), TextAlign::Center);
        assert_eq!(resolved_vertical_align(&label), VerticalAlign::Middle);
    }

    /// The migration guard. A scene from before the fields existed carries `None`, and
    /// must come back looking exactly as it did — which means the container decides,
    /// not a constant.
    #[test]
    fn a_scene_written_before_the_fields_existed_keeps_its_look() {
        // `r##` rather than `r#`: the colours contain `"#`, which would close a `r#"`
        // literal in the middle of the scene.
        let json = r##"{"type":"osidraw","version":1,"source":"test","elements":[
          {"id":"a","type":"text","x":0,"y":0,"width":80,"height":20,"angle":0,
           "strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"hachure",
           "strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"seed":1,
           "text":"loose","fontSize":20,
           "version":1,"versionNonce":1,"updated":0,"isDeleted":false},
          {"id":"b","type":"text","x":0,"y":0,"width":80,"height":20,"angle":0,
           "strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"hachure",
           "strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"seed":1,
           "text":"bound","fontSize":20,"containerId":"shape",
           "version":1,"versionNonce":1,"updated":0,"isDeleted":false}
        ]}"##;
        let parsed = elements_from_json(json).expect("an old scene must still parse");

        let loose = parsed.iter().find(|e| e.id == "a").unwrap();
        assert!(loose.text_align.is_none(), "nothing set it, so it is unset");
        assert_eq!(resolved_text_align(loose), TextAlign::Left);
        assert_eq!(resolved_vertical_align(loose), VerticalAlign::Top);

        let bound = parsed.iter().find(|e| e.id == "b").unwrap();
        assert_eq!(resolved_text_align(bound), TextAlign::Center);
        assert_eq!(resolved_vertical_align(bound), VerticalAlign::Middle);
    }

    /// An explicit value always wins, including one that matches what the default would
    /// have said. Otherwise "left-align this label" would be indistinguishable from
    /// "never asked", and would flip back to centre on the next load.
    #[test]
    fn an_explicit_choice_outranks_the_role() {
        let mut label = text_at(0.0, 0.0, 100.0, 20.0);
        label.container_id = Some("shape".into());
        label.text_align = Some(TextAlign::Left);
        label.vertical_align = Some(VerticalAlign::Bottom);
        assert_eq!(resolved_text_align(&label), TextAlign::Left);
        assert_eq!(resolved_vertical_align(&label), VerticalAlign::Bottom);
    }
}

/// The horizontal half: where a line starts, and what the canvas is told.
///
/// The two have to agree. `fill_text(line, x)` means "put the *anchor* of the line at
/// x", and which end of the line the anchor is depends on `textAlign` — so an x computed
/// for one setting and drawn under another lands a whole text-width away.
mod horizontal {
    use super::*;

    #[test]
    fn left_anchors_at_the_left_edge() {
        assert_eq!(canvas_text_align(TextAlign::Left), "left");
        assert_close(text_anchor_x(TextAlign::Left, 200.0), 0.0);
    }

    #[test]
    fn centre_anchors_at_the_middle() {
        assert_eq!(canvas_text_align(TextAlign::Center), "center");
        assert_close(text_anchor_x(TextAlign::Center, 200.0), 100.0);
    }

    #[test]
    fn right_anchors_at_the_right_edge() {
        assert_eq!(canvas_text_align(TextAlign::Right), "right");
        assert_close(text_anchor_x(TextAlign::Right, 200.0), 200.0);
    }

    /// A short line inside a wide box is the only case that can tell the three apart —
    /// at equal width every alignment draws in the same place, which is how a broken
    /// implementation passes a careless test.
    #[test]
    fn the_three_put_a_short_line_in_three_different_places() {
        let box_width = 300.0;
        let left = text_anchor_x(TextAlign::Left, box_width);
        let centre = text_anchor_x(TextAlign::Center, box_width);
        let right = text_anchor_x(TextAlign::Right, box_width);
        assert!(left < centre && centre < right, "{left} {centre} {right}");
    }
}

/// The vertical half, which is geometry rather than a canvas setting: the label is a real
/// element with its own `y`, so aligning it means moving it inside the container.
mod vertical {
    use super::*;

    /// 200 tall, a 20-tall label: the three answers are visibly different numbers.
    fn container_and_label() -> (DrawElement, DrawElement) {
        let container = box_at(10.0, 40.0, 120.0, 200.0);
        let mut label = text_at(0.0, 0.0, 60.0, 20.0);
        label.container_id = Some(container.id.clone());
        (container, label)
    }

    #[test]
    fn top_puts_the_label_against_the_top_edge() {
        let (container, mut label) = container_and_label();
        label.vertical_align = Some(VerticalAlign::Top);
        let laid = layout_label(label, &container);
        assert_close(laid.y, 40.0 + LABEL_PADDING);
    }

    #[test]
    fn middle_centres_it() {
        let (container, mut label) = container_and_label();
        label.vertical_align = Some(VerticalAlign::Middle);
        let laid = layout_label(label, &container);
        assert_close(laid.y, 40.0 + 200.0 / 2.0 - 20.0 / 2.0);
    }

    #[test]
    fn bottom_puts_it_against_the_bottom_edge() {
        let (container, mut label) = container_and_label();
        label.vertical_align = Some(VerticalAlign::Bottom);
        let laid = layout_label(label, &container);
        assert_close(laid.y, 40.0 + 200.0 - 20.0 - LABEL_PADDING);
    }

    /// The behaviour that shipped before the field existed. Left unset, a label still
    /// centres — so this commit changes no existing board.
    #[test]
    fn unset_still_centres_the_way_it_always_did() {
        let (container, label) = container_and_label();
        let laid = layout_label(label, &container);
        assert_close(laid.y, 40.0 + 200.0 / 2.0 - 20.0 / 2.0);
    }

    /// A label taller than the space it has cannot honour top *and* bottom at once.
    /// Clamping to the top is the readable answer: text that overflows downward is still
    /// read from its first line, text pushed up off the shape is not.
    #[test]
    fn a_label_taller_than_its_container_starts_at_the_top() {
        let container = box_at(0.0, 0.0, 120.0, 30.0);
        let mut label = text_at(0.0, 0.0, 60.0, 90.0);
        label.container_id = Some(container.id.clone());
        label.vertical_align = Some(VerticalAlign::Bottom);
        let laid = layout_label(label, &container);
        assert_close(laid.y, 0.0);
    }
}

/// Setting the alignment from the editor's own controls.
mod setters {
    use super::*;

    fn text_selected() -> (DrawEngine, String) {
        let text = text_at(0.0, 0.0, 100.0, 20.0);
        let id = text.id.clone();
        let mut engine = engine_with_measure(vec![text]);
        engine.select(vec![id.clone()]);
        (engine, id)
    }

    fn find(engine: &DrawEngine, id: &str) -> DrawElement {
        engine
            .get_scene()
            .into_iter()
            .find(|e| e.id == id)
            .expect("element is still in the scene")
    }

    #[test]
    fn set_text_align_marks_the_selected_text() {
        let (mut engine, id) = text_selected();
        engine.set_text_align(TextAlign::Right);
        assert_eq!(find(&engine, &id).text_align, Some(TextAlign::Right));
    }

    #[test]
    fn set_vertical_align_marks_the_selected_text() {
        let (mut engine, id) = text_selected();
        engine.set_vertical_align(VerticalAlign::Bottom);
        assert_eq!(
            find(&engine, &id).vertical_align,
            Some(VerticalAlign::Bottom)
        );
    }

    /// Selecting a shape selects the shape, not its label — the label is not separately
    /// selectable while bound. So the alignment controls have to reach through the
    /// container, or they would be dead for every label on the board.
    #[test]
    fn aligning_a_shape_reaches_the_label_inside_it() {
        let rect = box_at(0.0, 0.0, 200.0, 120.0);
        let rect_id = rect.id.clone();
        let mut engine = engine_with_measure(vec![rect]);
        engine.handle_double_click(100.0, 60.0);
        let label_id = engine
            .get_scene()
            .into_iter()
            .find(|e| e.kind == DrawElementType::Text)
            .expect("double-clicking a shape makes a label")
            .id;

        engine.select(vec![rect_id]);
        engine.set_text_align(TextAlign::Right);
        engine.set_vertical_align(VerticalAlign::Top);

        let label = find(&engine, &label_id);
        assert_eq!(label.text_align, Some(TextAlign::Right));
        assert_eq!(label.vertical_align, Some(VerticalAlign::Top));
    }

    /// Vertical alignment is a position, so setting it has to move the label there and
    /// then — not on the next unrelated relayout.
    #[test]
    fn setting_the_vertical_alignment_moves_the_label_now() {
        let rect = box_at(0.0, 0.0, 200.0, 200.0);
        let rect_id = rect.id.clone();
        let mut engine = engine_with_measure(vec![rect]);
        engine.handle_double_click(100.0, 100.0);
        let label_id = engine
            .get_scene()
            .into_iter()
            .find(|e| e.kind == DrawElementType::Text)
            .unwrap()
            .id;
        engine.set_element_text(&label_id, "hi");

        let centred = find(&engine, &label_id).y;
        engine.select(vec![rect_id]);
        engine.set_vertical_align(VerticalAlign::Top);
        let raised = find(&engine, &label_id).y;

        assert!(
            raised < centred,
            "top must sit above middle: {raised} {centred}"
        );
        assert_close(raised, LABEL_PADDING);
    }

    #[test]
    fn alignment_is_undoable() {
        let (mut engine, id) = text_selected();
        engine.set_text_align(TextAlign::Right);
        engine.undo();
        assert_ne!(find(&engine, &id).text_align, Some(TextAlign::Right));
    }

    /// With nothing selected the choice is remembered for whatever is drawn next, the
    /// same way the stroke colour is. Otherwise picking an alignment before typing does
    /// nothing at all, which reads as a broken button.
    #[test]
    fn with_no_selection_the_choice_applies_to_the_next_text() {
        let mut engine = engine_with_measure(vec![]);
        engine.set_text_align(TextAlign::Right);
        engine.handle_double_click(50.0, 50.0);
        let created = engine
            .get_scene()
            .into_iter()
            .find(|e| e.kind == DrawElementType::Text)
            .unwrap();
        assert_eq!(created.text_align, Some(TextAlign::Right));
    }
}

/// Both fields have to survive the wire, or the alignment is a session-local illusion
/// that resets the next time the board is opened.
mod persistence {
    use super::*;

    #[test]
    fn both_alignments_round_trip_through_json() {
        let mut text = text_at(5.0, 5.0, 100.0, 20.0);
        text.text = Some("aligned".into());
        text.text_align = Some(TextAlign::Right);
        text.vertical_align = Some(VerticalAlign::Bottom);

        let json = scene_to_json(std::slice::from_ref(&text));
        let parsed = elements_from_json(&json).expect("round trip");
        assert_eq!(parsed[0].text_align, Some(TextAlign::Right));
        assert_eq!(parsed[0].vertical_align, Some(VerticalAlign::Bottom));
    }

    /// The names on the wire are the ones the contract package and Excalidraw both use.
    /// A rename here is a silent data loss on every board already saved.
    #[test]
    fn the_wire_names_are_camel_case() {
        let mut text = text_at(0.0, 0.0, 10.0, 20.0);
        text.text = Some("x".into());
        text.text_align = Some(TextAlign::Center);
        text.vertical_align = Some(VerticalAlign::Middle);
        let json = scene_to_json(std::slice::from_ref(&text));
        // Whitespace-stripped, because the exporter pretty-prints: this test is about the
        // key and the value, not about where the formatter puts its spaces.
        let compact: String = json.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(compact.contains(r#""textAlign":"center""#), "{json}");
        assert!(compact.contains(r#""verticalAlign":"middle""#), "{json}");
    }

    /// Unset stays absent rather than being written out as a guess, so an old board
    /// round-trips byte-identically and the default keeps being negotiable later.
    #[test]
    fn an_unset_alignment_is_not_written() {
        let mut text = text_at(0.0, 0.0, 10.0, 20.0);
        text.text = Some("x".into());
        let json = scene_to_json(std::slice::from_ref(&text));
        assert!(!json.contains("textAlign"), "{json}");
        assert!(!json.contains("verticalAlign"), "{json}");
    }
}
