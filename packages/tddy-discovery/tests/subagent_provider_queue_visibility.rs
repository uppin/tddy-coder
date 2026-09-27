//! Integration tests: a turn waiting for its provider says so in the receipt its caller reads.
//!
//! The enforcement is pinned in `provider_queue.rs`; this is the half the *calling agent*
//! sees. Both halves are needed and neither substitutes for the other: a queue nobody can observe
//! reproduces session 01a0e200, where Gemma's turn was stuck behind FastContext's runaway for 38
//! minutes and every number the main agent could read said it was fine.
//!
//! It said so because the only queue in the model is per **conversation**, and Gemma's
//! conversation had exactly one turn on it. `queuePosition: 0`, `queueSize: 1` — accurate, and
//! completely misleading. `PendingTurns::queue_size` even states the assumption in its own doc:
//!
//! > Per conversation, because a conversation is what a turn queues on: turns on two
//! > conversations hold different session mutexes and wait for nothing of each other's.
//!
//! The first clause is true and stays true. The second is what 01a0e200 falsified: the two turns
//! held different session mutexes and one waited 38 minutes for the other, because what they
//! contended on was the provider, which the model does not represent. So the conversation queue is
//! not wrong — it is one dimension short, and these tests add the missing one beside it.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md (this behaviour has no section there yet)

use tddy_discovery::subagent_runtime::{pending_turn_json, PendingTurns};

const OLLAMA: &str = "http://127.0.0.1:11434";
const ANOTHER_PROVIDER: &str = "https://api.example.invalid";

/// The two halves of the incident, named as they were: one runaway turn and one that never ran.
const THE_RUNAWAY: &str = "response-fastcontext";
const THE_STARVED: &str = "response-gemma";

/// A task that will never finish on its own — the abort handle a registered turn is held by, with
/// nothing else about a real turn to get in the way. Mirrors the helper the in-module tests use.
fn a_turn_still_running() -> tokio::task::AbortHandle {
    tokio::spawn(std::future::pending::<()>()).abort_handle()
}

/// The two conversations of session 01a0e200: different conversations, one shared provider.
fn the_incidents_two_turns() -> PendingTurns {
    let mut pending = PendingTurns::default();
    pending.start(
        THE_RUNAWAY,
        "conv-fastcontext",
        OLLAMA,
        a_turn_still_running(),
    );
    pending.start(THE_STARVED, "conv-gemma", OLLAMA, a_turn_still_running());
    pending
}

fn a_receipt_for(pending: &PendingTurns, response_id: &str) -> serde_json::Value {
    serde_json::from_str(&pending_turn_json(pending, response_id))
        .expect("a deferral must serialize to JSON")
}

// ─── Main functionality ──────────────────────────────────────────────────────

/// The control: a turn with the provider to itself is not waiting on anybody.
#[tokio::test]
async fn a_turn_alone_on_its_provider_reports_nothing_ahead_of_it() {
    // Given one turn, on an otherwise idle provider
    let mut pending = PendingTurns::default();
    pending.start(
        THE_RUNAWAY,
        "conv-fastcontext",
        OLLAMA,
        a_turn_still_running(),
    );

    // Then it is at the front of that provider's line
    assert_eq!(pending.provider_queue_position(THE_RUNAWAY), Some(0));
    assert_eq!(pending.provider_queue_size(OLLAMA), 1);
}

/// The Gemma case, in the smallest form that shows it.
#[tokio::test]
async fn a_turn_accepted_behind_another_on_the_same_provider_reports_one_ahead() {
    // Given the incident's two turns, on two conversations and one provider
    let pending = the_incidents_two_turns();

    // Then the second is told it is behind the first
    assert_eq!(pending.provider_queue_position(THE_STARVED), Some(1));
    assert_eq!(pending.provider_queue_size(OLLAMA), 2);
}

/// The conversation dimension keeps its old meaning, so the two numbers are not the same number
/// under two names. This is the test that would have caught the incident: the conversation queue
/// says "nothing ahead of you" and the provider queue says "one ahead", and both are true.
#[tokio::test]
async fn a_turn_at_the_front_of_its_conversation_can_still_be_behind_on_its_provider() {
    // Given the incident's two turns
    let pending = the_incidents_two_turns();

    // Then the starved turn is first on its own conversation and second on the provider
    assert_eq!(
        pending.queue_position(THE_STARVED),
        Some(0),
        "its conversation really is idle — this number was never wrong, only incomplete"
    );
    assert_eq!(
        pending.provider_queue_position(THE_STARVED),
        Some(1),
        "and this is the one that would have said it was not running"
    );
}

