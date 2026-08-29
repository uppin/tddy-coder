//! Acceptance: what a session room *does* once it is open — `docs/ft/daemon/session-worktree-sync.md`.
//!
//! Every piece of this feature has a suite of its own: `session_activity_wiring_acceptance.rs`
//! pins what a tick measures, `session_activity_delta_acceptance.rs` what the ring retains and how
//! a tick's patch is sliced, `session_room_wiring_acceptance.rs` what a daemon hosting no room
//! answers. All of them pass with the poll loop wired to nothing — which is exactly how
//! `SessionDeltaStore::attribute` came to have no production caller, and every `delta_for_call` to
//! answer `UnknownCall`, with no test able to see it.
//!
//! This suite is the one that can see it. `SessionRoomRegistry::register` is private and a
//! `BroadcastPublisher` cannot be constructed from outside the crate, so there is no seam to inject
//! a room through: the only way to reach the loop that runs inside one is to open a real room, on a
//! real LiveKit server, against a real git checkout, and then watch what the tick does to both.
//!
//! ⚠️ **Slow on purpose, and far outside the ordinary integration-test budget.** Every test here
//! starts (or reuses) a LiveKit container, opens a room in it, and then waits for a poll tick that
//! shells out to git — seconds each, where an integration test is budgeted milliseconds. There is
//! no cheaper way to observe the wiring: the thing under test *is* the task the room spawns.
//! `#[serial]`, because the container is shared, and every wait is bounded by a condition rather
//! than a sleep.
//!
//! Requires a LiveKit server: Docker, or `LIVEKIT_TESTKIT_WS_URL` pointing at a running one.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pretty_assertions::assert_eq;
use prost::Message as _;
use serial_test::serial;
use tddy_core::agent_activity::{append_agent_activity, AgentActivityRecord, STATUS_COMPLETED};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::session_room::{
    ActivityDelta, DaemonRoomHosting, DeltaScope, SessionDeltaStore, SessionRoomRegistry,
};
use tddy_livekit::{connect_client, BroadcastChannel, BroadcastMessage, ConnectedClient};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::MultiRpcService;
use tddy_service::proto::connection::AgentActivityRecord as AgentActivityRecordMessage;
use tddy_service::session_activity::SESSION_ACTIVITY_TOPIC;
use tddy_testing_commons::wait::eventually;

/// The daemon hosting every room below. Its RPC surface is served on `daemon-{instance_id}`, which
/// is the identity a subscriber waits for before it trusts that the room is up.
const INSTANCE_ID: &str = "session-room-sync-host";

/// The LiveKit dev credentials the testkit mints its own tokens with — the daemon under test has to
/// be configured with the same pair to create rooms on that server.
const LK_API_KEY: &str = "devkey";
const LK_API_SECRET: &str = "secret";

/// Short enough that a tick lands inside a test's patience, long enough that a loaded machine is
/// not spending its whole slice shelling out to git. The configured floor is 100ms.
const POLL_INTERVAL_MS: u64 = 200;

/// A tracked file, committed before the room opens, so the daemon's opening measurement already
/// accounts for it and an edit to it is the only thing a following tick has to say.
const SEEDED_FILE: &str = "notes.md";
const SEEDED_CONTENTS: &str = "one\n";
const EDITED_CONTENTS: &str = "one\ntwo\n";

/// Files used only to move the checkout without touching [`SEEDED_FILE`] — a tick has to have
/// written one WIP tree before the next can be a diff against it.
///
/// Two names rather than one written twice, because a measurement counts untracked files and never
/// reads them: rewriting `scratch-1.txt` leaves the snapshot identical, and a tick that measures no
/// change writes no tree. A second file is a change the measurement can see.
const A_SCRATCH_FILE: &str = "scratch-1.txt";
const ANOTHER_SCRATCH_FILE: &str = "scratch-2.txt";

