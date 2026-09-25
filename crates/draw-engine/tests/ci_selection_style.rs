//! The properties panel's one read of the selection.
//!
//! The panel used to read each value on its own — the first selected element's style
//! for the colours, `get_font_size` for the size, the arrowheads from a snapshot taken
//! at the last selection event — so a mixed selection showed the first element's values
//! as if they were everyone's, and an undo or a peer's edit left the panel showing what
//! was there before. `selection_style` is the oracle's `getFormValue` +
//! `reduceToCommonValue` (`actions/actionProperties.tsx@1118751f:229-270`) as one pass,
//! and `style_revision` says when it has to be asked again.

mod common;
use common::*;
use draw_engine::engine::{Edges, SelectionStyle};
use draw_engine::*;

fn with_id(mut element: DrawElement, id: &str) -> DrawElement {
    element.id = id.into();
    element
}

/// A shape holding a label, as the engine makes one.
fn labelled(mut shape: DrawElement, id: &str, label_id: &str) -> Vec<DrawElement> {
    shape.id = id.into();
    shape.bound_text_id = Some(label_id.into());
    let mut label = text_at(shape.x + 10.0, shape.y + 10.0, 80.0, 20.0);
    label.id = label_id.into();
    label.text = Some("label".into());
    label.container_id = Some(id.into());
    vec![shape, label]
}

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|element| element.id == id)
        .expect("element exists")
}

fn selected(elements: Vec<DrawElement>, ids: &[&str]) -> DrawEngine {
    let mut engine = engine_with_measure(elements);
    engine.select(ids.iter().map(|id| id.to_string()).collect());
    engine
}

mod values {
    use super::*;

    #[test]
    fn a_value_everything_shares_is_the_value() {
        let engine = selected(
            vec![
                with_id(box_at(0.0, 0.0, 50.0, 50.0), "a"),
                with_id(ellipse_at(100.0, 0.0, 50.0, 50.0), "b"),
            ],
            &["a", "b"],
        );
        let style = engine.selection_style();
        assert_eq!(style.count, 2);
        assert_eq!(style.stroke_color.as_deref(), Some("#1e1e1e"));
        assert_eq!(style.stroke_width, Some(2.0));
        assert_eq!(style.opacity, Some(100.0));
    }

    /// `reduceToCommonValue` answers `null` for a disagreement, and the panel shows
    /// nothing as current rather than the first element's value.
    #[test]
    fn a_value_they_disagree_on_is_mixed() {
        let mut red = with_id(box_at(0.0, 0.0, 50.0, 50.0), "a");
        red.stroke_color = "#e03131".into();
        red.opacity = 40.0;
        let engine = selected(
            vec![red, with_id(box_at(100.0, 0.0, 50.0, 50.0), "b")],
            &["a", "b"],
        );
        let style = engine.selection_style();
        assert_eq!(style.stroke_color, None);
        assert_eq!(style.opacity, None);
        assert_eq!(style.stroke_width, Some(2.0), "the rest still agree");
    }

    /// `changeFillStyle`'s form value asks only what has a fill style (`:664-670`).
    #[test]
    fn the_fill_style_is_asked_only_of_what_has_one() {
        let mut arrow = with_id(connector(0.0, 0.0, 100.0, 0.0, DrawElementType::Arrow), "b");
        arrow.fill_style = FillStyle::Solid;
        let mut shape = with_id(box_at(0.0, 0.0, 50.0, 50.0), "a");
        shape.fill_style = FillStyle::CrossHatch;
        let engine = selected(vec![shape, arrow], &["a", "b"]);
        assert_eq!(
            engine.selection_style().fill_style,
            Some(FillStyle::CrossHatch)
        );
    }

    /// `changeRoundness` asks everything but arrows (`:1807-1816`).
    #[test]
    fn the_edges_are_read_off_everything_but_arrows() {
        let mut round = with_id(box_at(0.0, 0.0, 50.0, 50.0), "a");
        round.roundness = Some(8.0);
        let mut curved = with_id(connector(0.0, 0.0, 100.0, 0.0, DrawElementType::Arrow), "b");
        curved.roundness = None;
        let engine = selected(vec![round, curved], &["a", "b"]);
        assert_eq!(engine.selection_style().edges, Some(Edges::Round));

        let mut sharp = with_id(diamond_at(0.0, 100.0, 50.0, 50.0), "c");
        sharp.roundness = None;
        let mut engine = engine;
        engine.set_scene(Scene::new(
            engine
                .get_scene()
                .into_iter()
                .chain([sharp])
                .collect::<Vec<_>>(),
        ));
        engine.select(vec!["a".into(), "c".into()]);
        assert_eq!(engine.selection_style().edges, None, "round and sharp");
    }

