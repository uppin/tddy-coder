//! What a session room is called, what it knows about its checkout, when it says so — and the
//! task that hosts it.
//!
//! Product contract: `docs/ft/daemon/session-room.md`; module docs:
//! `packages/tddy-daemon/docs/session-room.md`.
//!
//! Naming and the snapshot→event rules are pure, and snapshotting is `git` against a checkout on
//! disk, so the room's lifecycle at the bottom of this file is built on a layer that is already
//! pinned without it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use log::warn;
use prost::Message as _;
use tddy_livekit::{
    BroadcastPublisher, JoinedParticipant, LiveKitParticipant, RoomMetadataClient, RpcService,
    TokenGenerator,
};
use tddy_rpc::Status;

use crate::config::DaemonConfig;
use crate::livekit_peer_discovery::daemon_rpc_identity;
use crate::worktrees::{parse_git_diff_numstat, WorktreeNumstat};

/// The data-channel topic worktree activity is broadcast on, re-exported from where its payload's
/// schema lives: `tddy-tools` receives on the same topic and reaches `tddy-service`
/// unconditionally, while its LiveKit dependency is feature-gated.
pub use tddy_service::worktree_activity::WORKTREE_ACTIVITY_TOPIC;

/// The broadcast wire types, re-exported so a caller reaches the topic's payload and the module that
/// produces it through one import.
pub use tddy_service::proto::worktree_activity::{WorktreeActivityEvent, WorktreeActivityKind};

/// The name of the room that belongs to the checkout owned by `codebase_session_id`.
///
/// Both the daemon holding the worktree and the daemon running the agent derive this from an id they
/// already exchange, so the room name never has to travel as a field of its own — and the two can
/// never disagree about it.
pub fn session_room_name(codebase_session_id: &str) -> String {
    format!("session-{codebase_session_id}")
}

/// Everything one poll of a checkout observed.
///
/// `changed_paths` is carried here and published in room metadata but never in an event: an event
/// says *that* the checkout moved, and reading it is what the file-access RPCs in the same room are
/// for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorktreeSnapshot {
    pub head_commit: String,
    pub branch: String,
    pub changed_paths: Vec<String>,
    pub changed_files: u32,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub untracked_files: u32,
}

impl WorktreeSnapshot {
    /// True when the two snapshots disagree about the **tracked** diff — the paths and their line
    /// counts, everything `git diff --numstat HEAD` measures.
    ///
    /// HEAD is deliberately excluded: it is the other half of the comparison and gets its own event.
    ///
    /// So is `untracked_files`, for two reasons. An event carries counts only, and a HEAD-relative
    /// diff has no line counts for a path git does not track — so a newly written untracked file
    /// could only be announced as `files=0 +0 -0`, which tells a receiver nothing it can act on.
    /// And every write is untracked for the moment between the file appearing and `git add`
    /// staging it, so treating that as activity would put an empty event in front of the commit
    /// event that actually describes the work. The fact is not lost: it lives in the room's
    /// metadata, which [`WorktreeSnapshot`] equality still covers, so a joiner reads the untracked
    /// count from state rather than from a notification with nothing in it.
    fn tracked_diff_differs(&self, other: &Self) -> bool {
        self.changed_paths != other.changed_paths
            || self.changed_files != other.changed_files
            || self.lines_added != other.lines_added
            || self.lines_removed != other.lines_removed
    }
}

/// Measure `worktree_root` as it stands right now, within [`DEFAULT_GIT_TIMEOUT`].
///
/// The counts come from the same `git diff --numstat HEAD` the Worktrees screen reads, parsed by
/// the same [`parse_git_diff_numstat`], so a room and the screen can never quote different totals
/// for one checkout. Untracked files are counted and nothing more: a HEAD-relative diff cannot
/// produce line counts for a path git does not track, and inventing them is exactly how the two
/// would diverge.
///
/// A checkout git cannot read snapshots as empty rather than failing — this feeds a periodic poll
/// whose only recourse is to try again on the next tick.
pub fn snapshot_worktree(worktree_root: &Path) -> WorktreeSnapshot {
    snapshot_worktree_within(worktree_root, DEFAULT_GIT_TIMEOUT)
}

/// [`snapshot_worktree`] with the whole measurement — every `git` it runs — bounded by `budget`.
///
/// The budget is a deadline over the sequence, not a per-command allowance, so a repository that
/// answers three commands slowly cannot spend three budgets: what the poll loop waits for is what
/// the operator configured (`session_room.git_timeout_ms`), once.
pub fn snapshot_worktree_within(worktree_root: &Path, budget: Duration) -> WorktreeSnapshot {
    let deadline = Instant::now() + budget;
    let numstat = numstat_within(worktree_root, deadline);
    WorktreeSnapshot {
        head_commit: git_stdout(worktree_root, &["rev-parse", "HEAD"], deadline),
        branch: git_stdout(
            worktree_root,
            &["rev-parse", "--abbrev-ref", "HEAD"],
            deadline,
        ),
        changed_paths: numstat.paths,
        changed_files: numstat.changed_files,
        lines_added: numstat.lines_added,
        lines_removed: numstat.lines_removed,
        untracked_files: untracked_file_count(worktree_root, deadline),
    }
}

/// The working tree's diff against HEAD, run and parsed exactly as [`crate::worktrees`] does it for
/// the Worktrees screen — only under this module's deadline.
fn numstat_within(worktree_root: &Path, deadline: Instant) -> WorktreeNumstat {
    parse_git_diff_numstat(&git_stdout(
        worktree_root,
        &["diff", "--numstat", "HEAD"],
        deadline,
    ))
}

