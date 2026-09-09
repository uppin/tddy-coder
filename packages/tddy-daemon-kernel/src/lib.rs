//! The symbols every `tddy-daemon` subsystem reaches into `connection_service` for.
//!
//! `connection_service` was 23,099 lines and is the daemon's RPC entry point, so nothing that
//! depends on it can leave the crate. But the dependency is far narrower than that size suggests:
//! across 106 daemon modules, only **eight files** hold a real code reference to it, and four of
//! those reference a single symbol each. This crate is those symbols, and extracting them is what
//! turns a subsystem move from impossible into mechanical.
//!
//! It exists for a specific reason that is easy to miss: **`pub(crate)` does not cross a crate
//! boundary.** `docs/dev/todo/2026-08-29-connection-service-rs-is-19-600-lines.md` predicted the
//! cost of a split as *"`ConnectionServiceImpl`'s ~60 private fields would have to become
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

use std::path::PathBuf;
use std::sync::Arc;

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
pub fn now_unix_ms() -> u64 {
    // TODO(host-worktree-services): implement
    unimplemented!("tddy-daemon-kernel::now_unix_ms")
}

/// A trimmed string, or `None` when it was blank.
///
/// Duplicated inside `connection_service` and recorded in `docs/dev/todo/`. Every subsystem that
/// carries handlers out of that file needs it, so it lands here once instead of once per crate.
pub fn trim_to_option(_value: &str) -> Option<String> {
    // TODO(host-worktree-services): implement
    unimplemented!("tddy-daemon-kernel::trim_to_option")
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
    // TODO(host-worktree-services): implement — senders and pending, both behind a std Mutex
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
        _session_id: &str,
    ) -> tokio::sync::broadcast::Receiver<tddy_core::agent_activity::AgentActivityRecord> {
        // TODO(host-worktree-services): implement
        unimplemented!("AgentActivityHub::subscribe")
    }

    /// Publish a record to a session's subscribers, creating the sender on first use.
    pub fn publish(
        &self,
        _session_id: &str,
        _record: tddy_core::agent_activity::AgentActivityRecord,
    ) {
        // TODO(host-worktree-services): implement
        unimplemented!("AgentActivityHub::publish")
    }

    /// Record a `call_id` as in flight, awaiting its terminal row.
    pub fn push_pending(&self, _session_id: &str, _call_id: &str) {
        // TODO(host-worktree-services): implement
        unimplemented!("AgentActivityHub::push_pending")
    }

    /// Take the most recent in-flight `call_id` for a session, if any.
    pub fn pop_pending(&self, _session_id: &str) -> Option<String> {
        // TODO(host-worktree-services): implement
        unimplemented!("AgentActivityHub::pop_pending")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three implementations this replaces disagreed about overflow, and one of them truncated.
    /// A truncating cast turns a far-future clock into a timestamp in the *past*, which downstream
    /// is indistinguishable from correct data — so the stamp has to be monotonic in the input right
    /// up to the ceiling.
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

    #[test]
    fn stamps_do_not_go_backwards() {
        // Given
        let first = now_unix_ms();

        // When
        let second = now_unix_ms();

        // Then
        assert!(second >= first, "{second} came back before {first}");
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
