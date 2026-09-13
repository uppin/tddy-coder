//! The symbols every `tddy-daemon` subsystem reaches into `connection_service` for.
//!
//! `connection_service` was 23,099 lines and is the daemon's RPC entry point, so nothing that
//! depends on it can leave the crate. But the dependency is far narrower than that size suggests:
//! across 106 daemon modules, only **eight files** hold a real code reference to it, and four of
//! those reference a single symbol each. This crate is those symbols, and extracting them is what
//! turns a subsystem move from impossible into mechanical.
//!
//! It exists for a specific reason that is easy to miss: **`pub(crate)` does not cross a crate
//! boundary.** A `docs/dev/todo/` entry (since closed and removed) predicted the
//! cost of a split as *"`DaemonSessionHost`'s ~60 private fields would have to become
//! `pub(crate)` or move behind accessors"* — which is true for a module split and insufficient for a
//! crate split. Widening to `pub(crate)` buys nothing once the consumer is a different crate, so the
//! shared surface has to be a crate of its own.
//!
//! # What is here, and who reached for it
//!
//! | Symbol | Reached by |
//! |---|---|
//! | [`AgentActivityHub`] | the sandbox subsystem (5 sites), `session_agent_inference.rs` |
//! | [`now_unix_ms`] | the sandbox subsystem (2 sites), `telegram_session_subscriber.rs` |
//! | [`HOST_DOCUMENT_FRAME_BYTES`] | `context_files.rs` |
//! | [`SessionUserResolver`], [`SessionsBaseResolver`] | `auth.rs`, and five further subsystems |
//! | [`trim_to_option`] | duplicated inside `connection_service` — see below |
//!
//! # This crate de-duplicates as well as relocates
//!
//! Three of these symbols already existed more than once, and not identically:
//!
//! - **`now_unix_ms` existed three times with three different overflow behaviours** —
//!   `session_agent_status.rs` saturated at `u64::MAX`, `host_registry.rs` returned `i64` and
//!   documented an explicit refusal for a pre-1970 clock (*"a clock before 1970 stamps every row
//!   '20000 days ago', which reads as data loss rather than as the misconfigured clock it is"*), and
//!   `connection_service/host_messages.rs` used a bare `as u64` that **truncates** rather than
//!   saturating. Consolidating them is a behaviour decision, not a move, and it is made here once:
//!   see [`now_unix_ms`].
//! - **`SessionUserResolver` was defined twice**, in `task_service.rs` and in
//!   `connection_service/service_util.rs`, with identical shapes. One definition survives.
//! - **`trim_to_option` was duplicated inside `connection_service` itself**, recorded in
//!   `docs/dev/todo/2026-08-13-tddy-daemon-connection-service-rs-repeats-a-trim-to-option-string-bloc.md`.
//!   Every subsystem that carries handlers out would otherwise copy it again, once per crate.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// What `#unbundle` node 1 added, and the granularity rule it follows.
//
// Everything below is a **symbol** lift, not a module move — the transitive closure a moving
// subsystem actually calls, and nothing else. The measurements that set each boundary:
// `host_tooling`, `ssh_agent` and `host_private_key` reach 3 entry points of `spawner.rs`, whose
// closure is 179 of its 2,539 lines; `remote_git_service` reaches 5 symbols of `pty_runtime.rs`,
// 63 of 400; four modules reach 4 resolvers of `user_sessions_path.rs`, 34 of 210. The origin
// module re-exports every name in each case, so no caller in `tddy-daemon` changed and there is
// exactly one definition of each.
//
// [`config`] is the deliberate exception and the only whole-module move here: `DaemonConfig` is a
// single ~100-field struct that four moving modules and every handler in both new services take by
// reference and read disjointly, so the symbol *is* the file. See the changeset's
// `## Decisions & Trade-offs`.
pub mod config;
pub mod daemon_identity;
pub mod peer_forwarding;
pub mod privilege_drop;
pub mod spawn_as_user;
pub mod user_paths;

