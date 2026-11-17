use std::time::Duration;

/// Cadence at which a `StreamSessionAgents` subscription re-sends the roster it last sent, when
/// nothing has been published in the meantime.
///
/// A roster nobody is changing produces no frames for hours, and a *forwarded* subscription — the
/// split session's case, where the agent runs on one daemon and its roster lives on another — rides
/// a relay that terminates a stream which stops producing
/// ([`tddy_daemon_kernel::peer_forwarding::PEER_FORWARD_STREAM_IDLE_TIMEOUT`]). That deadline is
/// deliberate: a stream that goes quiet must never read as one that ended, or a truncated forward
/// passes for a complete one. So the roster has to keep talking. Re-sending the applied revision is
/// the cheapest frame there is — applying a revision already held is a defined no-op that announces
/// no change — and at this cadence two of them fit inside the deadline, so one lost frame does not
/// end the subscription.
pub const ROSTER_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(8);