    /// An arrow's unset heads are the defaults it is drawn with: none at the start, an
    /// arrow at the end.
    #[test]
    fn arrowheads_are_read_off_arrows_as_they_are_drawn() {
        let mut dotted = with_id(connector(0.0, 0.0, 100.0, 0.0, DrawElementType::Arrow), "a");
        dotted.start_arrowhead = Some(Arrowhead::Dot);
        let plain = with_id(
            connector(0.0, 50.0, 100.0, 50.0, DrawElementType::Arrow),
            "b",
        );
        let rect = with_id(box_at(0.0, 100.0, 50.0, 50.0), "c");
        let engine = selected(vec![dotted, plain, rect], &["a", "b", "c"]);
        let style = engine.selection_style();
        assert_eq!(style.start_arrowhead, None, "dot and none");
        assert_eq!(style.end_arrowhead, Some(Arrowhead::Arrow));
    }
}

/// A label is not separately selectable, so the text controls read — and write — the
/// label of the selected shape (`getFormValue` through `getBoundTextElement`,
/// `:1051-1070`, `:1611-1630`, `:1709-1728`).
mod labels {
    use super::*;

    #[test]
    fn a_selected_shape_reports_its_labels_text_props() {
        let mut elements = labelled(box_at(0.0, 0.0, 200.0, 100.0), "box", "label");
        elements[1].font_size = Some(28.0);
        elements[1].text_align = Some(TextAlign::Right);
        elements[1].vertical_align = Some(VerticalAlign::Bottom);
        let engine = selected(elements, &["box"]);
        let style = engine.selection_style();
        assert_eq!(style.font_size, Some(28.0));
        assert_eq!(style.text_align, Some(TextAlign::Right));
        assert_eq!(style.vertical_align, Some(VerticalAlign::Bottom));
        assert!(
            style.kinds.contains(&DrawElementType::Text),
            "the label counts"
        );
    }

    #[test]
    fn two_labels_that_disagree_are_mixed() {
        let mut elements = labelled(box_at(0.0, 0.0, 200.0, 100.0), "a", "la");
        elements.extend(labelled(box_at(300.0, 0.0, 200.0, 100.0), "b", "lb"));
        elements[1].font_size = Some(28.0);
        let engine = selected(elements, &["a", "b"]);
        assert_eq!(engine.selection_style().font_size, None);
    }

    /// The label's colour is the shape's text colour, but the colour row reads the
    /// shape itself — as the oracle's does.
    #[test]
    fn the_colour_row_reads_the_shape() {
        let mut elements = labelled(box_at(0.0, 0.0, 200.0, 100.0), "box", "label");
        elements[1].stroke_color = "#2f9e44".into();
        let engine = selected(elements, &["box"]);
        assert_eq!(
            engine.selection_style().stroke_color.as_deref(),
            Some("#1e1e1e")
        );
    }
}

/// Which text controls apply: `suppportsHorizontalAlign` and `shouldAllowVerticalAlign`
/// (`packages/element/src/textElement.ts@1118751f:446-477`).
mod predicates {
    use super::*;

    #[test]
    fn a_label_in_a_shape_aligns_both_ways() {
        let engine = selected(
            labelled(box_at(0.0, 0.0, 200.0, 100.0), "box", "l"),
            &["box"],
        );
        let style = engine.selection_style();
        assert!(style.text_alignable);
        assert!(style.vertical_alignable);
    }

    #[test]
    fn free_text_aligns_across_but_not_down() {
        let engine = selected(vec![with_id(text_at(0.0, 0.0, 80.0, 20.0), "t")], &["t"]);
        let style = engine.selection_style();
        assert!(style.text_alignable);
        assert!(!style.vertical_alignable);
    }

    #[test]
    fn an_arrows_label_aligns_neither_way() {
        let arrow = connector(0.0, 0.0, 200.0, 0.0, DrawElementType::Arrow);
        let engine = selected(labelled(arrow, "arrow", "l"), &["arrow"]);
        let style = engine.selection_style();
        assert!(!style.text_alignable);
        assert!(!style.vertical_alignable);
        assert_eq!(style.vertical_align, Some(VerticalAlign::Middle));
    }

    #[test]
    fn a_shape_without_a_label_has_no_text() {
        let engine = selected(vec![with_id(box_at(0.0, 0.0, 50.0, 50.0), "a")], &["a"]);
        let style = engine.selection_style();
        assert!(!style.text_alignable);
        assert!(!style.vertical_alignable);
        assert_eq!(style.kinds, vec![DrawElementType::Rectangle]);
    }

