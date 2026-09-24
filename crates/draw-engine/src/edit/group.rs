//! Groups, and the one rule that decides what a click selects.
//!
//! A group is not an entity. It is an **id that several elements carry**, and
//! `DrawElement::group_ids` lists the ones an element belongs to, innermost first. That
//! array *is* the nesting — there is no tree and no parent pointer — so a group inside a
//! group is simply two ids on the same element.
//!
//! Transcribed from the oracle and confirmed against the running one; the observations
//! and the confidence markers are in `docs/reference/groups.md`.

use std::collections::HashSet;

use crate::scene::element::DrawElement;

/// The group a click on `element` should select, given the group currently being edited.
///
/// This is the whole of nested-group selection, and every other behaviour here falls out
/// of it — `selectGroupsFromGivenElements`, `packages/element/src/groups.ts:243-270`:
///
/// - Nothing entered: the **outermost** group, so clicking any member takes the lot.
/// - Inside `editing`: the outermost group still **strictly inside** it, so each step in
///   reveals one more level.
/// - Nothing left inside: `None`, and the element itself is what gets selected.
///
/// Note it is the outermost of what *remains* after truncation, not the innermost. That
/// is what makes a double click descend exactly one level per press rather than jumping
/// straight to the leaf.
pub fn selected_group_for<'a>(
    element: &'a DrawElement,
    editing: Option<&str>,
) -> Option<&'a String> {
    let mut ids = element.group_ids.as_slice();
    if let Some(editing) = editing {
        // `?`, so an element that is not in the edited group at all yields nothing.
        // Falling through to its outermost group would reach *outside* the group being
        // edited, which is the one thing stepping into a group is meant to prevent.
        let index = ids.iter().position(|id| id == editing)?;
        ids = &ids[..index];
    }
    ids.last()
}

/// Whether `element` belongs to `group`, at any depth.
pub fn is_in_group(element: &DrawElement, group: &str) -> bool {
    element.group_ids.iter().any(|id| id == group)
}

/// Grows a set of ids to cover every element of every group it touches.
///
/// Group-aware through [`selected_group_for`], so what a click selects and what a
/// selection expands to are decided by one piece of code and cannot drift apart.
///
/// Generic over the iterator, and `Clone` rather than a slice, so the selection path can
/// walk the scene by reference instead of deep-copying every element on the board to ask
/// "is this one in a group?". The two passes are inherent: the first learns which groups
/// are involved, the second collects their members.
pub fn expand_to_groups_among<'a, I>(
    elements: I,
    ids: impl IntoIterator<Item = String>,
) -> HashSet<String>
where
    I: Iterator<Item = &'a DrawElement> + Clone,
{
    expand_within(elements, ids, None)
}

/// [`expand_to_groups_among`], restricted to the inside of the group being edited.
pub fn expand_within<'a, I>(
    elements: I,
    ids: impl IntoIterator<Item = String>,
    editing: Option<&str>,
) -> HashSet<String>
where
    I: Iterator<Item = &'a DrawElement> + Clone,
{
    let mut out: HashSet<String> = ids.into_iter().collect();
    let mut groups: HashSet<&String> = HashSet::new();
    for element in elements.clone() {
        if out.contains(&element.id) {
            if let Some(group) = selected_group_for(element, editing) {
                groups.insert(group);
            }
        }
    }
    if groups.is_empty() {
        return out;
    }
    for element in elements {
        // Membership at *any* depth: an element two levels down is still inside the
        // group being selected, and leaving it behind would tear a group in half the
        // moment one was nested.
        if element.group_ids.iter().any(|id| groups.contains(id)) {
            out.insert(element.id.clone());
        }
    }
    out
}

pub fn expand_to_groups(
    elements: &[DrawElement],
    ids: impl IntoIterator<Item = String>,
) -> HashSet<String> {
    expand_to_groups_among(elements.iter(), ids)
}

/// `ids` and the label of every shape among them.
///
/// A label is never selected on its own — a click on the words selects the shape — so
/// whatever acts on a selection reads the labels in, or a shape leaves its words behind:
/// `getSelectedElements` with `includeBoundTextElement: true`,
/// `packages/element/src/selection.ts:184-193`.
pub fn with_labels<'a>(
    elements: impl IntoIterator<Item = &'a DrawElement>,
    ids: &HashSet<String>,
) -> HashSet<String> {
    let mut out = ids.clone();
    out.extend(
        elements
            .into_iter()
            .filter(|el| ids.contains(&el.id))
            .filter_map(|el| el.bound_text_id.clone()),
    );
    out
}