/// The call id the agent's `Edit` is recorded under. Fixed rather than generated: it is the key the
/// whole attribution path is looked up by, and a test that generated it would read as though the
/// value mattered.
const AN_EDIT_CALL: &str = "0199c7a4-6c2c-7c9a-9d1e-3f0a1b2c3d4e";

/// A session this daemon hosts no room for, asked about while it is hosting one for another.
const ANOTHER_SESSION: &str = "1780828020298-not-hosted-here";

/// A cold LiveKit container has to admit every participant, the daemon shells out to git on a
/// blocking pool for each tick, and a delta needs the tick *after* the one that wrote the first
/// tree — so a run can legitimately span several poll intervals on a machine that is also
/// compiling. Well past the integration budget by design (see the module note); the condition
/// decides when to stop, and this only decides when to give up.
const ACTIVITY_TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A room this daemon hosts for a real checkout, measured by a real poll loop.
struct AnOpenSessionRoom {
    session_id: String,
    /// The checkout the room measures — the one the WIP ref is published in.
    worktree: PathBuf,
    /// The session directory, where the agent's `agent-activity.jsonl` is written.
    session_dir: PathBuf,
    /// The commit the checkout is on. Nothing below commits, so it is the base every delta and
    /// every record names.
    head: String,
    rooms: SessionRoomRegistry,
    room_name: String,
    ws_url: String,
    _testkit: LiveKitTestkit,
    _home: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
}

/// Open the room of a session whose checkout lives here, exactly as `StartSession` does: the
/// registry creates it, joins it as `daemon-{instance_id}`, and starts measuring the checkout.
///
/// `suffix` names this test's own session, and therefore its own room, so one test's ticks and
/// broadcasts can never be mistaken for another's on a shared server.
async fn an_open_session_room(suffix: &str) -> AnOpenSessionRoom {
    let testkit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker, or LIVEKIT_TESTKIT_WS_URL)");
    let ws_url = testkit.get_ws_url();
    let session_id = format!("1780828020298-{suffix}");

    let home = tempfile::tempdir().expect("tempdir");
    let worktree = home.path().join("checkout");
    let head = a_session_checkout(&worktree);
    let session_dir = unified_session_dir_path(&home.path().join("sessions"), &session_id);
    std::fs::create_dir_all(&session_dir).expect("create the session directory");

    let config_dir = tempfile::tempdir().expect("tempdir");
    let config_path = config_dir.path().join("daemon.yaml");
    std::fs::write(&config_path, a_daemon_yaml(&ws_url)).expect("write daemon.yaml");
    let config = DaemonConfig::load(&config_path).expect("daemon.yaml must load");

    let rooms = SessionRoomRegistry::new();
    let hosting = DaemonRoomHosting {
        config: &config,
        instance_id: INSTANCE_ID,
        rooms: &rooms,
    }
    .for_worktree(&session_id, &worktree, &session_dir);
    // Nothing here calls into the room's RPC surface — `session_room_acceptance.rs` pins that — so
    // the daemon serves an empty service. What is under test is the task beside it.
    let opened = rooms
        .open(&hosting, MultiRpcService::new(Vec::new()))
        .await
        .expect("opening the session room must succeed")
        .expect("a daemon configured with LiveKit must host a room");
    let room_name = opened.room;

    AnOpenSessionRoom {
        session_id,
        worktree,
        session_dir,
        head,
        rooms,
        room_name,
        ws_url,
        _testkit: testkit,
        _home: home,
        _config_dir: config_dir,
    }
}

impl AnOpenSessionRoom {
    /// Replace a file's contents in one step no poll can catch half-done.
    ///
    /// `std::fs::write` truncates before it writes, so a tick landing in between measures a state
    /// nobody asked for and diffs the next one against it. Writing beside the target and renaming
    /// makes the change atomic.
    fn write_in_worktree(&self, path: &str, contents: &str) {
        let staged = self.worktree.join(format!(".{path}.partial"));
        std::fs::write(&staged, contents).expect("write the replacement beside the target");
        std::fs::rename(&staged, self.worktree.join(path))
            .expect("swap the replacement into place");
    }