    #[test]
    fn filled_kinds_are_the_ones_with_a_visible_background() {
        let mut filled = with_id(box_at(0.0, 0.0, 50.0, 50.0), "a");
        filled.background_color = "#ffc9c9".into();
        let mut clear = with_id(ellipse_at(100.0, 0.0, 50.0, 50.0), "b");
        clear.background_color = "#ffffff00".into();
        let engine = selected(vec![filled, clear], &["a", "b"]);
        let style = engine.selection_style();
        assert_eq!(style.filled_kinds, vec![DrawElementType::Rectangle]);
        assert_eq!(
            style.kinds,
            vec![DrawElementType::Rectangle, DrawElementType::Ellipse]
        );
    }

    #[test]
    fn align_and_distribute_come_from_the_units() {
        let three = vec![
            with_id(box_at(0.0, 0.0, 50.0, 50.0), "a"),
            with_id(box_at(100.0, 0.0, 50.0, 50.0), "b"),
            with_id(box_at(200.0, 0.0, 50.0, 50.0), "c"),
        ];
        let style = selected(three.clone(), &["a", "b", "c"]).selection_style();
        assert!(style.can_align && style.can_distribute);
        let style = selected(three, &["a"]).selection_style();
        assert!(!style.can_align && !style.can_distribute);
    }
}

/// With nothing selected the panel sets up the next element, and shows what it will get.
mod nothing_selected {
    use super::*;

    #[test]
    fn it_reads_the_next_style() {
        let mut engine = engine_with_measure(vec![]);
        engine.set_next_style(DrawElementStylePatch {
            stroke_color: Some("#1971c2".into()),
            opacity: Some(60.0),
            ..Default::default()
        });
        engine.set_font_size(36.0);
        let style = engine.selection_style();
        assert_eq!(style.count, 0);
        assert_eq!(style.stroke_color.as_deref(), Some("#1971c2"));
        assert_eq!(style.opacity, Some(60.0));
        assert_eq!(style.font_size, Some(36.0));
        assert_eq!(style.text_align, Some(TextAlign::Left));
        assert_eq!(
            style.edges,
            Some(Edges::Round),
            "new shapes are round by default"
        );
        assert_eq!(style.end_arrowhead, Some(Arrowhead::Arrow));
        assert!(style.kinds.is_empty());
    }
}

/// `style_revision` moves whenever the summary may have: the host asks again then, and
/// only then.
mod revision {
    use super::*;

    fn two_boxes() -> DrawEngine {
        engine_with_measure(vec![
            with_id(box_at(0.0, 0.0, 50.0, 50.0), "a"),
            with_id(box_at(100.0, 0.0, 50.0, 50.0), "b"),
        ])
    }

    #[test]
    fn a_selection_change_moves_it() {
        let mut engine = two_boxes();
        let before = engine.style_revision();
        engine.select(vec!["a".into()]);
        assert_ne!(engine.style_revision(), before);
    }

    #[test]
    fn a_style_change_moves_it() {
        let mut engine = two_boxes();
        engine.select(vec!["a".into()]);
        let before = engine.style_revision();
        engine.apply_style(stroke_patch("#e03131"));
        assert_ne!(engine.style_revision(), before);
    }

    #[test]
    fn undo_and_redo_move_it() {
        let mut engine = two_boxes();
        engine.select(vec!["a".into()]);
        engine.apply_style(stroke_patch("#e03131"));
        let before = engine.style_revision();
        engine.undo();
        let undone = engine.style_revision();
        assert_ne!(undone, before);
        engine.redo();
        assert_ne!(engine.style_revision(), undone);
    }

    fn remote(element: DrawElement) -> String {
        scene_to_json(&[element])
    }

    #[test]
    fn a_peers_edit_to_the_selection_moves_it() {
        let mut engine = two_boxes();
        engine.select(vec!["a".into()]);
        let before = engine.style_revision();
        let mut theirs = element(&engine, "a");
        theirs.stroke_color = "#2f9e44".into();
        theirs.version += 1;
        assert!(engine.apply_remote_patch(&remote(theirs)));
        assert_ne!(engine.style_revision(), before);
        assert_eq!(
            engine.selection_style().stroke_color.as_deref(),
            Some("#2f9e44")
        );
    }

    /// Nothing the panel shows changed, so the host has nothing to ask.
    #[test]
    fn a_peers_edit_elsewhere_does_not() {
        let mut engine = two_boxes();
        engine.select(vec!["a".into()]);
        let before = engine.style_revision();
        let mut theirs = element(&engine, "b");
        theirs.stroke_color = "#2f9e44".into();
        theirs.version += 1;
        assert!(engine.apply_remote_patch(&remote(theirs)));
        assert_eq!(engine.style_revision(), before);
    }

    #[test]
    fn a_peers_edit_to_the_selected_shapes_label_moves_it() {
        let mut engine = engine_with_measure(labelled(box_at(0.0, 0.0, 200.0, 100.0), "box", "l"));
        engine.select(vec!["box".into()]);
        let before = engine.style_revision();
        let mut theirs = element(&engine, "l");
        theirs.font_size = Some(36.0);
        theirs.version += 1;
        assert!(engine.apply_remote_patch(&remote(theirs)));
        assert_ne!(engine.style_revision(), before);
        assert_eq!(engine.selection_style().font_size, Some(36.0));
    }

