use std::collections::HashSet;

use crate::scene::element::DrawElement;

pub fn expand_to_groups(
    elements: &[DrawElement],
    ids: impl IntoIterator<Item = String>,
) -> HashSet<String> {
    let mut out: HashSet<String> = ids.into_iter().collect();
    let mut groups = HashSet::new();
    for element in elements {
        if out.contains(&element.id) {
            if let Some(group) = &element.group_id {
                groups.insert(group.clone());
            }
        }
    }
    if !groups.is_empty() {
        for element in elements {
            if let Some(group) = &element.group_id {
                if groups.contains(group) {
                    out.insert(element.id.clone());
                }
            }
        }
    }
    out
}

pub fn group_patches(
    elements: &[DrawElement],
    ids: &HashSet<String>,
    group_id: &str,
) -> Vec<DrawElement> {
    elements
        .iter()
        .filter(|el| ids.contains(&el.id) && !el.is_deleted)
        .cloned()
        .map(|mut el| {
            el.group_id = Some(group_id.to_string());
            el
        })
        .collect()
}

pub fn ungroup_patches(elements: &[DrawElement], ids: &HashSet<String>) -> Vec<DrawElement> {
    elements
        .iter()
        .filter(|el| ids.contains(&el.id) && !el.is_deleted && el.group_id.is_some())
        .cloned()
        .map(|mut el| {
            el.group_id = None;
            el
        })
        .collect()
}

pub fn is_single_group(elements: &[DrawElement], ids: &HashSet<String>) -> bool {
    let mut group: Option<Option<String>> = None;
    let mut count = 0;
    for element in elements {
        if !ids.contains(&element.id) || element.is_deleted {
            continue;
        }
        count += 1;
        let current = element.group_id.clone();
        match &group {
            None => group = Some(current),
            Some(existing) if existing != &current => return false,
            _ => {}
        }
    }
    count > 1 && group.flatten().is_some()
}