/// What has to be announced to turn `prev` into `next`, numbered from `next_seq`.
///
/// Identical snapshots announce nothing. Polling is how change is detected here, so a poll that
/// spoke about an idle checkout would flood the room at the poll rate.
///
/// A commit that also cleared the working tree is two facts and gets two events, in that order: a
/// receiver that understood only one kind would otherwise silently miss the other. The branch name
/// is not among them — it is state, carried in room metadata, and moving a checkout onto another
/// branch is only ever interesting through the HEAD it lands on. Neither is the untracked count;
/// see [`WorktreeSnapshot::tracked_diff_differs`].
pub fn activity_between(
    prev: &WorktreeSnapshot,
    next: &WorktreeSnapshot,
    next_seq: u64,
    at_unix_ms: u64,
) -> Vec<WorktreeActivityEvent> {
    let mut events = Vec::new();
    let mut seq = next_seq;
    if prev.head_commit != next.head_commit {
        events.push(activity_event(
            WorktreeActivityKind::Commit,
            next,
            seq,
            at_unix_ms,
        ));
        seq += 1;
    }
    if prev.tracked_diff_differs(next) {
        events.push(activity_event(
            WorktreeActivityKind::FilesChanged,
            next,
            seq,
            at_unix_ms,
        ));
    }
    events
}

/// How many changed paths the room's metadata carries before it stops listing them.
///
/// Room metadata is a single string field on the LiveKit server, which rejects an oversized write —
/// and the poll loop holds a change back until the write lands, so an unbounded list would not
/// merely be big, it would wedge the room: the same rejected write retried at the poll rate for as
/// long as the checkout stays that dirty. A branch rebase or a generated-file sweep reaches
/// thousands of paths easily. The counts beside the list are the true totals either way; the list
/// is what a joiner reads to know *which* files, and a reader that needs more than this many names
/// wants the file-access RPCs in the same room, not a bigger metadata blob.
pub const MAX_METADATA_CHANGED_PATHS: usize = 200;

/// The room's metadata: the whole current picture, so a participant joining mid-stream has it
/// without a round trip and without having observed a single event.
///
/// JSON, unlike the events beside it, because this is a LiveKit string field browsers read as well
/// as daemons, and a snapshot rather than a message on a schema-versioned channel.
///
/// `changed_paths` is capped at [`MAX_METADATA_CHANGED_PATHS`]; when it was cut short the object
/// also carries `changed_paths_truncated: true`, so a reader can tell "these are all of them" from
/// "these are the first 200" without comparing the list's length against `changed_files` and
/// guessing. The key is absent when nothing was dropped, so the common case reads as the plain
/// object it always was.
pub fn room_metadata_json(
    snapshot: &WorktreeSnapshot,
    attachments: &[String],
    at_unix_ms: u64,
) -> String {
    let truncated = snapshot.changed_paths.len() > MAX_METADATA_CHANGED_PATHS;
    let mut metadata = serde_json::json!({
        "head_commit": snapshot.head_commit,
        "branch": snapshot.branch,
        "changed_paths": &snapshot.changed_paths[..snapshot.changed_paths.len().min(MAX_METADATA_CHANGED_PATHS)],
        "changed_files": snapshot.changed_files,
        "lines_added": snapshot.lines_added,
        "lines_removed": snapshot.lines_removed,
        "untracked_files": snapshot.untracked_files,
        "attachments": attachments,
        "updated_at_unix_ms": at_unix_ms,
    });
    if truncated {
        if let Some(object) = metadata.as_object_mut() {
            object.insert("changed_paths_truncated".to_string(), true.into());
        }
    }
    metadata.to_string()
}

/// One event about `snapshot`, of `kind`. Every kind carries the same measurement — the state the
/// checkout is in now — so a receiver never has to know which fields a given kind bothered to fill.
fn activity_event(
    kind: WorktreeActivityKind,
    snapshot: &WorktreeSnapshot,
    seq: u64,
    at_unix_ms: u64,
) -> WorktreeActivityEvent {
    WorktreeActivityEvent {
        kind: kind as i32,
        seq,
        at_unix_ms,
        head_commit: snapshot.head_commit.clone(),
        changed_files: snapshot.changed_files,
        lines_added: snapshot.lines_added,
        lines_removed: snapshot.lines_removed,
    }
}

/// How many files in the checkout git has never been told about: the `??` entries of
/// `git status --porcelain`, and only those. Every other status line describes a path the numstat
/// diff has already accounted for.
fn untracked_file_count(worktree_root: &Path, deadline: Instant) -> u32 {
    git_stdout(worktree_root, &["status", "--porcelain"], deadline)
        .lines()
        .filter(|line| line.starts_with("??"))
        .count() as u32
}

/// The budget one measurement of a checkout gets when the caller names none: the shipped
/// `session_room.git_timeout_ms`, which [`crate::config`] reads from here so the two cannot drift.
pub const DEFAULT_GIT_TIMEOUT: Duration = Duration::from_millis(5_000);

