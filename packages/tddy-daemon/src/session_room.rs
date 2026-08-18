//! What a session room is called, what it knows about its checkout, when it says so — and the
//! task that hosts it.
//!
//! Product contract: `docs/ft/daemon/session-room.md`; module docs:
//! `packages/tddy-daemon/docs/session-room.md`.
//!
//! Naming and the snapshot→event rules are pure, and snapshotting is `git` against a checkout on
//! disk, so the room's lifecycle at the bottom of this file is built on a layer that is already
//! pinned without it.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use log::warn;
use prost::Message as _;
use tddy_core::agent_activity::{read_agent_activity, AgentActivityRecord, STATUS_RUNNING};
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

/// The data-channel topic the agent's own tool calls are broadcast on inside a session room,
/// re-exported for the same reason as [`WORKTREE_ACTIVITY_TOPIC`]: publisher and receiver live in
/// different crates, and a topic each spelled for itself fails as silence.
pub use tddy_service::session_activity::SESSION_ACTIVITY_TOPIC;

/// The data-channel topic a session's agent roster is broadcast on inside its room, re-exported for
/// the same reason as the two above.
pub use tddy_service::session_agents::SESSION_AGENTS_TOPIC;

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
    /// The tree object of the whole working tree as it stands, staged into a **temporary** index.
    ///
    /// This is what makes a delta possible: diffing two of these yields an ordinary git patch that
    /// carries deletions, renames, modes and binary content, and — unlike `git diff HEAD` — it sees
    /// a newly written *untracked* file, which is exactly what a `Write` produces.
    ///
    /// The index is temporary and that is not an optimisation: `git add -A` against the agent's own
    /// index would rewrite its staging area mid-session.
    ///
    /// Empty when the tree could not be written; the tick then produces no delta rather than a
    /// wrong one.
    pub wip_tree: String,
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
        // Deliberately NOT measured here. Writing a WIP tree is not a read: `git add -A` stages
        // the whole checkout and materialises loose blob and tree objects in the project's shared
        // object database. Doing that inside a function named "snapshot" would make every poll of
        // every hosted room — twice a second by default — leave objects behind that nothing
        // references, in a repository shared by every worktree, held for git's two-week prune
        // grace period.
        //
        // So the tree is measured by whoever is about to *use* it, via [`write_wip_tree_within`],
        // and published as a ref in the same breath ([`publish_wip_ref`]) so the objects it writes
        // are reachable from the moment they exist.
        wip_tree: String::new(),
    }
}

/// The tree object of `worktree_root`'s whole working tree, staged into a temporary index.
///
/// Runs `git add -A` with `GIT_INDEX_FILE` pointed at a scratch file, then `git write-tree`. The
/// agent's own index is never touched.
pub fn write_wip_tree_within(worktree_root: &Path, budget: Duration) -> String {
    write_wip_tree_by(worktree_root, Instant::now() + budget)
}

/// [`write_wip_tree_within`] under a deadline its caller already owns, so a snapshot spends one
/// budget over its whole sequence rather than one budget per measurement.
///
/// Empty when the tree could not be written, for every reason alike: this feeds a poll whose only
/// recourse is the next tick, and a tick with no tree produces no delta rather than a wrong one.
fn write_wip_tree_by(worktree_root: &Path, deadline: Instant) -> String {
    let git_dir = git_stdout(
        worktree_root,
        &["rev-parse", "--absolute-git-dir"],
        deadline,
    );
    if git_dir.is_empty() {
        warn!("session_room: {worktree_root:?} named no git dir to stage a WIP tree in");
        return String::new();
    }

    // The scratch index lives inside the git directory, and that placement is deliberate twice
    // over. Anywhere under the worktree would stage the index file into the very tree being
    // measured. The system temp directory is usually a different filesystem, which turns the copy
    // below into a byte-for-byte transfer of a file that can be hundreds of megabytes, every tick.
    let scratch = match tempfile::Builder::new()
        .prefix("tddy-wip-index-")
        .tempdir_in(&git_dir)
    {
        Ok(scratch) => scratch,
        Err(e) => {
            warn!("session_room: no scratch index for a WIP tree of {worktree_root:?}: {e}");
            return String::new();
        }
    };
    let scratch_index = scratch.path().join("index");

    // Seeded from the agent's own index rather than started empty, because `git add -A` against an
    // empty index re-hashes every file in the checkout — on a large repository that is minutes of
    // work, repeated at the poll interval. The copy carries git's stat cache, so staging only
    // hashes what actually changed since the agent's last `git` command.
    //
    // A copy, and never the file itself: `git add -A` against the agent's index would rewrite its
    // staging area mid-session, which is the one thing this measurement must never do.
    let agents_index = Path::new(&git_dir).join("index");
    if agents_index.exists() {
        if let Err(e) = std::fs::copy(&agents_index, &scratch_index) {
            warn!("session_room: the index of {worktree_root:?} could not be copied for a WIP tree: {e}");
            return String::new();
        }
    }

    // A torn copy — the agent's git was rewriting its index as this read it — fails the staging
    // below rather than yielding a tree that is half one state and half another.
    let scratch_env = [("GIT_INDEX_FILE", scratch_index.as_os_str())];
    if let Err(reason) = git_output(worktree_root, &["add", "-A"], &scratch_env, deadline) {
        warn!("session_room: staging a WIP tree failed: {reason}");
        return String::new();
    }
    git_stdout_with(worktree_root, &["write-tree"], &scratch_env, deadline)
}

/// The worktree's `HEAD`, re-exported from where it now lives.
///
/// Moved to `tddy-core` so the coder's presenter can stamp records with it too — `tddy-core` is
/// the only crate both the daemon and the coder depend on. Re-exported here because every caller
/// in this crate reaches it through this module.
pub use tddy_core::git_head::read_head_commit;

/// The delta one poll tick produces, or `None` when there is nothing to say.
///
/// `None` rather than an empty delta whenever the tick cannot be described, which is never the same
/// as "the tick changed nothing": no previous tree to diff from (the first tick of a room), no
/// current tree (the measurement failed), two identical trees, an unknown `HEAD`, or a diff git
/// could not take. An empty *patch* means "this tick moved nothing" and is a delta a client may be
/// handed; `None` means there is no tick to hand out at all.
///
/// The distinction is the whole safety property. A client that is handed an empty patch records the
/// tick as applied and moves its sequence past it, so an empty patch standing in for a failure is a
/// change the mirror will never learn about and never reconcile.
pub fn tick_delta(
    worktree_root: &Path,
    prev: &WorktreeSnapshot,
    next: &WorktreeSnapshot,
    seq: u64,
) -> Option<ActivityDelta> {
    if prev.wip_tree.is_empty() || next.wip_tree.is_empty() || prev.wip_tree == next.wip_tree {
        return None;
    }
    // A delta whose base is unknown is worse than no delta: the client compares it against its own
    // HEAD, an empty string matches nothing, and every tick would reconcile forever while
    // reporting a mismatch against a commit that was never read rather than saying so.
    if next.head_commit.is_empty() {
        log::warn!(
            "session_room: no delta for seq {seq}: the checkout's HEAD could not be read, so a \
             patch could not name the commit it applies onto"
        );
        return None;
    }
    let patch = match diff_between(worktree_root, &prev.wip_tree, &next.wip_tree, &[]) {
        Ok(patch) => patch,
        Err(reason) => {
            warn!("session_room: no delta for seq {seq}: {reason}");
            return None;
        }
    };
    Some(ActivityDelta {
        seq,
        prev_seq: seq.saturating_sub(1),
        // Where the checkout *ended up*, not where it started. A client applies a delta after it has
        // followed HEAD, so a tick that spanned a commit and named the old commit would be rejected
        // by every client that kept up.
        base_commit: next.head_commit.clone(),
        patch,
        // The whole window, unscoped. The store narrows a tick per call on the way out, and a delta
        // recorded already narrowed could never be sliced into the several calls that share its tick.
        scoped_paths: Vec::new(),
    })
}

/// The ref under which a session's uncommitted state is published, so a client can `git fetch` it.
///
/// Under `refs/tddy/` rather than `refs/heads/` because it is not a branch: it must never appear in
/// `git branch`, never be a push target, and never be something `git checkout` offers by name in
/// the daemon-side repository an agent is working in.
pub fn wip_ref_name(session_id: &str) -> String {
    format!("refs/tddy/session/{session_id}/wip")
}

