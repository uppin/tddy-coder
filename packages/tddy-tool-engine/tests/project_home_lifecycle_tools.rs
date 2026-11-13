//! `SetSessionMark`, `DeleteSession`, `ClearWorktree`, `DeleteWorktree` and `RestoreWorktree`: the
//! tools' own rules — how arguments reach the port, what is refused before it is asked, and what
//! they answer (`#orchestrators` 5/11).
//!
//! Writing the mark, deleting and the git work are the daemon's: here the port is a fake that
//! records what it was asked and answers what it was given, so only the tools' rules are read.
//!
//! Changeset: docs/dev/1-WIP/2026-11-10-session-lifecycle-tools.md

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_core::orchestration_mark::{OrchestrationMark, OrchestrationMarkKind};
use tddy_tool_engine::project_home_disk_refresh::RefreshQueued;
use tddy_tool_engine::project_home_lifecycle_tools::{
    execute_project_home_lifecycle_tool, ProjectHomeLifecyclePort, SessionMark, CLEAR_WORKTREE,
    DELETE_SESSION, DELETE_WORKTREE, RESTORE_WORKTREE, SET_SESSION_MARK,
};
use tddy_tool_engine::project_home_tools::{
    is_project_home_target_tool, ListProjectSessionsResult, ListedSession, ProjectHomePort,
};
use tddy_tool_engine::project_home_worktree_tools::WorktreeEntry;
use tddy_tool_engine::session_follow::{SessionActivity, ToolCallBodies, TranscriptItem};
use tddy_tool_engine::start_ledger::{StartHandle, StartLedgerRecord};
use tddy_tool_engine::start_options::{StartOptions, StartOptionsQuery};
use tddy_tool_engine::start_session_args::StartSessionArgs;
use tddy_tool_engine::ToolOutcome;
use tddy_worktree_service::session_git_ops::PushIdentity;
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget, TargetRecord};

const PROJECT: &str = "proj-a";
const HOME: &str = "home";
/// A stopped session of the project, its worktree listed.
const IDLE: &str = "idle";
/// A running session of the project, its worktree listed.
const RUNNING: &str = "running";
/// A stopped session of the project whose worktree was deleted: recorded, no longer listed.
const WITHOUT_WORKTREE: &str = "without-worktree";
const ELSEWHERE: &str = "a-session-of-another-project";
/// A listed worktree no session owns.
const ORPHAN_WORKTREE: &str = "/worktrees/orphan";
/// The project's home worktree: the one checkout main and every orchestrator share.
const PROJECT_HOME_WORKTREE: &str = "/worktrees/proj-a/home";
/// An orchestrator of the project, on the home worktree beside main.
const ORCHESTRATOR: &str = "orchestrator";
const NOTE: &str = "waiting on #881";

fn worktree_of(session_id: &str) -> String {
    format!("/worktrees/{session_id}")
}

fn the_home() -> HomeCaller {
    HomeCaller {
        session_id: HOME.to_string(),
        project_id: PROJECT.to_string(),
    }
}

fn a_record(session_id: &str, project_id: &str, is_active: bool) -> TargetRecord {
    a_record_on(&worktree_of(session_id), session_id, project_id, is_active)
}

fn a_record_on(
    worktree: &str,
    session_id: &str,
    project_id: &str,
    is_active: bool,
) -> TargetRecord {
    TargetRecord {
        session_id: session_id.to_string(),
        project_id: project_id.to_string(),
        worktree: Some(PathBuf::from(worktree)),
        branch: format!("feature/{session_id}"),
        base_ref: "origin/main".to_string(),
        is_active,
    }
}

fn a_listed_worktree(path: &str) -> WorktreeEntry {
    WorktreeEntry {
        path: path.to_string(),
        branch_label: "feature".to_string(),
        ..Default::default()
    }
}