/// Trimmed stdout of a git command in `worktree_root`, or the empty string when git could not
/// answer before `deadline`.
///
/// The child is **killed** on expiry rather than abandoned. This runs on a blocking-pool thread, and
/// a `git` that never returns — the stale `index.lock` case, or a network filesystem that stopped
/// answering — would otherwise cost one live process and one occupied pool thread *per poll*,
/// forever: at a two-second interval that is a room quietly eating a process-wide pool (512 threads
/// by default) that the spawn worker and every other blocking caller shares.
fn git_stdout(worktree_root: &Path, args: &[&str], deadline: Instant) -> String {
    let budget = deadline.saturating_duration_since(Instant::now());
    if budget.is_zero() {
        warn!(
            "snapshot_worktree: no time left for git {args:?} in {worktree_root:?}; the measurement is incomplete"
        );
        return String::new();
    }
    let mut child = match Command::new("git")
        .current_dir(worktree_root)
        .args(args)
        // This measures a checkout somebody is *working in*, several times a second. `git status`
        // and `git diff` refresh the index as a side effect, which takes `index.lock` — the same
        // lock the developer's own `git add`, `git commit` or rebase needs, and which only one
        // process can hold. Polling would then both lose its own measurement to whoever holds it
        // and fail the developer's git for as long as this one runs. `GIT_OPTIONAL_LOCKS=0` is
        // git's own answer for read-only observers: skip the refresh rather than take the lock.
        .env("GIT_OPTIONAL_LOCKS", "0")
        // A git that decides to ask for credentials would sit on a prompt no one can answer until
        // the kill below; closed stdin makes it fail instead.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            warn!("snapshot_worktree: git {args:?} in {worktree_root:?} could not be run: {e}");
            return String::new();
        }
    };

    // Drained on its own thread, and the thread's completion is what the deadline is measured
    // against: git blocks once it has written ~64 KB into an unread pipe, so a caller that watched
    // only the exit status would time out and kill a healthy `git diff` over a large change set.
    // EOF here means git closed stdout, which it does as it exits.
    let Some(stdout) = child.stdout.take() else {
        warn!("snapshot_worktree: git {args:?} in {worktree_root:?} produced no stdout pipe");
        return String::new();
    };
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Bytes, decoded lossily below: a repository can hold a path that is not valid UTF-8, and
        // losing the whole measurement over one such filename would be worse than showing it with
        // replacement characters.
        let mut collected = Vec::new();
        let read = std::io::Read::read_to_end(&mut std::io::BufReader::new(stdout), &mut collected);
        let _ = finished_tx.send(read.map(|_| collected));
    });

    match finished_rx.recv_timeout(budget) {
        Ok(Ok(collected)) => match child.wait() {
            Ok(status) if status.success() => {
                String::from_utf8_lossy(&collected).trim().to_string()
            }
            Ok(status) => {
                warn!("snapshot_worktree: git {args:?} in {worktree_root:?} exited {status}");
                String::new()
            }
            Err(e) => {
                warn!("snapshot_worktree: git {args:?} in {worktree_root:?} could not be waited on: {e}");
                String::new()
            }
        },
        Ok(Err(e)) => {
            warn!("snapshot_worktree: reading git {args:?} in {worktree_root:?} failed: {e}");
            String::new()
        }
        Err(_) => {
            // Killed *and* reaped: a `git` left running holds an index lock and a file handle on a
            // directory the daemon may be about to remove, and an unreaped one stays a zombie for
            // the life of the daemon.
            let _ = child.kill();
            let _ = child.wait();
            warn!(
                "snapshot_worktree: git {args:?} in {worktree_root:?} did not answer within {}ms and was killed",
                budget.as_millis()
            );
            String::new()
        }
    }
}

// ---------------------------------------------------------------------------
// Room lifecycle
// ---------------------------------------------------------------------------

/// Lifetime of the join token the facilitating daemon holds its own session room with.
///
/// Matches the split agent's token (`split_session::SPLIT_AGENT_TOKEN_TTL`), for the same reason:
/// long enough that a working day never re-authenticates, short enough that a leaked token expires.
/// The LiveKit SDK refreshes a live connection's credentials on the signalling channel, so this
/// bounds the *join*, not how long the daemon may stay in the room.
const SESSION_ROOM_TOKEN_TTL: Duration = Duration::from_secs(86_400);

/// Opens the room of a session whose agent this daemon is about to spawn.
///
/// A trait object rather than the registry plus a generic `S: RpcService` parameter, because the
/// agent-start path is a free function already carrying twenty-odd arguments: threading the service
/// type through it would make every caller — and every one of its callers — name the daemon's
/// concrete server type for a value none of them otherwise mention.
///
/// Returning `Ok(None)` means this daemon hosts no rooms at all (no LiveKit credentials), which is
/// not an error: sessions start exactly as they did before rooms existed.
#[async_trait::async_trait]
pub trait SessionRoomHost: Send + Sync {
    async fn open_for(
        &self,
        session_id: &str,
        worktree_root: &Path,
        session_dir: &Path,
    ) -> Result<Option<OpenedSessionRoom>, Status>;
}

/// This daemon, as configured, in its capacity as a host of session rooms.
///
/// One parameter rather than three because they are one fact, and because the registry is what
/// keeps a room open after the response that announced it has gone out.
pub struct DaemonRoomHosting<'a> {
    pub config: &'a DaemonConfig,
    /// This daemon's instance id: a room's RPC surface is served on `daemon-{instance_id}`, the
    /// same identity it answers on in the common room, so a caller addresses one daemon by one name
    /// wherever it meets it.
    pub instance_id: &'a str,
    pub rooms: &'a SessionRoomRegistry,
}

impl<'a> DaemonRoomHosting<'a> {
    /// This daemon hosting the room of one particular checkout. The overlap between the two lives
    /// here, so a caller naming a checkout cannot accidentally describe a different daemon than the
    /// registry it opens the room in belongs to.
    pub fn for_worktree(
        &self,
        codebase_session_id: &'a str,
        worktree_root: &'a Path,
        session_dir: &'a Path,
    ) -> SessionRoomHosting<'a> {
        SessionRoomHosting {
            config: self.config,
            instance_id: self.instance_id,
            codebase_session_id,
            worktree_root: Some(worktree_root),
            session_dir,
        }
    }

    /// This daemon hosting the room of a session whose checkout is on another daemon.
    ///
    /// The room is still this daemon's — it runs the agent — so everything but the measurement is
    /// identical to [`Self::for_worktree`]. There is no local path to record, which is exactly why
    /// `worktree_root` is optional.
    pub fn for_remote_worktree(
        &self,
        session_id: &'a str,
        session_dir: &'a Path,
    ) -> SessionRoomHosting<'a> {
        SessionRoomHosting {
            config: self.config,
            instance_id: self.instance_id,
            codebase_session_id: session_id,
            worktree_root: None,
            session_dir,
        }
    }
}

/// The checkout a room is being opened for, and the daemon opening it. Built by
/// [`DaemonRoomHosting::for_worktree`].
pub struct SessionRoomHosting<'a> {
    pub config: &'a DaemonConfig,
    /// See [`DaemonRoomHosting::instance_id`].
    pub instance_id: &'a str,
    /// The session that owns the checkout — the room's name is derived from it.
    pub codebase_session_id: &'a str,
    /// The checkout on *this* host, when there is one. `None` under split placement, where the
    /// files live on the codebase daemon and this daemon reaches them by asking. Only logging and
    /// `close_for_worktree` read it, so a remote room simply never matches a local path — which is
    /// correct: no local path names it.
    pub worktree_root: Option<&'a Path>,
    /// The session directory, read for the attachments the room advertises in its metadata.
    pub session_dir: &'a Path,
}

/// Where a hosted room is, for the `StartSessionResponse` that announces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedSessionRoom {
    pub room: String,
    pub url: String,
    pub server_identity: String,
}