/// Publish `wip_tree` as a commit parented on `head_commit` and point [`wip_ref_name`] at it.
///
/// This is what makes reconciliation a plain `git fetch` rather than a whole-worktree patch. The
/// mirror is a clone of this repository, so git already knows how to move only the objects it is
/// missing, delta-compressed — where a cumulative patch would re-send the entire dirty tree over a
/// data channel every time a client fell one tick behind.
///
/// Wrapped in a commit rather than published as a bare tree because a tree is not a fetchable tip;
/// the commit costs one object and makes the whole thing an ordinary fetch.
///
/// Returns the commit sha. The ref is deleted when the session's room closes — see
/// [`delete_wip_ref`] — so its objects become unreachable and ordinary `git gc` reclaims them.
pub fn publish_wip_ref(
    worktree_root: &Path,
    session_id: &str,
    head_commit: &str,
    wip_tree: &str,
) -> Result<String, String> {
    let deadline = Instant::now() + DEFAULT_GIT_TIMEOUT;
    let mut args = vec!["commit-tree", wip_tree];
    // Parentless only when the checkout has no HEAD to parent on — an unborn branch, or a HEAD the
    // measurement could not read. A commit-tree given an empty parent fails outright, and a WIP ref
    // that exists is worth more to a client than one that never appears; what it loses is the
    // shortcut of naming its base through the object graph, which the record's `head_commit` (empty
    // for the same reason) already tells it.
    if !head_commit.is_empty() {
        args.push("-p");
        args.push(head_commit);
    }
    let message = format!("tddy session {session_id}: work in progress");
    args.push("-m");
    args.push(&message);

    // Signed by the daemon under a fixed identity rather than by whatever `user.email` the checkout
    // happens to carry: this object is not the agent's work, it is a machine-made snapshot of it,
    // and `git commit-tree` refuses outright in a repository that has configured no identity at all.
    let identity: [(&str, &OsStr); 4] = [
        ("GIT_AUTHOR_NAME", OsStr::new(WIP_COMMIT_IDENTITY_NAME)),
        ("GIT_AUTHOR_EMAIL", OsStr::new(WIP_COMMIT_IDENTITY_EMAIL)),
        ("GIT_COMMITTER_NAME", OsStr::new(WIP_COMMIT_IDENTITY_NAME)),
        ("GIT_COMMITTER_EMAIL", OsStr::new(WIP_COMMIT_IDENTITY_EMAIL)),
    ];
    let commit = git_output(worktree_root, &args, &identity, deadline)?;
    let commit = String::from_utf8_lossy(&commit).trim().to_string();
    if commit.is_empty() {
        return Err(format!(
            "git commit-tree in {worktree_root:?} named no commit for tree {wip_tree}"
        ));
    }

    git_output(
        worktree_root,
        &["update-ref", &wip_ref_name(session_id), &commit],
        &[],
        deadline,
    )?;
    Ok(commit)
}

/// Who the WIP commits of every session are made by. Not a person and never presented as one: an
/// address in the reserved `.invalid` TLD cannot be mailed, so no one can mistake a snapshot for
/// work someone signed.
const WIP_COMMIT_IDENTITY_NAME: &str = "tddy-daemon";
const WIP_COMMIT_IDENTITY_EMAIL: &str = "tddy-daemon@tddy.invalid";

/// Drop a session's WIP ref, leaving its objects unreachable.
///
/// Called on the same path that closes the room. Without it a deleted session's uncommitted state
/// would be pinned in the project repository forever, which is a leak measured in whole worktrees.
/// A ref that was never published deletes without complaint — `git update-ref -d` treats an absent
/// ref as already gone — so closing a room that never got as far as publishing one is not an error.
pub fn delete_wip_ref(worktree_root: &Path, session_id: &str) -> Result<(), String> {
    let deadline = Instant::now() + DEFAULT_GIT_TIMEOUT;
    git_output(
        worktree_root,
        &["update-ref", "-d", &wip_ref_name(session_id)],
        &[],
        deadline,
    )?;
    Ok(())
}

/// One patch, and what a client needs to place it.
///
/// `patch` is the tick's diff **limited to `scoped_paths`** — a call's own files, not its window's.
/// A whole-tick delta is the same type with `scoped_paths` empty.
///
/// There is no cumulative variant: a client that has fallen behind resyncs by fetching the
/// session's WIP ref ([`publish_wip_ref`]), which git transfers incrementally.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActivityDelta {
    pub seq: u64,
    pub prev_seq: u64,
    pub base_commit: String,
    pub patch: Vec<u8>,
    /// The paths this patch was limited to. Empty means the whole tick.
    pub scoped_paths: Vec<String>,
}

/// Which slice of a tick's diff to serve.
///
/// The three are exhaustive and disjoint by construction: every path a tick touched is claimed by
/// some call or by none, so [`Self::Call`] over every call plus [`Self::Residual`] reconstructs
/// [`Self::Tick`] exactly. That property is the whole reason `Residual` exists — without it, a
/// change no tool declared would be attributed to nobody and reach no one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeltaScope {
    /// The paths the named call is credited with.
    #[default]
    Call,
    /// The paths of that tick claimed by no call at all.
    Residual,
    /// The whole tick, unscoped.
    Tick,
}

/// Why a delta could not be produced for a call.
///
/// Two variants rather than one because the client's response differs: an unknown call is a bug on
/// one side or the other, an aged-out delta is an ordinary reconcile. Collapsing them would make a
/// long-running mirror's normal recovery indistinguishable from a defect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaLookupError {
    UnknownCall { call_id: String },
    AgedOut { call_id: String, seq: u64 },
}

/// How many ticks of delta a hosted room keeps before the oldest falls out.
///
/// At the shipped `session_room.poll_interval_ms` (2 s) this is a little over two minutes of
/// history, which is the window in which a client that missed a broadcast can still be handed the
/// patch rather than having to recover. Beyond it there is nothing to buy: a client further behind
/// than this reconciles by fetching the WIP ref (AC12/AC13), which git transfers incrementally and
/// which is cheaper than the cumulative patch a longer ring would be standing in for.
pub const SESSION_DELTA_RING_TICKS: usize = 64;

/// How many bytes of patch a hosted room keeps across all the ticks it retains.
///
/// Sized so a full [`SESSION_DELTA_RING_TICKS`] window of ordinary editing — kilobytes a tick —
/// fits many times over, while one generated-file sweep or a rebase cannot make a single room the
/// largest thing in the daemon: a host runs one of these per hosted session, so this bound is
/// multiplied by every room open at once.
pub const SESSION_DELTA_RING_BYTES: usize = 16 * 1024 * 1024;

/// The bounded ring of recent tick deltas, and the `call_id → seq` index over it.
///
/// Bounded on both axes because neither alone is enough: a tick count bounds a busy session's
/// memory only if its patches are small, and a byte budget bounds a session that makes one enormous
/// change but not one that makes a million tiny ones.
#[derive(Debug)]
pub struct SessionDeltaStore {
    max_ticks: usize,
    max_bytes: usize,
    /// The retained ticks, oldest first.
    ticks: VecDeque<ActivityDelta>,
    /// `patch` bytes summed over [`Self::ticks`], carried alongside so eviction never walks the
    /// ring to find out whether it is still over budget.
    retained_bytes: usize,
    /// What each call declared, by the tick it landed in. Dropped with its tick: paths whose patch
    /// is gone can narrow nothing.
    claims: HashMap<u64, BTreeMap<String, Vec<String>>>,
    /// Which tick each call ever attributed belongs to, kept **after** that tick is evicted —
    /// which is the whole difference between [`DeltaLookupError::AgedOut`] and
    /// [`DeltaLookupError::UnknownCall`]. Forgetting a call along with its patch would make a long
    /// mirror's routine recovery indistinguishable from a defect on one side or the other.
    ///
    /// It costs a call id and a `u64` per tool call for as long as the room is open, and nothing
    /// more: the paths, which are the bulk of an attribution, live in `claims` and go with the tick.
    call_ticks: HashMap<String, u64>,
}

impl SessionDeltaStore {
    pub fn new(max_ticks: usize, max_bytes: usize) -> Self {
        Self {
            max_ticks,
            max_bytes,
            ticks: VecDeque::new(),
            retained_bytes: 0,
            claims: HashMap::new(),
            call_ticks: HashMap::new(),
        }
    }

    /// Record the delta produced by one tick, evicting oldest-first to stay within bounds.
    pub fn record(&mut self, delta: ActivityDelta) {
        let recorded_seq = delta.seq;
        self.retained_bytes = self.retained_bytes.saturating_add(delta.patch.len());
        self.ticks.push_back(delta);

        // Until *both* bounds hold, not either: a tick count bounds a busy session only if its
        // patches are small, and a byte budget bounds a session that makes one enormous change but
        // not one that makes a million tiny ones. A single patch bigger than the whole budget
        // therefore evicts itself, and a client asking for it is told it aged out — which is the
        // same fetch-the-WIP-ref reconcile as any other gap, rather than a bound that is not one.
        while !self.ticks.is_empty()
            && (self.ticks.len() > self.max_ticks || self.retained_bytes > self.max_bytes)
        {
            if let Some(evicted) = self.ticks.pop_front() {
                self.retained_bytes = self.retained_bytes.saturating_sub(evicted.patch.len());
                self.claims.remove(&evicted.seq);
            }
        }

        // Attributions for a tick that never arrived — a call credited to a window whose
        // measurement failed — would otherwise sit here for the life of the room. Anything older
        // than the oldest tick still held can serve nobody; anything newer is a call whose tick has
        // yet to be recorded, which is ordinary and must survive.
        let floor = self
            .ticks
            .front()
            .map(|tick| tick.seq)
            .unwrap_or_else(|| recorded_seq.saturating_add(1));
        self.claims.retain(|seq, _| *seq >= floor);
    }