/// What the port was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Asked {
    Mark {
        session_id: String,
        mark: Option<(OrchestrationMarkKind, Option<String>, String)>,
    },
    Terminate(String),
    DeleteSession(String),
    Clear(String),
    DeleteWorktree(String),
    Restore(String),
}

/// A host whose sessions are the home, an idle and a running session of the project, a
/// session of it whose worktree is gone, and a session of another project; whose worktrees are
/// theirs and an orphan; which records each action asked of it and does it, or answers `refusal`.
struct FakeHost {
    asked: Mutex<Vec<Asked>>,
    refusal: Option<String>,
}

fn a_host() -> FakeHost {
    FakeHost {
        asked: Mutex::new(Vec::new()),
        refusal: None,
    }
}

fn a_host_refusing(reason: &str) -> FakeHost {
    FakeHost {
        refusal: Some(reason.to_string()),
        ..a_host()
    }
}

impl FakeHost {
    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }

    fn records(&self, asked: Asked) -> Result<(), String> {
        self.asked.lock().unwrap().push(asked);
        match &self.refusal {
            Some(reason) => Err(reason.clone()),
            None => Ok(()),
        }
    }
}

fn shown(path: &Path) -> String {
    path.display().to_string()
}

#[async_trait]
impl ProjectHomeLifecyclePort for FakeHost {
    async fn set_session_mark(
        &self,
        target: &SessionTarget,
        mark: Option<&OrchestrationMark>,
    ) -> Result<(), String> {
        self.records(Asked::Mark {
            session_id: target.session_id.clone(),
            mark: mark.map(|m| (m.mark, m.note.clone(), m.set_by_session_id.clone())),
        })
    }

    async fn delete_session(&self, target: &SessionTarget) -> Result<(), String> {
        self.records(Asked::DeleteSession(target.session_id.clone()))
    }

    async fn clear_worktree(&self, _: &str, path: &Path) -> Result<(), String> {
        self.records(Asked::Clear(shown(path)))
    }

    async fn delete_worktree(&self, _: &str, path: &Path) -> Result<(), String> {
        self.records(Asked::DeleteWorktree(shown(path)))
    }

    async fn project_home_worktree(&self, _: &str) -> Result<PathBuf, String> {
        Ok(PathBuf::from(PROJECT_HOME_WORKTREE))
    }

    async fn restore_worktree(&self, target: &SessionTarget) -> Result<PathBuf, String> {
        self.records(Asked::Restore(target.session_id.clone()))?;
        Ok(target.worktree.clone())
    }
}

#[async_trait]
impl ProjectHomePort for FakeHost {
    async fn target_records(&self) -> anyhow::Result<Vec<TargetRecord>> {
        Ok(vec![
            a_record_on(PROJECT_HOME_WORKTREE, HOME, PROJECT, false),
            a_record_on(PROJECT_HOME_WORKTREE, ORCHESTRATOR, PROJECT, false),
            a_record(IDLE, PROJECT, false),
            a_record(RUNNING, PROJECT, true),
            a_record(WITHOUT_WORKTREE, PROJECT, false),
            a_record(ELSEWHERE, "another-project", false),
        ])
    }

    async fn list_worktrees(&self, _: &str) -> anyhow::Result<Vec<WorktreeEntry>> {
        Ok(vec![
            a_listed_worktree(PROJECT_HOME_WORKTREE),
            a_listed_worktree(&worktree_of(IDLE)),
            a_listed_worktree(&worktree_of(RUNNING)),
            a_listed_worktree(ORPHAN_WORKTREE),
        ])
    }

    async fn terminate_session(&self, target: &SessionTarget) -> anyhow::Result<()> {
        self.asked
            .lock()
            .unwrap()
            .push(Asked::Terminate(target.session_id.clone()));
        Ok(())
    }

    async fn list_sessions(&self, _: &str, _: &str) -> anyhow::Result<Vec<ListedSession>> {
        anyhow::bail!("not used by the lifecycle tools")
    }

