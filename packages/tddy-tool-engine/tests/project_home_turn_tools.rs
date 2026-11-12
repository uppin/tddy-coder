//! `SendSessionMessage` and `ResumeSession`: the tools' own rules — how arguments reach the port,
//! what is refused before it is asked, and what they answer (`#orchestrators` 4/11).
//!
//! Delivery and resume are the daemon's: here the port is a fake that records what it was asked and
//! answers what it was given, so only the tools' rules are read.
//!
//! Changeset: docs/dev/1-WIP/2026-11-10-session-turn-control.md

use std::sync::Mutex;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_tool_engine::project_home_disk_refresh::RefreshQueued;
use tddy_tool_engine::project_home_tools::{
    is_project_home_target_tool, ListProjectSessionsResult, ListedSession, ProjectHomePort,
};
use tddy_tool_engine::project_home_turn_tools::{
    execute_project_home_turn_tool, ProjectHomeTurnPort, ResumeOutcome, TurnDelivery,
    RESUME_SESSION, SEND_SESSION_MESSAGE,
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
const CHILD: &str = "child";
const ELSEWHERE: &str = "a-session-of-another-project";

fn the_home() -> HomeCaller {
    HomeCaller {
        session_id: HOME.to_string(),
        project_id: PROJECT.to_string(),
    }
}

fn a_record(session_id: &str, project_id: &str) -> TargetRecord {
    TargetRecord {
        session_id: session_id.to_string(),
        project_id: project_id.to_string(),
        worktree: Some(std::path::PathBuf::from(format!("/worktrees/{session_id}"))),
        branch: format!("feature/{session_id}"),
        base_ref: "origin/main".to_string(),
        is_active: true,
    }
}

/// What the port was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Asked {
    Send {
        session_id: String,
        text: String,
    },
    Resume {
        session_id: String,
        message: Option<String>,
    },
}

/// A host whose sessions are the home, a child of the project and a session of another project; it
/// records each delivery or resume asked of it and answers `delivery` / `outcome`, or `refusal`.
struct FakeHost {
    asked: Mutex<Vec<Asked>>,
    delivery: TurnDelivery,
    outcome: ResumeOutcome,
    refusal: Option<String>,
}

fn a_host_delivering_by(delivery: TurnDelivery) -> FakeHost {
    FakeHost {
        asked: Mutex::new(Vec::new()),
        delivery,
        outcome: ResumeOutcome::Resumed,
        refusal: None,
    }
}

fn a_host_resuming_with(outcome: ResumeOutcome) -> FakeHost {
    FakeHost {
        outcome,
        ..a_host_delivering_by(TurnDelivery::TerminalPaste)
    }
}

fn a_host_refusing(reason: &str) -> FakeHost {
    FakeHost {
        refusal: Some(reason.to_string()),
        ..a_host_delivering_by(TurnDelivery::TerminalPaste)
    }
}

impl FakeHost {
    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }

    fn refusal_or<T>(&self, answer: T) -> Result<T, String> {
        match &self.refusal {
            Some(reason) => Err(reason.clone()),
            None => Ok(answer),
        }
    }
}

#[async_trait]
impl ProjectHomeTurnPort for FakeHost {
    async fn send_session_message(
        &self,
        target: &SessionTarget,
        text: &str,
    ) -> Result<TurnDelivery, String> {
        self.asked.lock().unwrap().push(Asked::Send {
            session_id: target.session_id.clone(),
            text: text.to_string(),
        });
        self.refusal_or(self.delivery)
    }

    async fn resume_session(
        &self,
        target: &SessionTarget,
        message: Option<&str>,
    ) -> Result<ResumeOutcome, String> {
        self.asked.lock().unwrap().push(Asked::Resume {
            session_id: target.session_id.clone(),
            message: message.map(str::to_string),
        });
        self.refusal_or(self.outcome)
    }
}

#[async_trait]
impl ProjectHomePort for FakeHost {
    async fn target_records(&self) -> anyhow::Result<Vec<TargetRecord>> {
        Ok(vec![
            a_record(HOME, PROJECT),
            a_record(CHILD, PROJECT),
            a_record(ELSEWHERE, "another-project"),
        ])
    }

