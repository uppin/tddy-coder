//! Following the roster: one subscription, reconnected forever, and the pacing that decides when a
//! reconnect loop stops counting as service.
//!
//! Everything here is about *how it fails*. The happy path is one `StreamSessionAgents` call whose
//! frames are applied to the registry; what needs the code is a pass that opens and never delivers,
//! one that delivers and dies immediately, and a relay that tears the stream down on its own idle
//! deadline — each of which must reach a different conclusion about whether this process can still
//! claim to know the roster. See docs/ft/daemon/session-agent-roster.md § The roster stream.

use std::time::{Duration, Instant};

use prost::Message;
use tddy_service::proto::connection::{SessionAgentRoster, StreamSessionAgentsRequest};

use crate::session_tool_client::{
    detect_session_tool_transport, SessionToolEnvelope, SessionToolTransport,
};

use super::registry::LiveAgentRoster;
use super::seed::session_agent_roster;

/// How long opening the stream may take before the connection is treated as dead.
const STREAM_OPEN_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to wait for the snapshot every subscribe begins with.
///
/// `StreamSessionAgents` emits the current roster immediately, so a subscribe that produces nothing
/// in this window is a connection nobody is serving. It is the only deadline the stream carries:
/// once a snapshot has arrived, silence is what a roster nobody is changing looks like, and an idle
/// deadline would tear down a perfectly good subscription every few minutes.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(15);

/// The delay before the first reconnect, doubled per consecutive failure.
const RECONNECT_BACKOFF_START: Duration = Duration::from_millis(500);

/// The longest the reconnect loop waits between attempts. It never stops trying: an unavailable
/// roster is a state, not a terminal one, and a daemon that comes back must restore service without
/// the session being restarted.
const RECONNECT_BACKOFF_CEILING: Duration = Duration::from_secs(30);

/// How long a pass must last to count as service rather than churn.
///
/// This is what the backoff escalates on, and it is deliberately not the same question as the
/// unavailability budget below. A stream that delivers its opening snapshot and dies immediately,
/// every time, *is* being served — so it must not declare the roster unavailable — but it is still a
/// hot reconnect loop, and every attempt opens a fresh subscription on the daemon. A pass that ran
/// longer than this was a working subscription that something ended; anything shorter is churn.
///
/// Comfortably longer than the two deadlines a fruitless pass can burn ([`STREAM_OPEN_TIMEOUT`],
/// [`FIRST_FRAME_TIMEOUT`]), so a pass that spent its life waiting never reads as service.
///
/// Public because it is also bounded from above, by a constant in another crate: a relay tears a
/// forwarded stream down after its own idle deadline, and such a teardown must read as service —
/// a keepalive path that goes quiet costs one reconnect per deadline, which is service, whereas
/// classifying it as churn would park a working cross-host subscription at the ceiling below. The
/// relation is asserted where both constants are visible, in `tddy-daemon`'s
/// `livekit_peer_discovery`.
pub const PASS_LONG_ENOUGH_TO_BE_SERVICE: Duration = Duration::from_secs(30);

/// Consecutive failures before the roster is declared unavailable and subagent calls are refused.
///
/// More than one, because a daemon restart drops the stream and is recovered from in well under a
/// second; few, because every attempt after the first is time the registry spends possibly
/// disagreeing with the daemon.
const RECONNECT_ATTEMPTS_BEFORE_GIVING_UP: u32 = 3;

/// What one pass of the roster stream achieved, and how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterStreamOutcome {
    /// Snapshots applied before the pass ended — keepalives, which re-deliver the applied revision,
    /// included.
    applied: u64,
    /// The error the pass ended with, when it did not end with the far end closing cleanly.
    failure: Option<String>,
}

impl RosterStreamOutcome {
    /// A pass the far end closed after `applied` snapshots.
    pub fn closed(applied: u64) -> Self {
        Self {
            applied,
            failure: None,
        }
    }

    /// A pass that ended in an error after `applied` snapshots.
    pub fn broke(applied: u64, failure: String) -> Self {
        Self {
            applied,
            failure: Some(failure),
        }
    }

    /// Snapshots applied before the pass ended.
    pub fn applied(&self) -> u64 {
        self.applied
    }

