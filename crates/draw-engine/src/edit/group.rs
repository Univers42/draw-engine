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

/// Whether `group` is still a group: at least two live elements carry it.
///
/// Deleting all but one member leaves the id on the survivor, and a group of one is no
/// group — the oracle drops it wherever it would select it (`selectGroup`,
/// `packages/element/src/groups.ts:42-54`; `_selectGroups`, `:134-141`). So it cannot be
/// stepped into, ungrouped, or kept as the level being edited.
pub fn is_live_group<'a>(elements: impl Iterator<Item = &'a DrawElement>, group: &str) -> bool {
    elements
        .filter(|el| !el.is_deleted && is_in_group(el, group))
        .nth(1)
        .is_some()
}

/// The group directly around `group` on `element`, if any — the next id out in its
/// innermost → outermost list. `getParentEditingGroupId`,
/// `packages/excalidraw/actions/actionDeselect.ts:36-62`.
pub fn parent_group<'a>(element: &'a DrawElement, group: &str) -> Option<&'a String> {
    let index = element.group_ids.iter().position(|id| id == group)?;
    element.group_ids.get(index + 1)
}

/// Whether `editing` can stay the group being edited once `selected` is what is held.
///
/// The edited group is a claim about the selection — "everything held is inside this" —
/// and it holds only while something is held, all of it is inside, and the group is still
/// a group. The oracle drops it wherever one of those stops being true: on an empty
/// selection (`clearSelection`, `packages/excalidraw/components/App.tsx:12889-12902`;
/// `groups.ts:188-196`), on a press outside it (`App.tsx:9639-9650`), and once its group
/// is gone (`packages/element/src/delta.ts:806-818`). Kept past any of them, every later
/// click and marquee was resolved inside a group it had nothing to do with.
pub fn keeps_editing<'a, I>(elements: I, selected: &HashSet<String>, editing: &str) -> bool
where
    I: Iterator<Item = &'a DrawElement> + Clone,
{
    let mut held = elements
        .clone()
        .filter(|el| !el.is_deleted && selected.contains(&el.id))
        .peekable();
    held.peek().is_some()
        && held.all(|el| is_in_group(el, editing))
        && is_live_group(elements, editing)
}

/// Where deleting inside `editing` leaves you: the level still being edited, and what is
/// held there. `elements` is the scene after the deletion.
///
/// `actionDeleteSelected.tsx:130-170`, then `handleGroupEditingState` (`:190-205`), which
/// overrides the selection with the first member of whatever level is still open:
///
/// - two or more left: the group stays open, holding its first member;
/// - one left: that level is no group now, so it steps up to the group around it, if
///   that one still is;
/// - otherwise the group is left, holding the survivor at the top level.
///
/// Only what `touchable` accepts is held — the oracle takes the first sibling whatever it
/// is, but here a locked or peer-held element is never picked up, and holding one handed
/// it to the next Delete. With none left to hold, nothing is.
pub fn after_delete_within<'a, I>(
    elements: I,
    editing: &str,
    touchable: impl Fn(&DrawElement) -> bool,
) -> (Option<String>, HashSet<String>)
where
    I: Iterator<Item = &'a DrawElement> + Clone,
{
    let Some(any) = elements
        .clone()
        .find(|el| !el.is_deleted && is_in_group(el, editing))
    else {
        return (None, HashSet::new());
    };
    let level = if is_live_group(elements.clone(), editing) {
        Some(editing)
    } else {
        parent_group(any, editing)
            .map(String::as_str)
            .filter(|parent| is_live_group(elements.clone(), parent))
    };
    let first_in = |group: &str| {
        elements
            .clone()
            .find(|el| !el.is_deleted && is_in_group(el, group) && touchable(el))
    };
    match level.and_then(|level| Some((level, first_in(level)?))) {
        Some((level, held)) => (Some(level.to_string()), HashSet::from([held.id.clone()])),
        None => match first_in(editing) {
            Some(survivor) => (None, expand_within(elements, [survivor.id.clone()], None)),
            None => (None, HashSet::new()),
        },
    }
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

/// What moving, resizing or turning the selection carries: the unlocked part of it,
/// grown back to the groups that part belongs to, and nothing that is not selected.
///
/// A locked element is never picked up on its own, but a group is one thing. The oracle
/// selects a group's members with no lock filter (`packages/element/src/groups.ts:94-132`)
/// and drags every selected element, refusing only when *every* one is locked
/// (`packages/excalidraw/components/App.tsx:10899-10904`). Filtering locked elements out
/// one by one instead left a locked member behind while the rest of its group moved.
///
/// Grown from the unlocked part rather than taken whole because a locked element can be
/// in the selection here without a group to carry it: Select All takes locked elements
/// so they can be unlocked from the menu, where the oracle's skips them
/// (`actionSelectAll.ts:32-38`). Such an element, or a group locked throughout, stays.
pub fn carried_by<'a, I>(
    elements: I,
    selected: &HashSet<String>,
    editing: Option<&str>,
) -> HashSet<String>
where
    I: Iterator<Item = &'a DrawElement> + Clone,
{
    let unlocked = elements
        .clone()
        .filter(|el| selected.contains(&el.id) && !el.locked())
        .map(|el| el.id.clone());
    let mut carried = expand_within(elements, unlocked, editing);
    carried.retain(|id| selected.contains(id));
    carried
}

