//! Acceptance: an orchestrator keeps a ledger of its sessions in the sessions themselves and cleans
//! up after them — `SetSessionMark` marks a session paused (which also stops it), blocked or done,
//! any resume clears a paused mark, and `DeleteSession` deletes a stopped session — on its own host
//! or another (`#orchestrators` 5/11). The worktree tools' cases are beside the other worktree
//! tools, in `worktree_tools_acceptance.rs`; this file also drives them across hosts.
//!
//! A running session's process is stood in for by a `sleep` child whose pid is recorded in the
//! session's `.session.yaml`, which is where `SignalSession` and the listing read it from. The
//! caller is the project's main (`project-home`).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-10-session-lifecycle-tools.md.
//! Changeset: docs/dev/1-WIP/2026-11-10-session-lifecycle-tools.md

mod support;

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use support::{a_host, a_workspace_in, Host, InProcessPeers, HOST_A, HOST_B, THIS_PROJECT};
use tddy_core::orchestration_mark::{OrchestrationMark, OrchestrationMarkKind};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_service::proto::session::{
    ListSessionsRequest, ResumeSessionRequest, SessionService as _,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

/// The note the tests mark sessions with.
const NOTE: &str = "waiting on #881 to land";

// ── the world ────────────────────────────────────────────────────────────────────────────────

/// Two hosts of one project; main runs on host-a.
struct World {
    a: Host,
    b: Host,
    main: String,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let main = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&main);
    World { a, b, main }
}

/// A process standing in for a session's; killed when dropped, so a failing test leaks none.
struct StandIn(Child);

impl Drop for StandIn {
    fn drop(&mut self) {
        self.0.kill().ok();
        self.0.wait().ok();
    }
}

impl StandIn {
    /// How the process exited, waiting a few seconds for it; `None` while it still runs.
    fn exit_signal(&mut self) -> Option<i32> {
        use std::os::unix::process::ExitStatusExt;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                return status.signal();
            }
            if std::time::Instant::now() > deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Whether the process is still running once a short quiet period has passed.
    fn is_still_running(&mut self) -> bool {
        std::thread::sleep(Duration::from_millis(300));
        self.0.try_wait().unwrap().is_none()
    }
}

impl Host {
    fn session_dir(&self, session_id: &str) -> PathBuf {
        unified_session_dir_path(self.sessions.path(), session_id)
    }

    /// A session of the project on this host whose process is running.
    async fn a_running_session(&self) -> (String, StandIn) {
        let session_id = a_workspace_in(&self.daemon, THIS_PROJECT).await;
        let child = Command::new("sleep").arg("600").spawn().unwrap();
        let dir = self.session_dir(&session_id);
        let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
        meta.pid = Some(child.id());
        tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
        (session_id, StandIn(child))
    }

    /// A session of the project on this host with no process.
    async fn a_stopped_session(&self) -> String {
        a_workspace_in(&self.daemon, THIS_PROJECT).await
    }

    /// The mark `session_id`'s `.session.yaml` records.
    fn recorded_mark_of(&self, session_id: &str) -> Option<OrchestrationMark> {
        tddy_core::read_session_metadata(&self.session_dir(session_id))
            .unwrap()
            .orchestration_mark
    }

    /// Record `mark` on `session_id` directly in its `.session.yaml`, as `SetSessionMark` would.
    fn records_the_mark(&self, session_id: &str, mark: OrchestrationMark) {
        let dir = self.session_dir(session_id);
        let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
        meta.orchestration_mark = Some(mark);
        tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
    }

    /// The mark `ListSessions` lists for `session_id`, as `(mark, note, set_by_session_id)`.
    async fn listed_mark_of(&self, session_id: &str) -> Option<(String, String, String)> {
        let listed = self
            .daemon
            .list_sessions(Request::direct(ListSessionsRequest {
                session_token: TEST_TOKEN.to_string(),
            }))
            .await
            .expect("ListSessions")
            .into_inner();
        let entry = listed
            .sessions
            .into_iter()
            .find(|entry| entry.session_id == session_id)
            .expect("the session is listed");
        entry
            .orchestration_mark
            .map(|mark| (mark.mark, mark.note, mark.set_by_session_id))
    }

    /// Resume `session_id` as the web's Resume does: the `ResumeSession` RPC.
    async fn resumes(&self, session_id: &str) {
        self.daemon
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .expect("the session resumes");
    }

    fn holds_session(&self, session_id: &str) -> bool {
        self.session_dir(session_id).exists()
    }
}

// ── what a tool call answered ────────────────────────────────────────────────────────────────

/// What came back from a tool call as the agent sees it: an answer, or why there is none. A call
/// the daemon refuses outright is a refusal here too, so a missing tool reads as one.
struct Answer {
    refused: bool,
    text: String,
}

impl Answer {
    fn assert_answered(&self) -> serde_json::Value {
        assert!(
            !self.refused,
            "expected an answer, was refused: {}",
            self.text
        );
        serde_json::from_str(&self.text).expect("the answer is JSON")
    }