    #[test]
    fn a_text_commit_moves_it() {
        let mut engine = engine_with_measure(vec![with_id(text_at(0.0, 0.0, 80.0, 20.0), "t")]);
        engine.select(vec!["t".into()]);
        let before = engine.style_revision();
        engine.set_element_text("t", "hello");
        assert_ne!(engine.style_revision(), before);
    }

    #[test]
    fn the_next_style_moves_it() {
        let mut engine = two_boxes();
        let before = engine.style_revision();
        engine.set_next_style(stroke_patch("#e03131"));
        let after_stroke = engine.style_revision();
        assert_ne!(after_stroke, before);
        engine.set_font_size(28.0);
        assert_ne!(engine.style_revision(), after_stroke);
    }
}

/// A slider drag is a stream of previews and one commit: live on the canvas, one step of
/// undo, and nothing sent until the commit.
mod preview {
    use super::*;

    fn opacity(value: f64) -> DrawElementStylePatch {
        DrawElementStylePatch {
            opacity: Some(value),
            ..Default::default()
        }
    }

    #[test]
    fn a_drag_is_one_step_of_undo() {
        let mut engine = engine_with_measure(labelled(box_at(0.0, 0.0, 200.0, 100.0), "box", "l"));
        engine.select(vec!["box".into()]);
        for value in [90.0, 70.0, 50.0, 30.0] {
            engine.preview_style(opacity(value));
            assert_eq!(element(&engine, "box").opacity, value, "live on the canvas");
            assert_eq!(element(&engine, "l").opacity, value, "the label follows");
        }
        engine.apply_style(opacity(30.0));
        engine.undo();
        assert_eq!(
            element(&engine, "box").opacity,
            100.0,
            "one undo undoes the drag"
        );
        assert_eq!(element(&engine, "l").opacity, 100.0);
    }

    /// What the host's release relies on: a drag that ends where it began is still ended
    /// by a commit. Until then the previewed element is pending and a peer's copy of it is
    /// refused, as during any gesture; the commit changes nothing, records nothing, and
    /// takes the peer's copy in.
    #[test]
    fn a_drag_back_to_the_start_ends_in_the_peers_copy() {
        let mut engine = engine_with_measure(vec![with_id(box_at(0.0, 0.0, 50.0, 50.0), "a")]);
        engine.select(vec!["a".into()]);
        for value in [70.0, 40.0, 70.0, 100.0] {
            engine.preview_style(opacity(value));
        }
        let mut theirs = element(&engine, "a");
        theirs.stroke_color = "#2f9e44".into();
        theirs.version += 5;
        assert!(
            !engine.apply_remote_patch(&scene_to_json(&[theirs])),
            "refused mid-gesture"
        );

        engine.apply_style(opacity(100.0));

        assert_eq!(element(&engine, "a").stroke_color, "#2f9e44");
        engine.undo();
        assert_eq!(
            element(&engine, "a").stroke_color,
            "#2f9e44",
            "the commit recorded no step of its own"
        );
    }

    #[test]
    fn a_preview_is_not_sent() {
        let mut engine = engine_with_measure(vec![with_id(box_at(0.0, 0.0, 50.0, 50.0), "a")]);
        engine.select(vec!["a".into()]);
        let version = element(&engine, "a").version;
        let _ = engine.drain_events();
        engine.preview_style(opacity(50.0));
        let events = engine.drain_events();
        assert!(events.scene_delta.is_none() && events.scene_json.is_none());
        assert_eq!(
            element(&engine, "a").version,
            version,
            "unstamped until the commit"
        );
        engine.apply_style(opacity(50.0));
        assert!(element(&engine, "a").version > version);
        let delta = engine
            .drain_events()
            .scene_delta
            .expect("the commit is sent");
        assert_eq!(delta.updated.len(), 1);
    }

    fn ben_holds(ids: &[&str]) -> Vec<Peer> {
        vec![Peer {
            id: "ben".into(),
            name: "Ben".into(),
            color: "#e03131".into(),
            holds: ids.iter().map(|id| (*id).to_string()).collect(),
            preview: Vec::new(),
        }]
    }

    /// Ben's commit of the label: new words, at the next version.
    fn bens_label(engine: &DrawEngine) -> DrawElement {
        let mut theirs = element(engine, "l");
        theirs.text = Some("typed by Ben".into());
        theirs.original_text = theirs.text.clone();
        theirs.opacity = 100.0;
        theirs.version += 1;
        theirs
    }

