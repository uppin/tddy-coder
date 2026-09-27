//! Integration tests: one model call at a time per provider endpoint, with the waiting made ours.
//!
//! A local provider is a single-slot resource. Session 01a0e200 showed what happens when nothing
//! models that: FastContext's turn entered a runaway and held Ollama's only runner, and the Gemma
//! turn issued 14 minutes later sat inside Ollama's socket for **38m21s** without executing a
//! token. Ollama did not even evaluate it — `starting mlx runner subprocess model=gemma4:e4b-mlx`
//! is timestamped one second *after* the operator cancelled, and the request then died
//! `499 | 38m21s`, beside FastContext's `500 | 52m9s`.
//!
//! The waiting already existed; it was just invisible and unbounded, because it happened inside
//! someone else's queue. These tests pin an admission gate on *this* side of the socket, where the
//! wait can be counted, reported, bounded and cancelled.
//!
//! Keyed by provider endpoint rather than by model: the contended resource is the server, and two
//! models on one Ollama contend exactly as two calls to one model do. Two *different* endpoints do
//! not contend at all, and a queue that serialised them would invent a delay — which is why that
//! is a test here and not an afterthought.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md (this behaviour has no section there yet)

use std::time::Duration;
use tddy_discovery::subagent_runtime::{ProviderQueue, ProviderQueueError};

/// The endpoint the incident ran on, and a second one standing in for any other provider.
const OLLAMA: &str = "http://127.0.0.1:11434";
const ANOTHER_PROVIDER: &str = "https://api.example.invalid";

/// Long enough that an ungated call would have completed many times over — a wiremock round trip
/// is about a millisecond — so a caller still waiting at the end of it is genuinely gated and not
/// merely unscheduled.
const LONG_ENOUGH_TO_PROVE_IT_IS_WAITING: Duration = Duration::from_millis(150);

/// Generous enough to absorb a loaded CI machine, short enough to stay inside the integration
/// budget. A hand-off that has not happened by now is not slow, it is broken.
const LONG_ENOUGH_FOR_A_HANDOFF: Duration = Duration::from_millis(500);

/// A wait budget small enough to expire inside a test without waiting on a real one.
const A_SHORT_WAIT_BUDGET: Duration = Duration::from_millis(100);

const FIRST_CALLER: &str = "response-fastcontext";
const SECOND_CALLER: &str = "response-gemma";

// ─── Main functionality ──────────────────────────────────────────────────────

/// The common case must cost nothing: a provider nobody is using hands the slot straight over.
#[tokio::test]
async fn an_idle_provider_admits_a_caller_immediately() {
    // Given a queue nobody is using
    let queue = ProviderQueue::new();

    // When a caller asks for the slot
    let admitted =
        tokio::time::timeout(LONG_ENOUGH_FOR_A_HANDOFF, queue.admit(OLLAMA, FIRST_CALLER))
            .await
            .expect("an idle provider must not make a caller wait");

    // Then it holds it
    let _held = admitted.expect("an idle provider admits");
    assert_eq!(queue.in_flight(OLLAMA), 1);
}

/// The gate itself. Without it the second call goes to the socket and waits there, uncounted.
#[tokio::test]
async fn a_second_caller_on_the_same_provider_waits_while_the_first_holds_the_slot() {
    // Given a caller holding the provider's only slot
    let queue = ProviderQueue::new();
    let _held = queue
        .admit(OLLAMA, FIRST_CALLER)
        .await
        .expect("the first caller is admitted");

    // When a second caller asks for the same provider
    let second = tokio::time::timeout(
        LONG_ENOUGH_TO_PROVE_IT_IS_WAITING,
        queue.admit(OLLAMA, SECOND_CALLER),
    )
    .await;

    // Then it is still waiting
    assert!(
        second.is_err(),
        "a second call to a single-slot provider must wait here, where the wait can be seen, \
         rather than inside the provider's own socket where it cannot"
    );
}

/// The other half: the gate must open again, or it is a deadlock rather than a queue.
#[tokio::test]
async fn releasing_the_slot_admits_the_caller_that_was_waiting() {
    // Given a caller holding the slot
    let queue = ProviderQueue::new();
    let held = queue
        .admit(OLLAMA, FIRST_CALLER)
        .await
        .expect("the first caller is admitted");

    // When it releases
    drop(held);

    // Then the next caller gets in
    let second = tokio::time::timeout(
        LONG_ENOUGH_FOR_A_HANDOFF,
        queue.admit(OLLAMA, SECOND_CALLER),
    )
    .await
    .expect("a released slot must be handed on");
    second.expect("the waiting caller is admitted");
}

/// The correctness heart of keying by endpoint. Serialising unrelated providers would invent
/// exactly the delay this whole mechanism exists to remove.
#[tokio::test]
async fn callers_on_different_providers_never_wait_for_each_other() {
    // Given a caller holding one provider's slot
    let queue = ProviderQueue::new();
    let _held = queue
        .admit(OLLAMA, FIRST_CALLER)
        .await
        .expect("the first caller is admitted");

    // When a caller asks for a different provider
    let other = tokio::time::timeout(
        LONG_ENOUGH_FOR_A_HANDOFF,
        queue.admit(ANOTHER_PROVIDER, SECOND_CALLER),
    )
    .await
    .expect("a different endpoint is a different resource and must not wait");

    // Then it is admitted straight away
    let _other = other.expect("the other provider is idle");
    assert_eq!(queue.in_flight(ANOTHER_PROVIDER), 1);
}

