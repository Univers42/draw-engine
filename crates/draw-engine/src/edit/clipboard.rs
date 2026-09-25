use std::collections::{HashMap, HashSet};

use crate::edit::group::{is_in_group, selected_group_for, with_labels};
use crate::export::json::{elements_from_json, scene_to_json};
use crate::scene::binding::{anchor, set_anchor, Anchor, End};
use crate::scene::element::{new_element_id, DrawElement};
use crate::scene::is_frame;

pub fn expand_for_copy(elements: &[DrawElement], ids: &HashSet<String>) -> Vec<DrawElement> {
    expand_for_copy_among(elements.iter(), ids)
}

/// The same, over anything that can be walked — so a caller holding a scene does not have
/// to flatten it into a `Vec<DrawElement>` first.
///
/// That flattening was the cost: duplicating *one* shape deep-copied every element on the
/// board to build a slice it then filtered back down to one. Walking references instead
/// leaves the cost proportional to what is being copied, which is what a person expects
/// when they press Ctrl+D.
pub fn expand_for_copy_among<'a>(
    elements: impl IntoIterator<Item = &'a DrawElement>,
    ids: &HashSet<String>,
) -> Vec<DrawElement> {
    // Collected once, then walked for the children a selected frame brings, the labels
    // everything brings, and what is wanted.
    let elements: Vec<&DrawElement> = elements.into_iter().collect();
    // A selected frame brings what is in it: copy and Ctrl+D both take the selection with
    // `includeElementsInFrames` (`selection.ts@1118751f:196-209`, from
    // `actionClipboard.tsx@1118751f:29-33` and `actionDuplicateSelection.tsx@1118751f:63-71`),
    // a group member frame included (`duplicate.ts@1118751f:333-341`).
    let frames: HashSet<&str> = elements
        .iter()
        .filter(|el| ids.contains(&el.id) && is_frame(el))
        .map(|el| el.id.as_str())
        .collect();
    let mut grown = ids.clone();
    if !frames.is_empty() {
        grown.extend(
            elements
                .iter()
                .filter(|el| !el.is_deleted)
                .filter(|el| el.frame_id.as_deref().is_some_and(|f| frames.contains(f)))
                .map(|el| el.id.clone()),
        );
    }
    let wanted = with_labels(elements.iter().copied(), &grown);
    elements
        .into_iter()
        .filter(|element| !element.is_deleted && wanted.contains(&element.id))
        .cloned()
        .collect()
}

/// What a duplicated run is placed beside: the id every member of it shares, and how to
/// recognise the untouched original that anchors it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DuplicateRun {
    /// A group — the level `selected_group_for` names, or, with no sub-group of its own,
    /// the group being edited itself: the copy stays in it, so the whole edited group is
    /// where it lands (`duplicate_selection`'s prior, narrower rule, generalised here). A
    /// member frame's own children join this run too (`expand_for_copy_among`,
    /// `duplicate_runs`'s `grouped_frames`), even with no group id of their own — but the
    /// frame itself, which does carry one, is always in the run as well, so `owns` below
    /// never has to recognise a plain child on its own to find the run's anchor.
    Group(String),
    /// A frame, with the children of it also being duplicated.
    Frame(String),
    /// A container, with its bound label also being duplicated.
    Container(String),
    /// Nothing shared with anything else being duplicated: its own id.
    Solo(String),
}

impl DuplicateRun {
    /// Whether `element`, untouched in the scene, belongs to this run — so it can anchor
    /// the copies of it.
    pub fn owns(&self, element: &DrawElement) -> bool {
        match self {
            DuplicateRun::Group(group) => is_in_group(element, group),
            DuplicateRun::Frame(frame) => {
                element.id == *frame || element.frame_id.as_deref() == Some(frame.as_str())
            }
            DuplicateRun::Container(container) => {
                element.id == *container
                    || element.container_id.as_deref() == Some(container.as_str())
            }
            DuplicateRun::Solo(id) => element.id == *id,
        }
    }
}

