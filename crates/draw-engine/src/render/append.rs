//! Bringing a cached picture up to date by painting on top of it.
//!
//! The painter keeps the board in an offscreen picture and redrew all of it whenever the
//! scene changed. Most changes to a big board are an element *added on top* — a shape
//! drawn, a stroke, a paste, every Ctrl+D of a stack of copies — and for those, the new
//! picture is the old one with the new elements painted over it: the same operations in
//! the same order, so the same pixels. Measured on a board of 9,000 shapes at 15%, a
//! duplicate went from ten milliseconds of rasterising the whole board to painting one
//! shape.
//!
//! This decides *whether* that is so, from the scene's journal. It has to be right in the
//! conservative direction only: any change it cannot prove is on top means a redraw.

use std::collections::HashSet;

use crate::scene::{Change, DrawElement};

/// What a cached picture holds, beyond the key it was painted under.
#[derive(Clone, Debug, PartialEq)]
pub struct PictureBase {
    /// The scene revision it was painted at.
    pub revision: u64,
    /// Elements on top of the stack that were left out of it: the live elements of a
    /// gesture, which are drawn over the picture every frame rather than into it.
    pub left_out: Vec<String>,
}

/// The elements to paint over the picture to make it the scene as it is now — the top of
/// the visible stack, in order — or `None` when painting on top cannot get there.
///
/// `changes` is the scene's journal since `base.revision`, or `None` when the journal
/// does not reach back that far. `visible` is what the frame draws, in stacking order.
pub fn plan_append<'a>(
    base: &PictureBase,
    changes: Option<&[(u64, Change)]>,
    visible: &[&'a DrawElement],
) -> Option<Vec<&'a DrawElement>> {
    let changes = changes?;
    let mut on_top: HashSet<&str> = base.left_out.iter().map(String::as_str).collect();
    for (_, change) in changes {
        match change {
            Change::Appended(id) => {
                on_top.insert(id.as_str());
            }
            // A change to something already in the picture cannot be painted over it:
            // the old version is in there.
            Change::Touched(id) if !on_top.contains(id.as_str()) => return None,
            Change::Touched(_) => {}
            Change::Rearranged => return None,
        }
    }
    // They must be the top of the stack, and nothing else may be: anything of the picture
    // sitting above one of them would end up underneath it.
    let first = visible
        .iter()
        .position(|element| on_top.contains(element.id.as_str()))
        .unwrap_or(visible.len());
    let tail = &visible[first..];
    tail.iter()
        .all(|element| on_top.contains(element.id.as_str()))
        .then(|| tail.to_vec())
}