    /// A slider drag previews the shape and its label. Ben takes the label mid-drag, to
    /// type into it: the preview lets go of it there and then, and what he commits is
    /// taken as it comes. Committed by the release instead, the preview was stamped above
    /// his copy and the label went back to its old words for everyone.
    #[test]
    fn a_label_a_peer_takes_mid_drag_keeps_what_they_typed() {
        let mut engine = engine_with_measure(labelled(box_at(0.0, 0.0, 200.0, 100.0), "box", "l"));
        engine.select(vec!["box".into()]);
        engine.preview_style(opacity(30.0));

        engine.set_peers(ben_holds(&["l"]));
        assert_eq!(
            element(&engine, "l").opacity,
            100.0,
            "the preview let go of it"
        );
        let theirs = bens_label(&engine);
        assert!(engine.apply_remote_patch(&scene_to_json(std::slice::from_ref(&theirs))));
        engine.apply_style(opacity(30.0));

        let label = element(&engine, "l");
        assert_eq!(
            (label.text.as_deref(), label.version, label.opacity),
            (theirs.text.as_deref(), theirs.version, 100.0),
            "his words, his stamp"
        );
        assert_eq!(element(&engine, "box").opacity, 30.0);
    }

    /// He lets go before the drag ends: the label is the drag's again, his words in it.
    #[test]
    fn a_label_given_back_mid_drag_is_styled_with_their_words() {
        let mut engine = engine_with_measure(labelled(box_at(0.0, 0.0, 200.0, 100.0), "box", "l"));
        engine.select(vec!["box".into()]);
        engine.preview_style(opacity(30.0));
        engine.set_peers(ben_holds(&["l"]));
        let theirs = bens_label(&engine);
        engine.apply_remote_patch(&scene_to_json(std::slice::from_ref(&theirs)));
        engine.set_peers(Vec::new());

        engine.preview_style(opacity(20.0));
        engine.apply_style(opacity(20.0));

        let label = element(&engine, "l");
        assert_eq!(label.text.as_deref(), Some("typed by Ben"));
        assert_eq!(label.opacity, 20.0);
        assert!(label.version > theirs.version);
    }

    /// Taken whole, the shape goes back as it was and is his: nothing of the drag is
    /// committed on it, at the release or at the next edit of anything else.
    #[test]
    fn a_shape_a_peer_takes_mid_drag_is_put_back_and_never_committed() {
        let mut engine = engine_with_measure(vec![
            with_id(box_at(0.0, 0.0, 50.0, 50.0), "a"),
            with_id(box_at(100.0, 0.0, 50.0, 50.0), "b"),
        ]);
        let before = element(&engine, "a");
        engine.select(vec!["a".into()]);
        engine.preview_style(opacity(30.0));

        engine.set_peers(ben_holds(&["a"]));
        engine.apply_style(opacity(30.0));
        engine.set_peers(Vec::new());
        engine.select(vec!["b".into()]);
        engine.apply_style(stroke_patch("#2f9e44"));

        assert_eq!(element(&engine, "a"), before);
    }
}

/// `actionCopyStyles` / `actionPasteStyles` (`actions/actionStyles.ts@1118751f:51-236`).
mod copy_paste {
    use super::*;

    fn styled(mut element: DrawElement) -> DrawElement {
        element.stroke_color = "#e03131".into();
        element.background_color = "#ffc9c9".into();
        element.fill_style = FillStyle::CrossHatch;
        element.stroke_width = 4.0;
        element.stroke_style = StrokeStyle::Dashed;
        element.roughness = 2.0;
        element.opacity = 60.0;
        element.roundness = Some(8.0);
        element
    }

    #[test]
    fn nothing_selected_copies_nothing() {
        let mut engine = engine_with_measure(vec![]);
        assert!(!engine.copy_styles());
    }

    #[test]
    fn the_shape_styles_transfer() {
        let source = styled(with_id(box_at(0.0, 0.0, 50.0, 50.0), "src"));
        let mut target = with_id(ellipse_at(100.0, 0.0, 50.0, 50.0), "dst");
        target.x = 100.0;
        let mut engine = engine_with_measure(vec![source, target]);
        engine.select(vec!["src".into()]);
        assert!(engine.copy_styles());
        engine.select(vec!["dst".into()]);
        engine.paste_styles();

        let pasted = element(&engine, "dst");
        assert_eq!(pasted.stroke_color, "#e03131");
        assert_eq!(pasted.background_color, "#ffc9c9");
        assert_eq!(pasted.fill_style, FillStyle::CrossHatch);
        assert_eq!(pasted.stroke_width, 4.0);
        assert_eq!(pasted.stroke_style, StrokeStyle::Dashed);
        assert_eq!(pasted.roughness, 2.0);
        assert_eq!(pasted.opacity, 60.0);
        assert_eq!(
            pasted.roundness, None,
            "an ellipse takes no corners (`:126-133`)"
        );
        assert_eq!(pasted.x, 100.0, "geometry is not a style");
    }