    async fn list_sessions(&self, _: &str, _: &str) -> anyhow::Result<Vec<ListedSession>> {
        anyhow::bail!("not used by the turn tools")
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
        anyhow::bail!("not used by the turn tools")
    }

    async fn get_session_start(&self, _: &str, _: &str) -> anyhow::Result<StartLedgerRecord> {
        anyhow::bail!("not used by the turn tools")
    }

    async fn cancel_session_start(&self, _: &str, _: &str) -> anyhow::Result<()> {
        anyhow::bail!("not used by the turn tools")
    }

    async fn push_identity(&self, _: &SessionTarget) -> anyhow::Result<PushIdentity> {
        anyhow::bail!("not used by the turn tools")
    }

    async fn list_worktrees(&self, _: &str) -> anyhow::Result<Vec<WorktreeEntry>> {
        anyhow::bail!("not used by the turn tools")
    }

    async fn session_activity(&self, _: &SessionTarget) -> anyhow::Result<SessionActivity> {
        anyhow::bail!("not used by the turn tools")
    }

    async fn session_transcript(&self, _: &SessionTarget) -> anyhow::Result<Vec<TranscriptItem>> {
        anyhow::bail!("not used by the turn tools")
    }

    async fn session_tool_call(
        &self,
        _: &SessionTarget,
        _: &str,
    ) -> anyhow::Result<Option<ToolCallBodies>> {
        anyhow::bail!("not used by the turn tools")
    }

    async fn start_options(&self, _: &str, _: StartOptionsQuery) -> anyhow::Result<StartOptions> {
        anyhow::bail!("not used by the turn tools")
    }

    async fn terminate_session(&self, _: &SessionTarget) -> anyhow::Result<()> {
        anyhow::bail!("not used by the turn tools")
    }

    async fn refresh_worktree_disk_usage(
        &self,
        _: &str,
        _: Option<&str>,
    ) -> anyhow::Result<RefreshQueued> {
        anyhow::bail!("not used by the turn tools")
    }
}

async fn calling(host: &FakeHost, tool: &str, args: Value) -> ToolOutcome {
    execute_project_home_turn_tool(host, &the_home(), tool, &args.to_string()).await
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

// ── SendSessionMessage ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_message_reaches_the_port_for_its_target_and_answers_how_it_was_delivered() {
    // Given a host that delivers by terminal paste
    let host = a_host_delivering_by(TurnDelivery::TerminalPaste);

    // When the home agent sends the child a message
    let outcome = calling(
        &host,
        SEND_SESSION_MESSAGE,
        json!({ "session_id": CHILD, "text": "carry on" }),
    )
    .await;

    // Then the port was asked once, for that session and text, and the answer says how
    assert_eq!(
        host.asked(),
        vec![Asked::Send {
            session_id: CHILD.to_string(),
            text: "carry on".to_string()
        }]
    );
    assert_eq!(
        answer_of(&outcome),
        json!({ "session_id": CHILD, "delivered_via": "terminal_paste" })
    );
}

#[tokio::test]
async fn an_acp_delivery_answers_acp_prompt() {
    // Given a host that delivers by ACP prompt
    let host = a_host_delivering_by(TurnDelivery::AcpPrompt);

    // When the home agent sends the child a message
    let outcome = calling(
        &host,
        SEND_SESSION_MESSAGE,
        json!({ "session_id": CHILD, "text": "carry on" }),
    )
    .await;

    // Then the answer names the ACP prompt
    assert_eq!(answer_of(&outcome)["delivered_via"], json!("acp_prompt"));
}

#[tokio::test]
async fn an_empty_message_is_refused_before_the_port_is_asked() {
    // Given a host
    let host = a_host_delivering_by(TurnDelivery::TerminalPaste);

    // When the home agent sends a message that is only whitespace
    let outcome = calling(
        &host,
        SEND_SESSION_MESSAGE,
        json!({ "session_id": CHILD, "text": "  \n" }),
    )
    .await;

    // Then it is refused, naming the tool and the text, and nothing was asked of the port
    let refusal = refusal_of(&outcome);
    assert!(
        refusal.contains(SEND_SESSION_MESSAGE) && refusal.contains("text"),
        "refusal: {refusal}"
    );
    assert_eq!(host.asked(), Vec::new());
}

