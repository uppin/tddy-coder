//! Clones: the independent checkout a remote agent reads its files from, one per **(session,
//! owning daemon)**.
//!
//! Product contract: `docs/ft/daemon/session-agent-roster.md` § Clones.
//! Module docs: `packages/tddy-daemon/docs/session-agent-roster.md`.
//!
//! Two daemons appear here and they hold different halves:
//!
//! - The **facilitating daemon** owns the roster, so it holds [`SessionAgentCloneStore`] — what it
//!   asked each peer to build, and what that peer has since reported back about it. It never opens
//!   a clone; it could not, the files are on another host.
//! - The **owning daemon** holds the checkout, so it holds [`HostedAgentClones`] and runs
//!   [`CloneMirror`] against the session's room. It is the only side that can say whether the
//!   checkout is ready or has diverged, which is why those facts are *pushed* to the facilitating
//!   daemon (`ReportAgentCloneState`) rather than polled out of it.
//!
//! The mirror is one-way. Anything that changed inside a clone changed underneath the syncer, and
//! that is reported as a divergence before it is repaired — a mirror that repairs itself without
//! saying so hides a real fault (AC40).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use prost::Message as _;
use tddy_livekit::client_connect::{connect_client, ConnectedClient};
use tddy_livekit::{BroadcastChannel, RpcClient, TokenGenerator};
use tddy_rpc::Status;
use tddy_service::proto::connection::{
    AgentActivityDeltaChunk, AgentActivityRecord, AgentCloneState, ExecuteToolChunk,
    ExecuteToolRequest, ReportAgentCloneStateRequest,
};
use tddy_service::proto::worktree_activity::WorktreeActivityEvent;
use tddy_service::session_activity::SESSION_ACTIVITY_TOPIC;
use tddy_service::worktree_activity::WORKTREE_ACTIVITY_TOPIC;
use tddy_session_sync::{
    decide_record, reassemble, ApplyOutcome, Mirror, MirrorMarker, RecordDecision,
};

const CONNECTION_SERVICE: &str = "connection.ConnectionService";

/// How long the owning daemon waits for the facilitating daemon to be visible in the session room
/// before giving up on the clone.
///
/// The facilitating daemon opens the room and joins it *before* it forwards the clone's start, so
/// it is already there by the time this runs; the wait covers the peer's own signalling round trip
/// rather than a race. Bounded because a clone whose room never materializes must fail visibly —
/// an unreachable facilitating daemon is one of the four distinct failures this feature refuses to
/// blur together.
const FACILITATING_DAEMON_WAIT: Duration = Duration::from_secs(30);

/// How long a `StreamExecuteTool` or `StreamAgentActivityDelta` addressed at the facilitating
/// daemon may say nothing before it is declared gone.
///
/// A chunk-framed message that loses a frame wedges with no error at all, so silence is given a
/// deadline (the same discipline `tddy-session-sync` applies to the same two streams).
const CLONE_RPC_SILENCE_BUDGET: Duration = Duration::from_secs(30);

/// Lifetime of the join token the owning daemon mints for the session's room.
///
/// One working day, matching every other room credential in the repo. A clone outliving its token
/// is a clone that stops syncing, so this is a ceiling on one attachment's life rather than on the
/// session's — a detach and re-attach mints another.
const CLONE_ROOM_TOKEN_TTL: Duration = Duration::from_secs(86_400);

/// How long the first restore waits for the session's WIP ref to exist.
///
/// Comfortably more than one session-room poll interval (2 s by default, and configurable upward),
/// so an ordinary open-then-attach never times out; short enough that a facilitating daemon which
/// never publishes the ref surfaces as a failed clone rather than as one stuck provisioning.
const FIRST_RESTORE_WAIT: Duration = Duration::from_secs(45);

/// How often that first restore is retried while it waits.
const FIRST_RESTORE_RETRY_INTERVAL: Duration = Duration::from_millis(500);

// ---------------------------------------------------------------------------
// The facilitating daemon's side
// ---------------------------------------------------------------------------

/// What the facilitating daemon knows about one clone.
#[derive(Debug, Clone)]
pub struct AgentClone {
    /// The `workspace` session on the owning daemon that holds the checkout. Minted here, before
    /// the peer is contacted, so a forward that never answers still leaves this side able to name —
    /// and therefore tear down — whatever the peer built.
    pub codebase_session_id: String,
    pub state: AgentCloneState,
    pub error: String,
    /// Where the checkout landed on the owning daemon, once it has said. `None` until then: a path
    /// guessed before the peer reported one would name a directory nobody created.
    pub worktree_path: Option<PathBuf>,
    /// Every reconcile the owning daemon has reported, oldest first.
    pub divergences: Vec<String>,
}

/// One `ReportAgentCloneState` call, as the store reads it.
///
/// A struct rather than seven arguments because six of the seven are strings and the wrong pairing
/// of two of them is a report accepted about somebody else's checkout — the one thing
/// [`SessionAgentCloneStore::record_report`] exists to refuse.
#[derive(Debug, Clone)]
pub struct AgentCloneReport {
    /// The facilitating daemon's session whose roster this clone serves.
    pub session_id: String,
    /// The owning daemon reporting.
    pub daemon_instance_id: String,
    /// The `workspace` session on it that holds the checkout.
    pub codebase_session_id: String,
    pub state: AgentCloneState,
    pub error: String,
    /// Where the checkout is on the owning daemon, when it named one.
    pub worktree_path: Option<PathBuf>,
    /// Reconciles observed since the previous report.
    pub divergences: Vec<String>,
}

/// The clones this daemon has asked peers to build for the sessions it facilitates.
///
/// Keyed by **(session, owning daemon)** rather than by agent: two agents owned by one host share
/// one checkout, and a checkout each would multiply disk and sync cost for isolation a read-only
/// mirror does not need (§ One clone per (session, remote daemon)).
#[derive(Default)]
pub struct SessionAgentCloneStore {
    clones: Mutex<HashMap<(String, String), AgentClone>>,
}