/// The live rooms this daemon hosts, keyed by the session that owns each checkout.
///
/// Owns the joined participant: a `LiveKitParticipant` that is dropped leaves the room, so a room
/// whose handle lived only as long as `StartSession` would be empty by the time the agent arrived.
///
/// TODO(session-room): re-open the rooms of surviving workspace sessions at daemon startup. The
/// registry is built empty, so a daemon restart leaves an existing checkout's room without its
/// host — and a split agent resumed against it would find no `daemon-{instance_id}` to call.
#[derive(Default)]
pub struct SessionRoomRegistry {
    rooms: Mutex<HashMap<String, SessionRoomTask>>,
}

/// One hosted room: the task serving RPC in it, and the task measuring the checkout it describes.
struct SessionRoomTask {
    /// The checkout this room describes. Kept so a caller holding only a path — `RemoveWorktree`,
    /// which never learns a session id — can close the room before the directory goes.
    /// The local checkout this room measures, when there is one; `None` under split placement.
    worktree_root: Option<PathBuf>,
    serve: tokio::task::JoinHandle<()>,
    poll: tokio::task::JoinHandle<()>,
    /// Set when hosting ends, by whichever side ended it. The poll loop reads it so it stops
    /// measuring a checkout it can no longer broadcast about, and the serving task reads it to tell
    /// "the operator closed this room" from "this daemon fell out of it".
    stopped: Arc<AtomicBool>,
}

impl Drop for SessionRoomTask {
    /// Stop hosting. Aborting the serving task drops the `LiveKitParticipant` it owns, and with it
    /// the `Room` — which is what actually takes this daemon out of the room.
    ///
    /// In `Drop` rather than in a method, because everything that ends a room's life ends by
    /// dropping this: `close`, a replacement under the same session id, and the registry itself
    /// going away. A task pair that outlived its entry would hold a LiveKit connection open and
    /// keep shelling out to git for a session nothing points at any more.
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.serve.abort();
        self.poll.abort();
    }
}