    /// Record one of the agent's tool calls the way the agent itself does — by appending to the
    /// session's activity log, which is the only thing the daemon reads them from.
    fn record_activity(&self, record: &AgentActivityRecord) {
        append_agent_activity(&self.session_dir, record).expect("append to agent-activity.jsonl");
    }

    fn delta_ring(&self) -> Arc<Mutex<SessionDeltaStore>> {
        self.rooms
            .delta_store(&self.session_id)
            .expect("a hosted room must have a ring of tick deltas")
    }

    /// Every ref the daemon has published under `refs/tddy/` in the checkout.
    fn tddy_refs(&self) -> String {
        git_ok(
            &self.worktree,
            &["for-each-ref", "--format=%(refname)", "refs/tddy/"],
        )
    }

    fn wip_ref(&self) -> String {
        format!("refs/tddy/session/{}/wip", self.session_id)
    }

    /// The commit the session's WIP ref points at, once a tick has published it.
    async fn await_wip_ref(&self) -> String {
        let expected = self.wip_ref();
        eventually(
            &format!("a poll tick to publish {expected}"),
            ACTIVITY_TIMEOUT,
            || {
                let published = self.tddy_refs();
                (published == expected)
                    .then_some(())
                    .ok_or_else(|| format!("refs/tddy/ held {published:?}"))
            },
        )
        .await;
        git_ok(&self.worktree, &["rev-parse", &expected])
    }

    /// Wait until the ring holds `at_least` tick deltas.
    async fn await_recorded_ticks(&self, at_least: usize) {
        let ring = self.delta_ring();
        eventually(
            &format!("the room to record {at_least} tick delta(s)"),
            ACTIVITY_TIMEOUT,
            || {
                let held = ring.lock().expect("the delta ring").len();
                (held >= at_least)
                    .then_some(())
                    .ok_or_else(|| format!("the ring held {held} tick(s)"))
            },
        )
        .await;
    }

    /// Resume at the start of a poll window, so everything the caller does next is measured by one
    /// tick.
    ///
    /// A call is credited to whichever tick is running when its row lands in
    /// `agent-activity.jsonl`. A tick falling between an edit and the row crediting it credits the
    /// call to a window that does not carry the change, so the delta served for that call is
    /// **empty** — which is exactly how this suite failed under CI load, where the scheduler can
    /// part two adjacent statements by more than a 200 ms tick.
    ///
    /// Nudging the checkout guarantees the next tick has a change to record, so the ring is certain
    /// to grow rather than the wait hanging on an idle tree. `eventually` observes that growth
    /// within 25 ms, leaving most of `POLL_INTERVAL_MS` for the two file writes that follow.
    async fn at_a_fresh_poll_window(&self) {
        let held = self.delta_ring().lock().expect("the delta ring").len();
        self.write_in_worktree(ANOTHER_SCRATCH_FILE, "nudge so this tick records a delta\n");
        self.await_recorded_ticks(held + 1).await;
    }

    /// The delta served for `call_id`, once the daemon has attributed that call to a tick.
    async fn await_delta_for(&self, call_id: &str) -> ActivityDelta {
        let ring = self.delta_ring();
        eventually(
            &format!("the daemon to attribute call {call_id} to a tick"),
            ACTIVITY_TIMEOUT,
            || {
                ring.lock()
                    .expect("the delta ring")
                    .delta_for_call(call_id, DeltaScope::Call)
                    .map_err(|e| format!("the ring answered {e:?}"))
            },
        )
        .await
    }

