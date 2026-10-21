//! The sync loop: what the syncer does with each broadcast, and the git it runs to stay equal.
//!
//! Product contract: `docs/ft/daemon/session-worktree-sync.md` § Client — the managed mirror
//! (AC27-AC33).
//!
//! Three things happen here and nothing else does:
//!
//! | Signal | What follows |
//! |---|---|
//! | first attach, on an empty destination | check out the project, then restore the session's uncommitted state from its WIP ref |
//! | a `session.activity` record naming a fresh tick | fetch that tick's patch and hand it to the [`Mirror`] |
//! | a `worktree.activity` `commit` | restore from the WIP ref, whose parent is the new `HEAD` |
//!
//! …plus the one that is not a signal at all: a [`ApplyOutcome::NeedsReconcile`] restores from the
//! WIP ref too, and logs at `error` what diverged (AC31, AC32).
//!
//! Everything decidable without I/O is a pure function over injected values — which scope to ask
//! for, what a stream of frames reassembles to, whether a record is worth fetching, and the exact
//! argv of every git command. A live room decides none of it, so none of it needs one to be tested.

pub(crate) mod git;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use prost::Message as _;
use tddy_service::proto::activity::{
    AgentActivityDeltaChunk, AgentActivityDeltaRequest, AgentActivityRecord, DeltaScope,
};
use tddy_service::proto::worktree_activity::{WorktreeActivityEvent, WorktreeActivityKind};

use crate::apply::{ApplyOutcome, Delta};
use crate::attach::{AttachedSession, SessionAddress};
use crate::credentials::Credentials;
use crate::mirror::{Mirror, MirrorError, MirrorMarker};

/// The service the delta stream is served by, in the session's room.
/// The coordinate `StreamAgentActivityDelta` is served at, read from `tddy-service` so this
/// subscriber and the daemon serving it cannot disagree about the name.
const ACTIVITY_SERVICE: &str = tddy_service::session_activity::ACTIVITY_SERVICE;

/// The method this subscriber calls to read a session's deltas.
///
/// `pub` because the agent-clone mirror in `tddy-session-agents` calls the same method against the
/// same service, and a second spelling of it there fails as `not_found` at runtime rather than at
/// compile time.
pub const STREAM_DELTA_METHOD: &str = "StreamAgentActivityDelta";

/// The git transport that already exists: `GIT_SSH_COMMAND` looked up on `PATH`, exactly as
/// `git clone udoo-1780828020298:my-app` uses it. See `packages/tddy-remote-git-repo/README.md`.
pub const REMOTE_GIT_SSH_COMMAND: &str = "tddy-remote-git-repo";

/// Where a fetched WIP ref lands locally. Under `refs/tddy/` rather than `refs/heads/` for the
/// reason the daemon publishes it there: it is not a branch, must never appear in `git branch`, and
/// is nothing the reader of this mirror should have to reason about.
pub const LOCAL_WIP_REF: &str = "refs/tddy/wip";

/// The remote the mirror fetches from. Named `origin` because that is what a clone would have
/// called it, and a developer who opens the mirror in their own tools should find nothing exotic.
const ORIGIN: &str = "origin";

/// Which slice of a tick the mirror asks for — the **whole tick**, not the calling record's own
/// scope.
///
/// This is forced by the mirror's apply model rather than chosen. [`Mirror::apply`] de-duplicates
/// by tick `seq` (AC29: several calls sharing one poll window apply once), so exactly one patch per
/// tick ever reaches the working tree. Asking for [`DeltaScope::Call`] would deliver the first call
/// of a window and then silently drop every later one, because their deltas carry that same `seq`
/// and come back `AlreadyApplied`; asking for [`DeltaScope::Residual`] afterwards would be dropped
/// the same way, taking with it exactly the undeclared change that scope exists to rescue.
///
/// [`DeltaScope::Tick`] is by definition the union of every call's scope and the residual, so one
/// fetch per tick carries precisely what the mirror must write — no more, and nothing missing. The
/// narrower scopes are for a consumer that renders *what one call did*; this one reconstructs a
/// worktree.
pub const MIRROR_DELTA_SCOPE: DeltaScope = DeltaScope::Tick;