    async fn list_peer_sessions(&self, _: &str, _: &str) -> ListProjectSessionsResult {
        ListProjectSessionsResult {
            sessions: Vec::new(),
            unreachable_hosts: Vec::new(),
        }
    }

    async fn start_session(
        &self,
        _: &str,
        _: &str,
        _: StartSessionArgs,
    ) -> anyhow::Result<StartHandle> {
        anyhow::bail!("not used by the lifecycle tools")
    }

    async fn get_session_start(&self, _: &str, _: &str) -> anyhow::Result<StartLedgerRecord> {
        anyhow::bail!("not used by the lifecycle tools")
    }

    async fn cancel_session_start(&self, _: &str, _: &str) -> anyhow::Result<()> {
        anyhow::bail!("not used by the lifecycle tools")
    }

    async fn push_identity(&self, _: &SessionTarget) -> anyhow::Result<PushIdentity> {
        anyhow::bail!("not used by the lifecycle tools")
    }

    async fn session_activity(&self, _: &SessionTarget) -> anyhow::Result<SessionActivity> {
        anyhow::bail!("not used by the lifecycle tools")
    }

    async fn session_transcript(&self, _: &SessionTarget) -> anyhow::Result<Vec<TranscriptItem>> {
        anyhow::bail!("not used by the lifecycle tools")
    }

    async fn session_tool_call(
        &self,
        _: &SessionTarget,
        _: &str,
    ) -> anyhow::Result<Option<ToolCallBodies>> {
        anyhow::bail!("not used by the lifecycle tools")
    }

    async fn start_options(&self, _: &str, _: StartOptionsQuery) -> anyhow::Result<StartOptions> {
        anyhow::bail!("not used by the lifecycle tools")
    }

    async fn refresh_worktree_disk_usage(
        &self,
        _: &str,
        _: Option<&str>,
    ) -> anyhow::Result<RefreshQueued> {
        anyhow::bail!("not used by the lifecycle tools")
    }
}

async fn calling(host: &FakeHost, tool: &str, args: Value) -> ToolOutcome {
    execute_project_home_lifecycle_tool(host, &the_home(), tool, &args.to_string()).await
}

fn answer_of(outcome: &ToolOutcome) -> Value {
    assert!(!outcome.is_error, "{}", outcome.error_message);
    serde_json::from_str(&outcome.result_json).expect("the answer is JSON")
}

fn refusal_of(outcome: &ToolOutcome) -> String {
    assert!(
        outcome.is_error,
        "expected a refusal, got {}",
        outcome.result_json
    );
    outcome.error_message.clone()
}

fn assert_mentions(refusal: &str, words: &[&str]) {
    for word in words {
        assert!(
            refusal.contains(word),
            "the refusal does not name {word:?}: {refusal}"
        );
    }
}

fn marked(session_id: &str, kind: OrchestrationMarkKind) -> Asked {
    Asked::Mark {
        session_id: session_id.to_string(),
        mark: Some((kind, Some(NOTE.to_string()), HOME.to_string())),
    }
}

// ── SessionMark ──────────────────────────────────────────────────────────────────────────────

#[test]
fn the_four_mark_words_parse_and_anything_else_is_refused_naming_them() {
    // When / Then
    assert_eq!(
        SessionMark::parse("paused"),
        Ok(SessionMark::Mark(OrchestrationMarkKind::Paused))
    );
    assert_eq!(
        SessionMark::parse("blocked"),
        Ok(SessionMark::Mark(OrchestrationMarkKind::Blocked))
    );
    assert_eq!(
        SessionMark::parse("done"),
        Ok(SessionMark::Mark(OrchestrationMarkKind::Done))
    );
    assert_eq!(SessionMark::parse("none"), Ok(SessionMark::None));
    assert_mentions(
        &SessionMark::parse("snoozed").unwrap_err(),
        &["snoozed", "paused", "blocked", "done", "none"],
    );
}