/// Whether the picture already *is* everything below a gesture's live elements: every
/// element it lacks is live and on top, and every live element is one it lacks.
///
/// Then the layer a gesture draws over needs no painting at all. The first frame of every
/// new shape, stroke or Alt-drag copy used to paint all the rest of the board afresh —
/// ten milliseconds on 9,000 shapes, for a picture that was already on screen.
pub fn holds_all_but(
    base: &PictureBase,
    changes: Option<&[(u64, Change)]>,
    visible: &[&DrawElement],
    live: &[&DrawElement],
) -> bool {
    let Some(top) = plan_append(base, changes, visible) else {
        return false;
    };
    // The same elements: a live one the picture holds would show twice — once where it
    // was, in the picture, and once where it is going.
    top.len() == live.len() && top.iter().zip(live).all(|(a, b)| a.id == b.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::element::{create_element, DrawElementStyle, DrawElementType, Geometry};
    use crate::scene::Scene;

    fn shape(id: &str) -> DrawElement {
        let mut element = create_element(
            DrawElementType::Rectangle,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            DrawElementStyle::default(),
            0.0,
        );
        element.id = id.into();
        element
    }

    fn board() -> Scene {
        Scene::new(["a", "b", "c"].map(shape))
    }

    fn ids(elements: Option<Vec<&DrawElement>>) -> Option<Vec<String>> {
        elements.map(|all| all.into_iter().map(|e| e.id.clone()).collect())
    }

    fn plan(scene: &Scene, base: &PictureBase) -> Option<Vec<String>> {
        let visible = scene.ordered_refs();
        ids(plan_append(
            base,
            scene.changes_since(base.revision),
            &visible,
        ))
    }

    fn painted(scene: &Scene) -> PictureBase {
        PictureBase {
            revision: scene.revision(),
            left_out: Vec::new(),
        }
    }

    #[test]
    fn an_element_added_on_top_is_painted_over_the_picture() {
        let mut scene = board();
        let base = painted(&scene);
        scene.add(shape("d"));
        scene.add(shape("e"));
        assert_eq!(plan(&scene, &base), Some(vec!["d".into(), "e".into()]));
    }

    #[test]
    fn a_new_element_changed_again_is_still_only_on_top() {
        // A commit stamps what it made: the new element is touched after it is added,
        // and it is painted as it is now.
        let mut scene = board();
        let base = painted(&scene);
        scene.add(shape("d"));
        scene.update("d", |e| e.version += 1);
        assert_eq!(plan(&scene, &base), Some(vec!["d".into()]));
    }

    #[test]
    fn a_change_to_anything_in_the_picture_redraws_it() {
        let mut scene = board();
        let base = painted(&scene);
        scene.add(shape("d"));
        scene.update("b", |e| e.x += 1.0);
        assert_eq!(plan(&scene, &base), None);
    }

    #[test]
    fn a_reorder_or_a_hard_delete_redraws_it() {
        let mut scene = board();
        let base = painted(&scene);
        scene.bring_to_front("a");
        assert_eq!(plan(&scene, &base), None);

        let mut scene = board();
        let base = painted(&scene);
        scene.add(shape("d"));
        scene.discard("d");
        assert_eq!(plan(&scene, &base), None);
    }

    #[test]
    fn nothing_changed_is_nothing_to_paint() {
        let scene = board();
        assert_eq!(plan(&scene, &painted(&scene)), Some(vec![]));
    }

    #[test]
    fn a_picture_of_another_scene_is_never_painted_over() {
        // The trap: a new scene's journal is nothing but additions — its own elements —
        // so a picture of the old board would take them all, painted over the old board.
        let old = board();
        let base = painted(&old);
        let mut replacement = Scene::new(["x"].map(shape));
        replacement.add(shape("y"));
        assert_eq!(plan(&replacement, &base), None);
    }

    #[test]
    fn what_a_gesture_left_out_is_painted_on_top_when_it_ends() {
        // The live elements were drawn over the picture while the gesture ran; when it
        // ends they are painted into it, which is the whole scene again.
        let mut scene = board();
        let base = PictureBase {
            revision: scene.revision(),
            left_out: vec!["c".into()],
        };
        scene.update("c", |e| e.x += 5.0);
        assert_eq!(plan(&scene, &base), Some(vec!["c".into()]));
    }

    #[test]
    fn what_was_left_out_must_still_be_on_top() {
        // Left out of the picture but no longer the top of the stack: something of the
        // picture is now above it, and painting it over would put it underneath.
        let scene = board();
        let base = PictureBase {
            revision: scene.revision(),
            left_out: vec!["b".into()],
        };
        assert_eq!(plan(&scene, &base), None);
    }

    #[test]
    fn a_journal_that_no_longer_reaches_back_redraws_it() {
        let mut scene = board();
        let base = painted(&scene);
        for i in 0..2000 {
            scene.add(shape(&format!("n{i}")));
        }
        assert_eq!(plan(&scene, &base), None);
    }

    #[test]
    fn a_deleted_new_element_is_not_painted() {
        // A soft delete keeps the element, marked; nothing draws a tombstone.
        let mut scene = board();
        let base = painted(&scene);
        scene.add(shape("d"));
        scene.add(shape("e"));
        scene.remove("d", 0.0);
        assert_eq!(plan(&scene, &base), Some(vec!["e".into()]));
    }

    fn live<'a>(scene: &'a Scene, ids: &[&str]) -> Vec<&'a DrawElement> {
        scene
            .ordered_refs()
            .into_iter()
            .filter(|e| ids.contains(&e.id.as_str()))
            .collect()
    }

    fn holds(scene: &Scene, base: &PictureBase, ids: &[&str]) -> bool {
        holds_all_but(
            base,
            scene.changes_since(base.revision),
            &scene.ordered_refs(),
            &live(scene, ids),
        )
    }

    #[test]
    fn a_new_element_being_drawn_leaves_the_picture_as_it_is() {
        let mut scene = board();
        let base = painted(&scene);
        scene.add(shape("d"));
        scene.update("d", |e| e.width += 5.0);
        assert!(holds(&scene, &base, &["d"]));
    }

    #[test]
    fn an_element_the_picture_holds_cannot_be_live_over_it() {
        // Dragging something already drawn: the picture has it where it was.
        let scene = board();
        let base = painted(&scene);
        assert!(!holds(&scene, &base, &["c"]));
    }

    #[test]
    fn something_new_that_is_not_live_is_not_in_the_picture_either() {
        let mut scene = board();
        let base = painted(&scene);
        scene.add(shape("d"));
        scene.add(shape("e"));
        assert!(!holds(&scene, &base, &["e"]));
    }
}