pub use crate::sync::git::{
    first_attach_commands, reconcile_commands, wip_ref, GitInvocation, GitTransport,
};

/// What the syncer does about one activity record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordDecision {
    /// Fetch this tick's patch, addressed by the call that reported it.
    FetchDelta { call_id: String, seq: u64 },
    /// Nothing to fetch, and why — every one of these is logged rather than passed over in silence.
    Ignore(IgnoreReason),
}

/// Why a record produced no fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IgnoreReason {
    /// `activity_seq` is 0: no poll tick has measured this call yet, so its patch does not exist.
    /// The tick that covers it will be reported by a later record.
    NoTickYet,
    /// The mirror is already at or past this tick — the ordinary case for the second, third and
    /// fourth call of one poll window.
    AlreadyApplied { seq: u64, last_seq: u64 },
    /// The record carries no `call_id`, and a delta request has nothing else to address.
    Unaddressable { seq: u64 },
}

impl std::fmt::Display for IgnoreReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IgnoreReason::NoTickYet => {
                write!(f, "no poll tick has measured this call yet")
            }
            IgnoreReason::AlreadyApplied { seq, last_seq } => write!(
                f,
                "tick {seq} is already applied; the mirror is at {last_seq}"
            ),
            IgnoreReason::Unaddressable { seq } => write!(
                f,
                "the record for tick {seq} carries no call_id, so its delta cannot be addressed"
            ),
        }
    }
}

/// Whether a record's tick is one the mirror still needs.
///
/// Deliberately not filtered by `tool_name`: a whitelist of editing tools is a list that goes out
/// of date, and a tool missing from it is a change that reaches the mirror never. What decides is
/// the tick — if the mirror has not applied it, it needs it, whatever ran.
pub fn decide_record(record: &AgentActivityRecord, last_seq: u64) -> RecordDecision {
    // `NO_TICK`, not the literal 0, and the two are now genuinely different facts: `#unbundle`
    // node 7 numbers a session's deltas from `FIRST_TICK` (1), so this arm means "no tick has
    // covered this call yet" and nothing else. On the coordinate before it, a first delta was also
    // numbered 0 and this arm silently discarded it.
    if record.activity_seq == tddy_service::session_activity::NO_TICK {
        return RecordDecision::Ignore(IgnoreReason::NoTickYet);
    }
    if record.activity_seq <= last_seq {
        return RecordDecision::Ignore(IgnoreReason::AlreadyApplied {
            seq: record.activity_seq,
            last_seq,
        });
    }
    if record.call_id.is_empty() {
        return RecordDecision::Ignore(IgnoreReason::Unaddressable {
            seq: record.activity_seq,
        });
    }
    RecordDecision::FetchDelta {
        call_id: record.call_id.clone(),
        seq: record.activity_seq,
    }
}

/// What the syncer does about one worktree-activity event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeDecision {
    /// The checkout's `HEAD` moved; restore from git (AC28).
    Reconcile,
    /// Nothing to do: the event carries no paths and no content, and whatever it reports is
    /// delivered as a delta by the tick that measured it.
    Ignore,
}

pub fn decide_worktree(event: &WorktreeActivityEvent) -> WorktreeDecision {
    match event.kind() {
        WorktreeActivityKind::Commit => WorktreeDecision::Reconcile,
        WorktreeActivityKind::FilesChanged | WorktreeActivityKind::Unspecified => {
            WorktreeDecision::Ignore
        }
    }
}