    /// Credit a call with the paths it declared, in the tick it landed in.
    ///
    /// The paths are what narrow the tick's diff to this call. A call that declared none is still
    /// recorded — it belongs to the tick, and its delta is simply empty — so a lookup can tell
    /// "this call changed nothing it told us about" from "we have never heard of this call".
    pub fn attribute(&mut self, call_id: &str, seq: u64, changed_paths: &[String]) {
        self.call_ticks.insert(call_id.to_string(), seq);
        self.claims
            .entry(seq)
            .or_default()
            .insert(call_id.to_string(), changed_paths.to_vec());
    }

    /// The delta for `call_id` under `scope`: the tick's diff limited to that call's own paths
    /// ([`DeltaScope::Call`]), to the paths no call claimed ([`DeltaScope::Residual`]), or to
    /// nothing at all ([`DeltaScope::Tick`]).
    pub fn delta_for_call(
        &self,
        call_id: &str,
        scope: DeltaScope,
    ) -> Result<ActivityDelta, DeltaLookupError> {
        let seq = *self
            .call_ticks
            .get(call_id)
            .ok_or_else(|| DeltaLookupError::UnknownCall {
                call_id: call_id.to_string(),
            })?;
        let tick = self.tick(seq).ok_or_else(|| DeltaLookupError::AgedOut {
            call_id: call_id.to_string(),
            seq,
        })?;

        let (patch, scoped_paths) = match scope {
            DeltaScope::Call => {
                // A call that declared nothing gets nothing — never its window's other work.
                // Serving the whole tick here would credit this call with a neighbour's change and
                // have a client apply that change twice.
                let declared = self.declared_by(call_id, seq);
                let claimed: BTreeSet<&str> = declared.iter().map(String::as_str).collect();
                let (patch, _) =
                    select_sections(&tick.patch, |section| section.names_any_in(&claimed));
                (patch, declared)
            }
            DeltaScope::Residual => {
                let claimed = self.claimed_paths(seq);
                select_sections(&tick.patch, |section| !section.names_any_in(&claimed))
            }
            DeltaScope::Tick => (tick.patch.clone(), Vec::new()),
        };

        Ok(ActivityDelta {
            seq: tick.seq,
            prev_seq: tick.prev_seq,
            base_commit: tick.base_commit.clone(),
            patch,
            scoped_paths,
        })
    }

    /// The paths a tick touched that no call claimed.
    ///
    /// A `seq` the ring no longer holds is [`DeltaLookupError::AgedOut`] with no call to name: a
    /// tick that was never recorded and a tick that has been evicted are the same absence here, and
    /// both are answered by the same reconcile.
    pub fn residual_paths(&self, seq: u64) -> Result<Vec<String>, DeltaLookupError> {
        let tick = self.tick(seq).ok_or(DeltaLookupError::AgedOut {
            call_id: String::new(),
            seq,
        })?;
        let claimed = self.claimed_paths(seq);
        let (_, paths) = select_sections(&tick.patch, |section| !section.names_any_in(&claimed));
        Ok(paths)
    }

    /// How many ticks are currently retained.
    pub fn len(&self) -> usize {
        self.ticks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn tick(&self, seq: u64) -> Option<&ActivityDelta> {
        self.ticks.iter().find(|tick| tick.seq == seq)
    }

    /// The paths one call declared in `seq`, empty when it declared none.
    fn declared_by(&self, call_id: &str, seq: u64) -> Vec<String> {
        self.claims
            .get(&seq)
            .and_then(|claims| claims.get(call_id))
            .cloned()
            .unwrap_or_default()
    }

    /// Every path any call declared in `seq` — what the residual is the complement of.
    fn claimed_paths(&self, seq: u64) -> BTreeSet<&str> {
        self.claims
            .get(&seq)
            .into_iter()
            .flat_map(|claims| claims.values())
            .flatten()
            .map(String::as_str)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// A tick's patch, sliced by path
// ---------------------------------------------------------------------------
//
// A tick is measured once, for the whole window, and then partitioned: each call gets the slice
// that touches the files it declared, and what no call declared is the residual. The partition is
// taken from the patch that was recorded rather than by asking git for a narrower diff again —
// which is what makes every call's slice plus the residual add back up to exactly the bytes the
// tick produced, and keeps a lookup free of a subprocess and of any dependence on the checkout
// still holding the trees the tick was measured from.
//
// Slicing is safe because a patch is a concatenation of self-contained file sections: `git apply`
// accepts any subset of them, and the sections are delimited by a byte sequence that cannot occur
// anywhere else at the start of a line — a hunk's lines are prefixed with ' ', '+', '-' or '\',
// and a binary payload's lines are base85.

/// The line each file's section of a git patch begins with.
const PATCH_SECTION_HEADER: &[u8] = b"diff --git ";

/// One file's slice of a `git diff --binary` patch.
struct PatchSection<'a> {
    /// Every name the section is about — preimage and postimage, which differ for a rename. A call
    /// claims the section if it declared *either*: both are how that one change is described.
    names: Vec<String>,
    /// The single name the section is reported under: its postimage, or its preimage when it
    /// deleted the file — the same choice `git diff --name-only` makes, so a scope's paths and
    /// [`changed_paths_between`] describe one tick the same way.
    reported: String,
    bytes: &'a [u8],
}

impl PatchSection<'_> {
    fn names_any_in(&self, claimed: &BTreeSet<&str>) -> bool {
        self.names
            .iter()
            .any(|name| claimed.contains(name.as_str()))
    }
}

/// The bytes of the sections `keep` selects, concatenated in patch order, and the names they are
/// reported under.
fn select_sections(
    patch: &[u8],
    keep: impl Fn(&PatchSection<'_>) -> bool,
) -> (Vec<u8>, Vec<String>) {
    let mut bytes = Vec::new();
    let mut names = Vec::new();
    for section in patch_sections(patch) {
        if !keep(&section) {
            continue;
        }
        bytes.extend_from_slice(section.bytes);
        if !section.reported.is_empty() {
            names.push(section.reported);
        }
    }
    (bytes, names)
}

/// Split a patch into one section per file.
fn patch_sections(patch: &[u8]) -> Vec<PatchSection<'_>> {
    let mut starts = Vec::new();
    let mut offset = 0;
    for line in patch.split_inclusive(|byte| *byte == b'\n') {
        if line.starts_with(PATCH_SECTION_HEADER) {
            starts.push(offset);
        }
        offset += line.len();
    }
    starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = starts.get(index + 1).copied().unwrap_or(patch.len());
            patch_section(&patch[*start..end])
        })
        .collect()
}

/// Read one section's names out of its own header.
fn patch_section(bytes: &[u8]) -> PatchSection<'_> {
    let mut lines = bytes.split(|byte| *byte == b'\n');
    let header = lines.next().unwrap_or_default();
    let mut preimage = None;
    let mut postimage = None;
    for line in lines {
        // The header ends at the first hunk or binary payload. Past that point a line beginning
        // `--- ` is a removed line of the file's own content, and reading it as a name would take
        // the section's identity from the text it happens to contain.
        if line.starts_with(b"@@")
            || line.starts_with(b"GIT binary patch")
            || line.starts_with(b"Binary files ")
        {
            break;
        }
        if let Some(name) = line.strip_prefix(b"--- ".as_slice()) {
            preimage = side_name(name, "a/");
        } else if let Some(name) = line.strip_prefix(b"+++ ".as_slice()) {
            postimage = side_name(name, "b/");
        } else if let Some(name) = line
            .strip_prefix(b"rename from ".as_slice())
            .or_else(|| line.strip_prefix(b"copy from ".as_slice()))
        {
            preimage = Some(unquoted_name(trimmed_line_end(name)));
        } else if let Some(name) = line
            .strip_prefix(b"rename to ".as_slice())
            .or_else(|| line.strip_prefix(b"copy to ".as_slice()))
        {
            postimage = Some(unquoted_name(trimmed_line_end(name)));
        }
    }
    if preimage.is_none() && postimage.is_none() {
        // A binary file and a bare mode change carry no `---`/`+++` pair at all, so their only
        // names are the ones on the header line.
        (preimage, postimage) = header_names(header);
    }

    let mut names: Vec<String> = Vec::new();
    for name in [preimage.clone(), postimage.clone()].into_iter().flatten() {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    if names.is_empty() {
        warn!(
            "session_room: a patch section names no file and is served as unclaimed: {}",
            String::from_utf8_lossy(header)
        );
    }
    PatchSection {
        names,
        reported: postimage.or(preimage).unwrap_or_default(),
        bytes,
    }
}

