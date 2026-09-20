use std::collections::{HashMap, HashSet};

use crate::export::json::{elements_from_json, scene_to_json};
use crate::scene::element::{new_element_id, DrawElement};

pub fn expand_for_copy(elements: &[DrawElement], ids: &HashSet<String>) -> Vec<DrawElement> {
    let mut wanted = ids.clone();
    for element in elements {
        if wanted.contains(&element.id) {
            if let Some(bound) = &element.bound_text_id {
                wanted.insert(bound.clone());
            }
        }
    }
    elements
        .iter()
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
    let parsed = elements_from_json(json)?;
    let source: Vec<DrawElement> = parsed
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
                let group_id = element.group_id.as_ref().map(|old| {
                    group_map
                        .entry(old.clone())
                        .or_insert_with(new_element_id)
                        .clone()
                });
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
                element.group_id = group_id;
                element.version = 1;
                element.updated = now;
                element.is_deleted = false;
                element
            })
            .collect(),
    )
}