/// Part of the edited group taken out of it: each element of `moved` loses `editing`
/// and every group around it, keeping the groups inside; and each group it left that is
/// down to fewer than two live members stops being a group on whoever still carries it.
/// Returns the elements that change, unstamped.
///
/// `updateGroupIdsAfterEditingGroup` (`packages/excalidraw/components/App.tsx:
/// 11993-12040`), which runs when part of the edited group is dragged into or out of a
/// frame. Narrowed on purpose: the oracle then clears *every* group id of any element on
/// the board whose outermost group is down to one member, which rewrites — and here would
/// restamp and send — elements the drag never touched. Only the dead ids of the groups
/// that were left go, and only from their survivors.
pub fn leave_edited_group<'a, I>(
    elements: I,
    moved: &HashSet<String>,
    editing: &str,
) -> Vec<DrawElement>
where
    I: Iterator<Item = &'a DrawElement> + Clone,
{
    // The edited group and every group around it: the levels the part leaves.
    let mut left: HashSet<String> = HashSet::new();
    let mut out: Vec<DrawElement> = Vec::new();
    for element in elements.clone().filter(|el| moved.contains(&el.id)) {
        if let Some(at) = element.group_ids.iter().position(|id| id == editing) {
            left.extend(element.group_ids[at..].iter().cloned());
            let mut next = element.clone();
            next.group_ids.truncate(at);
            out.push(next);
        }
    }
    let taken_out: HashSet<String> = out.iter().map(|el| el.id.clone()).collect();
    let stays = |el: &DrawElement| !el.is_deleted && !taken_out.contains(&el.id);
    // Counted without the part taken out, which carries none of the groups it left.
    let dead: HashSet<String> = left
        .into_iter()
        .filter(|group| {
            elements
                .clone()
                .filter(|el| stays(el) && is_in_group(el, group))
                .nth(1)
                .is_none()
        })
        .collect();
    out.extend(
        elements
            .filter(|el| stays(el) && el.group_ids.iter().any(|g| dead.contains(g)))
            .map(|el| {
                let mut next = el.clone();
                next.group_ids.retain(|g| !dead.contains(g));
                next
            }),
    );
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
///
/// A group of one is not a level: the oracle never selects it (`groups.ts:134-141`), so
/// `actionUngroup` finds nothing to remove (`actionGroup.tsx:226-231`) and the dead id
/// stays on its survivor.
pub fn ungroup_patches(
    elements: &[DrawElement],
    ids: &HashSet<String>,
    editing: Option<&str>,
) -> Vec<DrawElement> {
    let mut doomed: HashSet<String> = elements
        .iter()
        .filter(|el| ids.contains(&el.id) && !el.is_deleted)
        .filter_map(|el| selected_group_for(el, editing).cloned())
        .collect();
    // Once per group, not per selected element: a scan of the board each.
    doomed.retain(|group| is_live_group(elements.iter(), group));
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
pub fn is_single_group<'a, I>(elements: I, ids: &HashSet<String>, editing: Option<&str>) -> bool
where
    I: IntoIterator<Item = &'a DrawElement>,
    I::IntoIter: Clone,
{
    let elements = elements.into_iter();
    // A label is carried by its shape and settles nothing here. It may or may not be in
    // the selection — a click on a group holds it, a click on its shape alone does not —
    // and on a board saved before labels joined groups it is in none.
    let shape = |el: &DrawElement| !el.is_deleted && el.container_id.is_none();
    let mut group: Option<&String> = None;
    let mut count = 0;
    for element in elements.clone() {
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
    // One shape is enough once its label shares the group: a group holding a labelled
    // shape and nothing else is still a group, as it is to the oracle's
    // `allElementsInSameGroup` (`actionGroup.tsx:73-83`). Counted as two, it wrapped
    // itself in a new level on every Ctrl+G.
    let Some(group) = group.filter(|group| count > 0 && is_live_group(elements.clone(), group))
    else {
        return false;
    };
    // Every member of that group has to be selected, or this is a part of a group rather
    // than the group itself.
    elements
        .filter(|el| shape(el) && is_in_group(el, group))
        .all(|el| ids.contains(&el.id))
}
