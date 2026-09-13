//! The `session.activity` broadcast — the agent's own tool calls, published into the session room.
//!
//! Product contract: `docs/ft/daemon/session-worktree-sync.md`.
//!
//! Beside [`crate::worktree_activity`], and for the same reason that module gives: publisher and
//! receiver live in different crates, so a topic each of them spelled for itself would fail as
//! *silence* — every receiver filters by topic, and a mismatch delivers nothing and reports
//! nothing.
//!
//! The payload is [`crate::proto::activity::AgentActivityRecord`], the same message
//! `StreamSessionActivity` returns. One schema for the record however it is delivered: a broadcast
//! that drifted from the stream would give two participants two different accounts of one call.

/// The data-channel topic agent activity is broadcast on inside a session room.
///
/// Deliberately not `tddy-rpc`: every RPC receiver in the system hard-filters on that topic, so a
/// record published there would be dropped by some peers and mistaken for a request by others.
/// Deliberately not [`crate::worktree_activity::WORKTREE_ACTIVITY_TOPIC`] either — the two carry
/// different schemas, and a receiver that wants only commits should not have to decode every tool
/// call to discover that.
pub const SESSION_ACTIVITY_TOPIC: &str = "session.activity";

/// The coordinate `activity.ActivityService` is served at, named once.
///
/// Here for the reason [`SESSION_ACTIVITY_TOPIC`] is here: the parties that have to agree on it
/// live in different crates — the `ServiceEntry` the daemon mounts, the peer a routed call is
/// forwarded to, `tddy-session-sync`'s delta subscriber, the agent-clone mirror, and
/// `tddy-coder`'s session participant. A name each of them spelled for itself fails as
/// `not_found` at runtime, one caller at a time.
pub const ACTIVITY_SERVICE: &str = "activity.ActivityService";

/// The wire's "no tick yet" value.
pub const NO_TICK: u64 = 0;

/// The first tick a session's deltas are numbered with.
///
/// 1, not 0, so that [`NO_TICK`] means only what it says.
///
/// `docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md` records the defect this
/// fixes, closing the `docs/dev/todo/` entry that raised it: a session's first activity delta used to be numbered 0 — which is
/// also the wire's "no tick" value — so a consumer could not tell *the first delta* from *no delta
/// yet*. `StreamAgentActivityDelta` moved to a new proto with `#unbundle` node 7, and carrying that
/// ambiguity into a fresh schema would have made it permanent.
pub const FIRST_TICK: u64 = 1;

/// Compiler-enforced: the first delta must not share the wire's absent value. If these two ever
/// converge the ambiguity above is back, and no runtime test is needed to say so.
const _: () = assert!(FIRST_TICK != NO_TICK);

/// The tick to stamp a session's next delta with, given the last one stamped.
///
/// `None` — no delta has been stamped for this session yet — is [`FIRST_TICK`], not [`NO_TICK`].
///
/// Saturating rather than wrapping at the top of the range, for the same reason: a wrap would land
/// the next tick on [`NO_TICK`] and reintroduce the ambiguity this function exists to remove. A
/// session would have to produce 2^64 deltas to reach it, and two deltas sharing the last tick is
/// a smaller lie than one claiming it does not exist.
///
/// Here rather than in `tddy-session-activity`, which serves the deltas, for the reason
/// [`ACTIVITY_SERVICE`] is here — and with a sharper cost. The producer
/// (`tddy-daemon-livekit`'s session room) and the consumer (`tddy-session-sync`) sit at opposite
/// ends of one wire and must agree on which number means absence. Reaching the rule through
/// `tddy-session-activity` made both of them link that crate — and through it `tddy-telegram`,
/// `teloxide` and a rustls stack — for three symbols totalling five lines.
/// `tddy-session-sync` is a shipped standalone binary; a WebRTC-and-bot-framework dependency tree
/// is not the price of a `const u64`. `tddy-service` is already a dependency of all four parties
/// and owns `activity.proto`, the file that declares the field these number.
#[must_use]
pub fn next_tick(last: Option<u64>) -> u64 {
    match last {
        None => FIRST_TICK,
        Some(last) => last.saturating_add(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_each_later_delta_after_the_one_before() {
        // Given
        let first = next_tick(None);

        // When
        let second = next_tick(Some(first));

        // Then
        assert_eq!(second, first + 1);
    }
}