/// Resolve a session token to the OS user that owns it.
///
/// The daemon builds exactly one of these — `auth::build_auth_entries` returns it — and every
/// service authenticates with a clone. It is an alias rather than a trait because the daemon's
/// wiring layer supplies a closure over the configured user mapping, and a trait would need a type
/// per closure.
pub type SessionUserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Resolve a session token to the base directory that user's sessions live under.
pub type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

/// Frame size for streamed host documents.
///
/// A `ReadHostDocument` response and a `StreamReadHostDocument` chunk are framed identically, so a
/// consumer that switches between them sees the same boundaries.
pub const HOST_DOCUMENT_FRAME_BYTES: usize = 48 * 1024;

/// Milliseconds since the Unix epoch, saturating rather than wrapping.
///
/// **This function resolves a real inconsistency rather than relocating one.** Of the three
/// implementations it replaces, one saturated at `u64::MAX`, one returned `i64` with an explicit
/// pre-1970 refusal, and one used a bare `as u64` cast that truncates. A truncating cast is the
/// wrong behaviour for a timestamp: it turns a clock far in the future into a timestamp in the past,
/// silently, which is indistinguishable from correct data downstream.
///
/// Saturation is chosen over the `i64` refusal because every caller here stamps a record it is
/// about to write, and a caller that cannot proceed without a plausible clock is better served by
/// checking the clock than by receiving an error from a timestamp function. Callers that need the
/// pre-1970 diagnostic keep their own check.
#[must_use]
pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(unix_ms_of)
        .unwrap_or_default()
}

/// The conversion [`now_unix_ms`] is built on: milliseconds since the epoch, saturated at
/// `u64::MAX`.
///
/// Split out from the clock read because it is the half that carries the behaviour decision, and a
/// function that reads `SystemTime::now()` cannot be handed a duration that overflows — a test
/// against the wall clock would pass just as happily against the truncating cast this replaces.
/// `Duration` counts seconds in a `u64`, so its millisecond count genuinely does not fit in one.
fn unix_ms_of(since_epoch: Duration) -> u64 {
    u64::try_from(since_epoch.as_millis()).unwrap_or(u64::MAX)
}

/// A trimmed string, or `None` when it was blank.
///
/// Duplicated inside `connection_service` and recorded in `docs/dev/todo/`. Every subsystem that
/// carries handlers out of that file needs it, so it lands here once instead of once per crate.
#[must_use]
pub fn trim_to_option(value: &str) -> Option<String> {
    let trimmed = value.trim();
    match trimmed.is_empty() {
        true => None,
        false => Some(trimmed.to_string()),
    }
}

/// Per-session live broadcast of agent activity, plus the stack of in-flight tool calls awaiting
/// their terminal row.
///
/// The sandbox subsystem holds an `Arc` of this and publishes into it; the activity stream handlers
/// subscribe. That is the whole reason it cannot stay inside `connection_service`: a sandbox crate
/// that had to depend on the daemon's RPC entry point to publish an activity record would defeat the
/// extraction entirely.
///
/// Fields are private with accessors, deliberately. They were `pub(crate)` inside the daemon, and
/// `pub(crate)` does not cross a crate boundary — so the choice was accessors or `pub` fields, and
/// the two `StdMutex`es guard an invariant (a pending `call_id` is pushed before its terminal row is
/// popped) that public fields would let a consumer break.
#[derive(Debug, Default)]
pub struct AgentActivityHub {
    /// Per-session live broadcast; the sender is created lazily on first subscribe.
    senders: StdMutex<
        HashMap<
            String,
            tokio::sync::broadcast::Sender<tddy_core::agent_activity::AgentActivityRecord>,
        >,
    >,
    /// Per-session stack of in-flight `call_id`s awaiting their terminal row.
    pending: StdMutex<HashMap<String, Vec<String>>>,
}

impl AgentActivityHub {
    /// Broadcast capacity per session.
    ///
    /// Sized so a burst of tool calls between a slow subscriber's polls rarely forces a `Lagged`;
    /// the relay tolerates `Lagged` regardless.
    pub const CAPACITY: usize = 256;