/// Groups `source` (elements about to be duplicated, in scene order — already grown by
/// [`expand_for_copy_among`], so a selected frame's plain children are in it)
/// into the runs a copy joins as one block — a group, a frame with the children of it also
/// being duplicated, a container with its label. Transcribes the oracle's
/// `insertBeforeOrAfterIndex`, which splices every copy of a run in together, found with
/// `findLastIndex` (`duplicate.ts@1118751f:322-436`): a group by
/// `el.groupIds?.includes(groupId)` (`:333-341`) — which also takes a member frame's
/// children, substituted for the frame in the same flat-mapped array — a frame by
/// `el.frameId === frameId || el.id === frameId` (`:364-372`), a container by
/// `el.id === element.id || containerId === element.id` (`:381-389`). Anything else is a
/// run of one, directly above itself (`:426-431`).
///
/// The anchor is not resolved here: [`DuplicateRun::owns`] is, so the caller can search the
/// scene as it stands with the new copies already in it, where a run's untouched original
/// can be an element that was never itself duplicated — the rest of a group entered but
/// only partly copied.
pub fn duplicate_runs(
    source: &[DrawElement],
    editing: Option<&str>,
) -> Vec<(DuplicateRun, Vec<usize>)> {
    let present: HashSet<&str> = source.iter().map(|el| el.id.as_str()).collect();
    let frame_ids: HashSet<&str> = source
        .iter()
        .filter(|el| is_frame(el))
        .map(|el| el.id.as_str())
        .collect();
    // A frame that is itself a group member: its children join the *group's* run, not a
    // run of the frame's own — the oracle builds one array for the whole group,
    // substituting a member frame's children for the frame inside it
    // (`duplicate.ts@1118751f:333-341`), so they land in the same consecutive block as
    // the group's other members rather than splitting off to stack under the frame alone.
    let grouped_frames: HashMap<&str, &str> = source
        .iter()
        .filter(|el| is_frame(el))
        .filter_map(|el| Some((el.id.as_str(), selected_group_for(el, editing)?.as_str())))
        .collect();
    let mut seen: Vec<DuplicateRun> = Vec::new();
    let mut runs: HashMap<DuplicateRun, Vec<usize>> = HashMap::new();
    for (i, element) in source.iter().enumerate() {
        let key = if let Some(group) = selected_group_for(element, editing) {
            DuplicateRun::Group(group.clone())
        } else if let Some(editing) = editing.filter(|editing| is_in_group(element, editing)) {
            // No sub-group of its own, but still inside the group being edited: the copy
            // stays there too, so the whole edited group is the run — not just this one
            // element, which would split the group in the stack around it.
            DuplicateRun::Group(editing.to_string())
        } else if let Some(group) = element
            .frame_id
            .as_deref()
            .and_then(|frame| grouped_frames.get(frame))
        {
            // A plain child of a group member frame, with no group id of its own: folded
            // into what is copied by `expand_for_copy_among` before this ran, and
            // this is where that copy is told the same run as the frame it belongs to.
            DuplicateRun::Group((*group).to_string())
        } else if is_frame(element) {
            DuplicateRun::Frame(element.id.clone())
        } else if let Some(frame) = element
            .frame_id
            .as_deref()
            .filter(|id| frame_ids.contains(id))
        {
            DuplicateRun::Frame(frame.to_string())
        } else if element
            .bound_text_id
            .as_deref()
            .is_some_and(|label| present.contains(label))
        {
            // The container itself: its own id is the run's key, so its label — checked
            // below — joins the same run.
            DuplicateRun::Container(element.id.clone())
        } else if let Some(container) = element
            .container_id
            .as_deref()
            .filter(|id| present.contains(id))
        {
            DuplicateRun::Container(container.to_string())
        } else {
            DuplicateRun::Solo(element.id.clone())
        };
        if !runs.contains_key(&key) {
            seen.push(key.clone());
        }
        runs.entry(key).or_default().push(i);
    }
    seen.into_iter()
        .map(|key| {
            let members = runs.remove(&key).expect("just recorded");
            (key, members)
        })
        .collect()
}

pub fn serialize_selection(elements: &[DrawElement], ids: &HashSet<String>) -> Option<String> {
    let copied = expand_for_copy(elements, ids);
    if copied.is_empty() {
        None
    } else {
        Some(scene_to_json(&copied))
    }
}

fn remap_ref(r#ref: Option<&str>, id_map: &HashMap<String, String>) -> Option<String> {
    r#ref.and_then(|id| id_map.get(id).cloned())
}

pub fn materialize_elements(
    json: &str,
    offset_x: f64,
    offset_y: f64,
    now: f64,
) -> Option<Vec<DrawElement>> {
    materialize(elements_from_json(json)?, offset_x, offset_y, now)
}

/// Fresh copies of `source`: new ids, remapped references, offset, version reset.
///
/// Split out of [`materialize_elements`] so that duplicating does not have to go through
/// JSON. The clipboard needs the text because the text is what crosses the process
/// boundary; Ctrl+D does not, and serialising a selection only to parse it straight back
/// was most of what that keystroke cost.
pub fn materialize(
    source: Vec<DrawElement>,
    offset_x: f64,
    offset_y: f64,
    now: f64,
) -> Option<Vec<DrawElement>> {
    materialize_within(source, offset_x, offset_y, now, None)
}

