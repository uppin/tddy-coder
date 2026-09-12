//! What an agent is doing, what a session's status is, and the transcript of both.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 7, serving `activity.ActivityService` —
//! families M and N, 8 methods. It publishes into and reads from
//! [`tddy_daemon_kernel::AgentActivityHub`], which is node 1's.
//!
//! # The delta tick numbering changed with this move, deliberately
//!
//! `docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md` records the fix, closing the
//! `docs/dev/todo/` entry that raised it: a session's first activity delta used to be numbered 0 — which is also the wire's "no tick"
//! value, so a consumer could not tell *the first delta* from *no delta yet*.
//!
//! `StreamAgentActivityDelta` moved to a **new proto** in this node, and carrying that ambiguity
//! into a fresh schema would have made it permanent. So [`FIRST_TICK`] is 1 and [`NO_TICK`] is 0.
//!
//! The rule itself lives in `tddy-service` beside `activity.proto` — see
//! [`tddy_service::session_activity::next_tick`] for why — and is re-exported here because this is
//! the crate that stamps the ticks.

pub mod service;
pub mod session_notification_subscribers;
pub mod session_notifications;
pub mod streams;

pub use service::{
    ActivityPorts, ActivityServiceImpl, DeltaLookup, DeltaScope, MeasuredDelta, OsUserResolver,
    SessionDeltaStores, SessionLabels,
};

/// How a session's activity deltas are numbered: the wire's absent value, the first real tick, and
/// the rule that produces the next one.
///
/// Defined in `tddy-service` and re-exported here. The producer of these ticks is this crate and
/// `tddy-daemon-livekit`'s session room; the consumer is `tddy-session-sync`, a standalone shipped
/// binary. Reaching the rule through this crate would put its `livekit` and `tddy-telegram`
/// dependency trees in that binary for three symbols.
pub use tddy_service::session_activity::{next_tick, FIRST_TICK, NO_TICK};