impl SessionRoomRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create the worktree's room, publish its opening metadata, and join it serving `service`.
    ///
    /// `Ok(None)` means this daemon has no LiveKit credentials, so **nothing** was created: the
    /// room is an addition to a worktree-serving daemon, not a prerequisite for one, and an
    /// operator who never configured LiveKit keeps the daemon they had (PRD NFR4).
    ///
    /// A configured daemon that *cannot* host the room fails instead. The agent's join token is
    /// minted for this room and no other, so a session that started with an empty room field would
    /// be a session whose agent has no route to its own checkout — reported as an error rather than
    /// returned as a success with the room quietly missing.
    ///
    /// The order is the point. The room exists, carries its metadata, and has this daemon in it
    /// before `StartSession` returns — and the agent is only spawned after that response, so the
    /// facilitating daemon being the first participant is a consequence of sequencing rather than a
    /// race it hopes to win (PRD FR2).
    pub async fn open<S: RpcService>(
        &self,
        hosting: &SessionRoomHosting<'_>,
        service: S,
    ) -> Result<Option<OpenedSessionRoom>, Status> {
        let worktree_root = hosting.worktree_root.ok_or_else(|| {
            Status::internal(format!(
                "session {} has no checkout on this host, so it cannot be measured locally; open its room with a remote source instead",
                hosting.codebase_session_id
            ))
        })?;
        let source = Arc::new(LocalCheckout {
            worktree_root: worktree_root.to_path_buf(),
            session_dir: hosting.session_dir.to_path_buf(),
            git_timeout: hosting.config.session_room_git_timeout(),
        });
        self.open_measured_by(hosting, service, source).await
    }

    /// [`Self::open`] for a checkout this daemon does not hold.
    ///
    /// Only the measurement differs: the room, its RPC surface and its identity are the same, so a
    /// participant cannot tell a split placement from a local one — which is the whole reason the
    /// room lives with the agent rather than with the files (PRD FR3).
    pub async fn open_measured_by<S: RpcService>(
        &self,
        hosting: &SessionRoomHosting<'_>,
        service: S,
        source: Arc<dyn WorktreeSource>,
    ) -> Result<Option<OpenedSessionRoom>, Status> {
        let Some(credentials) = LiveKitCredentials::from_config(hosting.config) else {
            log::debug!(
                "session_room: no LiveKit credentials configured; session {} keeps its worktree and hosts no room",
                hosting.codebase_session_id
            );
            return Ok(None);
        };
        let room_name = session_room_name(hosting.codebase_session_id);
        let identity = daemon_rpc_identity(hosting.instance_id);

        // Measured before the room exists so its opening metadata already describes the checkout: a
        // participant that joins in the same instant the daemon does still reads a real summary.
        let measured = match source.measure().await {
            Measurement::Taken(measured) => measured,
            Measurement::Unavailable => {
                return Err(Status::internal(format!(
                    "worktree {:?} could not be measured; its room cannot be opened with a summary of it",
                    hosting.worktree_root,
                )))
            }
            Measurement::Gone => {
                return Err(Status::internal(format!(
                    "worktree {:?} does not exist, so there is no checkout for {room_name} to be about",
                    hosting.worktree_root,
                )))
            }
        };

        let metadata = create_room(&credentials, &room_name, &measured).await?;
        let joined = join_room(&credentials, &room_name, &identity, service).await?;
        self.register(hosting, &room_name, joined, metadata, measured, source);
        self.abandon_if_session_is_gone(hosting, &room_name)?;

        log::info!(
            "session_room: hosting {room_name} as {identity} for worktree {:?}",
            hosting.worktree_root
        );
        Ok(Some(OpenedSessionRoom {
            room: room_name,
            url: credentials.url,
            server_identity: identity,
        }))
    }

    /// Hold the joined connection and start measuring the checkout, under the session's id.
    ///
    /// Registering is what keeps the room alive past this call *and* what makes it closable: the
    /// tasks are spawned here, in the same step that stores the handle, so there is no window in
    /// which a live connection and a git-polling loop exist that nothing can reach.
    fn register<S: RpcService>(
        &self,
        hosting: &SessionRoomHosting<'_>,
        room_name: &str,
        joined: JoinedParticipant<S>,
        metadata: RoomMetadataClient,
        measured: MeasuredWorktree,
        source: Arc<dyn WorktreeSource>,
    ) {
        let stopped = Arc::new(AtomicBool::new(false));
        // Taken before `serve` consumes the connection: the poll loop broadcasts on the same
        // connection that serves RPC, because a second connection would mean a second participant
        // in a room whose whole claim is that only this daemon is in it.
        let publisher = joined.broadcast_on(WORKTREE_ACTIVITY_TOPIC);

        let serve_stopped = Arc::clone(&stopped);
        let serve_room = room_name.to_string();
        let serve = tokio::spawn(async move {
            joined.serve(Arc::clone(&serve_stopped)).await;
            // Reached only when the connection ended on its own — `close` aborts this task instead.
            // Loud, because from here on the room has no RPC server: every `ExecuteTool` an agent
            // in it makes will wait for a participant that is no longer there, and nothing else in
            // the system notices.
            if !serve_stopped.swap(true, Ordering::Relaxed) {
                log::error!(
                    "session_room: {serve_room} lost its host; file access and activity in that room have stopped until the session is restarted"
                );
            }
        });

        let poll = tokio::spawn(
            SessionRoomPoll {
                room_name: room_name.to_string(),
                source,
                metadata,
                publisher,
                interval: hosting.config.session_room_poll_interval(),
                previous: measured.snapshot,
                previous_attachments: measured.attachments,
                next_seq: 0,
                stopped: Arc::clone(&stopped),
            }
            .run(),
        );

        // A session id names one session on one daemon, so a second room under the same id is a
        // room the first one's tasks are still holding open — replaced rather than left running
        // beside it. The previous entry is dropped outside the lock: dropping it aborts two tasks,
        // and no other caller of this registry should wait behind that.
        let previous = self.lock_rooms().insert(
            hosting.codebase_session_id.to_string(),
            SessionRoomTask {
                worktree_root: hosting.worktree_root.map(Path::to_path_buf),
                serve,
                poll,
                stopped,
            },
        );
        drop(previous);
    }

    /// Undo the open when the session it belongs to was deleted while it was being opened.
    ///
    /// A caller that fails a split start tears the codebase session down immediately, which can
    /// land while this daemon is still inside `StartSession` — its `DeleteSession` then finds
    /// nothing registered, and the room would go on being hosted for a session directory that no
    /// longer exists, for the life of the daemon. Checked *after* registering, so the window is
    /// closed from both ends: a delete arriving later finds the entry, and one that arrived earlier
    /// is caught here.
    fn abandon_if_session_is_gone(
        &self,
        hosting: &SessionRoomHosting<'_>,
        room_name: &str,
    ) -> Result<(), Status> {
        if hosting.session_dir.is_dir() {
            return Ok(());
        }
        self.close(hosting.codebase_session_id);
        Err(Status {
            code: tddy_rpc::Code::Aborted,
            message: format!(
                "session {} was deleted while {room_name} was being opened; the room was closed again",
                hosting.codebase_session_id
            ),
        })
    }

    /// Stop hosting the room belonging to `codebase_session_id`, if this daemon hosts one.
    pub fn close(&self, codebase_session_id: &str) {
        let removed = self.lock_rooms().remove(codebase_session_id);
        if removed.is_some() {
            log::info!(
                "session_room: closed {} with its session",
                session_room_name(codebase_session_id)
            );
        }
        // Dropped here rather than inside the lock: `Drop` aborts the room's two tasks, and holding
        // the registry across that would make every other opener and closer queue behind it.
        drop(removed);
    }

    /// Stop hosting the room of the checkout at `worktree_root`, whichever session owns it.
    ///
    /// `RemoveWorktree` deletes a checkout by path and knows nothing about sessions or rooms; a room
    /// left behind polls a directory that is gone, warning at the poll rate forever.
    pub fn close_for_worktree(&self, worktree_root: &Path) {
        let owning: Vec<String> = self
            .lock_rooms()
            .iter()
            .filter(|(_, task)| {
                task.worktree_root
                    .as_deref()
                    .is_some_and(|held| same_worktree(held, worktree_root))
            })
            .map(|(session_id, _)| session_id.clone())
            .collect();
        for session_id in owning {
            log::info!(
                "session_room: closing {} because its checkout {worktree_root:?} is being removed",
                session_room_name(&session_id)
            );
            self.close(&session_id);
        }
    }

    /// The registry map, taking a poisoned lock's contents rather than panicking.
    ///
    /// Every critical section here is a plain map insert/remove/scan, so a poisoned lock means some
    /// other thread panicked mid-map — recoverable state for this map, and not worth turning a
    /// `StartSession` or a `DeleteSession` into a failure over.
    fn lock_rooms(&self) -> MutexGuard<'_, HashMap<String, SessionRoomTask>> {
        self.rooms.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Create the room with its opening metadata, returning the client that keeps that metadata current.
async fn create_room(
    credentials: &LiveKitCredentials,
    room_name: &str,
    measured: &MeasuredWorktree,
) -> Result<RoomMetadataClient, Status> {
    let metadata = RoomMetadataClient::with_api_key(
        &credentials.url,
        &credentials.api_key,
        &credentials.api_secret,
    );
    metadata
        .create_with_metadata(
            room_name,
            &room_metadata_json(&measured.snapshot, &measured.attachments, unix_ms()),
        )
        .await
        .map_err(|e| Status::internal(format!("creating session room {room_name}: {e}")))?;
    Ok(metadata)
}

/// Mint this daemon's own join token for the room and join it, serving `service` there.
///
/// Awaited by the caller — that is what makes the facilitating daemon the first participant rather
/// than the likeliest one (PRD FR2).
async fn join_room<S: RpcService>(
    credentials: &LiveKitCredentials,
    room_name: &str,
    identity: &str,
    service: S,
) -> Result<JoinedParticipant<S>, Status> {
    let token_generator = TokenGenerator::new(
        credentials.api_key.clone(),
        credentials.api_secret.clone(),
        room_name.to_string(),
        identity.to_string(),
        SESSION_ROOM_TOKEN_TTL,
    );
    LiveKitParticipant::join(
        &credentials.url,
        &token_generator,
        service,
        tddy_livekit::RoomOptions::default(),
        None,
        None,
        None,
    )
    .await
    .map_err(|e| Status::internal(format!("joining session room {room_name}: {e:#}")))
}