    /// Join the room the way another participant would and listen on `session.activity`.
    ///
    /// Subscribed before the record is appended, because a broadcast is delivered to whoever is
    /// already in the room and is gone for everyone else.
    async fn a_subscriber_to_session_activity(&self, identity: &str) -> ASessionActivitySubscriber {
        let token = self
            ._testkit
            .generate_token(&self.room_name, identity)
            .expect("a LiveKit token for a subscriber");
        let connection = connect_client(
            &self.ws_url,
            &token,
            &format!("daemon-{INSTANCE_ID}"),
            ACTIVITY_TIMEOUT,
        )
        .await
        .unwrap_or_else(|e| panic!("{identity} must join {}: {e}", self.room_name));
        let records =
            BroadcastChannel::new(connection.room.clone(), SESSION_ACTIVITY_TOPIC).subscribe();
        ASessionActivitySubscriber {
            records,
            _connection: connection,
        }
    }
}

/// A participant of the session room, receiving what is broadcast on `session.activity`.
struct ASessionActivitySubscriber {
    records: tokio::sync::mpsc::UnboundedReceiver<BroadcastMessage>,
    /// The room handle: dropping it leaves the room, so a subscriber that kept only the receiver
    /// would be disconnected before the first record arrived.
    _connection: ConnectedClient,
}

impl ASessionActivitySubscriber {
    async fn next_record(&mut self) -> AgentActivityRecordMessage {
        let message = tokio::time::timeout(ACTIVITY_TIMEOUT, self.records.recv())
            .await
            .expect("an activity record must be broadcast within the timeout")
            .expect("the session.activity subscription must stay open");
        AgentActivityRecordMessage::decode(&message.payload[..])
            .expect("a session.activity payload must decode as an AgentActivityRecord")
    }
}

/// The `daemon.yaml` a daemon that hosts session rooms on `ws_url` is configured with.
fn a_daemon_yaml(ws_url: &str) -> String {
    format!(
        "daemon_instance_id: {INSTANCE_ID}\n\
         session_room:\n  poll_interval_ms: {POLL_INTERVAL_MS}\n\
         livekit:\n  url: {ws_url}\n  api_key: {LK_API_KEY}\n  api_secret: {LK_API_SECRET}\n  \
         common_room: session-room-sync-lobby\n"
    )
}

/// A checkout holding [`SEEDED_FILE`] from its first commit, and the commit it is on.
fn a_session_checkout(root: &Path) -> String {
    std::fs::create_dir_all(root).expect("create the checkout");
    git_ok(root, &["init", "--initial-branch=main"]);
    git_ok(root, &["config", "user.email", "agent@example.com"]);
    git_ok(root, &["config", "user.name", "Agent"]);
    std::fs::write(root.join(SEEDED_FILE), SEEDED_CONTENTS).expect("seed the checkout");
    git_ok(root, &["add", "."]);
    git_ok(root, &["commit", "-m", "seed"]);
    head_commit_of(root)
}

/// The completed `Edit` an agent records for a file it just wrote — the row the writer of the log
/// persists, stamped as `agent_activity_stamping` stamps it.
///
/// `head_commit` and `changed_paths` are stamped by whoever *recorded* the call, never by the room:
/// the coder participant for a tool or cursor-cli session, the daemon for a claude-cli one. So a row
/// in the log already carries the commit the call ran upon and the worktree-relative path it
/// declared, and it is those paths that credit a call with its own slice of a tick.
///
/// `activity_seq` is the one field a row does not arrive with: nothing knows which tick will cover
/// the call until a tick does.
fn an_edit_record(call_id: &str, file_path: &str, head_commit: &str) -> AgentActivityRecord {
    AgentActivityRecord {
        call_id: call_id.to_string(),
        tool_name: "Edit".to_string(),
        input: serde_json::json!({ "file_path": file_path }),
        status: STATUS_COMPLETED.to_string(),
        result: serde_json::Value::Null,
        error_message: String::new(),
        started_unix_ms: 1_780_828_020_298,
        completed_unix_ms: 1_780_828_020_299,
        source: "claude-cli".to_string(),
        head_commit: head_commit.to_string(),
        activity_seq: 0,
        changed_paths: vec![file_path.to_string()],
    }
}

// ---------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------