/// [`materialize`] for a copy made inside `editing`, the group being edited, which the
/// copy stays in. A paste uses [`materialize`], as the oracle pastes through
/// `duplicateElements` with `type: "everything"` and no group,
/// `packages/excalidraw/components/App.duplicate.ts:101-111` — a paste is new content,
/// not a copy of something inside the group.
pub fn materialize_within(
    source: Vec<DrawElement>,
    offset_x: f64,
    offset_y: f64,
    now: f64,
    editing: Option<&str>,
) -> Option<Vec<DrawElement>> {
    let source: Vec<DrawElement> = source
        .into_iter()
        .filter(|element| !element.is_deleted)
        .collect();
    if source.is_empty() {
        return None;
    }
    let mut id_map = HashMap::new();
    let mut group_map = HashMap::new();
    for element in &source {
        id_map.insert(element.id.clone(), new_element_id());
    }
    Some(
        source
            .into_iter()
            .map(|mut element| {
                // A fresh id for every level inside the group being edited — every level,
                // when none is — and that group and all around it kept, so a copy made
                // inside a group stays in it: `getNewGroupIdsForDuplication`,
                // `packages/element/src/groups.ts:397-413`. Regenerating the edited group
                // too put the copy in a group of its own, outside the one it was made in.
                //
                // Two elements that shared a group still share its copy — the map is keyed
                // by the old id, so the structure survives while the identity does not.
                // Remapping only the outermost would join the copy to the original one
                // level down.
                let inside = editing
                    .and_then(|editing| element.group_ids.iter().position(|id| id == editing))
                    .unwrap_or(element.group_ids.len());
                let mut group_ids = element.group_ids.clone();
                for level in &mut group_ids[..inside] {
                    *level = group_map
                        .entry(level.clone())
                        .or_insert_with(new_element_id)
                        .clone();
                }
                element.id = id_map
                    .get(&element.id)
                    .cloned()
                    .unwrap_or_else(new_element_id);
                element.x += offset_x;
                element.y += offset_y;
                // An end whose shape was not copied lets go, anchor and all.
                for end in [End::Start, End::End] {
                    let copied = anchor(&element, end).and_then(|a| {
                        Some(Anchor {
                            element_id: id_map.get(&a.element_id)?.clone(),
                            ..a
                        })
                    });
                    set_anchor(&mut element, end, copied);
                }
                element.container_id = remap_ref(element.container_id.as_deref(), &id_map);
                element.bound_text_id = remap_ref(element.bound_text_id.as_deref(), &id_map);
                element.group_ids = group_ids;
                element.version = 1;
                element.updated = now;
                // A copy is a new note, dated now (`duplicate.ts@1118751f:116-117`).
                if crate::scene::sticky::is_sticky_note(&element) {
                    element.created = Some(crate::scene::sticky::wall_clock_ms());
                }
                element.is_deleted = false;
                element
            })
            .collect(),
    )
}

#[cfg(test)]
mod duplicate_runs_tests {
    use super::{duplicate_runs, DuplicateRun};
    use crate::scene::element::{create_element_default, DrawElementType, Geometry};
    use crate::scene::DrawElement;

    fn el(id: &str, kind: DrawElementType) -> DrawElement {
        let mut element = create_element_default(
            kind,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
        );
        element.id = id.to_string();
        element
    }

    fn rect(id: &str) -> DrawElement {
        el(id, DrawElementType::Rectangle)
    }

    /// Two elements with nothing in common are each their own run, anchored on themselves —
    /// what makes a plain Ctrl+D land directly above its own source.
    #[test]
    fn ungrouped_elements_are_solo_runs() {
        let source = vec![rect("a"), rect("b")];
        let runs = duplicate_runs(&source, None);
        assert_eq!(
            runs,
            vec![
                (DuplicateRun::Solo("a".into()), vec![0]),
                (DuplicateRun::Solo("b".into()), vec![1]),
            ]
        );
    }

    /// A whole group is one run, anchored on its topmost member — the oracle keeps a
    /// group's copies as one consecutive block above the group (`duplicate.ts@1118751f:
    /// 322-348`).
    #[test]
    fn a_group_is_one_run_anchored_on_its_top_member() {
        let mut a = rect("a");
        a.group_ids = vec!["g".into()];
        let mut b = rect("b");
        b.group_ids = vec!["g".into()];
        let source = vec![a, rect("x"), b];
        let runs = duplicate_runs(&source, None);
        assert_eq!(
            runs,
            vec![
                (DuplicateRun::Group("g".into()), vec![0, 2]),
                (DuplicateRun::Solo("x".into()), vec![1]),
            ]
        );
    }