#[tokio::test]
async fn a_message_to_a_session_of_another_project_is_refused_before_the_port_is_asked() {
    // Given a host holding a session of another project
    let host = a_host_delivering_by(TurnDelivery::TerminalPaste);

    // When the home agent sends it a message
    let outcome = calling(
        &host,
        SEND_SESSION_MESSAGE,
        json!({ "session_id": ELSEWHERE, "text": "carry on" }),
    )
    .await;

    // Then the target rules refuse it, and nothing was asked of the port
    assert!(refusal_of(&outcome).contains(ELSEWHERE));
    assert_eq!(host.asked(), Vec::new());
}

#[tokio::test]
async fn a_refusal_of_the_port_comes_back_naming_the_tool_and_why() {
    // Given a host that refuses: the session is stopped
    let host = a_host_refusing("session child is stopped; resume it with ResumeSession");

    // When the home agent sends it a message
    let outcome = calling(
        &host,
        SEND_SESSION_MESSAGE,
        json!({ "session_id": CHILD, "text": "carry on" }),
    )
    .await;

    // Then the refusal is the port's, under the tool's name
    let refusal = refusal_of(&outcome);
    assert!(
        refusal.starts_with(SEND_SESSION_MESSAGE) && refusal.contains("ResumeSession"),
        "refusal: {refusal}"
    );
}

// ── ResumeSession ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_resume_passes_its_message_to_the_port_and_answers_the_outcome() {
    // Given a host that resumes
    let host = a_host_resuming_with(ResumeOutcome::Resumed);

    // When the home agent resumes the child with a message
    let outcome = calling(
        &host,
        RESUME_SESSION,
        json!({ "session_id": CHILD, "message": "pick up where you left off" }),
    )
    .await;

    // Then the port was asked for that session and message, and the answer is resumed
    assert_eq!(
        host.asked(),
        vec![Asked::Resume {
            session_id: CHILD.to_string(),
            message: Some("pick up where you left off".to_string())
        }]
    );
    assert_eq!(
        answer_of(&outcome),
        json!({ "session_id": CHILD, "outcome": "resumed" })
    );
}

#[tokio::test]
async fn a_resume_without_a_message_asks_the_port_for_none_and_answers_running() {
    // Given a host whose session is already running
    let host = a_host_resuming_with(ResumeOutcome::Running);

    // When the home agent resumes it without a message
    let outcome = calling(&host, RESUME_SESSION, json!({ "session_id": CHILD })).await;

    // Then the port was asked for no message, and the answer is running
    assert_eq!(
        host.asked(),
        vec![Asked::Resume {
            session_id: CHILD.to_string(),
            message: None
        }]
    );
    assert_eq!(answer_of(&outcome)["outcome"], json!("running"));
}

// ── across hosts ─────────────────────────────────────────────────────────────────────────────

#[test]
fn both_tools_are_target_tools_the_daemon_forwards_to_the_named_host() {
    assert_eq!(
        (
            is_project_home_target_tool(SEND_SESSION_MESSAGE),
            is_project_home_target_tool(RESUME_SESSION),
        ),
        (true, true)
    );
}

// ── what a project-home session is offered ──────────────────────────────────────────────────

#[test]
fn a_project_home_session_is_offered_both_turn_tools() {
    // Given the catalogue a project-home session is offered
    let offered: Vec<String> = tddy_tool_engine::project_home_tools::project_home_offered_catalog()
        .into_iter()
        .map(|tool| tool.name)
        .collect();

    // Then both turn tools are among them
    assert!(
        offered.iter().any(|name| name == SEND_SESSION_MESSAGE)
            && offered.iter().any(|name| name == RESUME_SESSION),
        "offered: {offered:?}"
    );
}