    /// Subscribe to a session's activity, creating the sender on first use.
    pub fn subscribe(
        &self,
        session_id: &str,
    ) -> tokio::sync::broadcast::Receiver<tddy_core::agent_activity::AgentActivityRecord> {
        self.senders
            .lock()
            .expect("agent activity hub mutex poisoned")
            .entry(session_id.to_string())
            .or_insert_with(|| tokio::sync::broadcast::channel(Self::CAPACITY).0)
            .subscribe()
    }

    /// Publish a record to a session's subscribers.
    ///
    /// A no-op when nobody has subscribed to the session: the durable `agent-activity.jsonl` log is
    /// the source of truth and the hub only accelerates live delivery. Minting a sender here instead
    /// would retain up to [`CAPACITY`](Self::CAPACITY) records in the broadcast ring for a session
    /// no reader will ever attach to.
    pub fn publish(
        &self,
        session_id: &str,
        record: tddy_core::agent_activity::AgentActivityRecord,
    ) {
        let sender = self
            .senders
            .lock()
            .expect("agent activity hub mutex poisoned")
            .get(session_id)
            .cloned();
        if let Some(sender) = sender {
            // `Err` means no live receivers; the durable log still holds the record.
            let _ = sender.send(record);
        }
    }

    /// Record a `call_id` as in flight, awaiting its terminal row.
    pub fn push_pending(&self, session_id: &str, call_id: &str) {
        self.pending
            .lock()
            .expect("agent activity hub mutex poisoned")
            .entry(session_id.to_string())
            .or_default()
            .push(call_id.to_string());
    }