/// Two endpoints are two resources. Counting them together would report a wait that does not
/// exist, which is as damaging as missing one that does.
#[tokio::test]
async fn turns_on_different_providers_do_not_count_against_each_other() {
    // Given two turns on two different providers
    let mut pending = PendingTurns::default();
    pending.start(
        THE_RUNAWAY,
        "conv-fastcontext",
        OLLAMA,
        a_turn_still_running(),
    );
    pending.start(
        THE_STARVED,
        "conv-gemma",
        ANOTHER_PROVIDER,
        a_turn_still_running(),
    );

    // Then neither is behind the other
    assert_eq!(pending.provider_queue_position(THE_STARVED), Some(0));
    assert_eq!(pending.provider_queue_size(OLLAMA), 1);
    assert_eq!(pending.provider_queue_size(ANOTHER_PROVIDER), 1);
}

// ─── Edge cases ──────────────────────────────────────────────────────────────

/// A caller polling asks "how much longer", so the answer has to be current rather than the one it
/// queued at — the same contract the conversation queue already keeps.
#[tokio::test]
async fn the_provider_queue_drains_as_the_turn_in_front_finishes() {
    // Given a turn waiting behind the runaway
    let mut pending = the_incidents_two_turns();

    // When the one in front ends
    pending.resolve(THE_RUNAWAY, r#"{"stopReason":"end_turn"}"#.to_string());

    // Then the one behind moves up
    assert_eq!(pending.provider_queue_position(THE_STARVED), Some(0));
    assert_eq!(pending.provider_queue_size(OLLAMA), 1);
}

/// An unknown id is unknown. `Some(0)` would read as "about to run" — the reading that made a
/// 38-minute wait look like work in progress.
#[tokio::test]
async fn a_turn_nobody_registered_has_no_provider_position() {
    // Given an empty registry
    let pending = PendingTurns::default();

    // Then an unknown id reports nothing, and an untouched provider is empty
    assert_eq!(pending.provider_queue_position("never-started"), None);
    assert_eq!(pending.provider_queue_size(OLLAMA), 0);
}

// ─── The receipt the calling agent reads ─────────────────────────────────────

/// A caller holding a `responseId` and nothing else has to be able to see which provider it is
/// queued on — otherwise "behind one" names no resource and suggests no action.
#[tokio::test]
async fn the_pending_receipt_names_the_provider_the_turn_is_queued_on() {
    // Given the incident's two turns
    let pending = the_incidents_two_turns();

    // When the starved turn's receipt is read
    let receipt = a_receipt_for(&pending, THE_STARVED);

    // Then it names the endpoint
    assert_eq!(receipt["provider"].as_str(), Some(OLLAMA));
}

/// The receipt is the whole point: this is the JSON `subagent_prompt` and `subagent_await` hand
/// back, and in 01a0e200 every field in it said the turn was fine.
#[tokio::test]
async fn the_pending_receipt_reports_the_provider_queue_beside_the_conversation_queue() {
    // Given the incident's two turns
    let pending = the_incidents_two_turns();

    // When the starved turn's receipt is read
    let receipt = a_receipt_for(&pending, THE_STARVED);

    // Then both dimensions are reported, and they disagree
    assert_eq!(receipt["queuePosition"].as_u64(), Some(0));
    assert_eq!(receipt["providerQueuePosition"].as_u64(), Some(1));
    assert_eq!(receipt["providerQueueSize"].as_u64(), Some(2));
}

/// A turn nothing is stored for must report absence, not zero — in the receipt as much as in the
/// table, because the receipt is where a caller actually looks.
#[tokio::test]
async fn a_receipt_for_an_unregistered_turn_reports_no_provider_queue_rather_than_an_empty_one() {
    // Given an empty registry
    let pending = PendingTurns::default();

    // When a receipt is built for an id it never saw
    let receipt = a_receipt_for(&pending, "never-started");

    // Then the provider fields are absent rather than zeroed
    assert!(
        receipt["provider"].is_null()
            && receipt["providerQueuePosition"].is_null()
            && receipt["providerQueueSize"].is_null(),
        "an unknown turn has no queue to report; got: {receipt}"
    );
}