    fn assert_refused_saying(&self, reason: &str) -> &Self {
        assert!(self.refused, "expected a refusal, got {}", self.text);
        assert!(
            self.text.contains(reason),
            "refused, but not for {reason:?}: {}",
            self.text
        );
        self
    }
}

async fn call(host: &Host, caller: &str, tool: &str, args: serde_json::Value) -> Answer {
    let result = host
        .daemon
        .execute_tool(Request::direct(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: caller.to_string(),
            tool_name: tool.to_string(),
            args_json: args.to_string(),
            daemon_instance_id: String::new(),
            conversation_id: String::new(),
        }))
        .await;
    match result {
        Ok(response) => {
            let response = response.into_inner();
            Answer {
                refused: response.is_error,
                text: if response.is_error {
                    response.error_message
                } else {
                    response.result_json
                },
            }
        }
        Err(status) => Answer {
            refused: true,
            text: format!("{:?}: {}", status.code(), status.message()),
        },
    }
}

impl World {
    /// `SetSessionMark` from main for `session_id` on host-a.
    async fn main_marks(&self, session_id: &str, mark: &str) -> Answer {
        call(
            &self.a,
            &self.main,
            "SetSessionMark",
            serde_json::json!({ "session_id": session_id, "mark": mark, "note": NOTE }),
        )
        .await
    }

    /// `DeleteSession` from main for `session_id` on host-a.
    async fn main_deletes(&self, session_id: &str) -> Answer {
        call(
            &self.a,
            &self.main,
            "DeleteSession",
            serde_json::json!({ "session_id": session_id }),
        )
        .await
    }

    /// `tool` from main with `args`, naming host-b.
    async fn main_calls_on_host_b(&self, tool: &str, mut args: serde_json::Value) -> Answer {
        args["daemon_instance_id"] = serde_json::json!(HOST_B);
        call(&self.a, &self.main, tool, args).await
    }
}

/// A mark as the record holds it, ignoring when it was set.
fn the_mark(
    mark: &Option<OrchestrationMark>,
) -> Option<(OrchestrationMarkKind, Option<&str>, &str)> {
    mark.as_ref().map(|mark| {
        (
            mark.mark,
            mark.note.as_deref(),
            mark.set_by_session_id.as_str(),
        )
    })
}

fn listed(mark: &str, note: &str, set_by: &str) -> Option<(String, String, String)> {
    Some((mark.to_string(), note.to_string(), set_by.to_string()))
}

// ── SetSessionMark ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn marking_a_running_session_paused_stops_it_and_records_the_mark() {
    // Given a running session of the project
    let world = a_world().await;
    let (session, mut process) = world.a.a_running_session().await;

    // When main marks it paused, with a note
    let marked = world.main_marks(&session, "paused").await;

    // Then it is answered with the mark, the note and who set it ...
    let answer = marked.assert_answered();
    assert_eq!(
        (
            &answer["session_id"],
            &answer["mark"],
            &answer["note"],
            &answer["set_by"]
        ),
        (
            &serde_json::json!(session),
            &serde_json::json!("paused"),
            &serde_json::json!(NOTE),
            &serde_json::json!(world.main)
        )
    );
    assert!(answer["at_unix_ms"].as_u64().is_some(), "{answer}");
    // ... its process got SIGTERM, as TerminateSession sends it ...
    assert_eq!(process.exit_signal(), Some(libc::SIGTERM));
    // ... the session's record holds the mark ...
    assert_eq!(
        the_mark(&world.a.recorded_mark_of(&session)),
        Some((
            OrchestrationMarkKind::Paused,
            Some(NOTE),
            world.main.as_str()
        ))
    );
    // ... and the session list carries it
    assert_eq!(
        world.a.listed_mark_of(&session).await,
        listed("paused", NOTE, &world.main)
    );
}

#[tokio::test]
async fn resuming_a_paused_session_clears_its_mark() {
    // Given a stopped session its orchestrator marked paused
    let world = a_world().await;
    let session = world.a.a_stopped_session().await;
    world.a.records_the_mark(
        &session,
        OrchestrationMark {
            mark: OrchestrationMarkKind::Paused,
            note: Some(NOTE.to_string()),
            set_by_session_id: world.main.clone(),
            at_unix_ms: 1_794_000_000_000,
        },
    );

    // When it is resumed, as the web's Resume does
    world.a.resumes(&session).await;

    // Then it is no longer marked, in its record or in the list
    assert_eq!(world.a.recorded_mark_of(&session), None);
    assert_eq!(world.a.listed_mark_of(&session).await, None);
}