    #[test]
    fn it_copies_the_first_selected_in_stacking_order() {
        let bottom = styled(with_id(box_at(0.0, 0.0, 50.0, 50.0), "bottom"));
        let top = with_id(box_at(100.0, 0.0, 50.0, 50.0), "top");
        let target = with_id(diamond_at(200.0, 0.0, 50.0, 50.0), "dst");
        let mut engine = engine_with_measure(vec![bottom, top, target]);
        engine.select(vec!["top".into(), "bottom".into()]);
        engine.copy_styles();
        engine.select(vec!["dst".into()]);
        engine.paste_styles();
        let pasted = element(&engine, "dst");
        assert_eq!(pasted.stroke_color, "#e03131");
        assert_eq!(pasted.roundness, Some(8.0), "a diamond takes corners");
    }

    /// A label takes the copied label's styles, and is left alone when there is none.
    #[test]
    fn labels_take_the_copied_label() {
        let mut elements = labelled(styled(box_at(0.0, 0.0, 200.0, 100.0)), "src", "src-l");
        elements[1].stroke_color = "#2f9e44".into();
        elements[1].font_size = Some(36.0);
        elements[1].text_align = Some(TextAlign::Left);
        elements.extend(labelled(box_at(300.0, 0.0, 200.0, 100.0), "dst", "dst-l"));
        let mut engine = engine_with_measure(elements);
        engine.select(vec!["src".into()]);
        engine.copy_styles();
        engine.select(vec!["dst".into()]);
        engine.paste_styles();

        assert_eq!(element(&engine, "dst").stroke_color, "#e03131");
        let label = element(&engine, "dst-l");
        assert_eq!(label.stroke_color, "#2f9e44");
        assert_eq!(label.font_size, Some(36.0));
        assert_eq!(label.text_align, Some(TextAlign::Left));
        assert_eq!(
            label.background_color, "transparent",
            "the copied label's own"
        );
    }

    /// A label pasted a larger size grows its shape to hold it, as every other change to
    /// a label's font does: the paste lays it out with its container,
    /// `redrawTextBoundingBox(newTextElement, container)` (`actions/actionStyles.ts@1118751f:174`).
    #[test]
    fn a_label_pasted_a_larger_size_grows_its_shape() {
        let mut elements = labelled(box_at(0.0, 0.0, 200.0, 100.0), "src", "src-l");
        elements[1].font_size = Some(80.0);
        elements.extend(labelled(box_at(300.0, 0.0, 200.0, 40.0), "dst", "dst-l"));
        let mut engine = engine_with_measure(elements);
        engine.select(vec!["src".into()]);
        engine.copy_styles();
        engine.select(vec!["dst".into()]);
        engine.paste_styles();

        let (shape, label) = (element(&engine, "dst"), element(&engine, "dst-l"));
        assert_eq!(label.font_size, Some(80.0));
        assert!(
            shape.y <= label.y && label.y + label.height <= shape.y + shape.height,
            "the label {}..{} fits in its shape {}..{}",
            label.y,
            label.y + label.height,
            shape.y,
            shape.y + shape.height
        );
    }

    #[test]
    fn a_label_is_untouched_when_nothing_was_copied_for_it() {
        let source = styled(with_id(box_at(0.0, 0.0, 50.0, 50.0), "src"));
        let mut elements = vec![source];
        elements.extend(labelled(box_at(300.0, 0.0, 200.0, 100.0), "dst", "dst-l"));
        let mut engine = engine_with_measure(elements);
        let before = element(&engine, "dst-l");
        engine.select(vec!["src".into()]);
        engine.copy_styles();
        engine.select(vec!["dst".into()]);
        engine.paste_styles();
        assert_eq!(element(&engine, "dst").stroke_color, "#e03131");
        let after = element(&engine, "dst-l");
        assert_eq!(after.stroke_color, before.stroke_color);
        assert_eq!(after.opacity, before.opacity);
        assert_eq!(after.font_size, before.font_size);
    }