/// Whether two paths name the same checkout, resolved through symlinks when both still exist.
///
/// `RemoveWorktree` is given a path by a browser that got it from a listing, which need not be
/// spelled the way the session recorded it (`/tmp` vs `/private/tmp` on macOS, a symlinked home).
fn same_worktree(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// The LiveKit deployment this daemon can host rooms on.
struct LiveKitCredentials {
    url: String,
    api_key: String,
    api_secret: String,
}

impl LiveKitCredentials {
    /// All three or nothing: a daemon holding two of them can neither create a room nor mint a
    /// token for one, so a partial configuration is read as an unconfigured one rather than as a
    /// host that fails halfway through every session start.
    fn from_config(config: &DaemonConfig) -> Option<Self> {
        fn configured(field: &Option<String>) -> Option<String> {
            field
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        }
        let livekit = config.livekit.as_ref()?;
        Some(Self {
            url: configured(&livekit.url)?,
            api_key: configured(&livekit.api_key)?,
            api_secret: configured(&livekit.api_secret)?,
        })
    }
}

/// One measurement of the hosted checkout, and what the session shares alongside it.
pub struct MeasuredWorktree {
    pub snapshot: WorktreeSnapshot,
    pub attachments: Vec<String>,
}

/// What one attempt to measure the checkout produced.
pub enum Measurement {
    Taken(MeasuredWorktree),
    /// Nothing was measured this time, and the next tick should simply try again — the source's
    /// own problem (a repository that did not answer, a host that could not be reached), not the
    /// checkout's.
    Unavailable,
    /// The checkout is gone. Distinct from `Unavailable` because there is nothing to come back to:
    /// a missing checkout answers every git question with silence, which read as a measurement
    /// would look like "HEAD moved to the empty sha" and announce a commit that never happened.
    Gone,
}

/// Where a room's picture of its checkout comes from.
///
/// A trait rather than a call to git, because the daemon hosting a room and the host holding the
/// checkout need not be the same machine: the only source today measures a local checkout, and a
/// remote one — asking the daemon that holds the worktree over RPC — is the shape this exists for.
/// The poll loop below is written against this and knows nothing about git.
#[async_trait::async_trait]
pub trait WorktreeSource: Send + Sync {
    async fn measure(&self) -> Measurement;
}

/// A checkout on another daemon, measured by asking it.
///
/// The facilitating daemon hosts the room but holds no files under split placement, so one poll is
/// one `GetWorktreeSnapshot` round trip to the codebase daemon, which runs the very same
/// measurement against its own filesystem. Deliberately one call rather than three: assembling the
/// snapshot where the files are means a split placement costs latency and never a different answer.
pub struct RemoteCheckout {
    /// The RPC surface used to reach the peer — this daemon's own service, which routes on
    /// `daemon_instance_id` exactly as a tool call does.
    service: Arc<dyn RemoteSnapshotSource>,
    /// The session on the *codebase* daemon whose `repo_path` is the checkout: for a split session
    /// that is the paired `workspace` session, never the agent's own id.
    codebase_session_id: String,
    codebase_instance_id: String,
    /// Where each poll's credential comes from, rather than one credential held for the session:
    /// see [`SessionTokenMinter`].
    token_minter: Arc<dyn SessionTokenMinter>,
    /// The local session dir, because attachments live with the agent (PRD FR10) and are therefore
    /// measured here rather than fetched from the peer.
    session_dir: PathBuf,
}

/// The credential one poll authenticates with.
///
/// A minter rather than a token, because a token is a thing that expires: a session token lives
/// [`tddy_github::SESSION_TOKEN_TTL`] — five minutes — while a room outlives the agent it belongs
/// to, so anything frozen at session start stops being accepted by the codebase daemon long before
/// the room stops asking, and every poll after that reads as an unreachable peer. This daemon holds
/// the deployment's signing secret, so it can issue one per poll and needs no long-lived bearer
/// token in memory at all.
///
/// Infallible: a minter can only be built where a signing secret and a verified identity are
/// already proven to exist, so "nothing to sign with" is a precondition of constructing one and
/// never a condition a poll has to handle.
pub trait SessionTokenMinter: Send + Sync {
    fn mint(&self) -> String;
}

/// One `GetWorktreeSnapshot` call, as the room's poll loop needs it.
///
/// A trait so the room does not depend on `ConnectionServiceImpl`: the daemon supplies the real
/// implementation, and the dependency runs one way.
#[async_trait::async_trait]
pub trait RemoteSnapshotSource: Send + Sync {
    async fn snapshot(
        &self,
        session_token: &str,
        codebase_session_id: &str,
        codebase_instance_id: &str,
    ) -> Result<WorktreeSnapshot, Status>;
}

impl RemoteCheckout {
    pub fn new(
        service: Arc<dyn RemoteSnapshotSource>,
        codebase_session_id: String,
        codebase_instance_id: String,
        token_minter: Arc<dyn SessionTokenMinter>,
        session_dir: PathBuf,
    ) -> Self {
        Self {
            service,
            codebase_session_id,
            codebase_instance_id,
            token_minter,
            session_dir,
        }
    }
}

#[async_trait::async_trait]
impl WorktreeSource for RemoteCheckout {
    /// A peer that cannot be reached costs this room a tick, never a wrong answer.
    ///
    /// `Unavailable` rather than an empty snapshot on purpose: an empty `head_commit` differs from
    /// the previous one, so reporting it would broadcast a `commit` event to the empty sha every
    /// time the network hiccuped.
    ///
    /// The credential is minted per poll ([`SessionTokenMinter`]), so a room stays able to measure
    /// for as long as it runs instead of for one token's lifetime.
    async fn measure(&self) -> Measurement {
        let session_token = self.token_minter.mint();
        match self
            .service
            .snapshot(
                &session_token,
                &self.codebase_session_id,
                &self.codebase_instance_id,
            )
            .await
        {
            Ok(snapshot) => Measurement::Taken(MeasuredWorktree {
                snapshot,
                attachments: attachment_basenames(&self.session_dir),
            }),
            Err(status) => {
                log::debug!(
                    "session_room: measuring session {} on daemon {} failed: {status}",
                    self.codebase_session_id,
                    self.codebase_instance_id
                );
                Measurement::Unavailable
            }
        }
    }
}

/// A checkout on this host's filesystem, measured by shelling out to git.
struct LocalCheckout {
    worktree_root: PathBuf,
    session_dir: PathBuf,
    git_timeout: Duration,
}

#[async_trait::async_trait]
impl WorktreeSource for LocalCheckout {
    /// Measure the checkout without ever making the caller wait on git.
    ///
    /// The measurement runs on the blocking pool and bounds *itself* by `git_timeout`
    /// ([`snapshot_worktree_within`]), killing any `git` that overruns. That is the difference
    /// between "the waiter gave up" and "the work stopped": a repository that has stopped answering
    /// (a stale index lock, a filesystem that went away) costs this worktree its freshness for a
    /// tick and nothing else, rather than leaving a pool thread and up to four `git` children
    /// behind on every poll until the process-wide blocking pool is full and every other blocking
    /// caller — the spawn worker, worktree removal — is stuck behind it (PRD NFR2).
    async fn measure(&self) -> Measurement {
        if !self.worktree_root.is_dir() {
            return Measurement::Gone;
        }
        let worktree_root = self.worktree_root.clone();
        let session_dir = self.session_dir.clone();
        let git_timeout = self.git_timeout;
        match tokio::task::spawn_blocking(move || MeasuredWorktree {
            snapshot: snapshot_worktree_within(&worktree_root, git_timeout),
            attachments: attachment_basenames(&session_dir),
        })
        .await
        {
            Ok(measured) => Measurement::Taken(measured),
            Err(join_error) => {
                // A panic in the measuring task is a bug, not a slow repository.
                warn!("session_room: measuring the checkout panicked: {join_error}");
                Measurement::Unavailable
            }
        }
    }
}

/// The basenames of what the session shares, so a joining agent discovers them from the room's
/// metadata instead of asking for a listing it would first have to know to request.
fn attachment_basenames(session_dir: &Path) -> Vec<String> {
    crate::session_attachments::list_session_attachments(session_dir)
        .into_iter()
        .map(|attachment| attachment.basename)
        .collect()
}

/// How many consecutive failed metadata writes a room complains about before it goes quiet about
/// them.
///
/// A write the server keeps rejecting — a payload it will not take, a room deleted underneath the
/// host — fails the same way every tick. Reporting it forever turns one broken room into a log an
/// operator stops reading; reporting the first few, then only on recovery, keeps the fact and drops
/// the repetition. The change itself is still retried, because the next tick measures again.
const METADATA_WRITE_FAILURES_BEFORE_QUIET: u32 = 3;

/// The loop that keeps a room's metadata and its participants current with the checkout.
struct SessionRoomPoll {
    room_name: String,
    /// Where each tick's picture of the checkout comes from — local git today, see
    /// [`WorktreeSource`].
    source: Arc<dyn WorktreeSource>,
    metadata: RoomMetadataClient,
    publisher: BroadcastPublisher,
    interval: Duration,
    /// The last measurement that was successfully announced. Only advanced once the room reflects
    /// it, so a failed announcement is retried rather than skipped.
    previous: WorktreeSnapshot,
    /// The attachment list last written to the room's metadata. Compared alongside the snapshot
    /// because attaching a document to a running session changes what the room advertises (PRD
    /// FR11) without touching the checkout at all — gating on git alone would leave the new
    /// attachment invisible until some unrelated edit happened to fire.
    previous_attachments: Vec<String>,
    next_seq: u64,
    /// Set when this room stops being hosted, by `close`/`Drop` or by the serving task noticing the
    /// connection ended. Polling past that point measures a checkout to broadcast into a connection
    /// that is gone.
    stopped: Arc<AtomicBool>,
}

impl SessionRoomPoll {
    async fn run(mut self) {
        let mut failed_metadata_writes = 0u32;
        loop {
            tokio::time::sleep(self.interval).await;
            if self.stopped.load(Ordering::Relaxed) {
                log::debug!(
                    "session_room: {} is no longer hosted; stopped measuring its checkout",
                    self.room_name
                );
                return;
            }
            let measured = match self.source.measure().await {
                Measurement::Taken(measured) => measured,
                Measurement::Unavailable => continue,
                // A checkout that is gone is not a checkout that changed: a missing directory
                // answers every git question with silence, which would read as "HEAD moved to the
                // empty sha" and broadcast a commit event for a commit that never happened.
                // `DeleteSession` and `RemoveWorktree` both close the room first; this covers
                // whatever removes a checkout without saying so.
                Measurement::Gone => {
                    warn!(
                        "session_room: {} stopped measuring: its checkout is gone",
                        self.room_name
                    );
                    return;
                }
            };
            if measured.snapshot == self.previous
                && measured.attachments == self.previous_attachments
            {
                continue;
            }
            if self.announce(measured, failed_metadata_writes).await {
                failed_metadata_writes = 0;
            } else {
                failed_metadata_writes = failed_metadata_writes.saturating_add(1);
            }
        }
    }

    /// Publish one change: **metadata first, then the events**. `true` when the metadata write
    /// landed, which is what a caller counts consecutive failures with.
    ///
    /// That order is the contract a receiver relies on. It makes "an event was observed" imply "the
    /// room's metadata already reflects it", so a participant woken by an event never reads a
    /// snapshot older than the event that woke it — and a late joiner that saw no event at all
    /// still reads the current summary (PRD FR9, AC7). Publishing first and writing after would
    /// leave a window in which both are wrong for whoever reacted fastest.
    async fn announce(&mut self, measured: MeasuredWorktree, previous_failures: u32) -> bool {
        let at_unix_ms = unix_ms();
        let metadata_json =
            room_metadata_json(&measured.snapshot, &measured.attachments, at_unix_ms);
        if let Err(e) = self
            .metadata
            .set_metadata(&self.room_name, &metadata_json)
            .await
        {
            // `previous` is deliberately left where it was: the next tick measures again and takes
            // this whole step from the top, rather than announcing a change whose state never
            // landed.
            if previous_failures < METADATA_WRITE_FAILURES_BEFORE_QUIET {
                warn!(
                    "session_room: {} metadata write failed, holding the change back: {e}",
                    self.room_name
                );
            } else if previous_failures == METADATA_WRITE_FAILURES_BEFORE_QUIET {
                warn!(
                    "session_room: {} metadata write has failed {} times in a row ({e}); staying quiet about it until one succeeds",
                    self.room_name,
                    previous_failures + 1
                );
            }
            return false;
        }
        if previous_failures > METADATA_WRITE_FAILURES_BEFORE_QUIET {
            log::info!(
                "session_room: {} metadata write succeeded again after {previous_failures} failures",
                self.room_name
            );
        }

        let events = activity_between(
            &self.previous,
            &measured.snapshot,
            self.next_seq,
            at_unix_ms,
        );
        self.previous = measured.snapshot;
        self.previous_attachments = measured.attachments;
        self.next_seq += events.len() as u64;
        for event in events {
            // A publish that fails still consumed its sequence number. `seq` is documented as
            // monotonic with gaps meaning "an event was lost", so a receiver can tell that from a
            // renumbered stream in which nothing appears to have gone missing.
            if let Err(e) = self.publisher.publish(&event.encode_to_vec()).await {
                warn!(
                    "session_room: {} could not broadcast activity seq={}: {e}",
                    self.room_name, event.seq
                );
            }
        }
        true
    }
}

/// Now, in milliseconds since the epoch. A clock set before 1970 reads as 0 rather than taking a
/// poll loop down with it.
fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod snapshot_worktree_tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git must be on PATH");
        assert!(
            output.status.success(),
            "git {args:?} failed in {dir:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// A checkout with one committed file and one untracked file beside it.
    fn a_checkout_with_an_untracked_file() -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-q", "-b", "main"]);
        std::fs::write(repo.path().join("shipped.txt"), "shipped\n").unwrap();
        git(repo.path(), &["add", "shipped.txt"]);
        git(repo.path(), &["commit", "-qm", "seed"]);
        std::fs::write(repo.path().join("scratch.txt"), "not added\n").unwrap();
        repo
    }

    /// Take the index lock the way a developer's own `git add` does for the moment it runs.
    fn while_another_git_holds_the_index_lock(repo: &Path) {
        std::fs::write(repo.join(".git").join("index.lock"), b"").unwrap();
    }

    #[test]
    fn a_checkout_is_measured_while_another_git_holds_the_index_lock() {
        // Given a checkout whose index is locked, as it is for the moment any `git add`, `git
        // commit` or rebase in it is running
        let repo = a_checkout_with_an_untracked_file();
        while_another_git_holds_the_index_lock(repo.path());

        // When the room polls it
        let snapshot = snapshot_worktree(repo.path());

        // Then the poll still measured the checkout. This runs against a worktree a developer is
        // working in, several times a second: a measurement that needs the index lock both loses
        // the race — reporting `head_commit: ""`, which reads as a commit to the empty sha — and
        // takes the lock often enough to fail the developer's own git.
        assert_eq!(snapshot.branch, "main");
        assert_eq!(snapshot.untracked_files, 1);
        assert!(
            !snapshot.head_commit.is_empty(),
            "HEAD must still resolve while the index is locked"
        );
    }
}