    /// Whether this pass counts against [`RECONNECT_ATTEMPTS_BEFORE_GIVING_UP`].
    ///
    /// A pass that applied a snapshot proved the subscription was being served, so however it ended
    /// it is a reconnect — a daemon restart, a relay giving up on a stream that went quiet — and not
    /// the broken setup the budget exists to declare. Only a pass that produced nothing counts, and
    /// how it ended makes no difference to that: counting an error against the budget would let
    /// three passes that each delivered a good roster add up to a registry that refuses every
    /// subagent call.
    pub fn counts_as_a_failure(&self) -> bool {
        self.applied == 0
    }

    /// Why the pass ended, as the refusal spells it.
    pub fn reason(&self) -> String {
        match &self.failure {
            Some(failure) => failure.clone(),
            None if self.applied == 0 => "the stream ended before any snapshot arrived".to_string(),
            None => "the stream ended".to_string(),
        }
    }
}

/// How the reconnect loop paces itself: two independent questions that one counter used to answer.
///
/// **Is the roster unavailable?** Only when nothing is being served — a pass that applied a snapshot
/// proves the subscription worked, so however it ended it is a reconnect and not the broken setup
/// the budget exists to declare.
///
/// **How long before trying again?** That is about the *rate* of reconnects, not about whether they
/// were served. Answering it from the same predicate pinned the delay at
/// [`RECONNECT_BACKOFF_START`] for any failure mode that reliably delivers a frame first — which,
/// since the daemon sends the current roster on subscribe and keeps it alive, is every failure mode
/// that gets as far as a subscription.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ReconnectPacing {
    unserved: u32,
    churn: u32,
}

impl ReconnectPacing {
    /// Record a pass that ran for `lasted`, and report how long to wait before reconnecting.
    pub fn record(&mut self, pass: &RosterStreamOutcome, lasted: Duration) -> Duration {
        if pass.counts_as_a_failure() {
            self.unserved = self.unserved.saturating_add(1);
        } else {
            self.unserved = 0;
        }
        if lasted >= PASS_LONG_ENOUGH_TO_BE_SERVICE {
            self.churn = 0;
        } else {
            self.churn = self.churn.saturating_add(1);
        }
        reconnect_backoff(self.churn)
    }

    /// Whether the roster should be declared unavailable and subagent calls refused.
    pub fn roster_is_unavailable(&self) -> bool {
        self.unserved >= RECONNECT_ATTEMPTS_BEFORE_GIVING_UP
    }

    /// Consecutive passes that served nothing — the count the refusal quotes.
    pub fn unserved(&self) -> u32 {
        self.unserved
    }

    /// Whether the reconnect delay has escalated all the way to [`RECONNECT_BACKOFF_CEILING`].
    ///
    /// Throttling that far is right — a run of subscriptions that each die as soon as they are
    /// served is a hot loop, and every attempt costs the daemon a fresh subscription — but being
    /// throttled *and* authoritative is not. A served pass leaves the roster
    /// `RosterCurrency::Current`, so without this the registry would answer every `resolve` from a
    /// snapshot as much as a whole ceiling old while the stream that should be refreshing it is
    /// deliberately asleep. Withdrawal keeps being enforced across it, because a roster that went
    /// stale still lists agents that demonstrably existed.
    pub fn throttled_to_the_ceiling(&self) -> bool {
        reconnect_backoff(self.churn) >= RECONNECT_BACKOFF_CEILING
    }
}

