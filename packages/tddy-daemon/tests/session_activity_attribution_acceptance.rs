//! What a poll tick does with the tail of its session's activity log — the half of AC4/AC5 of
//! `docs/ft/daemon/session-worktree-sync.md` that decides *which* tick a call belongs to.
//!
//! The decision is a pure function ([`tick_activity`]) precisely so it can be pinned without a
//! room: the loop that calls it needs LiveKit, a checkout and a timer, and none of those have
//! anything to say about which delta covers a call.
//!
//! Real git repositories and a real `agent-activity.jsonl` in temp directories, because both the
//! patch a call resolves to and the coalescing the log does are the things under test. Every
//! assertion about a patch is on the *effect* of applying it, never on its text.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_core::agent_activity::{
    append_agent_activity, read_agent_activity, AgentActivityRecord, STATUS_COMPLETED,
    STATUS_RUNNING,
};
use tddy_daemon::session_room::{
    diff_between, tick_activity, write_wip_tree_within, ActivityDelta, BroadcastActivityRows,
    DeltaScope, SessionDeltaStore, TickActivity, TickAttributionTarget,
};

/// Pathspec meaning "no narrowing" — the whole diff.
const EVERY_PATH: &[String] = &[];

/// The measurement budget the poll loop uses; generous here because a temp repo is fast and a tight
/// budget would make this suite fail under load rather than on a defect.
const A_GENEROUS_BUDGET: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// One tool call as its writer records it: completed, having declared nothing.
fn a_tool_call(call_id: &str) -> ToolCall {
    ToolCall {
        call_id: call_id.to_string(),
        status: STATUS_COMPLETED.to_string(),
        changed_paths: Vec::new(),
    }
}

struct ToolCall {
    call_id: String,
    status: String,
    changed_paths: Vec<String>,
}

impl ToolCall {
    /// The paths this call declared — an `Edit`'s or a `Write`'s `file_path`, as
    /// `tddy_core::agent_activity::declared_paths` extracts it.
    fn that_wrote(mut self, paths: &[&str]) -> Self {
        self.changed_paths = paths.iter().map(|path| path.to_string()).collect();
        self
    }

    /// The row written when the call starts, before it has produced anything.
    fn still_running(mut self) -> Self {
        self.status = STATUS_RUNNING.to_string();
        self
    }

    fn build(self) -> AgentActivityRecord {
        AgentActivityRecord {
            call_id: self.call_id,
            tool_name: "Write".to_string(),
            input: serde_json::json!({ "file_path": "alpha.txt" }),
            status: self.status,
            result: serde_json::Value::Null,
            error_message: String::new(),
            started_unix_ms: 1_700_000_000_000,
            completed_unix_ms: 0,
            source: "claude-cli".to_string(),
            head_commit: String::new(),
            // What every writer records: the tick that covers this call has not run yet, and
            // stamping it is exactly what this suite is about.
            activity_seq: 0,
            changed_paths: self.changed_paths,
        }
    }
}

/// A checkout with one commit, standing in for a session worktree.
fn a_session_worktree(root: &Path) -> String {
    git(root, &["init", "--initial-branch=main"]);
    git(root, &["config", "user.email", "agent@example.com"]);
    git(root, &["config", "user.name", "Agent"]);
    std::fs::write(root.join("README.md"), "one\n").expect("write README");
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "initial"]);
    git(root, &["rev-parse", "HEAD"]).trim().to_string()
}