impl SessionAgentCloneStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Claim the clone for `(session_id, daemon_instance_id)`, minting one if this is the first
    /// agent that daemon owns on this session.
    ///
    /// Returns the clone's session id and whether this call is the one that has to provision it.
    /// Claiming and deciding in one locked step is what makes two concurrent attaches produce one
    /// checkout instead of two — the second sees the first's reservation rather than an absence.
    pub fn claim(
        &self,
        session_id: &str,
        daemon_instance_id: &str,
        mint_id: impl FnOnce() -> String,
    ) -> (String, bool) {
        let mut clones = self.lock();
        let key = (session_id.to_string(), daemon_instance_id.to_string());
        if let Some(existing) = clones.get(&key) {
            return (existing.codebase_session_id.clone(), false);
        }
        let codebase_session_id = mint_id();
        clones.insert(
            key,
            AgentClone {
                codebase_session_id: codebase_session_id.clone(),
                state: AgentCloneState::Provisioning,
                error: String::new(),
                worktree_path: None,
                divergences: Vec::new(),
            },
        );
        (codebase_session_id, true)
    }

    /// The clone for `(session, daemon)`, if this daemon has one.
    pub fn get(&self, session_id: &str, daemon_instance_id: &str) -> Option<AgentClone> {
        self.lock()
            .get(&(session_id.to_string(), daemon_instance_id.to_string()))
            .cloned()
    }

    /// Record what the owning daemon reported about its clone.
    ///
    /// Refused for a clone this daemon never asked for, and for one whose id does not match what it
    /// asked for: the report authorizes a roster entry to start serving prompts, so accepting one
    /// about an unknown checkout would let any room participant mark an agent ready.
    ///
    /// Divergences accumulate. Each report carries what the owning daemon observed since its last
    /// one, and replacing rather than appending would hide a fault one report later.
    pub fn record_report(&self, report: &AgentCloneReport) -> Result<(), Status> {
        let AgentCloneReport {
            session_id,
            daemon_instance_id,
            codebase_session_id,
            state,
            error,
            worktree_path,
            divergences,
        } = report;
        let mut clones = self.lock();
        let key = (session_id.clone(), daemon_instance_id.clone());
        let clone = clones.get_mut(&key).ok_or_else(|| {
            Status::failed_precondition(format!(
                "session '{session_id}' has no clone on daemon '{daemon_instance_id}' to report on"
            ))
        })?;
        if &clone.codebase_session_id != codebase_session_id {
            return Err(Status::failed_precondition(format!(
                "daemon '{daemon_instance_id}' reported on workspace session \
                 '{codebase_session_id}', but session '{session_id}' asked it for '{}'",
                clone.codebase_session_id
            )));
        }
        clone.state = *state;
        clone.error = error.clone();
        if let Some(path) = worktree_path {
            clone.worktree_path = Some(path.clone());
        }
        clone.divergences.extend(divergences.iter().cloned());
        Ok(())
    }

    /// Mark the clone failed, with the reason an operator has to read to act on it.
    pub fn fail(&self, session_id: &str, daemon_instance_id: &str, error: impl Into<String>) {
        if let Some(clone) = self
            .lock()
            .get_mut(&(session_id.to_string(), daemon_instance_id.to_string()))
        {
            clone.state = AgentCloneState::Error;
            clone.error = error.into();
        }
    }

    /// Drop the clone's record, returning it so the caller can tear the checkout down under the id
    /// it was created with.
    pub fn forget(&self, session_id: &str, daemon_instance_id: &str) -> Option<AgentClone> {
        self.lock()
            .remove(&(session_id.to_string(), daemon_instance_id.to_string()))
    }

    /// Every clone a session created, as `(owning daemon, clone)` pairs. Used by `DeleteSession`,
    /// which has to reach hosts the operator never looked at.
    pub fn for_session(&self, session_id: &str) -> Vec<(String, AgentClone)> {
        self.lock()
            .iter()
            .filter(|((session, _), _)| session == session_id)
            .map(|((_, daemon), clone)| (daemon.clone(), clone.clone()))
            .collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), AgentClone>> {
        // What this guards is a map of plain records: a thread that panicked while holding it left
        // nothing half-written, and a poisoned lock that made every clone unreachable would be a
        // strictly worse outcome than reading what is there.
        self.clones.lock().unwrap_or_else(|e| e.into_inner())
    }
}

// ---------------------------------------------------------------------------
// The owning daemon's side
// ---------------------------------------------------------------------------

/// One checkout this daemon holds on another daemon's behalf, and the link back to it.
pub struct HostedClone {
    /// The facilitating daemon's session this checkout mirrors.
    pub session_id: String,
    pub facilitating_daemon_instance_id: String,
    /// This daemon's own `workspace` session holding the checkout.
    pub codebase_session_id: String,
    pub worktree_path: PathBuf,
    /// Addressed at `daemon-{facilitating}` inside the session room. Every mutating tool call a
    /// roster agent on this host makes goes through it, and so does every clone-state report.
    client: RpcClient,
    /// The credential presented to the facilitating daemon.
    ///
    /// TODO(session-agent-roster): this is the caller's own token, carried from the attach that
    /// created the clone, so a clone outlives its credential exactly as the split session's room
    /// poller used to (`split_session::RoomPollTokenMinter` is the fix — mint a fresh short-lived
    /// token per call under the *verified* caller instead of holding one).
    session_token: String,
    /// Held for the clone's life: dropping it leaves the session room, and a clone outside the room
    /// hears nothing further while failing at nothing.
    _room: Arc<livekit::Room>,
}