/// Follow the session's roster for the process lifetime, calling `on_change` once per applied
/// revision.
///
/// `on_change` is what emits the MCP `notifications/tools/list_changed`: this module holds no MCP
/// peer, and the roster's business is the roster.
pub fn follow_session_agent_roster(on_change: impl Fn() + Send + Sync + 'static) {
    let roster = session_agent_roster();
    let Some(transport) = detect_session_tool_transport() else {
        // No daemon in the loop at all — `tddy-sandbox-app`'s case. There is no roster to follow and
        // nothing that can go stale: the spawn seed is the whole roster, for the whole run.
        log::debug!(
            target: "tddy_tools::session_agents",
            "no session-tool transport is configured; the spawn seed is this session's whole roster"
        );
        return;
    };
    match &transport {
        // The sandbox tool-IPC socket now bridges `StreamSessionAgents` (and the conversation RPCs)
        // to the facilitating daemon over the `SessionChannel` — see
        // `tddy_sandbox_runner::ToolExecService` and `run_host_relay_with_rpc`. The subscription
        // proceeds exactly as it does over LiveKit: a fresh connection per stream, the first
        // frame replaces the seed, reconnect-on-drop with backoff. A daemon that does not serve
        // the RPC (the standalone app, via `NullRpcHandler`) refuses it, the roster goes
        // `Unavailable`, and `subagent_*` calls are refused — the safe behaviour for a session
        // with no daemon in the loop.
        SessionToolTransport::SandboxIpc { .. } | SessionToolTransport::LiveKit { .. } => {}
        // Refused rather than left on the seed: a registry frozen at spawn answers for agents that
        // have since been detached and refuses ones that have been attached, and says nothing.
        // TODO(session-agent-roster): give the HTTP transport a `StreamSessionAgents` client (or a
        // `ListSessionAgents` poll) so a daemon-HTTP session can address agents at all.
        SessionToolTransport::DaemonHttp { .. } => {
            let reason = "the roster stream has no client for the daemon-HTTP transport";
            log::error!(target: "tddy_tools::session_agents", "{reason}; subagent calls are refused");
            roster.mark_unavailable(reason);
            return;
        }
        SessionToolTransport::IncompleteLiveKit { missing } => {
            let reason = format!(
                "a LiveKit environment is set but {} is empty or unset",
                missing.join(", ")
            );
            log::error!(target: "tddy_tools::session_agents", "{reason}; subagent calls are refused");
            roster.mark_unavailable(&reason);
            return;
        }
    }
    tokio::spawn(async move { follow_roster(transport, roster, on_change).await });
}

/// Hold the stream open for the process lifetime, reconnecting when it drops.
async fn follow_roster(
    transport: SessionToolTransport,
    roster: &'static LiveAgentRoster,
    on_change: impl Fn() + Send + Sync,
) {
    let mut pacing = ReconnectPacing::default();
    loop {
        let opened_at = Instant::now();
        let pass = stream_roster_once(&transport, roster, &on_change).await;
        let last_failure = pass.reason();
        let backoff = pacing.record(&pass, opened_at.elapsed());
        if pass.counts_as_a_failure() {
            log::warn!(
                target: "tddy_tools::session_agents",
                "roster stream for session {}: {last_failure}",
                roster.session_id()
            );
        } else {
            log::warn!(
                target: "tddy_tools::session_agents",
                "roster stream for session {} ended after {} snapshot(s) ({last_failure}); \
                 reconnecting in {backoff:?}",
                roster.session_id(),
                pass.applied()
            );
        }
        if pacing.roster_is_unavailable() {
            // The last failure is carried into the refusal, not just the log: the main agent reads
            // the refusal and an operator reads it in the transcript, and "the roster went away" is
            // not diagnosable without the reason the connection gave.
            let reason = format!(
                "roster stream closed after {} reconnect attempts ({last_failure})",
                pacing.unserved()
            );
            log::error!(
                target: "tddy_tools::session_agents",
                "{reason}; every subagent call for session {} is refused until it recovers",
                roster.session_id()
            );
            roster.mark_unavailable(&reason);
        } else if pacing.throttled_to_the_ceiling() {
            // Served, so not unavailable — but the next frame is a whole ceiling away, and until it
            // arrives this process cannot claim to know the roster.
            let reason = format!(
                "roster stream keeps ending as soon as it has been served, so reconnects are \
                 throttled to {backoff:?} ({last_failure})"
            );
            log::error!(
                target: "tddy_tools::session_agents",
                "{reason}; every subagent call for session {} is refused until a frame arrives",
                roster.session_id()
            );
            roster.mark_unavailable(&reason);
        }
        tokio::time::sleep(backoff).await;
    }
}

/// Exponential backoff from [`RECONNECT_BACKOFF_START`], capped at [`RECONNECT_BACKOFF_CEILING`].
fn reconnect_backoff(consecutive_failures: u32) -> Duration {
    RECONNECT_BACKOFF_START
        .saturating_mul(2u32.saturating_pow(consecutive_failures.min(8)))
        .min(RECONNECT_BACKOFF_CEILING)
}

