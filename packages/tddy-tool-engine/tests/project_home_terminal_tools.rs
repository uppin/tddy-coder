//! `ReadSessionTerminal` answers what the daemon's port read, and says why when it read nothing
//! (`#home-inbox` 2/8).
//!
//! The daemon's side (the store lookup, the lock) is the port's: here it is a fake that holds one
//! grid per session, so the tool's own rules are what is read.

use std::collections::HashMap;

use async_trait::async_trait;
use tddy_task::{GridCursor, TextGrid};
use tddy_tool_engine::project_home_disk_refresh::RefreshQueued;
use tddy_tool_engine::project_home_terminal_tools::{
    execute_project_home_terminal_tool, ProjectHomeTerminalPort, READ_SESSION_TERMINAL,
};
use tddy_tool_engine::project_home_tools::{
    ListProjectSessionsResult, ListedSession, ProjectHomePort,
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

fn the_home() -> HomeCaller {
    HomeCaller {
        session_id: "home".to_string(),
        project_id: PROJECT.to_string(),
    }
}

/// What this host's terminals hold: a grid per session, or the reason there is none.
struct FakeTerminals {
    records: Vec<TargetRecord>,
    screens: HashMap<String, Result<TextGrid, String>>,
}

fn a_record(session_id: &str) -> TargetRecord {
    TargetRecord {
        session_id: session_id.to_string(),
        project_id: PROJECT.to_string(),
        worktree: Some(std::path::PathBuf::from(format!("/worktrees/{session_id}"))),
        branch: format!("feature/{session_id}"),
        base_ref: "origin/main".to_string(),
        is_active: true,
    }
}

fn a_grid() -> TextGrid {
    TextGrid {
        lines: vec!["Proceed? (y/n)".to_string(), String::new()],
        cursor: GridCursor {
            row: 0,
            col: 15,
            visible: true,
        },
        cols: 80,
        rows: 2,
        offset: 4096,
        bracketed_paste: true,
    }
}

fn a_host_whose(session_id: &str, shows: Result<TextGrid, String>) -> FakeTerminals {
    FakeTerminals {
        records: vec![a_record("home"), a_record(session_id)],
        screens: HashMap::from([(session_id.to_string(), shows)]),
    }
}

#[async_trait]
impl ProjectHomeTerminalPort for FakeTerminals {
    async fn read_terminal(&self, target: &SessionTarget) -> anyhow::Result<TextGrid> {
        match self.screens.get(&target.session_id) {
            Some(Ok(grid)) => Ok(grid.clone()),
            Some(Err(reason)) => anyhow::bail!("{reason}"),
            None => anyhow::bail!("no terminal of session {}", target.session_id),
        }
    }
}

#[async_trait]
impl ProjectHomePort for FakeTerminals {
    async fn target_records(&self) -> anyhow::Result<Vec<TargetRecord>> {
        Ok(self.records.clone())
    }

    async fn list_sessions(&self, _: &str, _: &str) -> anyhow::Result<Vec<ListedSession>> {
        anyhow::bail!("not used by ReadSessionTerminal")
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
        anyhow::bail!("not used by ReadSessionTerminal")
    }

    async fn get_session_start(&self, _: &str, _: &str) -> anyhow::Result<StartLedgerRecord> {
        anyhow::bail!("not used by ReadSessionTerminal")
    }

    async fn cancel_session_start(&self, _: &str, _: &str) -> anyhow::Result<()> {
        anyhow::bail!("not used by ReadSessionTerminal")
    }

    async fn push_identity(&self, _: &SessionTarget) -> anyhow::Result<PushIdentity> {
        anyhow::bail!("not used by ReadSessionTerminal")
    }

    async fn list_worktrees(&self, _: &str) -> anyhow::Result<Vec<WorktreeEntry>> {
        anyhow::bail!("not used by ReadSessionTerminal")
    }

    async fn session_activity(&self, _: &SessionTarget) -> anyhow::Result<SessionActivity> {
        anyhow::bail!("not used by ReadSessionTerminal")
    }

    async fn session_transcript(&self, _: &SessionTarget) -> anyhow::Result<Vec<TranscriptItem>> {
        anyhow::bail!("not used by ReadSessionTerminal")
    }

    async fn session_tool_call(
        &self,
        _: &SessionTarget,
        _: &str,
    ) -> anyhow::Result<Option<ToolCallBodies>> {
        anyhow::bail!("not used by ReadSessionTerminal")
    }

    async fn start_options(&self, _: &str, _: StartOptionsQuery) -> anyhow::Result<StartOptions> {
        anyhow::bail!("not used by ReadSessionTerminal")
    }

    async fn terminate_session(&self, _: &SessionTarget) -> anyhow::Result<()> {
        anyhow::bail!("not used by ReadSessionTerminal")
    }

    async fn refresh_worktree_disk_usage(
        &self,
        _: &str,
        _: Option<&str>,
    ) -> anyhow::Result<RefreshQueued> {
        anyhow::bail!("not used by ReadSessionTerminal")
    }
}

async fn reading(host: &FakeTerminals, args: serde_json::Value) -> ToolOutcome {
    execute_project_home_terminal_tool(host, &the_home(), READ_SESSION_TERMINAL, &args.to_string())
        .await
}

fn answer_of(outcome: &ToolOutcome) -> serde_json::Value {
    assert!(!outcome.is_error, "{}", outcome.error_message);
    serde_json::from_str(&outcome.result_json).expect("the answer is JSON")
}

#[tokio::test]
async fn the_answer_is_the_lines_cursor_size_and_offset_of_the_grid() {
    // Given a session whose terminal shows a prompt
    let host = a_host_whose("s1", Ok(a_grid()));

    // When the home session reads it
    let answer = answer_of(&reading(&host, serde_json::json!({ "session_id": "s1" })).await);

    // Then it is the grid, in the shape the tool documents
    assert_eq!(
        answer,
        serde_json::json!({
            "lines": ["Proceed? (y/n)", ""],
            "cursor": { "row": 0, "col": 15, "visible": true },
            "cols": 80,
            "rows": 2,
            "offset": 4096,
        })
    );
}

#[tokio::test]
async fn a_terminal_that_cannot_be_read_is_refused_with_the_ports_reason() {
    // Given a session whose terminal has not reported its size
    let host = a_host_whose(
        "s1",
        Err("session s1's terminal size is not known yet".to_string()),
    );

    // When the home session reads it
    let outcome = reading(&host, serde_json::json!({ "session_id": "s1" })).await;

    // Then the refusal names the tool and the reason
    assert!(outcome.is_error);
    assert!(
        outcome.error_message.contains(READ_SESSION_TERMINAL)
            && outcome
                .error_message
                .contains("terminal size is not known yet"),
        "{}",
        outcome.error_message
    );
}