/// The name on a `---`/`+++` line, or `None` for the `/dev/null` that stands in for the side a
/// creation or a deletion does not have.
fn side_name(name: &[u8], prefix: &str) -> Option<String> {
    let name = side_name_terminator_trimmed(trimmed_line_end(name));
    if name == b"/dev/null" {
        return None;
    }
    let name = unquoted_name(name);
    Some(
        name.strip_prefix(prefix)
            .map(str::to_string)
            .unwrap_or(name),
    )
}

/// The two names on a `diff --git a/<old> b/<new>` line.
///
/// Only consulted for a section that has no `---`/`+++` pair, because this is the ambiguous line:
/// an unquoted name may contain a space, and git leaves the split to the reader. Both sides naming
/// the same file is the case that is never ambiguous and is also the common one, so that is what is
/// looked for first; a rename of a spaced name git chose not to quote falls back to the first
/// ` b/`.
fn header_names(header: &[u8]) -> (Option<String>, Option<String>) {
    let Some(rest) = header.strip_prefix(PATCH_SECTION_HEADER) else {
        return (None, None);
    };
    let rest = trimmed_line_end(rest);

    if rest.starts_with(b"\"") {
        // git quotes both names or neither, so a quoted first name means a quoted second one.
        let Some((old, after)) = take_c_quoted(rest) else {
            return (None, None);
        };
        let after = after.strip_prefix(b" ".as_slice()).unwrap_or(after);
        let Some((new, _)) = take_c_quoted(after) else {
            return (None, None);
        };
        return (
            Some(without_prefix(old, "a/")),
            Some(without_prefix(new, "b/")),
        );
    }

    let rest = String::from_utf8_lossy(rest);
    let mut ambiguous = None;
    for (at, _) in rest.match_indices(" b/") {
        let (Some(old), Some(new)) = (
            rest[..at].strip_prefix("a/"),
            rest[at + 1..].strip_prefix("b/"),
        ) else {
            continue;
        };
        if old == new {
            return (Some(old.to_string()), Some(new.to_string()));
        }
        ambiguous.get_or_insert((Some(old.to_string()), Some(new.to_string())));
    }
    ambiguous.unwrap_or((None, None))
}

/// A name as git printed it, C-quoting undone when it was quoted.
fn unquoted_name(name: &[u8]) -> String {
    match take_c_quoted(name) {
        Some((unquoted, _)) => unquoted,
        None => String::from_utf8_lossy(name).into_owned(),
    }
}

/// Undo git's C-style quoting — the form it prints a name in when the name holds a byte that would
/// otherwise be ambiguous — returning the name and whatever followed the closing quote.
fn take_c_quoted(bytes: &[u8]) -> Option<(String, &[u8])> {
    let mut rest = bytes.strip_prefix(b"\"".as_slice())?;
    let mut name = Vec::new();
    loop {
        let byte = *rest.first()?;
        rest = &rest[1..];
        match byte {
            b'"' => {
                // Lossily, because a quoted name is quoted precisely when it holds bytes that may
                // not be text at all; naming it with replacement characters beats dropping the
                // section that mentions it.
                return Some((String::from_utf8_lossy(&name).into_owned(), rest));
            }
            b'\\' => {
                let escaped = *rest.first()?;
                rest = &rest[1..];
                match escaped {
                    b'a' => name.push(0x07),
                    b'b' => name.push(0x08),
                    b'f' => name.push(0x0c),
                    b'n' => name.push(b'\n'),
                    b'r' => name.push(b'\r'),
                    b't' => name.push(b'\t'),
                    b'v' => name.push(0x0b),
                    // Up to three octal digits: git's escape for a byte with no letter form of its
                    // own, which is how every non-ASCII path arrives under `core.quotePath`.
                    b'0'..=b'7' => {
                        let mut value = u32::from(escaped - b'0');
                        for _ in 0..2 {
                            match rest.first() {
                                Some(digit @ b'0'..=b'7') => {
                                    value = value * 8 + u32::from(digit - b'0');
                                    rest = &rest[1..];
                                }
                                _ => break,
                            }
                        }
                        name.push(value as u8);
                    }
                    // `\"` and `\\`, and anything else git decided to escape.
                    other => name.push(other),
                }
            }
            other => name.push(other),
        }
    }
}

/// `name` without git's `a/`/`b/` side prefix.
fn without_prefix(name: String, prefix: &str) -> String {
    name.strip_prefix(prefix)
        .map(str::to_string)
        .unwrap_or(name)
}

/// A patch line without the carriage return a checkout with CRLF endings may leave on it.
fn trimmed_line_end(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r".as_slice()).unwrap_or(line)
}

/// Drop the tab git appends to a `---`/`+++` name that it chose not to quote but that contains a
/// space.
///
/// Git terminates those two lines with a literal tab precisely when the unquoted name holds a
/// space, so that a reader can find where the name ends:
///
/// ```text
/// --- a/plain.txt          (no space  -> no terminator)
/// --- a/sp ace.txt<TAB>    (a space   -> terminated)
/// --- "a/\303\251.txt"     (quoted    -> no terminator, the quotes delimit it)
/// ```
///
/// Stripping one trailing tab is unambiguous: a name git left unquoted cannot itself contain a
/// tab, because git C-quotes any name with a control character in it. Without this, a spaced path
/// arrives as `sp ace.txt\t`, matches no declared path, and every call that edited such a file is
/// served an **empty** patch — indistinguishable from a call that declared nothing.
fn side_name_terminator_trimmed(name: &[u8]) -> &[u8] {
    name.strip_suffix(b"\t".as_slice()).unwrap_or(name)
}

/// The patch between two trees, or between `HEAD` and a tree when `from` is a commit, limited to
/// `paths`.
///
/// An empty `paths` means the whole diff; otherwise git's own pathspec limiting does the narrowing,
/// so a scoped patch is a real patch rather than a filtered rendering of one — `git apply` cannot
/// tell the difference, which is exactly the property that makes scoping safe.
///
/// `git diff --binary` output, verbatim: the client applies it with `git apply`, so anything this
/// function reformatted would be a difference the client has to understand.
///
/// Fallible on purpose. An empty patch is a **meaningful value** here — it is how a tick says it
/// moved nothing — so returning one for a diff that could not be taken would tell a mirror the
/// checkout is unchanged when it is not, and the mirror would record the tick as applied and never
/// reconcile it.
///
/// The paths are passed after `--`, because a path that happens to look like a revision would
/// otherwise select a commit instead of a file.
pub fn diff_between(
    worktree_root: &Path,
    from: &str,
    to: &str,
    paths: &[String],
) -> Result<Vec<u8>, String> {
    let deadline = Instant::now() + DEFAULT_GIT_TIMEOUT;
    // `--no-ext-diff` and `--no-textconv` because a repository may configure either, and both
    // replace the patch with something for a human to read: a client would receive a rendering
    // `git apply` cannot apply, and would have no way to tell that from a patch that simply failed.
    let mut args = vec![
        "diff",
        "--binary",
        "--no-ext-diff",
        "--no-textconv",
        from,
        to,
    ];
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
    }
    git_output(worktree_root, &args, &[], deadline)
}

/// The paths a diff between two trees touches, as plain relative paths.
///
/// `-z`-separated and therefore never C-quoted, unlike the `changed_paths` in room metadata: these
/// are used to open files and to build pathspecs, so a display-quoted name would select nothing.
pub fn changed_paths_between(
    worktree_root: &Path,
    from: &str,
    to: &str,
) -> Result<Vec<String>, String> {
    let deadline = Instant::now() + DEFAULT_GIT_TIMEOUT;
    let listed = git_output(
        worktree_root,
        &["diff", "--name-only", "-z", "--no-ext-diff", from, to],
        &[],
        deadline,
    )?;
    Ok(listed
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        // Lossily, for the same reason the rest of this module decodes lossily: a repository can
        // hold a path that is not valid UTF-8, and losing every other path over that one would be
        // worse than naming it with replacement characters.
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect())
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
    git_stdout_with(worktree_root, args, &[], deadline)
}

/// [`git_stdout`] with extra environment for the child — `GIT_INDEX_FILE` for the WIP tree, the
/// identity [`publish_wip_ref`] signs with — and the same "empty string when git could not answer".
fn git_stdout_with(
    worktree_root: &Path,
    args: &[&str],
    envs: &[(&str, &OsStr)],
    deadline: Instant,
) -> String {
    match git_output(worktree_root, args, envs, deadline) {
        Ok(stdout) => String::from_utf8_lossy(&stdout).trim().to_string(),
        Err(reason) => {
            warn!("snapshot_worktree: {reason}");
            String::new()
        }
    }
}