    /// Take the most recent in-flight `call_id` for a session, if any.
    pub fn pop_pending(&self, session_id: &str) -> Option<String> {
        self.pending
            .lock()
            .expect("agent activity hub mutex poisoned")
            .get_mut(session_id)
            .and_then(|stack| stack.pop())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// That the epoch and the unit are right: a stamp taken now has to land in this century. This
    /// says nothing about overflow — see [`saturates_a_millisecond_count_too_large_for_a_u64`],
    /// which is where that decision is actually pinned.
    #[test]
    fn stamps_a_plausible_current_time() {
        // Given a clock that is roughly now
        // When
        let stamped = now_unix_ms();

        // Then — after 2020-01-01 and before 2100-01-01, which is as tight as a wall clock allows
        assert!(
            stamped > 1_577_836_800_000,
            "stamp {stamped} is before 2020"
        );
        assert!(stamped < 4_102_444_800_000, "stamp {stamped} is after 2100");
    }

    /// A smoke test on the clock read, and nothing more: two reads nanoseconds apart would come
    /// back ordered from a truncating implementation too. The overflow behaviour is the
    /// conversion's, and it is asserted on the conversion.
    #[test]
    fn stamps_do_not_go_backwards() {
        // Given
        let first = now_unix_ms();

        // When
        let second = now_unix_ms();

        // Then
        assert!(second >= first, "{second} came back before {first}");
    }

    /// The behaviour decision this crate records. Of the three implementations consolidated here
    /// one used a bare `as u64`, which **wraps**: a clock far in the future comes back as a
    /// timestamp in the past, and downstream that is indistinguishable from correct data. The
    /// ceiling has to be reached and stayed at.
    #[test]
    fn saturates_a_millisecond_count_too_large_for_a_u64() {
        // Given a duration whose millisecond count overflows `u64` — `Duration` counts seconds in
        // a `u64`, so this is representable as a duration and not as a stamp
        let far_future = Duration::from_secs(u64::MAX);

        // When
        let stamped = unix_ms_of(far_future);

        // Then — the ceiling, not the wrapped remainder a truncating cast would produce
        assert_eq!(
            stamped,
            u64::MAX,
            "{} ms saturates to the ceiling; a truncating cast would have wrapped it to {}",
            far_future.as_millis(),
            far_future.as_millis() as u64
        );
    }

    /// Saturation must not start early: the largest stamp a `u64` can hold is a real timestamp, not
    /// an overflow, and it comes back exactly.
    #[test]
    fn converts_the_largest_representable_millisecond_count_exactly() {
        // Given
        let at_the_ceiling = Duration::from_millis(u64::MAX);

        // When
        let stamped = unix_ms_of(at_the_ceiling);

        // Then
        assert_eq!(stamped, u64::MAX);
    }

    /// The ordinary case the clock actually hands it, so the ceiling logic cannot be hiding a
    /// constant.
    #[test]
    fn converts_a_duration_the_clock_plausibly_yields_to_its_milliseconds() {
        // Given 2020-01-01T00:00:00Z as the clock would report it
        let since_epoch = Duration::from_secs(1_577_836_800) + Duration::from_millis(250);

        // When
        let stamped = unix_ms_of(since_epoch);

        // Then
        assert_eq!(stamped, 1_577_836_800_250);
    }

    #[test]
    fn reads_a_blank_string_as_absent() {
        assert_eq!(trim_to_option("   "), None);
        assert_eq!(trim_to_option(""), None);
    }

    #[test]
    fn trims_a_value_that_has_content() {
        assert_eq!(trim_to_option("  main  "), Some("main".to_string()));
    }

    /// The sandbox subsystem publishes into this hub and the activity handlers subscribe. That is
    /// the whole reason it cannot stay inside `connection_service`: a sandbox crate that had to
    /// depend on the daemon's RPC entry point to publish a record would defeat the extraction.
    #[test]
    fn delivers_a_published_record_to_a_subscriber_of_the_same_session() {
        // Given
        let hub = AgentActivityHub::default();
        let mut listener = hub.subscribe("session-a");

        // When
        hub.publish("session-a", a_record("Read"));

        // Then
        let seen = listener.try_recv().expect("the subscriber sees the record");
        assert_eq!(seen.tool_name, "Read");
    }

    #[test]
    fn does_not_deliver_a_record_to_a_subscriber_of_a_different_session() {
        // Given
        let hub = AgentActivityHub::default();
        let mut other = hub.subscribe("session-b");

        // When
        hub.publish("session-a", a_record("Read"));

        // Then
        assert!(
            other.try_recv().is_err(),
            "a session's activity must not leak to another session's subscriber"
        );
    }

    /// A pending `call_id` is pushed before its terminal row is popped, and the pair is per session.
    /// This is the invariant the private fields exist to protect — public fields would let a
    /// consumer pop a session's stack from under another.
    #[test]
    fn returns_the_most_recent_pending_call_for_a_session() {
        // Given
        let hub = AgentActivityHub::default();
        hub.push_pending("session-a", "call-1");
        hub.push_pending("session-a", "call-2");

        // When
        let popped = hub.pop_pending("session-a");

        // Then
        assert_eq!(popped.as_deref(), Some("call-2"));
    }

    #[test]
    fn has_no_pending_call_for_a_session_nothing_was_pushed_for() {
        // Given
        let hub = AgentActivityHub::default();
        hub.push_pending("session-a", "call-1");

        // When
        let popped = hub.pop_pending("session-b");

        // Then
        assert_eq!(popped, None);
    }

    /// Frame sizes are a wire contract: a `ReadHostDocument` response and a
    /// `StreamReadHostDocument` chunk are framed identically, so a consumer that switches between
    /// them sees the same boundaries. Pinning the number is what stops a well-meaning tidy-up from
    /// resegmenting a stream a client is already parsing.
    #[test]
    fn frames_host_documents_at_48_kib() {
        assert_eq!(HOST_DOCUMENT_FRAME_BYTES, 48 * 1024);
    }

    /// A record with only the field each test asserts on set to something meaningful. Every other
    /// field is at its zero value, which is what `AgentActivityRecord` means by "not yet known".
    fn a_record(tool: &str) -> tddy_core::agent_activity::AgentActivityRecord {
        tddy_core::agent_activity::AgentActivityRecord {
            call_id: format!("call-for-{tool}"),
            tool_name: tool.to_string(),
            input: serde_json::Value::Null,
            status: tddy_core::agent_activity::STATUS_RUNNING.to_string(),
            result: serde_json::Value::Null,
            error_message: String::new(),
            started_unix_ms: 0,
            completed_unix_ms: 0,
            source: "sandbox".to_string(),
            head_commit: String::new(),
            activity_seq: 0,
            changed_paths: Vec::new(),
        }
    }
}