impl HostedClone {
    /// Run one tool call against the **facilitating** daemon's authoritative worktree.
    ///
    /// Streamed rather than unary for the reason `StreamExecuteTool` exists: over LiveKit a result
    /// past `MAX_CHUNK_FRAME_BYTES` is chunk-framed, and one lost frame wedges the call with no
    /// error rather than failing it.
    pub async fn execute_tool_on_facilitator(
        &self,
        tool_name: &str,
        args_json: &str,
    ) -> Result<String, Status> {
        let request = ExecuteToolRequest {
            session_token: self.session_token.clone(),
            session_id: self.session_id.clone(),
            // Empty: the facilitating daemon serves this from its own worktree. Naming a daemon
            // here would ask it to forward the call somewhere else, which is the one place this
            // mutation must not land.
            daemon_instance_id: String::new(),
            tool_name: tool_name.to_string(),
            args_json: args_json.to_string(),
        };
        let mut frames = self
            .client
            .call_server_stream(
                CONNECTION_SERVICE,
                "StreamExecuteTool",
                request.encode_to_vec(),
            )
            .await
            .map_err(|status| {
                Status::unavailable(format!(
                    "proxying {tool_name} to daemon {} failed: {}",
                    self.facilitating_daemon_instance_id, status.message
                ))
            })?;

        let mut result = String::new();
        let mut saw_last = false;
        loop {
            match tokio::time::timeout(CLONE_RPC_SILENCE_BUDGET, frames.recv()).await {
                Ok(Some(Ok(bytes))) => {
                    let chunk = ExecuteToolChunk::decode(&bytes[..]).map_err(|e| {
                        Status::internal(format!("decode ExecuteToolChunk from facilitator: {e}"))
                    })?;
                    result.push_str(&String::from_utf8_lossy(&chunk.result_chunk));
                    if chunk.is_error {
                        return Err(Status::internal(format!(
                            "{tool_name} failed on daemon {}: {}",
                            self.facilitating_daemon_instance_id, chunk.error_message
                        )));
                    }
                    saw_last |= chunk.last;
                }
                Ok(Some(Err(status))) => {
                    return Err(Status::unavailable(format!(
                        "proxying {tool_name} to daemon {} failed mid-stream: {}",
                        self.facilitating_daemon_instance_id, status.message
                    )))
                }
                Ok(None) => break,
                Err(_) => {
                    return Err(Status::deadline_exceeded(format!(
                        "daemon {} sent nothing on {tool_name}'s result for {}s",
                        self.facilitating_daemon_instance_id,
                        CLONE_RPC_SILENCE_BUDGET.as_secs()
                    )))
                }
            }
        }
        // A stream that ended without its final frame was truncated, not completed. Returning what
        // arrived would hand a model half a file it has no way to know is half.
        if !saw_last {
            return Err(Status::internal(format!(
                "{tool_name}'s result from daemon {} ended without its final frame, so it was \
                 truncated rather than completed",
                self.facilitating_daemon_instance_id
            )));
        }
        Ok(result)
    }

    /// Tell the facilitating daemon how this clone is doing.
    async fn report(
        &self,
        state: AgentCloneState,
        error: &str,
        daemon_instance_id: &str,
        divergences: Vec<String>,
    ) {
        let request = ReportAgentCloneStateRequest {
            session_token: self.session_token.clone(),
            session_id: self.session_id.clone(),
            daemon_instance_id: daemon_instance_id.to_string(),
            codebase_session_id: self.codebase_session_id.clone(),
            clone_state: state as i32,
            clone_error: error.to_string(),
            worktree_path: self.worktree_path.to_string_lossy().into_owned(),
            divergences,
        };
        // Loud on failure and nothing else: this daemon cannot repair a facilitating daemon that is
        // not listening, and a clone that kept syncing while nobody knew its state would be worse
        // than one that says so in the log an operator is already reading.
        if let Err(e) = self
            .client
            .call_unary(
                CONNECTION_SERVICE,
                "ReportAgentCloneState",
                request.encode_to_vec(),
            )
            .await
        {
            log::error!(
                "agent clone {}: daemon {} did not accept a {state:?} report: {}",
                self.codebase_session_id,
                self.facilitating_daemon_instance_id,
                e.message
            );
        }
    }
}

/// The clones this daemon hosts for other daemons' sessions, keyed by the facilitating session.
///
/// One entry per session rather than per agent, for the same reason the facilitating side keys by
/// (session, daemon): every agent this daemon owns on that session reads the same checkout.
#[derive(Default)]
pub struct HostedAgentClones {
    by_session: Mutex<HashMap<String, Arc<HostedClone>>>,
}