    /// A text takes the source's font — or the defaults, when the source has none
    /// (`:136-157`) — and is measured again at its new size.
    #[test]
    fn text_takes_the_font_and_is_measured_again() {
        let mut source = with_id(text_at(0.0, 0.0, 80.0, 20.0), "src");
        source.text = Some("abc".into());
        source.font_size = Some(36.0);
        source.text_align = Some(TextAlign::Right);
        source.stroke_color = "#1971c2".into();
        let mut target = with_id(text_at(0.0, 100.0, 80.0, 20.0), "dst");
        target.text = Some("hello".into());
        let mut engine = engine_with_measure(vec![source, target]);
        engine.select(vec!["src".into()]);
        engine.copy_styles();
        engine.select(vec!["dst".into()]);
        engine.paste_styles();

        let pasted = element(&engine, "dst");
        assert_eq!(pasted.font_size, Some(36.0));
        assert_eq!(pasted.text_align, Some(TextAlign::Right));
        assert_eq!(pasted.stroke_color, "#1971c2");
        let (width, _) = measure_text("hello", 36.0);
        assert!(
            (pasted.width - width).abs() < 1.0,
            "measured at 36px: {} vs {width}",
            pasted.width
        );

        // From a shape, a text goes back to the defaults.
        let shape = with_id(box_at(0.0, 200.0, 50.0, 50.0), "shape");
        let mut scene = engine.get_scene();
        scene.push(shape);
        engine.set_scene(Scene::new(scene));
        engine.select(vec!["shape".into()]);
        engine.copy_styles();
        engine.select(vec!["dst".into()]);
        engine.paste_styles();
        let pasted = element(&engine, "dst");
        assert_eq!(pasted.font_size, Some(20.0));
        assert_eq!(pasted.text_align, Some(TextAlign::Left));
    }

    /// The engine makes a text with the default style's corners and no alignment written
    /// down. A paste that compared those as set stamped the twin, sent it and cost a step
    /// of undo that put nothing back. The oracle's text carries `roundness: null` and a
    /// written alignment (`packages/element/src/newElement.ts@1118751f:105`), so its
    /// `newElementWith` finds nothing to change (`mutateElement.ts@1118751f:170-172`).
    #[test]
    fn a_texts_style_pasted_onto_its_twin_is_not_an_edit() {
        let twin = |id: &str, y: f64| {
            let mut text = with_id(text_at(0.0, y, 80.0, 20.0), id);
            text.text = Some("abc".into());
            text
        };
        let (a, b) = (twin("a", 0.0), twin("b", 100.0));
        assert_eq!((b.roundness, b.text_align), (Some(8.0), None), "setup");
        let mut engine = engine_with_measure(vec![a, b]);
        engine.select(vec!["a".into()]);
        engine.copy_styles();
        engine.select(vec!["b".into()]);
        let before = element(&engine, "b");
        let _ = engine.drain_events();

        engine.paste_styles();

        assert_eq!(element(&engine, "b"), before);
        let events = engine.drain_events();
        assert!(events.scene_json.is_none());
        assert!(
            events
                .scene_delta
                .is_none_or(|delta| delta.updated.is_empty() && delta.removed.is_empty()),
            "nothing changed, so nothing is sent"
        );
    }

    /// From a shape, a text takes the family new text is written in: the oracle's
    /// `sourceText.fontFamily || DEFAULT_FONT_FAMILY` (`actions/actionStyles.ts@1118751f:143`).
    /// Asked of the engine rather than written down, so this follows the default when new
    /// text gets another one.
    #[test]
    fn from_a_shape_a_text_takes_the_family_new_text_gets() {
        let mut engine = engine_with_measure(vec![]);
        engine.set_tool(DrawTool::Text);
        engine.begin_pointer(100.0, 100.0, false, false);
        engine.end_pointer();
        let fresh = engine
            .get_scene()
            .into_iter()
            .find(|element| element.kind == DrawElementType::Text)
            .expect("the text tool made a text")
            .font_family;

        let mut target = with_id(text_at(0.0, 100.0, 80.0, 20.0), "dst");
        target.text = Some("hello".into());
        target.font_family = Some(8);
        let shape = with_id(box_at(0.0, 200.0, 50.0, 50.0), "shape");
        engine.set_scene(Scene::new(vec![shape, target]));
        engine.select(vec!["shape".into()]);
        engine.copy_styles();
        engine.select(vec!["dst".into()]);
        engine.paste_styles();

        assert_eq!(element(&engine, "dst").font_family, fresh);
    }

    /// From a shape, a text takes the line height of the family it gets:
    /// `sourceText.lineHeight || getLineHeight(fontFamily)` (`:143-157`). A text the text
    /// tool made already has both, so a shape in the default style pastes nothing new.
    #[test]
    fn a_shapes_default_style_on_a_new_text_is_not_an_edit() {
        let mut engine = engine_with_measure(vec![with_id(box_at(0.0, 0.0, 50.0, 50.0), "shape")]);
        engine.set_tool(DrawTool::Text);
        engine.begin_pointer(300.0, 300.0, false, false);
        engine.end_pointer();
        let request = engine
            .drain_events()
            .text_edit
            .expect("the text tool opens an editor");
        engine.set_element_text(&request.id, "hi");
        let before = element(&engine, &request.id);
        engine.select(vec!["shape".into()]);
        engine.copy_styles();
        engine.select(vec![request.id.clone()]);

        engine.paste_styles();

        assert_eq!(element(&engine, &request.id), before);
    }