    /// A frame that is itself a group member folds its plain child (no group id of its
    /// own) into the *group's* run, not a run of the frame's own — the oracle builds one
    /// array for the whole group, substituting the frame's children for the frame inside
    /// it (`duplicate.ts@1118751f:333-341`), so the child lands in the same consecutive
    /// block as the frame and its sibling rather than splitting off.
    #[test]
    fn a_group_member_frames_child_joins_the_groups_run() {
        let mut frame = el("f", DrawElementType::Frame);
        frame.group_ids = vec!["g".into()];
        let mut sibling = rect("s");
        sibling.group_ids = vec!["g".into()];
        let mut child = rect("c");
        child.frame_id = Some("f".into());
        let source = vec![child, frame, sibling];
        let runs = duplicate_runs(&source, None);
        assert_eq!(
            runs,
            vec![(DuplicateRun::Group("g".into()), vec![0, 1, 2])],
            "child, frame and sibling are one run — the child never gets a run of its own"
        );
    }

    /// A frame child that is *also* a direct group member resolves through the ordinary
    /// group branch, which runs first — it is not counted a second time through the
    /// frame-folding branch.
    #[test]
    fn a_frame_child_that_is_also_a_group_member_is_counted_once() {
        let mut frame = el("f", DrawElementType::Frame);
        frame.group_ids = vec!["g".into()];
        let mut child = rect("c");
        child.frame_id = Some("f".into());
        child.group_ids = vec!["g".into()];
        let source = vec![child, frame];
        let runs = duplicate_runs(&source, None);
        assert_eq!(runs, vec![(DuplicateRun::Group("g".into()), vec![0, 1])]);
    }

    /// Duplicating only the members of a group being edited, not the group itself, still
    /// keeps them one run — `selected_group_for` returns nothing for a level the edit is
    /// already inside, but the copy stays in the edited group all the same, so splitting
    /// them into runs of one would break the group apart in the stack around it.
    #[test]
    fn members_of_the_group_being_edited_are_one_run() {
        let mut a = rect("a");
        a.group_ids = vec!["g".into()];
        let mut b = rect("b");
        b.group_ids = vec!["g".into()];
        let source = vec![a, b];
        let runs = duplicate_runs(&source, Some("g"));
        assert_eq!(runs, vec![(DuplicateRun::Group("g".into()), vec![0, 1])]);
    }

    /// A frame duplicated with its children is one run, anchored on the topmost of the
    /// two — the frame's own run stacks exactly as the shape-drawn-into-a-frame case does
    /// (`duplicate.ts@1118751f:364-379`).
    #[test]
    fn a_frame_and_its_duplicated_children_are_one_run() {
        let child = {
            let mut c = rect("c");
            c.frame_id = Some("f".into());
            c
        };
        let frame = el("f", DrawElementType::Frame);
        let source = vec![child, frame];
        let runs = duplicate_runs(&source, None);
        assert_eq!(runs, vec![(DuplicateRun::Frame("f".into()), vec![0, 1])]);
    }

    /// A frame's child duplicated alone, without the frame, is its own run: the oracle's
    /// frame branch only triggers when the frame itself is also being duplicated
    /// (`frameIdsToDuplicate.has(element.frameId)`, `duplicate.ts@1118751f:349-352`).
    #[test]
    fn a_lone_frame_child_is_a_solo_run() {
        let mut child = rect("c");
        child.frame_id = Some("f".into());
        let source = vec![child];
        let runs = duplicate_runs(&source, None);
        assert_eq!(runs, vec![(DuplicateRun::Solo("c".into()), vec![0])]);
    }

    /// A container and its bound label are one run, keyed on the container — the label
    /// sits above it, so that is where the oracle's `findLastIndex` lands
    /// (`duplicate.ts@1118751f:381-397`).
    #[test]
    fn a_container_and_its_label_are_one_run() {
        let mut container = rect("box");
        container.bound_text_id = Some("lbl".into());
        let mut label = el("lbl", DrawElementType::Text);
        label.container_id = Some("box".into());
        let source = vec![container, label];
        let runs = duplicate_runs(&source, None);
        assert_eq!(
            runs,
            vec![(DuplicateRun::Container("box".into()), vec![0, 1])]
        );
    }
}