/// The raw stdout of a git command in `worktree_root`, or why it could not be had.
///
/// Bytes rather than a string, and un-trimmed, because a patch is neither: a `git diff --binary` of
/// a checkout holding a file that is not valid UTF-8 would not survive being decoded, and a patch
/// missing its final newline is one `git apply` rejects.
///
/// The failure is returned rather than logged here because the callers differ in what a failure
/// means: a measurement that could not be taken is a warning and an empty reading, while a WIP ref
/// that could not be published is an error its caller has to see.
fn git_output(
    worktree_root: &Path,
    args: &[&str],
    envs: &[(&str, &OsStr)],
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    let budget = deadline.saturating_duration_since(Instant::now());
    if budget.is_zero() {
        return Err(format!(
            "no time left for git {args:?} in {worktree_root:?}; the measurement is incomplete"
        ));
    }
    let mut command = Command::new("git");
    for (name, value) in envs {
        command.env(name, value);
    }
    let mut child = match command
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
            return Err(format!(
                "git {args:?} in {worktree_root:?} could not be run: {e}"
            ))
        }
    };

    // Drained on its own thread, and the thread's completion is what the deadline is measured
    // against: git blocks once it has written ~64 KB into an unread pipe, so a caller that watched
    // only the exit status would time out and kill a healthy `git diff` over a large change set.
    // EOF here means git closed stdout, which it does as it exits.
    let Some(stdout) = child.stdout.take() else {
        return Err(format!(
            "git {args:?} in {worktree_root:?} produced no stdout pipe"
        ));
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
            Ok(status) if status.success() => Ok(collected),
            Ok(status) => Err(format!("git {args:?} in {worktree_root:?} exited {status}")),
            Err(e) => Err(format!(
                "git {args:?} in {worktree_root:?} could not be waited on: {e}"
            )),
        },
        Ok(Err(e)) => Err(format!(
            "reading git {args:?} in {worktree_root:?} failed: {e}"
        )),
        Err(_) => {
            // Killed *and* reaped: a `git` left running holds an index lock and a file handle on a
            // directory the daemon may be about to remove, and an unreaped one stays a zombie for
            // the life of the daemon.
            let _ = child.kill();
            let _ = child.wait();
            Err(format!(
                "git {args:?} in {worktree_root:?} did not answer within {}ms and was killed",
                budget.as_millis()
            ))
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

/// Whether a session's WIP ref has been released, shared between the tick that publishes it and the
/// close that deletes it.
///
/// A lock rather than a flag, and held across the git call on both sides, because the two orderings
/// are not equally survivable. A tick that was already measuring when its room closed would
/// otherwise publish the ref *after* the release deleted it — and nothing would ever delete it
/// again, pinning a whole worktree of blobs in the repository every checkout of the project shares.
/// Holding the lock makes the two whole operations interleave: a tick that finds it released
/// publishes nothing, and a release that finds a publish in flight waits for it and then deletes
/// what it wrote.
type WipRefRelease = Arc<Mutex<bool>>;

/// [`WipRefRelease`], taking a poisoned lock's contents rather than panicking: what it guards is one
/// `bool`, so a thread that panicked while holding it left nothing to be inconsistent about — and a
/// room whose ref could no longer be released would leak exactly what the lock exists to prevent.
fn lock_wip_ref(released: &WipRefRelease) -> MutexGuard<'_, bool> {
    released.lock().unwrap_or_else(|e| e.into_inner())
}

/// One hosted room: the task serving RPC in it, and the task measuring the checkout it describes.
struct SessionRoomTask {
    /// The checkout this room describes. Kept so a caller holding only a path — `RemoveWorktree`,
    /// which never learns a session id — can close the room before the directory goes.
    /// The local checkout this room measures, when there is one; `None` under split placement.
    worktree_root: Option<PathBuf>,
    /// The ring of tick deltas this room's poll loop records into and its RPC surface serves slices
    /// of. Held here rather than by either of them, because they are two tasks and one ring: the
    /// loop writes it, `ReportAgentActivity` attributes calls into it, and
    /// `StreamAgentActivityDelta` reads it — all found by the same session id.
    deltas: Arc<Mutex<SessionDeltaStore>>,
    /// Where this room's `session.activity` records go out. An `Arc` because the publisher is not
    /// clonable and a broadcast is made from an RPC handler that holds the registry, not from the
    /// task that took it.
    activity: Arc<BroadcastPublisher>,
    /// Where this room's `session.agents` snapshots go out. Taken here for the same reason
    /// `activity` is — the publisher has to be built on the connection before it is consumed by the
    /// serving task — but published from the roster's own handlers rather than from the poll loop:
    /// a roster changes when an operator attaches an agent, not on a timer.
    agents: Arc<BroadcastPublisher>,
    /// Whether this session's WIP ref has been released. See [`WipRefRelease`].
    wip_ref_released: WipRefRelease,
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
        // Both taken before `serve` consumes the connection: the poll loop broadcasts on the same
        // connection that serves RPC, because a second connection would mean a second participant
        // in a room whose whole claim is that only this daemon is in it. Two topics on that one
        // connection — the checkout's movements and the agent's own calls — because a receiver that
        // wants only commits should not have to decode every tool call to discover that.
        let publisher = joined.broadcast_on(WORKTREE_ACTIVITY_TOPIC);
        let activity = Arc::new(joined.broadcast_on(SESSION_ACTIVITY_TOPIC));
        // A third topic, for the same reason there are two: a consumer that wants the roster should
        // not have to decode every tool call and every commit to find it, and the three carry
        // different schemas.
        let agents = Arc::new(joined.broadcast_on(SESSION_AGENTS_TOPIC));

        let deltas = Arc::new(Mutex::new(SessionDeltaStore::new(
            SESSION_DELTA_RING_TICKS,
            SESSION_DELTA_RING_BYTES,
        )));
        let wip_ref_released: WipRefRelease = Arc::new(Mutex::new(false));

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
                session_id: hosting.codebase_session_id.to_string(),
                worktree_root: hosting.worktree_root.map(Path::to_path_buf),
                session_dir: hosting.session_dir.to_path_buf(),
                git_timeout: hosting.config.session_room_git_timeout(),
                source,
                metadata,
                publisher,
                activity: Arc::clone(&activity),
                deltas: Arc::clone(&deltas),
                broadcast_rows: BroadcastActivityRows::default(),
                wip_ref_released: Arc::clone(&wip_ref_released),
                interval: hosting.config.session_room_poll_interval(),
                previous: measured.snapshot,
                previous_wip_tree: String::new(),
                previous_attachments: measured.attachments,
                next_seq: 0,
                next_delta_seq: 0,
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
                deltas,
                activity,
                agents,
                wip_ref_released,
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

    /// The ring of tick deltas of the room hosted for `codebase_session_id`, or `None` when this
    /// daemon hosts none.
    ///
    /// `None` rather than an empty ring, because the two are different answers: an empty ring
    /// reports every call as unknown — a defect on one side or the other — where "no room here"
    /// is the fact, and the only one a caller can route on.
    pub fn delta_store(&self, codebase_session_id: &str) -> Option<Arc<Mutex<SessionDeltaStore>>> {
        self.lock_rooms()
            .get(codebase_session_id)
            .map(|room| Arc::clone(&room.deltas))
    }

    /// Where a session's activity records are broadcast, or `None` when this daemon hosts no room
    /// for it — which is ordinary: a record is persisted whether or not there is a room to carry it,
    /// and a session whose agent runs on another daemon has its room over there.
    pub fn activity_publisher(&self, codebase_session_id: &str) -> Option<Arc<BroadcastPublisher>> {
        self.lock_rooms()
            .get(codebase_session_id)
            .map(|room| Arc::clone(&room.activity))
    }

    /// Where a session's roster snapshots are broadcast, or `None` when this daemon hosts no room
    /// for it.
    ///
    /// `None` is ordinary and is not an error: a roster is persisted and served over
    /// `ListSessionAgents`/`StreamSessionAgents` whether or not a room is open to carry it, and a
    /// daemon with no LiveKit credentials hosts no rooms at all.
    pub fn agents_publisher(&self, session_id: &str) -> Option<Arc<BroadcastPublisher>> {
        self.lock_rooms()
            .get(session_id)
            .map(|room| Arc::clone(&room.agents))
    }

    /// Whether this daemon currently hosts `session_id`'s room.
    ///
    /// Asked before admitting an owning daemon to it: a peer told to join a room nobody opened
    /// waits out its deadline against a participant that never arrives.
    pub fn hosts(&self, session_id: &str) -> bool {
        self.lock_rooms().contains_key(session_id)
    }

    // There is deliberately no `broadcast_activity` here, and adding one back would reintroduce a
    // bug this feature already shipped once.
    //
    // The poll loop is the **sole** broadcaster: it attributes each new record to the tick whose
    // delta covers it, stamps `activity_seq`, and publishes through its own clone of this
    // publisher. A second door — an RPC handler pushing a record the moment it arrives — publishes
    // *before* the tick holding that record's delta exists, so a client reacting to it looks the
    // delta up and is told `UnknownCall`; and once both doors are open, every claude-cli record
    // goes out twice, the early copy carrying `activity_seq: 0`.
    //
    // `activity_publisher` stays because the registry has to hand the poll loop its clone, and
    // because answering `None` for an unhosted session is the branch an RPC route tests.