/// The request that fetches one tick's patch.
pub fn delta_request(
    address: &SessionAddress,
    session_token: &str,
    call_id: &str,
) -> AgentActivityDeltaRequest {
    AgentActivityDeltaRequest {
        session_token: session_token.to_string(),
        session_id: address.session_id.clone(),
        // Routed explicitly rather than left empty: the daemon in the room is the one that owns
        // the session, but saying so costs nothing and a silently mis-routed delta is a mirror
        // built from another host's worktree.
        daemon_instance_id: address.daemon_instance_id.clone(),
        call_id: call_id.to_string(),
        scope: MIRROR_DELTA_SCOPE as i32,
    }
}

/// Why a stream of frames is not a patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaError {
    /// The stream ended without a single frame. Distinct from a patch of zero bytes, which is one
    /// frame carrying nothing — that is how "the call changed nothing" is said (AC9), and reading
    /// a failed stream as it would be a mirror that quietly skipped a tick.
    NoFrames,
    /// The frames disagree about what they are describing. Every describing field repeats on every
    /// frame precisely so this is detectable rather than resolved by trusting the first one.
    Inconsistent {
        field: &'static str,
        first: String,
        later: String,
    },
    /// Fewer bytes arrived than the frames said the patch has. `git apply` would reject the tail
    /// half of a hunk anyway; this says why, at the point the bytes went missing.
    Truncated { declared: u64, received: usize },
}

impl std::fmt::Display for DeltaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeltaError::NoFrames => write!(
                f,
                "the delta stream ended without a frame; a patch of no bytes is one empty frame, \
                 not none"
            ),
            DeltaError::Inconsistent {
                field,
                first,
                later,
            } => write!(
                f,
                "the delta's frames disagree about {field}: the first said {first}, a later one \
                 said {later}"
            ),
            DeltaError::Truncated { declared, received } => write!(
                f,
                "the delta declared {declared} bytes and {received} arrived"
            ),
        }
    }
}

impl std::error::Error for DeltaError {}

/// Reassemble the frames of one `StreamAgentActivityDelta` call into the patch they carry.
pub fn reassemble(chunks: &[AgentActivityDeltaChunk]) -> Result<Delta, DeltaError> {
    let Some(first) = chunks.first() else {
        return Err(DeltaError::NoFrames);
    };

    for later in &chunks[1..] {
        for (field, first, later) in [
            ("seq", first.seq.to_string(), later.seq.to_string()),
            (
                "prev_seq",
                first.prev_seq.to_string(),
                later.prev_seq.to_string(),
            ),
            (
                "base_commit",
                first.base_commit.clone(),
                later.base_commit.clone(),
            ),
            (
                "total_byte_size",
                first.total_byte_size.to_string(),
                later.total_byte_size.to_string(),
            ),
        ] {
            if first != later {
                return Err(DeltaError::Inconsistent {
                    field,
                    first,
                    later,
                });
            }
        }
    }

    let patch: Vec<u8> = chunks
        .iter()
        .flat_map(|chunk| chunk.patch.iter().copied())
        .collect();
    if patch.len() as u64 != first.total_byte_size {
        return Err(DeltaError::Truncated {
            declared: first.total_byte_size,
            received: patch.len(),
        });
    }

    Ok(Delta {
        seq: first.seq,
        prev_seq: first.prev_seq,
        base_commit: first.base_commit.clone(),
        patch,
        scoped_paths: first.scoped_paths.clone(),
    })
}