// ── SetSessionMark ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn paused_on_a_running_session_records_the_mark_then_stops_it() {
    // Given
    let host = a_host();

    // When the home agent marks the running session paused
    let outcome = calling(
        &host,
        SET_SESSION_MARK,
        json!({ "session_id": RUNNING, "mark": "paused", "note": NOTE }),
    )
    .await;

    // Then the mark was recorded, set by the home, and then the session was terminated ...
    assert_eq!(
        host.asked(),
        vec![
            marked(RUNNING, OrchestrationMarkKind::Paused),
            Asked::Terminate(RUNNING.to_string())
        ]
    );
    // ... and the answer names the mark, its note and who set it
    let answer = answer_of(&outcome);
    assert_eq!(
        (
            &answer["session_id"],
            &answer["mark"],
            &answer["note"],
            &answer["set_by"]
        ),
        (
            &json!(RUNNING),
            &json!("paused"),
            &json!(NOTE),
            &json!(HOME)
        )
    );
    assert!(answer["at_unix_ms"].as_u64().unwrap_or(0) > 0, "{answer}");
}

#[tokio::test]
async fn paused_on_a_stopped_session_records_the_mark_and_stops_nothing() {
    // Given
    let host = a_host();

    // When the home agent marks the stopped session paused
    let outcome = calling(
        &host,
        SET_SESSION_MARK,
        json!({ "session_id": IDLE, "mark": "paused", "note": NOTE }),
    )
    .await;

    // Then only the mark was recorded: there is no process to terminate
    answer_of(&outcome);
    assert_eq!(
        host.asked(),
        vec![marked(IDLE, OrchestrationMarkKind::Paused)]
    );
}

#[tokio::test]
async fn blocked_and_done_record_the_mark_and_stop_nothing() {
    // Given
    let host = a_host();

    // When the home agent marks the running session blocked, then done
    let blocked = calling(
        &host,
        SET_SESSION_MARK,
        json!({ "session_id": RUNNING, "mark": "blocked", "note": NOTE }),
    )
    .await;
    let done = calling(
        &host,
        SET_SESSION_MARK,
        json!({ "session_id": RUNNING, "mark": "done", "note": NOTE }),
    )
    .await;

    // Then both marks were recorded and the session was never terminated
    assert_eq!(answer_of(&blocked)["mark"], json!("blocked"));
    assert_eq!(answer_of(&done)["mark"], json!("done"));
    assert_eq!(
        host.asked(),
        vec![
            marked(RUNNING, OrchestrationMarkKind::Blocked),
            marked(RUNNING, OrchestrationMarkKind::Done)
        ]
    );
}

#[tokio::test]
async fn none_removes_the_mark() {
    // Given
    let host = a_host();

    // When the home agent marks a session none
    let outcome = calling(
        &host,
        SET_SESSION_MARK,
        json!({ "session_id": IDLE, "mark": "none" }),
    )
    .await;

    // Then the port was asked to remove its mark, and the answer says none
    assert_eq!(
        host.asked(),
        vec![Asked::Mark {
            session_id: IDLE.to_string(),
            mark: None
        }]
    );
    assert_eq!(answer_of(&outcome)["mark"], json!("none"));
}

#[tokio::test]
async fn an_unknown_mark_is_refused_before_the_port_is_asked() {
    // Given
    let host = a_host();

    // When the home agent marks a session with a word that is no mark
    let outcome = calling(
        &host,
        SET_SESSION_MARK,
        json!({ "session_id": IDLE, "mark": "snoozed" }),
    )
    .await;

    // Then it is refused naming the four marks, and nothing was asked
    assert_mentions(&refusal_of(&outcome), &["snoozed", "paused", "none"]);
    assert_eq!(host.asked(), Vec::<Asked>::new());
}