#[cfg(test)]
mod remote_checkout_tests {
    use super::*;

    /// The codebase daemon as this room reaches it, remembering every credential it was presented.
    ///
    /// The real peer authenticates each `GetWorktreeSnapshot` the same way it authenticates a tool
    /// call, so what a poll hands it is the whole of what this room's freshness depends on.
    #[derive(Default)]
    struct RecordingCodebaseDaemon {
        credentials_presented: Mutex<Vec<String>>,
    }

    impl RecordingCodebaseDaemon {
        fn credentials_presented(&self) -> Vec<String> {
            self.credentials_presented.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl RemoteSnapshotSource for RecordingCodebaseDaemon {
        async fn snapshot(
            &self,
            session_token: &str,
            _codebase_session_id: &str,
            _codebase_instance_id: &str,
        ) -> Result<WorktreeSnapshot, Status> {
            self.credentials_presented
                .lock()
                .unwrap()
                .push(session_token.to_string());
            Ok(WorktreeSnapshot {
                head_commit: "c0ffee".to_string(),
                branch: "main".to_string(),
                ..Default::default()
            })
        }
    }

    /// A signing secret holder that issues a distinguishable credential each time it is asked, so a
    /// poll reusing a stored one is visible rather than merely likely: two real tokens minted in the
    /// same second are byte-identical, which would make "they differ" untestable.
    struct CountingMinter {
        issued: Mutex<u32>,
    }

    impl CountingMinter {
        fn new() -> Self {
            Self {
                issued: Mutex::new(0),
            }
        }
    }

    impl SessionTokenMinter for CountingMinter {
        fn mint(&self) -> String {
            let mut issued = self.issued.lock().unwrap();
            *issued += 1;
            format!("minted-credential-{issued}")
        }
    }

    fn a_remote_checkout(
        peer: Arc<RecordingCodebaseDaemon>,
        minter: Arc<dyn SessionTokenMinter>,
    ) -> RemoteCheckout {
        RemoteCheckout::new(
            peer,
            "0199bbbb-0000-7000-8000-00000000000b".to_string(),
            "codebase-host".to_string(),
            minter,
            PathBuf::from("/nonexistent-session-dir"),
        )
    }

    #[tokio::test]
    async fn every_poll_presents_a_freshly_minted_credential() {
        // Given a room polling a checkout on another daemon
        let peer = Arc::new(RecordingCodebaseDaemon::default());
        let checkout = a_remote_checkout(Arc::clone(&peer), Arc::new(CountingMinter::new()));

        // When it is polled twice, as a long-lived room is
        checkout.measure().await;
        checkout.measure().await;

        // Then each poll authenticated with a credential of its own. A token frozen when the
        // session started is a five-minute one (`tddy_github::SESSION_TOKEN_TTL`), so a room built
        // on it stops being able to measure anything minutes in — and `measure` reports that as
        // `Unavailable`, which is indistinguishable from an unreachable peer.
        assert_eq!(
            peer.credentials_presented(),
            vec!["minted-credential-1", "minted-credential-2"]
        );
    }
}