/// Why a sync could not continue.
///
/// Every one of these ends the run with a non-zero exit (AC32). A syncer that carried on past any
/// of them would be reporting success over a mirror it had stopped being able to keep equal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncError {
    Mirror(MirrorError),
    /// A git command failed. Carries git's own stderr, unabridged.
    Git {
        command: String,
        stderr: String,
    },
    /// The command could not be run at all.
    GitUnavailable {
        command: String,
        reason: String,
    },
    /// The daemon refused the delta stream, or it failed mid-flight.
    Rpc {
        method: &'static str,
        code: String,
        message: String,
    },
    /// The daemon accepted the call and then said nothing for the connect budget. A chunk-framed
    /// message that loses a frame wedges with no error at all, so silence is given a deadline.
    Silent {
        method: &'static str,
        after: Duration,
    },
    /// A broadcast arrived that is not the message its topic carries.
    Undecodable {
        topic: &'static str,
        reason: String,
    },
    Delta(DeltaError),
    /// The room's event stream ended: the room closed, or the connection dropped. Rooms are not
    /// re-opened when a daemon restarts, so this exits rather than waiting for a room that will
    /// not come back.
    RoomClosed,
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::Mirror(e) => write!(f, "{e}"),
            SyncError::Git { command, stderr } => write!(f, "`{command}` failed: {stderr}"),
            SyncError::GitUnavailable { command, reason } => {
                write!(f, "could not run `{command}`: {reason}")
            }
            SyncError::Rpc {
                method,
                code,
                message,
            } => write!(f, "the daemon refused {method} ({code}): {message}"),
            SyncError::Silent { method, after } => write!(
                f,
                "the daemon sent nothing on {method} for {}s and never ended the stream",
                after.as_secs()
            ),
            SyncError::Undecodable { topic, reason } => write!(
                f,
                "a broadcast on \"{topic}\" could not be read as what that topic carries: {reason}"
            ),
            SyncError::Delta(e) => write!(f, "{e}"),
            SyncError::RoomClosed => write!(
                f,
                "the session's room closed; the mirror can no longer be kept equal to it"
            ),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<MirrorError> for SyncError {
    fn from(e: MirrorError) -> Self {
        SyncError::Mirror(e)
    }
}

impl From<DeltaError> for SyncError {
    fn from(e: DeltaError) -> Self {
        SyncError::Delta(e)
    }
}

/// Mirror the session until its room closes or something goes wrong.
///
/// Returns only on failure: a session being mirrored has no completion, so every exit from here is
/// an error and the process exits non-zero on all of them.
pub async fn run(credentials: &Credentials, attached: AttachedSession) -> Result<(), SyncError> {
    let AttachedSession {
        address,
        session_token,
        connection,
        mut session_activity,
        mut worktree_activity,
    } = attached;
    // Held for the whole run. Dropping this leaves the room, and a syncer outside the room hears
    // nothing further while failing at nothing.
    let _room = Arc::clone(&connection);

    let mut syncer = Syncer::open(credentials, address, session_token, connection.rpc())?;
    syncer.first_attach_if_empty().await?;

    loop {
        tokio::select! {
            message = session_activity.recv() => match message {
                Some(message) => syncer.on_activity(&message).await?,
                None => return Err(SyncError::RoomClosed),
            },
            message = worktree_activity.recv() => match message {
                Some(message) => syncer.on_worktree(&message).await?,
                None => return Err(SyncError::RoomClosed),
            },
        }
    }
}

/// The loop's state: the mirror, the client that fetches deltas for it, and what both are addressed
/// by. Private because it cannot be built without a joined room — everything in it that can be
/// decided without one is a free function above.
struct Syncer {
    address: SessionAddress,
    session_token: String,
    client: Arc<dyn tddy_rpc::RpcClientTransport>,
    mirror: Mirror,
    dest: PathBuf,
    transport: GitTransport,
    /// How long the daemon may say nothing on a stream before it is declared gone.
    rpc_timeout: Duration,
}

impl Syncer {
    fn open(
        credentials: &Credentials,
        address: SessionAddress,
        session_token: String,
        client: Arc<dyn tddy_rpc::RpcClientTransport>,
    ) -> Result<Self, SyncError> {
        // Ownership is settled before a single byte is written: an unmarked non-empty destination
        // and one marked for another session are both refused here (AC26).
        let mirror = Mirror::open_or_create(
            &credentials.dest,
            MirrorMarker {
                session_id: address.session_id.clone(),
                daemon_instance_id: address.daemon_instance_id.clone(),
                project: address.project_id.clone(),
                last_seq: 0,
                last_head_commit: String::new(),
            },
        )?;
        Ok(Self {
            address,
            session_token,
            client,
            mirror,
            dest: credentials.dest.clone(),
            transport: GitTransport {
                daemon_url: credentials.daemon_url.clone(),
                token: credentials.token.clone(),
            },
            rpc_timeout: credentials.connect_timeout,
        })
    }

