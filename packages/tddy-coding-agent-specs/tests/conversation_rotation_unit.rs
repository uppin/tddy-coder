//! A spec's conversation rotation, report by report (`#orchestrators` 6/11): an `ends` report for
//! the session's current conversation arms it, the `begins` report that follows rotates it, and a
//! clear of any other conversation — an agent tab sharing the session's hooks — rotates nothing
//! (developer decision 2026-11-13).

use std::collections::{BTreeMap, HashMap};

use pretty_assertions::assert_eq;
use tddy_coding_agent_specs::slots::conversation::{
    ConversationSlot, RotationHook, RotationSlot, RotationState, RotationStep,
};

const CURRENT: &str = "c-current";
const ANOTHER: &str = "c-a-tabs";
const NEXT: &str = "c-next";

fn a_hook(hook: &str, (field, value): (&str, &str)) -> RotationHook {
    RotationHook {
        hook: hook.to_string(),
        when: BTreeMap::from([(field.to_string(), value.to_string())]),
        id_field: "session_id".to_string(),
    }
}

/// Claude Code's `/clear`: `SessionEnd` with `reason: clear`, then `SessionStart` with
/// `source: clear`, each naming its conversation in `session_id`.
fn a_clear_rotation() -> RotationSlot {
    RotationSlot {
        ends: a_hook("SessionEnd", ("reason", "clear")),
        begins: a_hook("SessionStart", ("source", "clear")),
    }
}

fn fields(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

fn a_clear_ending(conversation: &str) -> HashMap<String, String> {
    fields(&[("reason", "clear"), ("session_id", conversation)])
}

fn a_clear_beginning(conversation: &str) -> HashMap<String, String> {
    fields(&[("source", "clear"), ("session_id", conversation)])
}

// ---------------------------------------------------------------------------
// RotationSlot::step
// ---------------------------------------------------------------------------

#[test]
fn the_current_conversation_ending_in_a_clear_arms_the_rotation() {
    // When
    let step = a_clear_rotation().step(
        RotationState::Idle,
        CURRENT,
        "SessionEnd",
        &a_clear_ending(CURRENT),
    );

    // Then
    assert_eq!(step, RotationStep::Arm);
}

#[test]
fn the_clear_that_follows_the_current_conversations_end_rotates_to_its_conversation() {
    // When
    let step = a_clear_rotation().step(
        RotationState::Armed,
        CURRENT,
        "SessionStart",
        &a_clear_beginning(NEXT),
    );

    // Then
    assert_eq!(step, RotationStep::RotateTo(NEXT.to_string()));
}

#[test]
fn a_clear_ending_another_conversation_arms_nothing() {
    // When an agent tab's conversation ends in a clear
    let step = a_clear_rotation().step(
        RotationState::Idle,
        CURRENT,
        "SessionEnd",
        &a_clear_ending(ANOTHER),
    );

    // Then
    assert_eq!(step, RotationStep::Unchanged);
}

#[test]
fn a_clear_beginning_that_nothing_armed_rotates_nothing() {
    // When
    let step = a_clear_rotation().step(
        RotationState::Idle,
        CURRENT,
        "SessionStart",
        &a_clear_beginning(NEXT),
    );

    // Then
    assert_eq!(step, RotationStep::Unchanged);
}

#[test]
fn a_session_start_that_is_not_a_clear_rotates_nothing_even_when_armed() {
    // When
    let step = a_clear_rotation().step(
        RotationState::Armed,
        CURRENT,
        "SessionStart",
        &fields(&[("source", "startup"), ("session_id", NEXT)]),
    );

    // Then
    assert_eq!(step, RotationStep::Unchanged);
}

// ---------------------------------------------------------------------------
// RotationHook::conversation_of
// ---------------------------------------------------------------------------

#[test]
fn a_matching_report_names_its_conversation() {
    // When
    let named = a_hook("SessionStart", ("source", "clear"))
        .conversation_of("SessionStart", &a_clear_beginning(NEXT));

    // Then
    assert_eq!(named.as_deref(), Some(NEXT));
}

#[test]
fn a_hook_with_no_condition_matches_every_report_of_its_event() {
    // Given
    let hook = RotationHook {
        when: BTreeMap::new(),
        ..a_hook("SessionStart", ("source", "clear"))
    };

    // When
    let named = hook.conversation_of("SessionStart", &fields(&[("session_id", NEXT)]));

    // Then
    assert_eq!(named.as_deref(), Some(NEXT));
}

#[test]
fn a_report_of_another_event_names_nothing() {
    // When
    let named = a_hook("SessionStart", ("source", "clear"))
        .conversation_of("Stop", &a_clear_beginning(NEXT));

    // Then
    assert_eq!(named, None);
}

#[test]
fn a_report_with_a_blank_id_names_nothing() {
    // When
    let named = a_hook("SessionStart", ("source", "clear"))
        .conversation_of("SessionStart", &a_clear_beginning("  "));

    // Then
    assert_eq!(named, None);
}

#[test]
fn only_a_daemon_session_id_conversation_declares_a_rotation() {
    // Given
    let rotating = ConversationSlot::DaemonSessionId {
        rotation: Some(a_clear_rotation()),
    };

    // When
    let declared = (rotating.rotation(), ConversationSlot::Worktree.rotation());

    // Then
    assert_eq!(declared, (Some(&a_clear_rotation()), None));
}

#[test]
fn a_rotation_is_reported_by_its_two_hooks() {
    // Given
    let rotation = a_clear_rotation();

    // When
    let hooks = rotation.hooks();

    // Then
    assert_eq!(hooks, ["SessionEnd", "SessionStart"]);
}