    /// Whether this daemon currently hosts the room for `codebase_session_id`. The room-admission
    /// handshake (PRD § "What attach does" step 3) consults this to refuse admitting an owning daemon
    /// to a session this daemon does not control — an unknown id is `NOT_FOUND`, never a token for a
    /// room nobody hosts.
    pub fn contains(&self, codebase_session_id: &str) -> bool {
        self.lock_rooms().contains_key(codebase_session_id)
    }

    /// Stop hosting the room belonging to `codebase_session_id`, if this daemon hosts one.
    pub fn close(&self, codebase_session_id: &str) {
        let removed = self.lock_rooms().remove(codebase_session_id);
        let Some(room) = removed else {
            return;
        };
        log::info!(
            "session_room: closed {} with its session",
            session_room_name(codebase_session_id)
        );

        // Before the tasks are dropped and before this returns, because both callers act on that
        // ordering: `DeleteSession` removes the session directory next, and `RemoveWorktree` the
        // checkout — and `git update-ref -d` in a directory that has gone deletes nothing and
        // reports why. Blocking here rather than on the blocking pool for the same reason: a
        // deletion that had merely been *scheduled* would race the removal it must precede.
        //
        // Deliberately not in `SessionRoomTask::drop`, which also runs when a room is replaced
        // under the same session id and when the registry itself goes away at shutdown. Neither
        // means the session is over: a replacement's client may be fetching the ref this instant
        // and the new room only republishes on its next tick, and a daemon restart leaves every
        // workspace session alive — dropping their refs there would unpin the uncommitted state of
        // every one of them until each room is opened again.
        if let Some(worktree_root) = room.worktree_root.as_deref() {
            // Taken for the whole release, so a tick that was already measuring when this ran
            // either published before it and is deleted here, or finds the ref released and
            // publishes nothing. What it costs is that a close waits out a publish already in
            // flight, bounded by that tick's own git budget.
            let mut released = lock_wip_ref(&room.wip_ref_released);
            *released = true;
            if let Err(reason) = delete_wip_ref(worktree_root, codebase_session_id) {
                // Loud, because what is left behind is a whole worktree of blobs pinned in a
                // repository shared by every checkout of the project, for as long as the ref lives.
                log::error!(
                    "session_room: {} still pins its uncommitted state: {reason}",
                    wip_ref_name(codebase_session_id)
                );
            }
        }

        // Dropped here rather than inside the lock: `Drop` aborts the room's two tasks, and holding
        // the registry across that would make every other opener and closer queue behind it.
        drop(room);
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

// ---------------------------------------------------------------------------
// Attributing the agent's calls to the tick that covers them
// ---------------------------------------------------------------------------
//
// The durable activity log is the one place every session type's tool calls meet: the daemon writes
// it for claude-cli and sandbox sessions, the coder participant for tool and cursor-cli ones. So the
// poll loop *tails that log* rather than being told about a call by whoever recorded it, and the
// room ends up with one broadcaster for every session type instead of one per writer (AC5).
//
// It is also the only ordering that can be right. A record broadcast by its writer goes out before
// the tick that measured the call has run, so a client that looks the delta up the moment it hears
// about the call is told the call is unknown — a defect on one side or the other, as far as the
// client can tell, rather than "the tick has not happened yet". Attributing here means the delta a
// record names already exists by the time the record does.

/// Which rows of a session's activity log a room has already put on the wire.
///
/// Keyed by call rather than by line, because [`read_agent_activity`] coalesces the log by
/// `call_id`: a call appears once, in its latest state, so "which rows are new" can never be a count
/// of lines read. Each call is remembered together with whether the row that went out for it was its
/// terminal one — a call is written twice, `running` when it starts and `completed`/`error` when it
/// finishes, and a plain seen-set would either leave a client watching a call that has long since
/// finished, or re-broadcast every retained call on every tick.
///
/// It costs a call id and a bool per tool call for as long as the room is open, which is what
/// [`SessionDeltaStore::call_ticks`] costs for the same reason: a call the log's tail cap has
/// dropped can no longer be read back, so forgetting it would only trade memory for the risk of
/// broadcasting it twice.
#[derive(Debug, Default)]
pub struct BroadcastActivityRows {
    /// `call_id` → whether the row already broadcast for that call was its terminal one.
    handled: HashMap<String, bool>,
}

impl BroadcastActivityRows {
    /// True when this exact row has already gone out: the same call, in the same state.
    ///
    /// A call whose running row went out is *not* already broadcast once its terminal row appears —
    /// that row supersedes it and carries the result — but it is once that terminal row has gone
    /// out, because a finished call produces no further rows.
    pub fn already_broadcast(&self, record: &AgentActivityRecord) -> bool {
        match self.handled.get(&record.call_id) {
            Some(broadcast_terminal) => *broadcast_terminal || !is_terminal(record),
            None => false,
        }
    }

    /// Remember that `record` went out, in the state it went out in.
    pub fn mark_broadcast(&mut self, record: &AgentActivityRecord) {
        self.handled
            .insert(record.call_id.clone(), is_terminal(record));
    }
}

/// Whether a row is the last one its call will ever produce.
///
/// By excluding `running` rather than by naming `completed` and `error`, so a status this build has
/// never seen is treated as a call that is over and broadcast once — where treating it as still
/// running would re-broadcast it on every tick for the life of the room.
fn is_terminal(record: &AgentActivityRecord) -> bool {
    record.status != STATUS_RUNNING
}

/// Where a tick can put the calls it found, which is what makes a record's delta resolvable at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickAttributionTarget {
    /// The delta this tick recorded, under `seq`: the checkout moved and its patch holds whatever
    /// these calls changed.
    ThisTicksDelta { seq: u64 },
    /// The checkout did not move, so there is no delta for a call to belong to and one has to be
    /// made: an **empty** delta at `next_seq`, on the commit the checkout is standing on.
    ///
    /// Empty rather than absent, because the two are different answers. A call that changed nothing
    /// is one frame with an empty patch (AC9); a call with no delta at all is
    /// [`DeltaLookupError::UnknownCall`], which a client reads as a defect rather than as "that call
    /// touched nothing".
    AnEmptyDelta { next_seq: u64, base_commit: String },
    /// This room measures no checkout of its own — split placement, where the files are on the
    /// codebase daemon. Its records still go out, because the room is where a participant learns
    /// what the agent did, but they carry `activity_seq` 0, the wire's "no tick has covered it yet":
    /// a seq naming a delta this room never recorded would send every client to reconcile against a
    /// ring that has nothing in it.
    NoCheckout,
}

/// What one tick does with the tail of its session's activity log.
#[derive(Debug, Default, PartialEq)]
pub struct TickActivity {
    /// The records to broadcast, each stamped with the seq of the delta that covers it.
    pub broadcast: Vec<AgentActivityRecord>,
    /// The delta the tick has to record before those records resolve to anything — always empty,
    /// always at the seq they were stamped with. `None` when the tick recorded a delta of its own,
    /// when there is nothing new to attribute, or when there is no checkout to attribute against.
    pub empty_delta: Option<ActivityDelta>,
}

/// The rows of `log` a tick has to broadcast, each attributed to the delta that covers it, and the
/// delta that has to be recorded for that attribution to mean anything.
///
/// The whole per-tick decision, with the log read, the ring and the broadcast left to the caller —
/// so what a tick decides is testable without a room, a checkout or a LiveKit connection.
///
/// A tick with nothing new to say numbers nothing: an empty delta per idle tick would consume the
/// sequence a client de-duplicates by at the poll rate, and every one of those numbers would read as
/// a delta that never arrived.
pub fn tick_activity(
    already_broadcast: &BroadcastActivityRows,
    log: &[AgentActivityRecord],
    target: &TickAttributionTarget,
) -> TickActivity {
    let mut broadcast: Vec<AgentActivityRecord> = log
        .iter()
        .filter(|record| !already_broadcast.already_broadcast(record))
        .cloned()
        .collect();
    if broadcast.is_empty() {
        return TickActivity::default();
    }

    let (seq, empty_delta) = match target {
        TickAttributionTarget::ThisTicksDelta { seq } => (*seq, None),
        TickAttributionTarget::AnEmptyDelta {
            next_seq,
            base_commit,
        } if !base_commit.is_empty() => (
            *next_seq,
            Some(ActivityDelta {
                seq: *next_seq,
                prev_seq: next_seq.saturating_sub(1),
                base_commit: base_commit.clone(),
                patch: Vec::new(),
                scoped_paths: Vec::new(),
            }),
        ),
        // A delta whose base is unknown is worse than no delta, exactly as [`tick_delta`] says: the
        // client compares `base_commit` against its own HEAD, an empty string matches nothing, and
        // it would reconcile against a patch claiming nothing changed forever. The records still go
        // out, carrying the honest "no tick has covered it yet".
        TickAttributionTarget::AnEmptyDelta { .. } => {
            warn!(
                "session_room: {} calls are unattributed: the checkout's HEAD could not be read, so \
                 an empty delta could not name the commit it applies onto",
                broadcast.len()
            );
            (0, None)
        }
        TickAttributionTarget::NoCheckout => (0, None),
    };

    for record in &mut broadcast {
        record.activity_seq = seq;
    }
    TickActivity {
        broadcast,
        empty_delta,
    }
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
    /// The session the room belongs to, which is what its WIP ref is named after — a room name
    /// would not do: a client fetches `refs/tddy/session/{session_id}/wip` from the project's
    /// repository, where rooms do not exist.
    session_id: String,
    /// The checkout on this host, when there is one. `None` under split placement, where the files
    /// are on the codebase daemon: a tick then measures through [`RemoteCheckout`] and produces no
    /// delta and no WIP ref, because both are made out of the objects of a repository this host
    /// does not have.
    worktree_root: Option<PathBuf>,
    /// The session directory holding the agent-activity log this loop tails. Local wherever the
    /// checkout is: the log is written beside the agent, which is on this host by definition —
    /// this daemon is the one hosting its room.
    session_dir: PathBuf,
    /// The budget one tick's git work gets, the same `session_room.git_timeout_ms` the measurement
    /// spends.
    git_timeout: Duration,
    /// Where each tick's picture of the checkout comes from — local git today, see
    /// [`WorktreeSource`].
    source: Arc<dyn WorktreeSource>,
    metadata: RoomMetadataClient,
    publisher: BroadcastPublisher,
    /// Where this loop puts the agent's own calls, on `session.activity` — the same publisher the
    /// registry hands out, because the two must be one connection: a second one would be a second
    /// participant in a room whose whole claim is that only this daemon is in it.
    activity: Arc<BroadcastPublisher>,
    /// The ring this loop records each tick's delta in, shared with the RPC surface that serves
    /// slices of it.
    deltas: Arc<Mutex<SessionDeltaStore>>,
    /// The rows of the activity log this loop has already broadcast. Held by the loop rather than by
    /// the room, because it is the loop's own place in a log nothing else reads.
    broadcast_rows: BroadcastActivityRows,
    /// See [`WipRefRelease`]: held across this loop's publish so a tick cannot outlive the close
    /// that released its ref.
    wip_ref_released: WipRefRelease,
    interval: Duration,
    /// The last measurement that was successfully announced. Only advanced once the room reflects
    /// it, so a failed announcement is retried rather than skipped.
    previous: WorktreeSnapshot,
    /// The WIP tree of the last tick that produced one — the tree every next delta is taken from.
    ///
    /// Beside [`Self::previous`] rather than inside it, because `previous` is compared against a
    /// fresh measurement to decide whether a tick has anything to say at all, and a measurement
    /// deliberately carries no tree ([`snapshot_worktree_within`]). Folding the tree into it would
    /// make every measurement differ from it and turn an idle room into one that rewrites its
    /// metadata, stages the whole checkout and publishes a ref at the poll rate.
    ///
    /// Left where it was when a tick's tree could not be written, rather than cleared: the ref
    /// still points at it, so its objects are still reachable and the next tick that succeeds takes
    /// a patch spanning both ticks. Clearing it would cost the *following* tick its delta too, and
    /// that pair of changes would then exist in no delta at all.
    previous_wip_tree: String,
    /// The attachment list last written to the room's metadata. Compared alongside the snapshot
    /// because attaching a document to a running session changes what the room advertises (PRD
    /// FR11) without touching the checkout at all — gating on git alone would leave the new
    /// attachment invisible until some unrelated edit happened to fire.
    previous_attachments: Vec<String>,
    next_seq: u64,
    /// The number the next delta this room produces will carry — its **own** sequence, not the one
    /// [`Self::next_seq`] numbers activity events with.
    ///
    /// Two counters because the two streams are de-duplicated separately and a gap means the same
    /// thing in both: "one was lost". A delta numbered out of the event space would inherit the
    /// events' gaps — a tick that announced a commit and a file change consumes two of those — and
    /// every one of them would read to a client as a delta that never arrived, sending it to fetch
    /// the WIP ref for nothing. It advances only when a delta was actually produced, so an idle
    /// tick, an unmeasurable one, and a tick whose tree did not change all cost nothing: what a
    /// client sees is `0, 1, 2, …` with a gap only where a delta really was lost.
    next_delta_seq: u64,
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
                // Costs the whole tick, the activity log included: a call attributed to a window
                // that was never measured would name a delta that does not exist, and the log is
                // read whole on the next tick anyway — nothing is lost by waiting for one that
                // knows what the checkout looks like.
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
            // Read before the announcement, which consumes the measurement: an empty delta has to
            // name the commit the checkout is standing on *now*, and `self.previous` is deliberately
            // left behind when a metadata write fails.
            let head_commit = measured.snapshot.head_commit.clone();

            // Before the change check, because a *quiet* session never reaches it. The WIP ref is
            // what a participant restores its own copy of the checkout from, and until this ran it
            // did not exist: `announce` publishes it, and `announce` only runs when the checkout has
            // moved. A client that attached to a session nobody is editing therefore had nothing to
            // restore from and stayed at whatever it happened to check out — indefinitely.
            if self.previous_wip_tree.is_empty() {
                self.seed_uncommitted_state(&measured.snapshot).await;
            }

            // A tick that measured no change still tails the activity log — it just has nothing to
            // announce, and leaves the metadata-failure count exactly where it was. A `Read`, a
            // `Grep`, a `Bash` that printed something: none of them move the checkout, and every one
            // of them is a call a participant in the room is entitled to hear about. Gating the log
            // on the checkout would make "the agent did something that changed nothing"
            // indistinguishable from silence.
            let mut recorded_delta_seq = None;
            if measured.snapshot != self.previous
                || measured.attachments != self.previous_attachments
            {
                let announced = self.announce(measured, failed_metadata_writes).await;
                recorded_delta_seq = announced.delta_seq;
                if announced.metadata_written {
                    failed_metadata_writes = 0;
                } else {
                    failed_metadata_writes = failed_metadata_writes.saturating_add(1);
                }
            }
            self.broadcast_new_activity(recorded_delta_seq, &head_commit)
                .await;
        }
    }

    /// Publish one change: **metadata first, then the events**.
    ///
    /// That order is the contract a receiver relies on. It makes "an event was observed" imply "the
    /// room's metadata already reflects it", so a participant woken by an event never reads a
    /// snapshot older than the event that woke it — and a late joiner that saw no event at all
    /// still reads the current summary (PRD FR9, AC7). Publishing first and writing after would
    /// leave a window in which both are wrong for whoever reacted fastest.
    async fn announce(&mut self, measured: MeasuredWorktree, previous_failures: u32) -> Announced {
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
            return Announced::held_back();
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
        // Between the two, because it needs both snapshots alive: the tree it writes is diffed
        // against the previous tick's, and the events above are about the pair it sits between.
        //
        // And before the events go out, for the reason the metadata write is: it makes "an event
        // was observed" imply the delta and the WIP ref for that tick already exist, so a receiver
        // that reacts to an event by fetching the session's uncommitted state cannot be handed the
        // one from before the event that woke it. What it costs is the tick's git work of latency
        // on the event, bounded by the same `session_room.git_timeout_ms` everything else here is.
        let delta_seq = self.record_uncommitted_state(&measured.snapshot).await;
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
        Announced {
            metadata_written: true,
            delta_seq,
        }
    }

    /// Stage this tick's working tree, keep the delta it produced, and publish the ref that makes
    /// the objects behind that delta reachable (AC13).
    ///
    /// All of it is git against a checkout, so all of it runs on the blocking pool — the same
    /// reason [`LocalCheckout::measure`] does. One dispatch rather than three: the tree, the diff
    /// taken from it and the ref that makes it fetchable are one tick's work, and a tree that had
    /// been written by a thread which then handed it on would for that moment be an object nothing
    /// in the repository names.
    ///
    /// Measuring is deliberately *not* where this happens. `git add -A` materialises loose objects
    /// in the repository every checkout of the project shares, so it belongs where the ref that
    /// makes them reachable is published in the same breath — here — rather than in a snapshot the
    /// room takes twice a second whether or not anything came of it.
    ///
    /// A failure costs this tick its delta and nothing else: the loop measures again, and a client
    /// that was left with a gap reconciles by fetching that ref, which is what it is for.
    ///
    /// Returns the seq of the delta this tick recorded, or `None` when it produced none — which is
    /// where the tick's tool calls are attributed, so it is answered here rather than inferred from
    /// [`Self::next_delta_seq`] by arithmetic.
    /// Publish the checkout's uncommitted state once, so a participant has something to restore
    /// from before the session has moved at all.
    ///
    /// **No delta is recorded**, and that is the difference from [`Self::record_uncommitted_state`]
    /// rather than an omission. A delta is a diff between two ticks; this one has no predecessor, so
    /// a delta taken here would be the whole worktree against nothing — which a client would then
    /// apply on top of a checkout that already contains it.
    ///
    /// Silent on failure beyond the warning [`write_wip_tree_within`] already logs: the next tick
    /// takes this from the top, and the client waiting on the ref reports its own timeout, which is
    /// the report an operator can act on.
    async fn seed_uncommitted_state(&mut self, measured: &WorktreeSnapshot) {
        let Some(worktree_root) = self.worktree_root.clone() else {
            // Split placement: the files are on the codebase daemon, and so is the ref.
            return;
        };
        let session_id = self.session_id.clone();
        let room_name = self.room_name.clone();
        let git_timeout = self.git_timeout;
        let head_commit = measured.head_commit.clone();
        let wip_ref_released = Arc::clone(&self.wip_ref_released);

        let staged = tokio::task::spawn_blocking(move || {
            let wip_tree = write_wip_tree_within(&worktree_root, git_timeout);
            if wip_tree.is_empty() {
                return String::new();
            }
            // Held across the publish: see [`WipRefRelease`].
            let released = lock_wip_ref(&wip_ref_released);
            if *released {
                return String::new();
            }
            if let Err(reason) =
                publish_wip_ref(&worktree_root, &session_id, &head_commit, &wip_tree)
            {
                warn!(
                    "session_room: {room_name} could not publish its opening uncommitted state: {reason}"
                );
                return String::new();
            }
            wip_tree
        })
        .await;

        match staged {
            Ok(wip_tree) if !wip_tree.is_empty() => self.previous_wip_tree = wip_tree,
            Ok(_) => {}
            Err(join_error) => warn!(
                "session_room: {} could not take its opening uncommitted state: {join_error}",
                self.room_name
            ),
        }
    }

    async fn record_uncommitted_state(&mut self, measured: &WorktreeSnapshot) -> Option<u64> {
        let Some(worktree_root) = self.worktree_root.clone() else {
            // Split placement. A delta is a diff between two objects of the project's repository
            // and the WIP ref lives in it, so both are published by the daemon that holds the
            // files; this one hosts the room and has nothing to stage.
            return None;
        };
        // The pair the delta is taken between: the previous tick's measurement carrying the tree it
        // wrote, and this one's carrying the tree about to be written.
        let previous = WorktreeSnapshot {
            wip_tree: self.previous_wip_tree.clone(),
            ..self.previous.clone()
        };
        let next = measured.clone();
        let seq = self.next_delta_seq;
        let session_id = self.session_id.clone();
        let room_name = self.room_name.clone();
        let git_timeout = self.git_timeout;
        let wip_ref_released = Arc::clone(&self.wip_ref_released);

        let produced = tokio::task::spawn_blocking(move || {
            let next = WorktreeSnapshot {
                wip_tree: write_wip_tree_within(&worktree_root, git_timeout),
                ..next
            };
            let delta = tick_delta(&worktree_root, &previous, &next, seq);
            if !next.wip_tree.is_empty() {
                // Held across the publish: see [`WipRefRelease`].
                let released = lock_wip_ref(&wip_ref_released);
                if *released {
                    log::debug!(
                        "session_room: {room_name} closed while this tick was measuring; its uncommitted state was not republished"
                    );
                } else if let Err(reason) =
                    publish_wip_ref(&worktree_root, &session_id, &next.head_commit, &next.wip_tree)
                {
                    // A client can still be handed this tick's delta; what it loses is the ref it
                    // would recover from if it ever fell behind, which is worth saying so.
                    warn!("session_room: {room_name} could not publish its uncommitted state: {reason}");
                }
            }
            (next.wip_tree, delta)
        })
        .await;

        let (wip_tree, delta) = match produced {
            Ok(produced) => produced,
            Err(join_error) => {
                // A panic in the tick's git work is a bug, not a slow repository — and it must not
                // take the loop with it: the room goes on measuring and announcing.
                warn!(
                    "session_room: {} could not take this tick's uncommitted state: {join_error}",
                    self.room_name
                );
                return None;
            }
        };
        if !wip_tree.is_empty() {
            self.previous_wip_tree = wip_tree;
        }
        let delta = delta?;
        let recorded_seq = delta.seq;
        // Advanced only for a delta that exists, so the numbers a client de-duplicates by have a
        // gap exactly where one was lost. See [`Self::next_delta_seq`].
        self.next_delta_seq += 1;
        // Poisoned by a panic in some other holder means one call's slice of one tick was left
        // half-computed; the ring itself is a queue of finished deltas, and refusing to record any
        // more of them would cost every client every remaining tick of the session.
        self.deltas
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record(delta);
        Some(recorded_seq)
    }

    /// Put the calls the agent has made since the last tick into the room, each attributed to the
    /// delta that covers it (AC4, AC5).
    ///
    /// The log is the source, rather than the writer telling the room directly, for two reasons.
    /// It is the one place every session type's calls meet — the daemon writes it for claude-cli
    /// and sandbox sessions, the coder participant for tool and cursor-cli ones — so tailing it
    /// makes this loop the single broadcaster instead of one per writer. And it is the only
    /// ordering in which a record can name a delta that exists: a writer broadcasting as it records
    /// publishes before the tick that measured the call has run, and a client looking that delta up
    /// is told the call is unknown.
    ///
    /// A log that cannot be read costs this tick its attribution and nothing more — never an empty
    /// reading, which would mark every call it holds as broadcast and lose them all silently.
    ///
    /// TODO(session-room): read only what was appended since the last tick. The file is read whole
    /// every `session_room.poll_interval_ms`, and it carries the full input and full output of every
    /// tool call, so a long session pays its own size twice a second for as long as its room is
    /// open.
    async fn broadcast_new_activity(&mut self, recorded_delta_seq: Option<u64>, head_commit: &str) {
        // Blocking file I/O, on the blocking pool for the same reason the tick's git work is: this
        // runs on the runtime that serves every room's RPC.
        let session_dir = self.session_dir.clone();
        let log = match tokio::task::spawn_blocking(move || read_agent_activity(&session_dir)).await
        {
            Ok(Ok(log)) => log,
            Ok(Err(e)) => {
                warn!(
                    "session_room: {} could not read its session's activity log; this tick attributed nothing: {e}",
                    self.room_name
                );
                return;
            }
            Err(join_error) => {
                // A panic while reading the log is a bug, not a slow filesystem — and it must not
                // take the loop with it.
                warn!(
                    "session_room: {} could not read its session's activity log: {join_error}",
                    self.room_name
                );
                return;
            }
        };

        let target = self.attribution_target(recorded_delta_seq, head_commit);
        let decided = tick_activity(&self.broadcast_rows, &log, &target);
        if decided.broadcast.is_empty() {
            return;
        }
        let numbered_a_delta = decided.empty_delta.is_some();
        {
            // Recorded before the calls are attributed to it, so no lookup can find a call whose
            // tick is not in the ring yet. One lock for both, and never held across the publish
            // below: it is a `std::sync::Mutex` guarding a ring the RPC surface reads.
            let mut store = self.deltas.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(delta) = decided.empty_delta {
                store.record(delta);
            }
            for record in &decided.broadcast {
                store.attribute(&record.call_id, record.activity_seq, &record.changed_paths);
            }
        }
        if numbered_a_delta {
            self.next_delta_seq += 1;
        }

        for record in decided.broadcast {
            // Marked before the publish, and marked even when the publish fails: a row that went
            // back into the queue would be attributed again on a later tick, and `attribute`
            // re-points the call at *that* tick — moving the call's delta to a window that does not
            // hold its change. A lost broadcast is a gap the client's own `seq` de-duplication
            // already describes; a re-attributed call is a wrong answer.
            self.broadcast_rows.mark_broadcast(&record);
            let frame = tddy_service::agent_activity_to_proto(record).encode_to_vec();
            if let Err(e) = self.activity.publish(&frame).await {
                warn!(
                    "session_room: {} could not broadcast an activity record: {e}",
                    self.room_name
                );
            }
        }
    }

    /// Where this tick's calls belong: the delta it recorded, an empty one it has to record, or
    /// nothing at all when this room measures no checkout of its own.
    fn attribution_target(
        &self,
        recorded_delta_seq: Option<u64>,
        head_commit: &str,
    ) -> TickAttributionTarget {
        if self.worktree_root.is_none() {
            return TickAttributionTarget::NoCheckout;
        }
        match recorded_delta_seq {
            Some(seq) => TickAttributionTarget::ThisTicksDelta { seq },
            None => TickAttributionTarget::AnEmptyDelta {
                next_seq: self.next_delta_seq,
                base_commit: head_commit.to_string(),
            },
        }
    }
}

/// What one tick's announcement did.
struct Announced {
    /// Whether the room's metadata took the change, which is what the loop counts consecutive
    /// failures with.
    metadata_written: bool,
    /// The seq of the delta this tick recorded, if it recorded one — where the calls this tick
    /// finds in the activity log are attributed.
    delta_seq: Option<u64>,
}

impl Announced {
    /// The metadata write failed, so nothing else in the tick ran: the change is held back and the
    /// next tick takes the whole step from the top.
    fn held_back() -> Self {
        Self {
            metadata_written: false,
            delta_seq: None,
        }
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