    /// Check out the project when the destination holds no repository yet (AC27).
    ///
    /// The presence of a repository is the question being asked, so `.git` is what answers it — the
    /// marker records what the syncer believes, and a run interrupted between the two would be a
    /// belief the directory does not support.
    async fn first_attach_if_empty(&mut self) -> Result<(), SyncError> {
        if self.dest.join(".git").exists() {
            return Ok(());
        }
        log::info!(
            "first attach: checking out {} from {}",
            self.address.project_id,
            self.address.daemon_instance_id
        );
        self.run_all(&first_attach_commands(&self.address, &self.transport))
            .await?;
        // Whatever tick the fetched WIP ref belongs to, it is not one this mirror applied a patch
        // for, so the applied sequence stays where it is.
        let seq = self.mirror.marker().last_seq;
        self.mirror.record_restored(seq)?;
        Ok(())
    }

    async fn on_activity(&mut self, payload: &[u8]) -> Result<(), SyncError> {
        let record = AgentActivityRecord::decode(payload).map_err(|e| SyncError::Undecodable {
            topic: tddy_service::session_activity::SESSION_ACTIVITY_TOPIC,
            reason: e.to_string(),
        })?;

        let (call_id, seq) = match decide_record(&record, self.mirror.marker().last_seq) {
            RecordDecision::FetchDelta { call_id, seq } => (call_id, seq),
            RecordDecision::Ignore(reason) => {
                log::debug!("{} ({}): {reason}", record.tool_name, record.call_id);
                return Ok(());
            }
        };

        let delta = self.fetch_delta(&call_id).await?;
        match self.mirror.apply(&delta)? {
            ApplyOutcome::Applied => {
                log::info!(
                    "applied tick {seq} ({} bytes) from {} ({call_id})",
                    delta.patch.len(),
                    record.tool_name
                );
                Ok(())
            }
            ApplyOutcome::AlreadyApplied => {
                log::debug!("tick {seq} was already applied");
                Ok(())
            }
            // AC32: named at `error`, with what diverged, before anything is done about it.
            ApplyOutcome::NeedsReconcile(reason) => {
                log::error!("mirror diverged at tick {seq}: {reason}");
                self.reconcile(seq).await
            }
        }
    }

    async fn on_worktree(&mut self, payload: &[u8]) -> Result<(), SyncError> {
        let event = WorktreeActivityEvent::decode(payload).map_err(|e| SyncError::Undecodable {
            topic: tddy_service::worktree_activity::WORKTREE_ACTIVITY_TOPIC,
            reason: e.to_string(),
        })?;

        match decide_worktree(&event) {
            WorktreeDecision::Reconcile => {
                log::info!(
                    "the session committed {}; restoring the mirror from its WIP ref",
                    event.head_commit
                );
                // The commit moved `HEAD`, not the applied tick, so the sequence stays where it is
                // — claiming a tick this restore did not demonstrably include would skip the next
                // delta and drift with nothing reporting it.
                let seq = self.mirror.marker().last_seq;
                self.reconcile(seq).await
            }
            WorktreeDecision::Ignore => {
                log::debug!(
                    "worktree activity seq {} carries no content to mirror",
                    event.seq
                );
                Ok(())
            }
        }
    }