/// Adds `group_id` as a new level around `ids`.
///
/// Inserted **before** `editing` when a group is being edited, and appended otherwise —
/// `addToGroup`, `groups.ts:311-326`. So grouping at the top level wraps outward, and
/// grouping inside a group nests inward, which is the only way to add a level in the
/// middle without rewriting what is around it.
///
/// Returns nothing for a selection that is already exactly this group: re-wrapping it
/// mints a fresh id over the same elements, so the group changes identity without
/// changing shape — which is what made Ctrl+G appear to do nothing at all.
pub fn group_patches(
    elements: &[DrawElement],
    ids: &HashSet<String>,
    group_id: &str,
    editing: Option<&str>,
) -> Vec<DrawElement> {
    if is_single_group(elements, ids, editing) {
        return Vec::new();
    }
    // A label joins the group with its shape, as the oracle groups the selection read
    // with `includeBoundTextElement: true`, `packages/excalidraw/actions/actionGroup.tsx:
    // 96-101`. Left out, the words were outside the group their shape is in, and every
    // operation on the group after that treated the two apart.
    let ids = with_labels(elements, ids);
    elements
        .iter()
        .filter(|el| ids.contains(&el.id) && !el.is_deleted)
        .cloned()
        .map(|mut el| {
            let at = editing
                .and_then(|editing| el.group_ids.iter().position(|id| id == editing))
                .unwrap_or(el.group_ids.len());
            el.group_ids.insert(at, group_id.to_string());
            el
        })
        .collect()
}

/// Removes one level from each selected element, keeping everything inside it.
///
/// The level removed is whatever [`selected_group_for`] would have selected — so it is
/// the level you can see, and ungrouping twice peels twice rather than flattening on the
/// first press. `removeFromSelectedGroups`, `groups.ts:327-330`, observed on
/// excalidraw.com: `[inner, outer]` becomes `[inner]`.
pub fn ungroup_patches(
    elements: &[DrawElement],
    ids: &HashSet<String>,
    editing: Option<&str>,
) -> Vec<DrawElement> {
    let doomed: HashSet<String> = elements
        .iter()
        .filter(|el| ids.contains(&el.id) && !el.is_deleted)
        .filter_map(|el| selected_group_for(el, editing).cloned())
        .collect();
    if doomed.is_empty() {
        return Vec::new();
    }
    elements
        .iter()
        .filter(|el| !el.is_deleted && el.group_ids.iter().any(|id| doomed.contains(id)))
        .cloned()
        .map(|mut el| {
            el.group_ids.retain(|id| !doomed.contains(id));
            el
        })
        .collect()
}

/// Whether the selection is exactly one group and nothing else.
///
/// The predicate the Ctrl+G toggle turns on, and the guard against re-wrapping. "Exactly"
/// is doing work: two members of a three-element group are *in* a group but are not that
/// group, and wrapping them is a real new level rather than a no-op.
pub fn is_single_group(
    elements: &[DrawElement],
    ids: &HashSet<String>,
    editing: Option<&str>,
) -> bool {
    // A label is carried by its shape and settles nothing here. It may or may not be in
    // the selection — a click on a group holds it, a click on its shape alone does not —
    // and on a board saved before labels joined groups it is in none.
    let shape = |el: &DrawElement| !el.is_deleted && el.container_id.is_none();
    let mut group: Option<&String> = None;
    let mut count = 0;
    for element in elements {
        if !ids.contains(&element.id) || !shape(element) {
            continue;
        }
        count += 1;
        // An element with no group at this level means the selection is not a single
        // group — it is a group plus something loose.
        let Some(current) = selected_group_for(element, editing) else {
            return false;
        };
        match group {
            None => group = Some(current),
            Some(existing) if existing != current => return false,
            _ => {}
        }
    }
    if count < 2 || group.is_none() {
        return false;
    }
    let group = group.expect("checked just above");
    // Every member of that group has to be selected, or this is a part of a group rather
    // than the group itself.
    elements
        .iter()
        .filter(|el| shape(el) && is_in_group(el, group))
        .all(|el| ids.contains(&el.id))
}