#[tokio::test]
async fn a_session_of_another_project_is_not_marked() {
    // Given
    let host = a_host();

    // When the home agent marks a session of another project
    let outcome = calling(
        &host,
        SET_SESSION_MARK,
        json!({ "session_id": ELSEWHERE, "mark": "done" }),
    )
    .await;

    // Then it is refused naming the session, and nothing was asked
    assert_mentions(&refusal_of(&outcome), &[ELSEWHERE]);
    assert_eq!(host.asked(), Vec::<Asked>::new());
}

// ── DeleteSession ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_session_deletes_a_stopped_session() {
    // Given
    let host = a_host();

    // When the home agent deletes the stopped session
    let outcome = calling(&host, DELETE_SESSION, json!({ "session_id": IDLE })).await;

    // Then the port deleted it, and the answer names it
    assert_eq!(host.asked(), vec![Asked::DeleteSession(IDLE.to_string())]);
    assert_eq!(answer_of(&outcome)["session_id"], json!(IDLE));
}

#[tokio::test]
async fn delete_session_refuses_a_running_session_before_the_port_is_asked() {
    // Given
    let host = a_host();

    // When the home agent deletes the running session
    let outcome = calling(&host, DELETE_SESSION, json!({ "session_id": RUNNING })).await;

    // Then it is refused, told to stop it first, and nothing was asked
    assert_mentions(&refusal_of(&outcome), &[RUNNING, "TerminateSession"]);
    assert_eq!(host.asked(), Vec::<Asked>::new());
}

// ── the worktree tools ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn clear_and_delete_reach_the_port_for_an_idle_sessions_listed_worktree() {
    // Given
    let host = a_host();
    let path = worktree_of(IDLE);

    // When the home agent clears, then deletes, the idle session's worktree
    let cleared = calling(&host, CLEAR_WORKTREE, json!({ "path": path })).await;
    let deleted = calling(&host, DELETE_WORKTREE, json!({ "path": path })).await;

    // Then the port did each, at that path, and each answer names it
    assert_eq!(
        host.asked(),
        vec![
            Asked::Clear(path.clone()),
            Asked::DeleteWorktree(path.clone())
        ]
    );
    assert_eq!(answer_of(&cleared)["path"], json!(path));
    assert_eq!(answer_of(&deleted)["path"], json!(path));
}

#[tokio::test]
async fn a_worktree_no_session_owns_can_be_cleared() {
    // Given
    let host = a_host();

    // When the home agent clears an orphan worktree
    let outcome = calling(&host, CLEAR_WORKTREE, json!({ "path": ORPHAN_WORKTREE })).await;

    // Then the port cleared it
    answer_of(&outcome);
    assert_eq!(
        host.asked(),
        vec![Asked::Clear(ORPHAN_WORKTREE.to_string())]
    );
}

#[tokio::test]
async fn a_path_that_is_no_listed_worktree_is_refused_naming_it() {
    // Given
    let host = a_host();

    // When the home agent clears and deletes a path that is not a worktree of the project
    let cleared = calling(
        &host,
        CLEAR_WORKTREE,
        json!({ "path": "/no/such/worktree" }),
    )
    .await;
    let deleted = calling(
        &host,
        DELETE_WORKTREE,
        json!({ "path": "/no/such/worktree" }),
    )
    .await;

    // Then both are refused naming it, and nothing was asked
    assert_mentions(&refusal_of(&cleared), &["/no/such/worktree"]);
    assert_mentions(&refusal_of(&deleted), &["/no/such/worktree"]);
    assert_eq!(host.asked(), Vec::<Asked>::new());
}