impl HostedAgentClones {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, session_id: &str) -> Option<Arc<HostedClone>> {
        self.lock().get(session_id).cloned()
    }

    fn insert(&self, clone: Arc<HostedClone>) {
        self.lock().insert(clone.session_id.clone(), clone);
    }

    /// Forget the clone of `codebase_session_id`, whichever session it mirrors.
    ///
    /// Addressed by the checkout rather than by the session because the caller that needs it —
    /// `DeleteSession` on the owning daemon — knows only the id of the session it is deleting.
    pub fn forget_checkout(&self, codebase_session_id: &str) {
        self.lock()
            .retain(|_, clone| clone.codebase_session_id != codebase_session_id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<HostedClone>>> {
        self.by_session.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Everything the owning daemon needs to turn a fresh `workspace` checkout into a live mirror.
pub struct CloneMirrorSpec {
    /// The facilitating daemon's session this checkout mirrors.
    pub session_id: String,
    pub facilitating_daemon_instance_id: String,
    /// This daemon's instance id, as it joins the session room under.
    pub owning_daemon_instance_id: String,
    pub codebase_session_id: String,
    pub worktree_path: PathBuf,
    /// The repository the checkout was cut from — where its WIP ref is fetched from.
    pub project_repo_path: PathBuf,
    pub project_id: String,
    pub session_token: String,
    pub livekit_url: String,
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    /// The facilitating daemon's HTTP URL, when this clone was provisioned from it via
    /// `tddy-remote-git-repo` (PRD AC37). When set, the checkout's `origin` remote points at the
    /// facilitating daemon's `remote_git.RemoteGitService`, so the WIP-ref fetch in [`CloneMirror`]
    /// runs `git fetch origin` with `GIT_SSH_COMMAND=tddy-remote-git-repo --daemon-url {url}
    /// --session-token {token}`. When `None`, the checkout shares the facilitating daemon's repo on
    /// a local filesystem and the fetch reads `project_repo_path` directly.
    pub facilitating_daemon_url: Option<String>,
    /// The first admission token the facilitating daemon minted for this owning daemon as part of
    /// the room-admission handshake (PRD § "What attach does" step 3). When non-empty, the mirror
    /// joins `first_admission_room` on `first_admission_url` with this token and nothing else, and
    /// runs the re-admit loop against `session_admission.SessionAdmissionService/AdmitOwningDaemon`
    /// over the common room before it expires. When empty, the facilitating daemon could not mint
    /// (LiveKit not configured) and the mirror falls back to self-minting a 24 h room token — a
    /// recorded deviation, never a silent one.
    pub first_admission_token: String,
    pub first_admission_url: String,
    pub first_admission_room: String,
    /// This daemon's common-room handle, so the re-admit loop can call the facilitating daemon's
    /// `AdmitOwningDaemon` over the common room every daemon already joined. `None` when this
    /// daemon has no LiveKit discovery (a test fixture), in which case the re-admit loop is skipped
    /// and the first admission token is used as a one-shot join.
    pub common_room_slot: Option<Arc<tokio::sync::RwLock<Option<Arc<livekit::prelude::Room>>>>>,
}

/// Join the session's room, restore the checkout from its WIP ref, and keep it equal until the room
/// closes.
///
/// Registers the clone and reports `READY` only once the **first restore has completed**: readiness
/// is a claim that a prompt can be served, and a checkout that has joined a room but not yet caught
/// up would answer a search from the state the worktree was cut in.
pub async fn run_clone_mirror(
    spec: CloneMirrorSpec,
    hosted: Arc<HostedAgentClones>,
) -> Result<(), Status> {
    let room_name = crate::session_room::session_room_name(&spec.session_id);
    let identity =
        crate::livekit_peer_discovery::daemon_rpc_identity(&spec.owning_daemon_instance_id);
    let facilitator_identity =
        crate::livekit_peer_discovery::daemon_rpc_identity(&spec.facilitating_daemon_instance_id);

    // The room-admission handshake (PRD § "What attach does" step 3). When the facilitating daemon
    // minted a scoped, short-TTL admission token and forwarded it, the owning daemon joins with
    // that token and nothing else — it does NOT self-mint, because the facilitating daemon is the
    // authority on who may join `session-{session_id}`, and a self-minted token would bypass the
    // admission registry and its revocation. When the facilitating daemon could not mint (LiveKit
    // not configured — `first_admission_token` empty), the owning daemon falls back to self-minting
    // a 24 h room token from the shared `livekit.api_secret`: a recorded deviation (PRD § Deviations)
    // — consistent with the existing multi-host trust model, weaker than the handshake.
    //
    // The handshake's re-admit loop lives below: when the session room drops the owning daemon (its
    // short-TTL admission token expired, or LiveKit kicked it), the mirror asks the facilitating
    // daemon for a fresh token and rejoins. A re-admit that returns `FAILED_PRECONDITION` means
    // the facilitating daemon revoked the admission (the last agent this daemon owned detached),
    // and the mirror stops — never silently, and never as a half-alive checkout nobody mirrors.
    let handshake_enabled =
        !spec.first_admission_token.is_empty() && spec.common_room_slot.is_some();
    log::info!(
        "agent clone {}: starting mirror of session {} into {} (handshake={}, \
         facilitator={})",
        spec.codebase_session_id,
        spec.session_id,
        spec.worktree_path.display(),
        if handshake_enabled {
            "on"
        } else {
            "off (self-mint fallback)"
        },
        spec.facilitating_daemon_instance_id
    );

    // The first token: the facilitating daemon's admission token when the handshake is in play,
    // otherwise a self-minted 24 h room token (the fallback).
    let self_minted_token = || -> Result<String, Status> {
        TokenGenerator::new(
            spec.livekit_api_key.clone(),
            spec.livekit_api_secret.clone(),
            room_name.clone(),
            identity.clone(),
            CLONE_ROOM_TOKEN_TTL,
        )
        .generate()
        .map_err(|e| Status::internal(format!("mint a join token for {room_name}: {e}")))
    };

    let mut current_token = if !spec.first_admission_token.is_empty() {
        spec.first_admission_token.clone()
    } else {
        self_minted_token()?
    };
    let mut current_url = if !spec.first_admission_url.is_empty() {
        spec.first_admission_url.clone()
    } else {
        spec.livekit_url.clone()
    };

    // The mirror survives every reconnect: it holds the checkout and the applied-sequence cursor,
    // neither of which depends on the room connection. A restore after a rejoin re-fetches the
    // WIP ref and catches the checkout up to whatever the session published while the mirror was
    // gone.
    let mut mirror = CloneMirror::open(&spec)?;
    let mut first_iteration = true;

    loop {
        let ConnectedClient { room, client } = match connect_client(
            &current_url,
            &current_token,
            &facilitator_identity,
            FACILITATING_DAEMON_WAIT,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                return Err(Status::unavailable(format!(
                    "the clone for session {} could not join {room_name} as {identity}: {e}",
                    spec.session_id
                )));
            }
        };

        // Both subscriptions are taken before the restore, so nothing published while the mirror
        // is catching up is lost: `BroadcastChannel::subscribe` takes its own room event handle,
        // and a record that arrives during the restore is decided against the marker the restore
        // wrote.
        let mut session_activity =
            BroadcastChannel::new(Arc::clone(&room), SESSION_ACTIVITY_TOPIC).subscribe();
        let mut worktree_activity =
            BroadcastChannel::new(Arc::clone(&room), WORKTREE_ACTIVITY_TOPIC).subscribe();

        let hosted_clone = Arc::new(HostedClone {
            session_id: spec.session_id.clone(),
            facilitating_daemon_instance_id: spec.facilitating_daemon_instance_id.clone(),
            codebase_session_id: spec.codebase_session_id.clone(),
            worktree_path: spec.worktree_path.clone(),
            client,
            session_token: spec.session_token.clone(),
            _room: room,
        });
        // Replaces any entry from a previous connection (HostedAgentClones::insert keys by
        // session_id), so an in-flight tool call addressed at the old connection's client does
        // not linger after the room it rode has gone.
        hosted.insert(Arc::clone(&hosted_clone));

        // The first connection waits for the session to publish its state before it restores;
        // a reconnection restores immediately, because the session has already published and the
        // WIP ref is the catch-up source.
        if first_iteration {
            if let Err(e) = mirror
                .restore_once_the_session_has_published_its_state()
                .await
            {
                hosted_clone
                    .report(
                        AgentCloneState::Error,
                        &e.to_string(),
                        &spec.owning_daemon_instance_id,
                        Vec::new(),
                    )
                    .await;
                return Err(Status::internal(e.to_string()));
            }
        } else if let Err(e) = mirror.restore().await {
            hosted_clone
                .report(
                    AgentCloneState::Error,
                    &e.to_string(),
                    &spec.owning_daemon_instance_id,
                    Vec::new(),
                )
                .await;
            return Err(Status::internal(e.to_string()));
        }

        hosted_clone
            .report(
                AgentCloneState::Ready,
                "",
                &spec.owning_daemon_instance_id,
                mirror.take_divergences(),
            )
            .await;
        if first_iteration {
            log::info!(
                "agent clone {}: mirroring session {} from {room_name} into {}",
                spec.codebase_session_id,
                spec.session_id,
                spec.worktree_path.display()
            );
        } else {
            log::info!(
                "agent clone {}: re-admitted to {room_name} after a reconnect",
                spec.codebase_session_id
            );
        }
        first_iteration = false;

        // Run the event loop until the room drops this participant. A `recv()` returning `None`
        // is the room closing — either the admission token expired and LiveKit kicked the owning
        // daemon, or the facilitating daemon left and the room emptied. Either way the mirror
        // tries to re-admit below; a re-admit that fails is the revocation signal.
        let mut room_closed = false;
        loop {
            let outcome = tokio::select! {
                message = session_activity.recv() => match message {
                    Some(message) => mirror.on_activity(&message.payload, &hosted_clone).await,
                    None => { room_closed = true; Ok(()) },
                },
                message = worktree_activity.recv() => match message {
                    Some(message) => mirror.on_worktree(&message.payload).await,
                    None => { room_closed = true; Ok(()) },
                },
            };
            if room_closed {
                break;
            }
            let divergences = mirror.take_divergences();
            match outcome {
                Ok(()) if divergences.is_empty() => {}
                Ok(()) => {
                    hosted_clone
                        .report(
                            AgentCloneState::Ready,
                            "",
                            &spec.owning_daemon_instance_id,
                            divergences,
                        )
                        .await
                }
                Err(reason) => {
                    hosted_clone
                        .report(
                            AgentCloneState::Error,
                            &reason,
                            &spec.owning_daemon_instance_id,
                            divergences,
                        )
                        .await;
                    return Err(Status::internal(reason));
                }
            }
        }

        // The room closed. Without the handshake there is no re-admit: a self-minted 24 h token
        // does not expire inside a session's lifetime, and a room that closed on a fallback path
        // is a fault, not a refresh.
        if !handshake_enabled {
            log::warn!(
                "agent clone {}: the session room {room_name} closed and the handshake is not \
                 enabled (no first-admission token from the facilitating daemon); the mirror stops",
                spec.codebase_session_id
            );
            hosted_clone
                .report(
                    AgentCloneState::Error,
                    "the session's room closed",
                    &spec.owning_daemon_instance_id,
                    Vec::new(),
                )
                .await;
            return Err(Status::internal("the session's room closed"));
        }

        // Re-admit: ask the facilitating daemon for a fresh scoped token over the common room. A
        // success hands back a token the next iteration joins with; a `FAILED_PRECONDITION` is the
        // revocation (the last agent this daemon owned detached), and the mirror stops.
        log::info!(
            "agent clone {}: the session room {room_name} closed; asking the facilitating daemon \
             {} to re-admit (AdmitOwningDaemon over the common room)",
            spec.codebase_session_id,
            spec.facilitating_daemon_instance_id
        );
        match re_admit(&spec, &room_name).await {
            Ok((token, url)) => {
                log::info!(
                    "agent clone {}: re-admitted by the facilitating daemon; rejoining {room_name} \
                     on {url}",
                    spec.codebase_session_id
                );
                current_token = token;
                current_url = url;
            }
            Err(reason) => {
                log::warn!(
                    "agent clone {}: re-admit failed, leaving the session room: {reason}",
                    spec.codebase_session_id
                );
                hosted_clone
                    .report(
                        AgentCloneState::Error,
                        &reason,
                        &spec.owning_daemon_instance_id,
                        Vec::new(),
                    )
                    .await;
                return Err(Status::internal(reason));
            }
        }
    }
}

/// Ask the facilitating daemon for a fresh admission token over the common room (PRD § "What
/// attach does" step 3 — the re-admit loop). Returns the token and the LiveKit server to rejoin
/// on; an error carries the reason the mirror should stop with (revocation or unreachable).
async fn re_admit(spec: &CloneMirrorSpec, _room_name: &str) -> Result<(String, String), String> {
    use prost::Message as _;
    use tddy_service::proto::session_admission::AdmitOwningDaemonRequest;

    let slot = spec.common_room_slot.clone().ok_or_else(|| {
        "the common room is not connected on this daemon; cannot re-admit".to_string()
    })?;
    let body = AdmitOwningDaemonRequest {
        session_token: spec.session_token.clone(),
        session_id: spec.session_id.clone(),
        owning_daemon_instance_id: spec.owning_daemon_instance_id.clone(),
    }
    .encode_to_vec();
    log::debug!(
        "agent clone {}: re-admit → forwarding AdmitOwningDaemon to daemon {} over the common \
         room (session {})",
        spec.codebase_session_id,
        spec.facilitating_daemon_instance_id,
        spec.session_id
    );
    let response = crate::livekit_peer_discovery::forward_to_peer(
        &slot,
        &spec.facilitating_daemon_instance_id,
        "session_admission.SessionAdmissionService",
        "AdmitOwningDaemon",
        body,
    )
    .await
    .map_err(|e| {
        log::warn!(
            "agent clone {}: re-admit RPC returned an error: code={:?} message={}",
            spec.codebase_session_id,
            e.code(),
            e.message()
        );
        format!(
            "re-admit RPC to the facilitating daemon failed: {}",
            e.message()
        )
    })?;
    let decoded = tddy_service::proto::session_admission::AdmitOwningDaemonResponse::decode(
        response.as_slice(),
    )
    .map_err(|e| format!("re-admit response did not decode: {e}"))?;
    if decoded.token.is_empty() || decoded.url.is_empty() {
        return Err(
            "the facilitating daemon admitted this daemon but returned no token or url".to_string(),
        );
    }
    Ok((decoded.token, decoded.url))
}

/// The mirror itself: the checkout, what it has applied, and the git it runs to stay equal.
struct CloneMirror {
    mirror: Mirror,
    worktree_path: PathBuf,
    /// The repository the WIP ref is fetched from.
    project_repo_path: PathBuf,
    session_id: String,
    /// The applied sequence as of the last restore — what tells a tree the mirror itself wrote into
    /// from one a second writer touched.
    restored_at_seq: u64,
    /// Reconciles observed since the last report. Drained by the loop, never dropped.
    divergences: Vec<String>,
    /// `GIT_SSH_COMMAND` for `git fetch origin` when the checkout was provisioned from the
    /// facilitating daemon (PRD AC37). `None` → fetch reads `project_repo_path` on a shared
    /// filesystem.
    ssh_command: Option<String>,
}

impl CloneMirror {
    /// Adopt the freshly created checkout.
    ///
    /// The marker is written **before** the first restore, which is what makes the directory the
    /// syncer's rather than merely one it is about to write into: `Mirror::open_or_create` refuses a
    /// non-empty directory that carries no marker, and a `git worktree add` leaves exactly such a
    /// directory behind. Writing it here is also what stops a checkout that was a mirror from later
    /// being mistaken for a workspace (§ Kept current from the room).
    fn open(spec: &CloneMirrorSpec) -> Result<Self, Status> {
        let marker = MirrorMarker {
            session_id: spec.session_id.clone(),
            daemon_instance_id: spec.facilitating_daemon_instance_id.clone(),
            project: spec.project_id.clone(),
            last_seq: 0,
            last_head_commit: String::new(),
        };
        let marker_path = spec
            .worktree_path
            .join(tddy_session_sync::mirror::MARKER_FILENAME);
        std::fs::write(
            &marker_path,
            serde_json::to_vec_pretty(&marker)
                .map_err(|e| Status::internal(format!("serialize the mirror marker: {e}")))?,
        )
        .map_err(|e| {
            Status::internal(format!(
                "claim {} as a mirror: {e}",
                spec.worktree_path.display()
            ))
        })?;
        let mirror = Mirror::open_or_create(&spec.worktree_path, marker)
            .map_err(|e| Status::internal(e.to_string()))?;
        // The transport-shim env var, only when this checkout was provisioned from the facilitating
        // daemon. `tddy-remote-git-repo` takes its daemon URL and session token as `--long` flags on
        // the `GIT_SSH_COMMAND` string, so a single env var carries both.
        let ssh_command = spec.facilitating_daemon_url.as_ref().map(|url| {
            format!(
                "{} --daemon-url {url} --session-token {}",
                crate::project_provision::resolve_remote_git_repo_path(),
                spec.session_token
            )
        });
        Ok(Self {
            mirror,
            worktree_path: spec.worktree_path.clone(),
            project_repo_path: spec.project_repo_path.clone(),
            session_id: spec.session_id.clone(),
            restored_at_seq: 0,
            divergences: Vec::new(),
            ssh_command,
        })
    }

    fn take_divergences(&mut self) -> Vec<String> {
        std::mem::take(&mut self.divergences)
    }

    /// The first restore, retried until the session's WIP ref exists.
    ///
    /// Ordering rather than tolerance: the facilitating daemon publishes that ref on its room's
    /// **first poll tick**, which can land after the clone was created — the peer answered the
    /// forwarded start as soon as the checkout was cut, and cutting a checkout is faster than one
    /// tick of a room that was opened moments earlier. Fetching a ref that has not been written yet
    /// fails with "couldn't find remote ref", which is not a fault to report, it is a moment too
    /// early.
    ///
    /// Bounded, because a ref that never comes *is* a fault: the last failure is returned as the
    /// clone's error, and the entry reports ERROR rather than sitting in PROVISIONING forever.
    async fn restore_once_the_session_has_published_its_state(&mut self) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + FIRST_RESTORE_WAIT;
        let mut last = self.restore().await;
        while last.is_err() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(FIRST_RESTORE_RETRY_INTERVAL).await;
            last = self.restore().await;
        }
        last.map_err(|reason| {
            format!(
                "the clone could not be restored from {} within {}s: {reason}",
                tddy_session_sync::wip_ref(&self.session_id),
                FIRST_RESTORE_WAIT.as_secs()
            )
        })
    }

    /// Restore the checkout from the session's WIP ref, recording anything that had changed in it
    /// first.
    ///
    /// The two git moves after the fetch are one operation split in two, and the split is the point.
    /// A plain `reset --hard` onto the WIP commit would park `HEAD` on a commit the session's own
    /// checkout is not on — every delta afterwards is cut from the session's real `HEAD`, so every
    /// one would be refused as a base-commit mismatch and every tick would reconcile again, forever.
    /// `HEAD` goes on the WIP commit's *parent*, which is exactly the session's `HEAD`, and the
    /// working tree is then filled from the WIP tree.
    async fn restore(&mut self) -> Result<(), String> {
        // Only when the mirror itself has written nothing since the last restore. `LOCAL_WIP_REF`
        // still points at the tick the last restore filled the tree from, so after a delta was
        // applied the diff lists exactly the files that delta legitimately wrote — reported as a
        // second writer, at `error`, on the ordinary apply-then-`worktree.activity` sequence.
        let applied = self.mirror.marker().last_seq;
        if applied == self.restored_at_seq {
            self.note_local_changes().await;
        }
        let wip = tddy_session_sync::wip_ref(&self.session_id);
        let local = tddy_session_sync::LOCAL_WIP_REF;
        // A facilitator-provisioned checkout (PRD AC37) fetches its WIP ref from `origin`, which
        // points at the facilitating daemon's `remote_git.RemoteGitService` — the only place that
        // holds the ref. A shared-filesystem checkout fetches from the local `project_repo_path`
        // directly, where the facilitating daemon published the ref.
        let fetch_source = if self.ssh_command.is_some() {
            "origin".to_string()
        } else {
            self.project_repo_path.to_string_lossy().into_owned()
        };
        self.git(&[
            "fetch",
            &fetch_source,
            // Forced: the ref moves to a new commit every tick and its old value is not an ancestor
            // of its new one.
            &format!("+{wip}:{local}"),
        ])
        .await?;
        self.git(&["reset", "--hard", &format!("{local}^")]).await?;
        self.git(&["read-tree", "-u", "--reset", local]).await?;
        // The restore is known to include whatever the WIP ref held, and nothing about which tick
        // that was — so the applied sequence stays where it is. Claiming a tick this restore did not
        // demonstrably include would skip the next delta and drift with nothing reporting it.
        self.mirror
            .record_restored(applied)
            .map_err(|e| e.to_string())?;
        self.restored_at_seq = applied;
        Ok(())
    }

    /// Record anything that changed inside the clone since the last restore.
    ///
    /// The mirror is one-way, so the working tree differing from the WIP tree it was last filled
    /// from means somebody wrote into the clone underneath the syncer. It is repaired by the restore
    /// that follows, but it is *reported* first: a mirror that repairs itself in silence hides a
    /// real fault, and the fault here is a second writer nobody knows about (AC40).
    ///
    /// Untracked files are deliberately not divergences. `read-tree -u --reset` leaves them alone,
    /// so they neither drift nor get repaired, and an agent's own scratch file is not a fault.
    ///
    /// Called only when the mirror has applied nothing since the last restore (see [`Self::restore`]):
    /// the ref it diffs against is the tick the tree was last filled from, so a delta applied in
    /// between is indistinguishable here from a second writer.
    async fn note_local_changes(&mut self) {
        let Ok(output) = self
            .git_output(&[
                "diff",
                "--name-only",
                tddy_session_sync::LOCAL_WIP_REF,
                "--",
            ])
            .await
        else {
            // No local WIP ref yet — this is the first restore, and there is nothing for the clone
            // to have diverged from.
            return;
        };
        let changed: Vec<&str> = output
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        if changed.is_empty() {
            return;
        }
        let reason = format!(
            "the clone at {} was modified underneath the mirror: {}",
            self.worktree_path.display(),
            changed.join(", ")
        );
        log::error!("agent clone: {reason}; restoring it from the session's WIP ref");
        self.divergences.push(reason);
    }

    /// One `session.activity` record: fetch the tick's patch and apply it, or reconcile.
    async fn on_activity(&mut self, payload: &[u8], link: &HostedClone) -> Result<(), String> {
        let record = AgentActivityRecord::decode(payload)
            .map_err(|e| format!("a \"{SESSION_ACTIVITY_TOPIC}\" broadcast did not decode: {e}"))?;
        let (call_id, seq) = match decide_record(&record, self.mirror.marker().last_seq) {
            RecordDecision::FetchDelta { call_id, seq } => (call_id, seq),
            RecordDecision::Ignore(reason) => {
                log::debug!("{} ({}): {reason}", record.tool_name, record.call_id);
                return Ok(());
            }
        };

        let delta = self.fetch_delta(link, &call_id).await?;
        match self.mirror.apply(&delta).map_err(|e| e.to_string())? {
            ApplyOutcome::Applied => {
                log::debug!(
                    "agent clone: applied tick {seq} ({} bytes) from {}",
                    delta.patch.len(),
                    record.tool_name
                );
                Ok(())
            }
            ApplyOutcome::AlreadyApplied => Ok(()),
            ApplyOutcome::NeedsReconcile(reason) => {
                let reason = format!("the clone diverged at tick {seq}: {reason}");
                log::error!("agent clone: {reason}");
                self.divergences.push(reason);
                self.restore().await
            }
        }
    }

    /// One `worktree.activity` event: the session's checkout moved, so restore from the WIP ref.
    ///
    /// Deliberately not filtered to `commit` events, which is where this parts company with the
    /// standalone syncer. That client mirrors a session whose every edit arrives as a reported tool
    /// call, so a `files changed` event carries nothing its delta will not also carry. A roster
    /// clone has no such guarantee: the main agent's edits reach the room as deltas only when its
    /// hooks report them, and a human, a `git checkout` or an unhooked agent moves the same
    /// worktree with no `call_id` to address a delta by. The WIP ref is the one thing that always
    /// describes the session's checkout, so every movement of it restores from there.
    async fn on_worktree(&mut self, payload: &[u8]) -> Result<(), String> {
        let event = WorktreeActivityEvent::decode(payload).map_err(|e| {
            format!("a \"{WORKTREE_ACTIVITY_TOPIC}\" broadcast did not decode: {e}")
        })?;
        log::debug!(
            "agent clone: session worktree moved (seq {}, head {}); restoring from its WIP ref",
            event.seq,
            event.head_commit
        );
        self.restore().await
    }

    /// Fetch one tick's patch from the facilitating daemon, frame by frame.
    async fn fetch_delta(
        &self,
        link: &HostedClone,
        call_id: &str,
    ) -> Result<tddy_session_sync::Delta, String> {
        let request = tddy_service::proto::connection::AgentActivityDeltaRequest {
            session_token: link.session_token.clone(),
            session_id: self.session_id.clone(),
            // Routed explicitly: the daemon in the room is the one that owns the session, but a
            // silently mis-routed delta is a mirror built from another host's worktree.
            daemon_instance_id: link.facilitating_daemon_instance_id.clone(),
            call_id: call_id.to_string(),
            scope: tddy_session_sync::MIRROR_DELTA_SCOPE as i32,
        };
        let mut frames = link
            .client
            .call_server_stream(
                CONNECTION_SERVICE,
                "StreamAgentActivityDelta",
                request.encode_to_vec(),
            )
            .await
            .map_err(|status| {
                format!(
                    "daemon {} refused StreamAgentActivityDelta ({}): {}",
                    link.facilitating_daemon_instance_id,
                    status.code.as_str(),
                    status.message
                )
            })?;

        let mut chunks = Vec::new();
        loop {
            // A deadline on *silence*, re-armed by every frame: a chunk-framed stream that lost a
            // frame waits forever and reports nothing, which is this transport's one failure mode.
            match tokio::time::timeout(CLONE_RPC_SILENCE_BUDGET, frames.recv()).await {
                Ok(Some(Ok(bytes))) => chunks.push(
                    AgentActivityDeltaChunk::decode(&bytes[..])
                        .map_err(|e| format!("a delta frame did not decode: {e}"))?,
                ),
                Ok(Some(Err(status))) => {
                    return Err(format!(
                        "StreamAgentActivityDelta failed mid-stream: {}",
                        status.message
                    ))
                }
                Ok(None) => break,
                Err(_) => {
                    return Err(format!(
                        "daemon {} sent nothing on StreamAgentActivityDelta for {}s",
                        link.facilitating_daemon_instance_id,
                        CLONE_RPC_SILENCE_BUDGET.as_secs()
                    ))
                }
            }
        }
        reassemble(&chunks).map_err(|e| e.to_string())
    }

    async fn git(&self, args: &[&str]) -> Result<(), String> {
        self.git_output(args).await.map(|_| ())
    }

    async fn git_output(&self, args: &[&str]) -> Result<String, String> {
        let mut command = tokio::process::Command::new("git");
        command.args(args).current_dir(&self.worktree_path);
        // Carry the transport-shim env var so `git fetch origin` reaches the facilitating daemon's
        // `remote_git.RemoteGitService` over `tddy-remote-git-repo` (PRD AC37). Unset for a
        // shared-filesystem checkout, which fetches a local path.
        if let Some(ref ssh) = self.ssh_command {
            command.env("GIT_SSH_COMMAND", ssh);
        }
        let output = command
            .output()
            .await
            .map_err(|e| format!("could not run `git {}`: {e}", args.join(" ")))?;
        if !output.status.success() {
            return Err(format!(
                "`git {}` failed in {}: {}",
                args.join(" "),
                self.worktree_path.display(),
                String::from_utf8_lossy(&output.stderr).trim_end()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// Where a clone's checkout is, given the `workspace` session that holds it.
pub fn clone_worktree_path(
    sessions_base: &Path,
    codebase_session_id: &str,
) -> Result<PathBuf, Status> {
    crate::workspace_session::resolve_worktree_root_for_session(sessions_base, codebase_session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_agents_on_one_daemon_claim_one_clone() {
        // Given
        let store = SessionAgentCloneStore::new();
        let mut minted = 0;
        let mut mint = || {
            minted += 1;
            format!("clone-{minted}")
        };

        // When — explorer@ws-01 attaches, then linter@ws-01
        let (first, first_provisions) = store.claim("session-1", "ws-01", &mut mint);
        let (second, second_provisions) = store.claim("session-1", "ws-01", &mut mint);

        // Then — one checkout, and only the first attach is on the hook for building it
        assert_eq!(first, second);
        assert!(first_provisions);
        assert!(!second_provisions);
    }

    #[test]
    fn each_owning_daemon_claims_its_own_clone() {
        // Given
        let store = SessionAgentCloneStore::new();
        let mut minted = 0;
        let mut mint = || {
            minted += 1;
            format!("clone-{minted}")
        };

        // When
        let (on_b, _) = store.claim("session-1", "ws-01", &mut mint);
        let (on_c, _) = store.claim("session-1", "ws-02", &mut mint);

        // Then — sharing a checkout across hosts is not expressible, and an entry recording the
        // wrong one would have a tool call read another host's tree
        assert_ne!(on_b, on_c);
    }

    #[test]
    fn a_report_about_a_checkout_nobody_asked_for_is_refused() {
        // Given a clone this daemon did ask for
        let store = SessionAgentCloneStore::new();
        let (asked_for, _) = store.claim("session-1", "ws-01", || "clone-1".to_string());

        // When a report names a different checkout
        let refused = store.record_report(&AgentCloneReport {
            session_id: "session-1".to_string(),
            daemon_instance_id: "ws-01".to_string(),
            codebase_session_id: "some-other-checkout".to_string(),
            state: AgentCloneState::Ready,
            error: String::new(),
            worktree_path: None,
            divergences: Vec::new(),
        });

        // Then — the report is what authorizes an entry to start serving prompts, so accepting one
        // about an unknown checkout would let any room participant mark an agent ready
        assert_eq!(asked_for, "clone-1");
        assert!(refused.is_err());
        assert_eq!(
            store.get("session-1", "ws-01").map(|c| c.state),
            Some(AgentCloneState::Provisioning)
        );
    }

    #[test]
    fn divergences_accumulate_across_reports() {
        // Given
        let store = SessionAgentCloneStore::new();
        store.claim("session-1", "ws-01", || "clone-1".to_string());

        // When the owning daemon reports two reconciles, one per report
        for reason in ["README.md changed", "src/main.rs changed"] {
            store
                .record_report(&AgentCloneReport {
                    session_id: "session-1".to_string(),
                    daemon_instance_id: "ws-01".to_string(),
                    codebase_session_id: "clone-1".to_string(),
                    state: AgentCloneState::Ready,
                    error: String::new(),
                    worktree_path: None,
                    divergences: vec![reason.to_string()],
                })
                .expect("a report about the clone this daemon asked for");
        }

        // Then — replacing rather than appending would hide the first fault one report later
        assert_eq!(
            store.get("session-1", "ws-01").map(|c| c.divergences.len()),
            Some(2)
        );
    }
}