    #[test]
    fn arrowheads_transfer_between_arrows_only() {
        let mut source = with_id(
            connector(0.0, 0.0, 100.0, 0.0, DrawElementType::Arrow),
            "src",
        );
        source.start_arrowhead = Some(Arrowhead::Dot);
        source.end_arrowhead = Some(Arrowhead::Bar);
        let arrow = with_id(
            connector(0.0, 50.0, 100.0, 50.0, DrawElementType::Arrow),
            "arrow",
        );
        let line = with_id(
            connector(0.0, 100.0, 100.0, 100.0, DrawElementType::Line),
            "line",
        );
        let mut engine = engine_with_measure(vec![source, arrow, line]);
        engine.select(vec!["src".into()]);
        engine.copy_styles();
        engine.select(vec!["arrow".into(), "line".into()]);
        engine.paste_styles();
        assert_eq!(
            element(&engine, "arrow").start_arrowhead,
            Some(Arrowhead::Dot)
        );
        assert_eq!(
            element(&engine, "arrow").end_arrowhead,
            Some(Arrowhead::Bar)
        );
        assert_eq!(element(&engine, "line").start_arrowhead, None);
    }

    #[test]
    fn a_frame_stays_clear_and_square() {
        let source = styled(with_id(box_at(0.0, 0.0, 50.0, 50.0), "src"));
        let mut frame = with_id(box_at(200.0, 0.0, 300.0, 300.0), "frame");
        frame.kind = DrawElementType::Frame;
        let mut engine = engine_with_measure(vec![source, frame]);
        engine.select(vec!["src".into()]);
        engine.copy_styles();
        engine.select(vec!["frame".into()]);
        engine.paste_styles();
        let pasted = element(&engine, "frame");
        assert_eq!(pasted.background_color, "transparent");
        assert_eq!(pasted.roundness, None);
    }

    #[test]
    fn a_paste_is_one_step_of_undo() {
        let source = styled(with_id(box_at(0.0, 0.0, 50.0, 50.0), "src"));
        let mut elements = vec![source];
        elements.extend(labelled(box_at(300.0, 0.0, 200.0, 100.0), "dst", "dst-l"));
        let mut engine = engine_with_measure(elements);
        engine.select(vec!["src".into()]);
        engine.copy_styles();
        engine.select(vec!["dst".into()]);
        engine.paste_styles();
        engine.undo();
        assert_eq!(element(&engine, "dst").stroke_color, "#1e1e1e");
    }

    #[test]
    fn pasting_with_nothing_copied_changes_nothing() {
        let mut engine = engine_with_measure(vec![with_id(box_at(0.0, 0.0, 50.0, 50.0), "a")]);
        let before = element(&engine, "a");
        engine.select(vec!["a".into()]);
        engine.paste_styles();
        assert_eq!(element(&engine, "a"), before);
    }
}

/// `getMostUsedCustomColors` counts over the live scene (`colorPickerUtils.ts@1118751f:57-94`);
/// the engine counts, the host filters out the palette.
#[test]
fn colour_counts_cover_the_live_scene() {
    let mut a = with_id(box_at(0.0, 0.0, 50.0, 50.0), "a");
    a.stroke_color = "#123456".into();
    let mut b = with_id(box_at(100.0, 0.0, 50.0, 50.0), "b");
    b.stroke_color = "#123456".into();
    let mut c = with_id(box_at(200.0, 0.0, 50.0, 50.0), "c");
    c.stroke_color = "#abcdef".into();
    c.background_color = "#abcdef".into();
    let mut gone = with_id(box_at(300.0, 0.0, 50.0, 50.0), "gone");
    gone.stroke_color = "#abcdef".into();
    gone.is_deleted = true;
    let engine = engine_with_measure(vec![a, b, c, gone]);

    let mut strokes = engine.color_counts(false);
    strokes.sort();
    assert_eq!(
        strokes,
        vec![("#123456".to_string(), 2), ("#abcdef".to_string(), 1)]
    );
    let backgrounds = engine.color_counts(true);
    assert!(backgrounds.contains(&("#abcdef".to_string(), 1)));
}

/// The summary is plain data, serialised once per revision for the host.
#[test]
fn it_serialises_as_the_host_reads_it() {
    let engine = selected(
        labelled(box_at(0.0, 0.0, 200.0, 100.0), "box", "l"),
        &["box"],
    );
    let style: SelectionStyle = engine.selection_style();
    let json = serde_json::to_value(&style).unwrap();
    assert_eq!(json["count"], 1);
    assert_eq!(json["kinds"], serde_json::json!(["rectangle", "text"]));
    assert_eq!(json["verticalAlignable"], true);
    assert_eq!(json["edges"], "round");
    assert!(
        json["startArrowhead"].is_null(),
        "no arrow, nothing to show"
    );
    assert!(json["fontSize"].is_number());
}