#[tokio::test]
async fn a_blocked_or_done_mark_leaves_the_session_running() {
    // Given a running session of the project
    let world = a_world().await;
    let (session, mut process) = world.a.a_running_session().await;

    // When main marks it blocked
    world
        .main_marks(&session, "blocked")
        .await
        .assert_answered();

    // Then it is still running, and marked blocked
    assert!(process.is_still_running(), "a blocked session was stopped");
    assert_eq!(
        the_mark(&world.a.recorded_mark_of(&session)),
        Some((
            OrchestrationMarkKind::Blocked,
            Some(NOTE),
            world.main.as_str()
        ))
    );

    // When main then marks it done
    world.main_marks(&session, "done").await.assert_answered();

    // Then it is still running, and listed as done
    assert!(process.is_still_running(), "a done session was stopped");
    assert_eq!(
        world.a.listed_mark_of(&session).await,
        listed("done", NOTE, &world.main)
    );
}

// ── DeleteSession ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_session_deletes_a_stopped_session_and_refuses_a_running_one() {
    // Given a stopped session and a running one
    let world = a_world().await;
    let stopped = world.a.a_stopped_session().await;
    let (running, mut process) = world.a.a_running_session().await;

    // When main deletes each
    let deleted = world.main_deletes(&stopped).await;
    let refused = world.main_deletes(&running).await;

    // Then the stopped one is gone ...
    assert_eq!(
        deleted.assert_answered()["session_id"],
        serde_json::json!(stopped)
    );
    assert!(
        !world.a.holds_session(&stopped),
        "the stopped session is still there"
    );
    // ... and the running one is refused, told to stop it first, and left as it was
    refused.assert_refused_saying("TerminateSession");
    assert!(
        world.a.holds_session(&running),
        "the running session was deleted"
    );
    assert!(
        process.is_still_running(),
        "the running session was stopped"
    );
}

// ── across hosts ─────────────────────────────────────────────────────────────────────────────

/// The path `ListWorktrees` on host-b lists `worktree` under.
async fn listed_on_host_b(world: &World, worktree: &Path) -> String {
    let listing = world
        .main_calls_on_host_b("ListWorktrees", serde_json::json!({}))
        .await
        .assert_answered();
    let wanted = worktree.canonicalize().unwrap();
    listing["worktrees"]
        .as_array()
        .expect("a worktrees array")
        .iter()
        .map(|row| row["path"].as_str().unwrap().to_string())
        .find(|path| Path::new(path).canonicalize().ok().as_deref() == Some(wanted.as_path()))
        .unwrap_or_else(|| panic!("{} is not listed on host-b: {listing}", worktree.display()))
}

#[tokio::test]
async fn all_five_tools_reach_another_host() {
    // Given four stopped sessions of the project on host-b; main is on host-a. One's worktree holds
    // build output, and another's worktree was deleted.
    let world = a_world().await;
    let to_mark = world.b.a_stopped_session().await;
    let to_delete = world.b.a_stopped_session().await;
    let to_clear = world.b.a_stopped_session().await;
    let to_remove = world.b.a_stopped_session().await;
    let build_output = world.b.worktree_of(&to_clear).join("target.o");
    std::fs::write(&build_output, b"object code").unwrap();
    let clear_path = listed_on_host_b(&world, &world.b.worktree_of(&to_clear)).await;
    let remove_path = listed_on_host_b(&world, &world.b.worktree_of(&to_remove)).await;

    // When main marks one blocked, deletes one, clears one's worktree, and deletes then restores
    // the last one's worktree, each naming host-b
    let marked = world
        .main_calls_on_host_b(
            "SetSessionMark",
            serde_json::json!({ "session_id": to_mark, "mark": "blocked", "note": NOTE }),
        )
        .await;
    let deleted = world
        .main_calls_on_host_b(
            "DeleteSession",
            serde_json::json!({ "session_id": to_delete }),
        )
        .await;
    let cleared = world
        .main_calls_on_host_b("ClearWorktree", serde_json::json!({ "path": clear_path }))
        .await;
    let removed = world
        .main_calls_on_host_b("DeleteWorktree", serde_json::json!({ "path": remove_path }))
        .await;
    let worktree_was_gone = !world.b.worktree_of(&to_remove).exists();
    let restored = world
        .main_calls_on_host_b(
            "RestoreWorktree",
            serde_json::json!({ "session_id": to_remove }),
        )
        .await;

    // Then each was done on host-b
    marked.assert_answered();
    assert_eq!(
        the_mark(&world.b.recorded_mark_of(&to_mark)),
        Some((
            OrchestrationMarkKind::Blocked,
            Some(NOTE),
            world.main.as_str()
        ))
    );
    deleted.assert_answered();
    assert!(
        !world.b.holds_session(&to_delete),
        "host-b still holds the deleted session"
    );
    cleared.assert_answered();
    assert!(
        !build_output.exists(),
        "host-b's build output was not cleared"
    );
    removed.assert_answered();
    assert!(worktree_was_gone, "host-b's worktree was not deleted");
    let restored_at = restored.assert_answered()["worktree_path"]
        .as_str()
        .expect("the restored worktree's path")
        .to_string();
    assert!(
        Path::new(&restored_at).is_dir(),
        "host-b's worktree was not restored at {restored_at}"
    );
}
