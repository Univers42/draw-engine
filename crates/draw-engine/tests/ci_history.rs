#![allow(clippy::ptr_arg)]
use draw_engine::*;

fn sig(s: &String) -> String {
    s.clone()
}

#[test]
fn history_initial_state_cannot_undo() {
    let history = SnapshotHistory::new("state0".to_string(), sig, 10);
    assert!(!history.can_undo());
}

#[test]
fn history_initial_state_cannot_redo() {
    let history = SnapshotHistory::new("state0".to_string(), sig, 10);
    assert!(!history.can_redo());
}

#[test]
fn history_single_push_enables_undo() {
    let mut history = SnapshotHistory::new("state0".to_string(), sig, 10);
    history.push("state1".to_string());
    assert!(history.can_undo());
    assert!(!history.can_redo());
}

#[test]
fn history_undo_returns_previous_state() {
    let mut history = SnapshotHistory::new("state0".to_string(), sig, 10);
    history.push("state1".to_string());
    let prev = history.undo();
    assert_eq!(prev, Some(&"state0".to_string()));
    assert!(!history.can_undo());
    assert!(history.can_redo());
}

#[test]
fn history_redo_restores_undone_state() {
    let mut history = SnapshotHistory::new("state0".to_string(), sig, 10);
    history.push("state1".to_string());
    history.undo();
    let next = history.redo();
    assert_eq!(next, Some(&"state1".to_string()));
    assert!(history.can_undo());
    assert!(!history.can_redo());
}

#[test]
fn history_deduplicates_identical_signature_consecutively() {
    let mut history = SnapshotHistory::new("state0".to_string(), sig, 10);
    history.push("state0".to_string());
    assert!(!history.can_undo());
}

#[test]
fn history_allows_same_state_after_intermediate_change() {
    let mut history = SnapshotHistory::new("state0".to_string(), sig, 10);
    history.push("state1".to_string());
    history.push("state0".to_string());
    assert!(history.can_undo());
    assert_eq!(history.undo(), Some(&"state1".to_string()));
    assert_eq!(history.undo(), Some(&"state0".to_string()));
}

#[test]
fn history_push_after_undo_truncates_redo_branch() {
    let mut history = SnapshotHistory::new("state0".to_string(), sig, 10);
    history.push("state1".to_string());
    history.push("state2".to_string());
    history.undo(); // now at state1
    history.push("state_branch".to_string()); // should truncate state2

    assert!(!history.can_redo());
    assert_eq!(history.undo(), Some(&"state1".to_string()));
    assert_eq!(history.undo(), Some(&"state0".to_string()));
}

#[test]
fn history_limit_evicts_oldest_snapshot() {
    let mut history = SnapshotHistory::new("state0".to_string(), sig, 3);
    history.push("state1".to_string());
    history.push("state2".to_string());
    history.push("state3".to_string()); // limit is 3, state0 is evicted

    assert_eq!(history.undo(), Some(&"state2".to_string()));
    assert_eq!(history.undo(), Some(&"state1".to_string()));
    assert!(!history.can_undo()); // state0 is gone
}

#[test]
fn history_undo_all_the_way_to_evicted_bottom() {
    let mut history = SnapshotHistory::new("s0".to_string(), sig, 2);
    history.push("s1".to_string());
    history.push("s2".to_string());
    history.push("s3".to_string());
    assert_eq!(history.undo(), Some(&"s2".to_string()));
    assert_eq!(history.undo(), None);
}

#[test]
fn history_reset_clears_stack_to_single_value() {
    let mut history = SnapshotHistory::new("state0".to_string(), sig, 10);
    history.push("state1".to_string());
    history.push("state2".to_string());
    history.reset("clean_state".to_string());

    assert!(!history.can_undo());
    assert!(!history.can_redo());
}

#[test]
fn history_undo_on_empty_returns_none() {
    let mut history = SnapshotHistory::new("init".to_string(), sig, 10);
    assert_eq!(history.undo(), None);
}

#[test]
fn history_redo_on_tip_returns_none() {
    let mut history = SnapshotHistory::new("init".to_string(), sig, 10);
    assert_eq!(history.redo(), None);
}

#[test]
fn history_multiple_undo_and_redo_cycles() {
    let mut history = SnapshotHistory::new("0".to_string(), sig, 10);
    history.push("1".to_string());
    history.push("2".to_string());
    for _ in 0..3 {
        assert_eq!(history.undo(), Some(&"1".to_string()));
        assert_eq!(history.redo(), Some(&"2".to_string()));
    }
}