// ─── Reporting ───────────────────────────────────────────────────────────────

/// Position is what turns "pending" into "pending, behind one". The holder is at the front, and
/// the caller behind it is told so — the number whose absence let Gemma look like it was running.
#[tokio::test]
async fn the_holder_is_at_the_front_and_a_waiter_is_told_how_many_are_ahead() {
    // Given a caller holding the slot and another waiting behind it
    let queue = ProviderQueue::new();
    let _held = queue
        .admit(OLLAMA, FIRST_CALLER)
        .await
        .expect("the first caller is admitted");
    let waiting = tokio::spawn({
        let queue = queue.clone();
        async move { queue.admit(OLLAMA, SECOND_CALLER).await }
    });
    // The spawned caller has to reach the queue before its position can be read, and a queued
    // future offers nothing to subscribe to — the only observable is the count this test is here
    // to check, so waiting on it would be the assertion polling itself. Hence a real sleep: a
    // bounded window for the spawned task to run, on the same budget that proves a gated caller
    // is genuinely waiting.
    tokio::time::sleep(LONG_ENOUGH_TO_PROVE_IT_IS_WAITING).await;

    // When both positions are read
    let holder = queue.position_of(FIRST_CALLER);
    let behind = queue.position_of(SECOND_CALLER);

    // Then the holder is at the front and the waiter is one back
    assert_eq!(holder, Some(0), "the caller holding the slot is the front");
    assert_eq!(behind, Some(1), "the caller behind it is told it is behind");
    waiting.abort();
}

/// A provider nobody has touched reports emptiness rather than a stale count.
#[tokio::test]
async fn a_provider_nobody_has_asked_for_has_nothing_in_flight_and_nobody_waiting() {
    // Given a fresh queue
    let queue = ProviderQueue::new();

    // Then an untouched provider is idle
    assert_eq!(queue.in_flight(OLLAMA), 0);
    assert_eq!(queue.waiting(OLLAMA), 0);
}

/// Releasing must clear the count, or the queue reports congestion that has passed.
#[tokio::test]
async fn a_released_slot_leaves_nothing_in_flight() {
    // Given a caller that held and released the slot
    let queue = ProviderQueue::new();
    let held = queue
        .admit(OLLAMA, FIRST_CALLER)
        .await
        .expect("the first caller is admitted");

    // When it releases
    drop(held);

    // Then the provider is idle again
    assert_eq!(queue.in_flight(OLLAMA), 0);
}

// ─── Error scenarios ─────────────────────────────────────────────────────────

/// 38 minutes of waiting is not patience, it is a missing deadline. A caller that cannot be served
/// must be told so, naming the provider it was queued on.
#[tokio::test]
async fn a_caller_that_waits_past_its_budget_is_refused_naming_the_provider() {
    // Given a queue with a short budget, and a caller holding the slot
    let queue = ProviderQueue::new().with_wait_timeout(A_SHORT_WAIT_BUDGET);
    let _held = queue
        .admit(OLLAMA, FIRST_CALLER)
        .await
        .expect("the first caller is admitted");

    // When a second caller waits it out
    let refused = tokio::time::timeout(
        LONG_ENOUGH_FOR_A_HANDOFF,
        queue.admit(OLLAMA, SECOND_CALLER),
    )
    .await
    .expect("the budget must expire on its own rather than hang")
    .expect_err("a caller past its budget is refused");

    // Then the refusal says where it was waiting
    assert_eq!(
        refused,
        ProviderQueueError::WaitedTooLong {
            provider: OLLAMA.to_string(),
            waited: A_SHORT_WAIT_BUDGET,
        }
    );
}

/// A refused caller must leave the line. Otherwise every expiry inflates the queue the next
/// caller is told it is behind.
#[tokio::test]
async fn a_refused_caller_stops_counting_as_waiting() {
    // Given a caller that waited out its budget
    let queue = ProviderQueue::new().with_wait_timeout(A_SHORT_WAIT_BUDGET);
    let _held = queue
        .admit(OLLAMA, FIRST_CALLER)
        .await
        .expect("the first caller is admitted");
    queue
        .admit(OLLAMA, SECOND_CALLER)
        .await
        .expect_err("the second caller is refused");

    // Then nobody is left queued behind the holder
    assert_eq!(queue.waiting(OLLAMA), 0);
    assert_eq!(queue.position_of(SECOND_CALLER), None);
}

// ─── API boundaries ──────────────────────────────────────────────────────────

/// An unknown caller has no position. `Some(0)` would read as "about to run", which is the reading
/// that made a 38-minute wait look like work in progress.
#[tokio::test]
async fn a_caller_the_queue_never_saw_has_no_position() {
    // Given a queue with one caller in it
    let queue = ProviderQueue::new();
    let _held = queue
        .admit(OLLAMA, FIRST_CALLER)
        .await
        .expect("the first caller is admitted");

    // Then an id it never saw is unknown, not at the front
    assert_eq!(queue.position_of("response-never-queued"), None);
}