#[tokio::test]
async fn every_worktree_tool_is_refused_while_the_owning_session_is_active() {
    // Given
    let host = a_host();
    let path = worktree_of(RUNNING);

    // When the home agent clears, deletes and restores the running session's worktree
    let refusals = [
        refusal_of(&calling(&host, CLEAR_WORKTREE, json!({ "path": path })).await),
        refusal_of(&calling(&host, DELETE_WORKTREE, json!({ "path": path })).await),
        refusal_of(&calling(&host, RESTORE_WORKTREE, json!({ "session_id": RUNNING })).await),
    ];

    // Then each is refused naming the active session, and nothing was asked
    for refusal in &refusals {
        assert_mentions(refusal, &[RUNNING, "active"]);
    }
    assert_eq!(host.asked(), Vec::<Asked>::new());
}

#[tokio::test]
async fn delete_worktree_refuses_the_projects_home_worktree_whoever_calls() {
    // Given an orchestrator calling, whose session shares the project's home worktree with main
    // (both idle, so the activity gate is not why)
    let host = a_host();
    let an_orchestrator = HomeCaller {
        session_id: ORCHESTRATOR.to_string(),
        project_id: PROJECT.to_string(),
    };

    // When it deletes the project's home worktree
    let outcome = execute_project_home_lifecycle_tool(
        &host,
        &an_orchestrator,
        DELETE_WORKTREE,
        &json!({ "path": PROJECT_HOME_WORKTREE }).to_string(),
    )
    .await;

    // Then it is refused as the home worktree, naming its path, and nothing was asked
    assert_mentions(
        &refusal_of(&outcome),
        &["home worktree", PROJECT_HOME_WORKTREE],
    );
    assert_eq!(host.asked(), Vec::<Asked>::new());
}

#[tokio::test]
async fn delete_worktree_passes_the_ports_refusal_through() {
    // Given a host that refuses a dirty worktree
    let host =
        a_host_refusing("has uncommitted changes: CommitSessionWorktree or StashSessionWorktree");

    // When the home agent deletes the idle session's worktree
    let outcome = calling(&host, DELETE_WORKTREE, json!({ "path": worktree_of(IDLE) })).await;

    // Then the refusal names the tool and the port's reason
    assert_mentions(
        &refusal_of(&outcome),
        &[
            DELETE_WORKTREE,
            "CommitSessionWorktree",
            "StashSessionWorktree",
        ],
    );
}

#[tokio::test]
async fn restore_worktree_restores_the_named_sessions_worktree() {
    // Given a stopped session whose worktree was deleted
    let host = a_host();

    // When the home agent restores it by its session id
    let outcome = calling(
        &host,
        RESTORE_WORKTREE,
        json!({ "session_id": WITHOUT_WORKTREE }),
    )
    .await;

    // Then the port restored that session's worktree, and the answer names where
    assert_eq!(
        host.asked(),
        vec![Asked::Restore(WITHOUT_WORKTREE.to_string())]
    );
    let answer = answer_of(&outcome);
    assert_eq!(
        (&answer["session_id"], &answer["worktree_path"]),
        (
            &json!(WITHOUT_WORKTREE),
            &json!(worktree_of(WITHOUT_WORKTREE))
        )
    );
}

#[tokio::test]
async fn restore_worktree_refuses_another_projects_session_before_the_port_is_asked() {
    // Given
    let host = a_host();

    // When the home agent restores a session of another project
    let outcome = calling(&host, RESTORE_WORKTREE, json!({ "session_id": ELSEWHERE })).await;

    // Then it is refused naming the session, and nothing was asked
    assert_mentions(&refusal_of(&outcome), &[ELSEWHERE]);
    assert_eq!(host.asked(), Vec::<Asked>::new());
}

// ── routing ──────────────────────────────────────────────────────────────────────────────────

#[test]
fn all_five_are_target_tools_so_a_call_naming_another_host_is_forwarded() {
    // When / Then
    for tool in [
        SET_SESSION_MARK,
        DELETE_SESSION,
        CLEAR_WORKTREE,
        DELETE_WORKTREE,
        RESTORE_WORKTREE,
    ] {
        assert!(
            is_project_home_target_tool(tool),
            "{tool} is not a target tool"
        );
    }
}