fn git_ok(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("git must be on PATH: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed in {cwd:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The sha `HEAD` resolves to, checked rather than trusted: a git that could not run answers with
/// an empty string, and so does a daemon whose measurement failed — an unchecked helper would
/// compare one failure against another and call them equal.
fn head_commit_of(root: &Path) -> String {
    let sha = git_ok(root, &["rev-parse", "HEAD"]);
    // The sha is whatever git minted at commit time; only its shape can be pinned here.
    assert!(
        sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
        "git rev-parse HEAD in {root:?} answered {sha:?}, which is not a commit sha"
    );
    sha
}

/// What `git apply` makes of `patch` in a directory holding `path` at `before`.
///
/// A real `git apply` in a directory that is not the session's checkout, because that is what a
/// mirror is: a patch that only applies where it was cut from would be no use to one.
fn applying(patch: &[u8], path: &str, before: &str) -> String {
    let mirror = tempfile::tempdir().expect("tempdir");
    std::fs::write(mirror.path().join(path), before).expect("seed the file the patch applies onto");
    let mut git = Command::new("git")
        .args(["apply", "-"])
        .current_dir(mirror.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("git must be on PATH: {e}"));
    git.stdin
        .take()
        .expect("git apply must take the patch on stdin")
        .write_all(patch)
        .expect("write the patch to git apply");
    let output = git.wait_with_output().expect("git apply must finish");
    assert!(
        output.status.success(),
        "git apply refused the patch:\n{}\n--- patch ---\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(patch)
    );
    std::fs::read_to_string(mirror.path().join(path)).expect("read the patched file")
}

// ---------------------------------------------------------------------------
// A room that is open has somewhere to keep its deltas
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn a_hosted_room_gives_its_session_a_ring_of_tick_deltas() {
    // Given a room this daemon opened for a checkout of its own
    let room = an_open_session_room("has-a-ring").await;

    // When the RPC surface looks for that session's ring
    let ring = room.rooms.delta_store(&room.session_id);

    // Then there is one. It is created with the room and shared by the three things that meet over
    // it — the poll loop that fills it, the report that attributes calls into it, and the stream
    // that serves slices of it — so a room without one would be a room whose ticks reach nobody.
    assert!(
        ring.is_some(),
        "a session whose room is hosted here must have a delta ring here"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn a_hosted_room_gives_no_ring_to_a_session_it_is_not_the_room_of() {
    // Given a daemon hosting a room for one session
    let room = an_open_session_room("ring-is-per-session").await;

    // When it is asked for the ring of a different session
    let ring = room.rooms.delta_store(ANOTHER_SESSION);

    // Then there is none. A ring handed out per *daemon* rather than per session would serve one
    // session's patches for another's calls, which is worse than serving nothing.
    assert!(
        ring.is_none(),
        "a session with no room here must have no delta ring here"
    );
}

// ---------------------------------------------------------------------------
// AC13 — a tick publishes the session's uncommitted state as a ref
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn a_tick_publishes_the_uncommitted_checkout_as_a_commit_parented_on_head() {
    // Given a hosted room whose checkout the agent has just dirtied
    let room = an_open_session_room("wip-ref").await;
    room.write_in_worktree(SEEDED_FILE, EDITED_CONTENTS);

    // When the next poll tick measures it
    let wip_commit = room.await_wip_ref().await;

    // Then the uncommitted state is fetchable, and the object graph says which commit it applies
    // onto: a mirror resets to the ref's *parent* to put its HEAD where every delta's base_commit
    // expects it, and lays the WIP tree over that.
    assert_eq!(
        git_ok(&room.worktree, &["rev-parse", &format!("{wip_commit}^")]),
        room.head
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn closing_a_room_stops_pinning_the_uncommitted_state_it_published() {
    // Given a hosted room that has published its checkout's uncommitted state
    let room = an_open_session_room("wip-ref-release").await;
    room.write_in_worktree(SEEDED_FILE, EDITED_CONTENTS);
    room.await_wip_ref().await;

    // When the session it belongs to is deleted
    room.rooms.close(&room.session_id);

    // Then nothing under `refs/tddy/` names those objects any more. A ref left behind pins a whole
    // worktree of blobs in the repository every checkout of the project shares, for the life of the
    // repository — `git gc` reclaims them only once nothing reaches them.
    assert_eq!(room.tddy_refs(), "");
}

// ---------------------------------------------------------------------------
// AC6 — the call an agent recorded is served the patch its tick produced
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn a_call_recorded_during_a_tick_is_served_the_patch_that_tick_produced() {
    // Given a hosted room that has already written one WIP tree, so the tick that follows has
    // something to diff against — the first tick of a room produces no delta by design
    let room = an_open_session_room("call-delta").await;
    room.write_in_worktree(A_SCRATCH_FILE, "scratch\n");
    room.await_wip_ref().await;

    // ...and a tick boundary just behind us, so the edit and the row crediting it fall in one
    // window rather than being parted by a tick
    room.at_a_fresh_poll_window().await;

    // When the agent makes an `Edit` and the hook records the completed call — that order, because
    // it is the one a PostToolUse hook produces: the tool writes, then the row appears. The reverse
    // is not merely unrealistic, it is the order whose race loses the change silently, since a call
    // credited to a window before its write leaves the delta carrying that write announced by
    // nothing; credited to a window after, the client sees a sequence gap and reconciles.
    room.write_in_worktree(SEEDED_FILE, EDITED_CONTENTS);
    room.record_activity(&an_edit_record(AN_EDIT_CALL, SEEDED_FILE, &room.head));

    // Then the daemon can answer for that call by id — the whole point of the wiring, and the exact
    // thing that was missing when `attribute` had no caller and every lookup was `UnknownCall`
    let delta = room.await_delta_for(AN_EDIT_CALL).await;

    // ...scoped to the file the call declared, naming the commit it applies onto
    assert_eq!(delta.scoped_paths, vec![SEEDED_FILE.to_string()]);
    assert_eq!(delta.base_commit, room.head);

    // ...and it really is the edit: applied to the file as it was, it reproduces the file as it is.
    assert_eq!(
        applying(&delta.patch, SEEDED_FILE, SEEDED_CONTENTS),
        EDITED_CONTENTS
    );
}

// ---------------------------------------------------------------------------
// AC4 — the record itself is broadcast into the room
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn a_recorded_call_is_broadcast_into_the_room_stamped_with_the_tick_it_ran_in() {
    // Given a participant listening on `session.activity`, and a session already past its first
    // recorded tick: seq 0 is both the first delta's number and the wire's "no tick has covered
    // this call yet" sentinel, so a record stamped during it could not tell the two apart
    let room = an_open_session_room("activity-broadcast").await;
    let mut subscriber = room
        .a_subscriber_to_session_activity("probe-activity")
        .await;
    room.write_in_worktree(A_SCRATCH_FILE, "scratch\n");
    room.await_wip_ref().await;
    room.write_in_worktree(ANOTHER_SCRATCH_FILE, "scratch again\n");
    room.await_recorded_ticks(1).await;

    // When the agent records an `Edit` and makes it
    room.record_activity(&an_edit_record(AN_EDIT_CALL, SEEDED_FILE, &room.head));
    room.write_in_worktree(SEEDED_FILE, EDITED_CONTENTS);

    // Then the room is told, and the record carries what a mirror needs to place it: the call it is
    // about, the commit it ran upon, and the tick whose delta covers it
    let record = subscriber.next_record().await;
    assert_eq!(record.call_id, AN_EDIT_CALL);
    assert_eq!(record.head_commit, room.head);
    // The exact number is whichever tick observed the call, which is a timing property of the run.
    // What it must never be is 0 — the value the wire reserves for "no tick has covered it yet",
    // which sends a client to reconcile for a call it was just handed.
    assert!(
        record.activity_seq > 0,
        "a broadcast record must name the tick that covers it, not the unattributed sentinel"
    );
}
