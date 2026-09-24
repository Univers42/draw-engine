use std::collections::{HashMap, HashSet};

use crate::edit::group::with_labels;
use crate::export::json::{elements_from_json, scene_to_json};
use crate::scene::element::{new_element_id, DrawElement};

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
    // Two passes — find the labels the selection drags along, then take what is wanted —
    // so the references are collected once rather than the iterator being walked twice.
    let elements: Vec<&DrawElement> = elements.into_iter().collect();
    let wanted = with_labels(elements.iter().copied(), ids);
    elements
        .into_iter()
        .filter(|element| !element.is_deleted && wanted.contains(&element.id))
        .cloned()
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
    materialize(elements_from_json(json)?, offset_x, offset_y, now, None)
}

/// Fresh copies of `source`: new ids, remapped references, offset, version reset.
///
/// Split out of [`materialize_elements`] so that duplicating does not have to go through
/// JSON. The clipboard needs the text because the text is what crosses the process
/// boundary; Ctrl+D does not, and serialising a selection only to parse it straight back
/// was most of what that keystroke cost.
///
/// `editing` is the group being edited, which a copy made inside it stays in. A paste
/// passes `None`, as the oracle pastes through `duplicateElements` with
/// `type: "everything"` and no group, `packages/excalidraw/components/App.duplicate.ts:
/// 101-111` — a paste is new content, not a copy of something inside the group.
pub fn materialize(
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
                element.start_binding = remap_ref(element.start_binding.as_deref(), &id_map);
                element.end_binding = remap_ref(element.end_binding.as_deref(), &id_map);
                element.container_id = remap_ref(element.container_id.as_deref(), &id_map);
                element.bound_text_id = remap_ref(element.bound_text_id.as_deref(), &id_map);
                element.group_ids = group_ids;
                element.version = 1;
                element.updated = now;
                element.is_deleted = false;
                element
            })
            .collect(),
    )
}