fn a_store() -> SessionDeltaStore {
    SessionDeltaStore::new(8, 1024 * 1024)
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Apply `patch` to a fresh clone of `origin` at `base_commit` and return that clone, so a test
/// asserts on files rather than on diff text.
fn a_worktree_with_patch_applied(
    origin: &Path,
    base_commit: &str,
    patch: &[u8],
) -> tempfile::TempDir {
    let clone = tempfile::tempdir().expect("tempdir");
    git(clone.path(), &["clone", &origin.to_string_lossy(), "."]);
    git(clone.path(), &["checkout", base_commit]);
    let patch_file = clone.path().join("incoming.patch");
    std::fs::write(&patch_file, patch).expect("write patch");
    git(clone.path(), &["apply", &patch_file.to_string_lossy()]);
    std::fs::remove_file(&patch_file).expect("remove patch file");
    clone
}

/// Write one row to the session's log exactly as its writer does, and read the whole log back the
/// way the poll loop does — coalesced by `call_id`, latest row per call.
fn the_log_after_recording(
    session_dir: &Path,
    row: AgentActivityRecord,
) -> Vec<AgentActivityRecord> {
    append_agent_activity(session_dir, &row).expect("the session's log must be writable");
    the_log(session_dir)
}

fn the_log(session_dir: &Path) -> Vec<AgentActivityRecord> {
    read_agent_activity(session_dir).expect("the session's log must be readable")
}

/// One poll tick's activity step, as the loop performs it: decide, record the delta the decision
/// asks for, credit every record it hands back, and remember the rows that went out.
///
/// Returns the records the tick broadcast, each stamped with the seq it was attributed to.
fn a_tick_over(
    log: &[AgentActivityRecord],
    target: &TickAttributionTarget,
    rows: &mut BroadcastActivityRows,
    store: &mut SessionDeltaStore,
) -> Vec<AgentActivityRecord> {
    let decided = tick_activity(rows, log, target);
    if let Some(delta) = decided.empty_delta {
        store.record(delta);
    }
    for record in &decided.broadcast {
        store.attribute(&record.call_id, record.activity_seq, &record.changed_paths);
        rows.mark_broadcast(record);
    }
    decided.broadcast
}

/// The tick a checkout that moved produces: a real patch between the two trees, recorded under
/// `seq`. Returns the store already holding it.
fn a_store_holding_the_tick_between(
    repo: &Path,
    head: &str,
    before: &str,
    after: &str,
    seq: u64,
) -> SessionDeltaStore {
    let mut store = a_store();
    store.record(ActivityDelta {
        seq,
        prev_seq: seq.saturating_sub(1),
        base_commit: head.to_string(),
        patch: diff_between(repo, before, after, EVERY_PATH)
            .expect("git must diff two trees it just wrote"),
        scoped_paths: Vec::new(),
    });
    store
}

fn call_ids(records: &[AgentActivityRecord]) -> Vec<&str> {
    records
        .iter()
        .map(|record| record.call_id.as_str())
        .collect()
}

// ---------------------------------------------------------------------------
// Attributing a call to the tick that covers it
// ---------------------------------------------------------------------------

#[test]
fn a_new_call_is_attributed_to_the_tick_that_covers_it_and_resolves_to_its_patch() {
    // Given a poll window in which the agent wrote a file, and a log holding that call
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    std::fs::write(repo.path().join("alpha.txt"), "from call a\n").expect("write");
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    let session = tempfile::tempdir().expect("tempdir");
    let log = the_log_after_recording(
        session.path(),
        a_tool_call("call-a").that_wrote(&["alpha.txt"]).build(),
    );

    // When the tick that recorded seq 3 tails the log
    let mut store = a_store_holding_the_tick_between(repo.path(), &head, &before, &after, 3);
    let broadcast = a_tick_over(
        &log,
        &TickAttributionTarget::ThisTicksDelta { seq: 3 },
        &mut BroadcastActivityRows::default(),
        &mut store,
    );

    // Then the record names the tick that holds its change, and that tick's patch is what the call
    // resolves to. Without this step nothing ever calls `attribute`, so every lookup is
    // `UnknownCall` and the record goes out claiming seq 0 — a feature whose halves each work and
    // which as a whole does nothing.
    assert_eq!(
        broadcast.iter().map(|r| r.activity_seq).collect::<Vec<_>>(),
        vec![3]
    );
    let resolved = store
        .delta_for_call("call-a", DeltaScope::Call)
        .expect("the call must resolve to the tick it was attributed to");
    let mirror = a_worktree_with_patch_applied(repo.path(), &head, &resolved.patch);
    assert_eq!(
        std::fs::read(mirror.path().join("alpha.txt")).expect("must exist"),
        b"from call a\n"
    );
}

#[test]
fn two_calls_in_one_tick_share_its_seq_and_each_resolve_to_their_own_patch() {
    // Given one poll window in which two calls each wrote their own file
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    std::fs::write(repo.path().join("alpha.txt"), "from call a\n").expect("write");
    std::fs::write(repo.path().join("beta.txt"), "from call b\n").expect("write");
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    let session = tempfile::tempdir().expect("tempdir");
    append_agent_activity(
        session.path(),
        &a_tool_call("call-a").that_wrote(&["alpha.txt"]).build(),
    )
    .expect("the session's log must be writable");
    let log = the_log_after_recording(
        session.path(),
        a_tool_call("call-b").that_wrote(&["beta.txt"]).build(),
    );

    // When the tick tails the log
    let mut store = a_store_holding_the_tick_between(repo.path(), &head, &before, &after, 1);
    let broadcast = a_tick_over(
        &log,
        &TickAttributionTarget::ThisTicksDelta { seq: 1 },
        &mut BroadcastActivityRows::default(),
        &mut store,
    );

    // Then both calls name the one window that measured them...
    assert_eq!(
        broadcast.iter().map(|r| r.activity_seq).collect::<Vec<_>>(),
        vec![1, 1]
    );
    // ...and each is served only its own file. Sharing a seq must not mean sharing a patch: a call
    // handed its neighbour's change makes a mirror apply that change twice.
    let for_a = store
        .delta_for_call("call-a", DeltaScope::Call)
        .expect("call-a must resolve");
    let mirror = a_worktree_with_patch_applied(repo.path(), &head, &for_a.patch);
    assert_eq!(
        std::fs::read(mirror.path().join("alpha.txt")).expect("must exist"),
        b"from call a\n"
    );
    assert!(
        !mirror.path().join("beta.txt").exists(),
        "beta.txt belongs to call-b and must not appear in call-a's delta"
    );
}

#[test]
fn a_call_in_a_tick_whose_tree_did_not_move_resolves_to_an_empty_patch() {
    // Given a call in a window that changed nothing on disk — a `Read`, a `Grep`, an `Edit` that
    // rewrote a file with what it already held
    let session = tempfile::tempdir().expect("tempdir");
    let log = the_log_after_recording(
        session.path(),
        a_tool_call("call-quiet").that_wrote(&["alpha.txt"]).build(),
    );
    let mut store = a_store();

    // When the tick tails the log having produced no delta of its own
    a_tick_over(
        &log,
        &TickAttributionTarget::AnEmptyDelta {
            next_seq: 0,
            base_commit: "c0ffee".to_string(),
        },
        &mut BroadcastActivityRows::default(),
        &mut store,
    );

    // Then the call resolves to an empty patch rather than to `UnknownCall` (AC9): "that call
    // touched nothing" is an answer a client applies and moves past, where an unknown call reads as
    // a defect on one side or the other.
    assert_eq!(
        store.delta_for_call("call-quiet", DeltaScope::Call),
        Ok(ActivityDelta {
            seq: 0,
            prev_seq: 0,
            base_commit: "c0ffee".to_string(),
            patch: Vec::new(),
            scoped_paths: vec!["alpha.txt".to_string()],
        })
    );
}

#[test]
fn a_tick_with_neither_a_tree_change_nor_a_new_call_numbers_nothing() {
    // Given a log whose only call has already gone out
    let session = tempfile::tempdir().expect("tempdir");
    let log = the_log_after_recording(session.path(), a_tool_call("call-a").build());
    let mut rows = BroadcastActivityRows::default();
    let mut store = a_store();
    a_tick_over(
        &log,
        &TickAttributionTarget::ThisTicksDelta { seq: 0 },
        &mut rows,
        &mut store,
    );

    // When an idle tick tails the same log
    let decided = tick_activity(
        &rows,
        &the_log(session.path()),
        &TickAttributionTarget::AnEmptyDelta {
            next_seq: 1,
            base_commit: "c0ffee".to_string(),
        },
    );

    // Then it says nothing and numbers nothing. An empty delta per idle tick would consume the
    // sequence a client de-duplicates by at the poll rate, and every one of those numbers would
    // read to that client as a delta that never arrived.
    assert_eq!(decided, TickActivity::default());
}

// ---------------------------------------------------------------------------
// Which rows are new
// ---------------------------------------------------------------------------

#[test]
fn a_call_already_broadcast_is_not_broadcast_again_when_the_log_is_read_afresh() {
    // Given a completed call that one tick has already put on the wire
    let session = tempfile::tempdir().expect("tempdir");
    let log = the_log_after_recording(session.path(), a_tool_call("call-a").build());
    let mut rows = BroadcastActivityRows::default();
    let mut store = a_store();
    a_tick_over(
        &log,
        &TickAttributionTarget::ThisTicksDelta { seq: 0 },
        &mut rows,
        &mut store,
    );

    // When the next tick reads the same log, which still holds that call
    let broadcast = a_tick_over(
        &the_log(session.path()),
        &TickAttributionTarget::ThisTicksDelta { seq: 1 },
        &mut rows,
        &mut store,
    );

    // Then it is not sent again. The log is read whole on every tick — it is coalesced by call, not
    // consumed — so "new" has to be a property of the row, or a room would replay its entire
    // history at the poll rate.
    assert_eq!(call_ids(&broadcast), Vec::<&str>::new());
}

#[test]
fn a_calls_terminal_row_supersedes_its_running_row_exactly_once() {
    // Given a call whose `running` row has already gone out
    let session = tempfile::tempdir().expect("tempdir");
    let running = the_log_after_recording(
        session.path(),
        a_tool_call("call-a").still_running().build(),
    );
    let mut rows = BroadcastActivityRows::default();
    let mut store = a_store();
    a_tick_over(
        &running,
        &TickAttributionTarget::ThisTicksDelta { seq: 0 },
        &mut rows,
        &mut store,
    );

    // When the call finishes and two further ticks read the log
    let finished = the_log_after_recording(session.path(), a_tool_call("call-a").build());
    let on_finishing = a_tick_over(
        &finished,
        &TickAttributionTarget::ThisTicksDelta { seq: 1 },
        &mut rows,
        &mut store,
    );
    let tick_after = a_tick_over(
        &the_log(session.path()),
        &TickAttributionTarget::ThisTicksDelta { seq: 2 },
        &mut rows,
        &mut store,
    );

    // Then the result reaches the room once: a client that heard only the `running` row would watch
    // a call that has long since finished, and one that heard the terminal row on every tick would
    // be told the same call finished forever.
    assert_eq!(
        on_finishing
            .iter()
            .map(|record| record.status.as_str())
            .collect::<Vec<_>>(),
        vec![STATUS_COMPLETED]
    );
    assert_eq!(call_ids(&tick_after), Vec::<&str>::new());
}

// ---------------------------------------------------------------------------
// When there is nothing to attribute against
// ---------------------------------------------------------------------------

#[test]
fn a_call_is_left_unattributed_when_the_checkouts_head_could_not_be_read() {
    // Given a tick that produced no delta and cannot name the commit an empty one would apply onto
    let session = tempfile::tempdir().expect("tempdir");
    let log = the_log_after_recording(session.path(), a_tool_call("call-a").build());

    // When it tails the log
    let decided = tick_activity(
        &BroadcastActivityRows::default(),
        &log,
        &TickAttributionTarget::AnEmptyDelta {
            next_seq: 7,
            base_commit: String::new(),
        },
    );

    // Then the call still reaches the room, carrying the wire's "no tick has covered it yet", and
    // no delta is invented for it. A patch whose base is unknown is worse than no patch: the client
    // compares `base_commit` against its own HEAD, an empty string matches nothing, and it would
    // reconcile against a delta claiming nothing changed forever.
    assert_eq!(
        decided
            .broadcast
            .iter()
            .map(|record| record.activity_seq)
            .collect::<Vec<_>>(),
        vec![0]
    );
    assert_eq!(decided.empty_delta, None);
}

#[test]
fn a_room_measuring_no_checkout_of_its_own_broadcasts_its_calls_unattributed() {
    // Given a split session: the agent is here, its files are on the codebase daemon, and this room
    // therefore records no deltas at all
    let session = tempfile::tempdir().expect("tempdir");
    let log = the_log_after_recording(session.path(), a_tool_call("call-a").build());

    // When the tick tails the log
    let decided = tick_activity(
        &BroadcastActivityRows::default(),
        &log,
        &TickAttributionTarget::NoCheckout,
    );

    // Then the call is still announced — the room is where a participant learns what the agent did,
    // whichever host holds the checkout — but names no tick, because a seq pointing into a ring
    // this room never fills would send every client to reconcile against nothing.
    assert_eq!(call_ids(&decided.broadcast), vec!["call-a"]);
    assert_eq!(
        decided
            .broadcast
            .iter()
            .map(|record| record.activity_seq)
            .collect::<Vec<_>>(),
        vec![0]
    );
    assert_eq!(decided.empty_delta, None);
}