/// Open the stream once and apply every snapshot it delivers, reporting what the pass achieved and
/// how it ended.
///
/// The applied count is carried out of *every* ending, the error ones included: a stream that served
/// a roster and then broke is a reconnect, and [`RosterStreamOutcome::counts_as_a_failure`] can only
/// tell that from a setup nothing is serving if it is told how much arrived.
async fn stream_roster_once(
    transport: &SessionToolTransport,
    roster: &LiveAgentRoster,
    on_change: &(impl Fn() + Send + Sync),
) -> RosterStreamOutcome {
    // The client is held for as long as the frames are read: it owns the connection they ride, and
    // dropping it would close the subscription it just opened.
    let (_client, mut frames) = match open_roster_stream(transport).await {
        Ok(opened) => opened,
        // Nothing was opened, so nothing was applied.
        Err(failure) => return RosterStreamOutcome::broke(0, failure),
    };

    let mut applied: u64 = 0;
    loop {
        // Only the first frame is waited for with a deadline: the daemon sends the current snapshot
        // on subscribe, so nothing by now means nothing is serving this subscription. After that a
        // silent stream is a roster nobody is changing — and one the daemon keeps alive by re-sending
        // the applied revision, so silence for long enough is the connection, not the roster.
        let frame = if applied == 0 {
            match tokio::time::timeout(FIRST_FRAME_TIMEOUT, frames.recv()).await {
                Ok(frame) => frame,
                Err(_) => {
                    return RosterStreamOutcome::broke(
                        applied,
                        format!(
                            "no roster snapshot within {}s of subscribing",
                            FIRST_FRAME_TIMEOUT.as_secs()
                        ),
                    )
                }
            }
        } else {
            frames.recv().await
        };
        let Some(frame) = frame else {
            return RosterStreamOutcome::closed(applied);
        };
        let bytes = match frame {
            Ok(bytes) => bytes,
            Err(e) => return RosterStreamOutcome::broke(applied, format!("roster stream: {e}")),
        };
        let snapshot = match SessionAgentRoster::decode(bytes.as_slice()) {
            Ok(snapshot) => snapshot,
            Err(e) => {
                return RosterStreamOutcome::broke(
                    applied,
                    format!("undecodable roster frame ({} bytes): {e}", bytes.len()),
                )
            }
        };
        let rev = snapshot.rev;
        let announced_before = roster.tool_list_change_count();
        roster.apply_snapshot(snapshot);
        applied += 1;
        // One notification per *applied* revision, so a re-delivered snapshot — a keepalive, or a
        // reconnect's opening frame — does not make the main agent re-list for nothing.
        if roster.tool_list_change_count() != announced_before {
            log::debug!(
                target: "tddy_tools::session_agents",
                "roster rev {rev} applied for session {}",
                roster.session_id()
            );
            on_change();
        }
    }
}

/// Open one `StreamSessionAgents` subscription over `transport`, returning the connection it rides
/// alongside its frames.
#[allow(clippy::type_complexity)]
async fn open_roster_stream(
    transport: &SessionToolTransport,
) -> Result<
    (
        std::sync::Arc<dyn tddy_rpc::RpcClientTransport>,
        tokio::sync::mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>>,
    ),
    String,
> {
    let (client, envelope) = connect_roster_stream(transport).await?;
    let request = StreamSessionAgentsRequest {
        session_token: envelope.session_token,
        session_id: envelope.session_id,
        daemon_instance_id: envelope.daemon_instance_id,
    };
    let call = client.call_server_stream(
        "connection.ConnectionService",
        "StreamSessionAgents",
        request.encode_to_vec(),
    );
    let frames = tokio::time::timeout(STREAM_OPEN_TIMEOUT, call)
        .await
        .map_err(|_| {
            format!(
                "StreamSessionAgents was not accepted within {}s",
                STREAM_OPEN_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| format!("StreamSessionAgents call: {e}"))?;
    Ok((client, frames))
}

/// The connection the roster stream rides, and the identity its request carries.
///
/// Shared with the conversation RPCs ([`super::link`]) so the stream that decides which agent ids
/// are addressable and the calls that address them cannot reach different daemons.
async fn connect_roster_stream(
    transport: &SessionToolTransport,
) -> Result<
    (
        std::sync::Arc<dyn tddy_rpc::RpcClientTransport>,
        SessionToolEnvelope,
    ),
    String,
> {
    super::link::connect_facilitating_daemon(transport).await
}