    /// Restore the mirror from git, and record how far the restore is *known* to reach.
    ///
    /// `applied_seq` is never more than the tick that provoked the restore. The WIP ref may well be
    /// newer than that, and claiming so would be the one mistake with no loud failure attached: a
    /// sequence recorded too high skips the delta that follows, and the mirror drifts silently. Too
    /// low costs one more reconcile, which is reported.
    ///
    /// TODO(session-worktree-sync): AC31's second half — a path git cannot reconstruct, because the
    /// session's `.gitignore` excludes it and it is therefore in no tree, is meant to be pulled
    /// whole with `StreamReadWorktreeFile`. That RPC answers `unimplemented` today, so nothing
    /// calls it yet and such a path is simply absent from the mirror rather than wrongly present.
    async fn reconcile(&mut self, applied_seq: u64) -> Result<(), SyncError> {
        self.run_all(&reconcile_commands(
            &self.address.session_id,
            &self.transport,
        ))
        .await?;
        self.mirror.record_restored(applied_seq)?;
        log::info!(
            "restored the mirror from {}",
            wip_ref(&self.address.session_id)
        );
        Ok(())
    }

    /// Fetch one tick's patch, frame by frame, and reassemble it.
    async fn fetch_delta(&self, call_id: &str) -> Result<Delta, SyncError> {
        let request = delta_request(&self.address, &self.session_token, call_id);
        let mut frames = self
            .client
            .call_server_stream(
                ACTIVITY_SERVICE,
                STREAM_DELTA_METHOD,
                request.encode_to_vec(),
            )
            .await
            .map_err(|status| SyncError::Rpc {
                method: STREAM_DELTA_METHOD,
                code: status.code.as_str().to_string(),
                message: status.message,
            })?;

        let mut chunks = Vec::new();
        loop {
            // A deadline on *silence*, re-armed by every frame. `Receiver::recv` is cancel-safe, so
            // nothing is lost by re-arming it; without it a stream that lost a chunk frame waits
            // forever and reports nothing, which is the one failure mode this transport has.
            match tokio::time::timeout(self.rpc_timeout, frames.recv()).await {
                Ok(Some(Ok(bytes))) => {
                    chunks.push(AgentActivityDeltaChunk::decode(&bytes[..]).map_err(|e| {
                        SyncError::Rpc {
                            method: STREAM_DELTA_METHOD,
                            code: "malformed".to_string(),
                            message: e.to_string(),
                        }
                    })?)
                }
                Ok(Some(Err(status))) => {
                    return Err(SyncError::Rpc {
                        method: STREAM_DELTA_METHOD,
                        code: status.code.as_str().to_string(),
                        message: status.message,
                    })
                }
                Ok(None) => break,
                Err(_) => {
                    return Err(SyncError::Silent {
                        method: STREAM_DELTA_METHOD,
                        after: self.rpc_timeout,
                    })
                }
            }
        }
        Ok(reassemble(&chunks)?)
    }

    async fn run_all(&self, commands: &[GitInvocation]) -> Result<(), SyncError> {
        for command in commands {
            run_git(&self.dest, command).await?;
        }
        Ok(())
    }
}

/// Run one git command in the mirror.
///
/// Spawned on the runtime rather than blocked on: a first attach fetches a whole repository, which
/// takes as long as it takes, and a blocked runtime thread is one the broadcast subscriptions share.
///
/// The inherited environment is kept and the invocation's variables are added to it: `git` needs
/// `PATH` to find `tddy-remote-git-repo` at all, and a cleared environment is how a transport that
/// works from a shell stops working from the same shell's child.
async fn run_git(dest: &Path, invocation: &GitInvocation) -> Result<(), SyncError> {
    let mut command = tokio::process::Command::new("git");
    command.args(&invocation.args).current_dir(dest);
    for (key, value) in &invocation.env {
        command.env(key, value);
    }
    let output = command
        .output()
        .await
        .map_err(|e| SyncError::GitUnavailable {
            command: invocation.command_line(),
            reason: e.to_string(),
        })?;
    if !output.status.success() {
        return Err(SyncError::Git {
            command: invocation.command_line(),
            stderr: String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_string(),
        });
    }
    Ok(())
}
