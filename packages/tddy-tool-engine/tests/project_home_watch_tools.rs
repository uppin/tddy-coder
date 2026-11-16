//! The watch tools' own rules — their definitions, how arguments reach the port, and the
//! refusals they give before it is asked (`#activity-wake` 2/3).
//!
//! The watches are the daemon's: here the port is a fake that records what it was asked.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-16-session-watches.md

use std::sync::Mutex;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_tool_engine::project_home_disk_refresh::RefreshQueued;
use tddy_tool_engine::project_home_tools::project_home_offered_catalog;
use tddy_tool_engine::project_home_tools::{
    ListProjectSessionsResult, ListedSession, ProjectHomePort,
};
use tddy_tool_engine::project_home_watch_tools::{
    execute_project_home_watch_tool, project_home_watch_tool_defs, ProjectHomeWatchPort,
    WatchRequest, CANCEL_SESSION_WATCH, CREATE_SESSION_WATCH, RENEW_SESSION_WATCH, WATCH_KINDS,
};
use tddy_tool_engine::project_home_worktree_tools::WorktreeEntry;
use tddy_tool_engine::session_follow::{SessionActivity, ToolCallBodies, TranscriptItem};
use tddy_tool_engine::start_ledger::{StartHandle, StartLedgerRecord};
use tddy_tool_engine::start_options::{StartOptions, StartOptionsQuery};
use tddy_tool_engine::start_session_args::StartSessionArgs;
use tddy_tool_engine::ToolOutcome;
use tddy_worktree_service::session_git_ops::PushIdentity;
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget, TargetRecord};

fn the_home() -> HomeCaller {
    HomeCaller {
        session_id: "home".to_string(),
        project_id: "proj-a".to_string(),
    }
}

/// What the port was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Asked {
    Create(WatchRequest),
    Renew { watch_id: String, ttl_seconds: u64 },
    Cancel { watch_id: String },
}

#[derive(Default)]
struct FakeWatches {
    asked: Mutex<Vec<Asked>>,
}

impl FakeWatches {
    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }
}

#[async_trait]
impl ProjectHomeWatchPort for FakeWatches {
    async fn create_watch(&self, _: &HomeCaller, request: WatchRequest) -> Result<Value, String> {
        self.asked.lock().unwrap().push(Asked::Create(request));
        Ok(json!({ "watch_id": "w-1" }))
    }
    async fn list_watches(&self, _: &HomeCaller) -> Result<Value, String> {
        Ok(json!({ "watches": [] }))
    }
    async fn renew_watch(
        &self,
        _: &HomeCaller,
        watch_id: &str,
        ttl_seconds: u64,
    ) -> Result<Value, String> {
        self.asked.lock().unwrap().push(Asked::Renew {
            watch_id: watch_id.to_string(),
            ttl_seconds,
        });
        Ok(json!({ "watch_id": watch_id }))
    }
    async fn cancel_watch(&self, _: &HomeCaller, watch_id: &str) -> Result<Value, String> {
        self.asked.lock().unwrap().push(Asked::Cancel {
            watch_id: watch_id.to_string(),
        });
        Ok(json!({ "cancelled": watch_id }))
    }
}

#[async_trait]
impl ProjectHomePort for FakeWatches {
    async fn target_records(&self) -> anyhow::Result<Vec<TargetRecord>> {
        anyhow::bail!("not used by the watch tools")
    }
    async fn list_sessions(&self, _: &str, _: &str) -> anyhow::Result<Vec<ListedSession>> {
        anyhow::bail!("not used by the watch tools")
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
        anyhow::bail!("not used by the watch tools")
    }
    async fn get_session_start(&self, _: &str, _: &str) -> anyhow::Result<StartLedgerRecord> {
        anyhow::bail!("not used by the watch tools")
    }
    async fn cancel_session_start(&self, _: &str, _: &str) -> anyhow::Result<()> {
        anyhow::bail!("not used by the watch tools")
    }
    async fn push_identity(&self, _: &SessionTarget) -> anyhow::Result<PushIdentity> {
        anyhow::bail!("not used by the watch tools")
    }
    async fn list_worktrees(&self, _: &str) -> anyhow::Result<Vec<WorktreeEntry>> {
        anyhow::bail!("not used by the watch tools")
    }
    async fn session_activity(&self, _: &SessionTarget) -> anyhow::Result<SessionActivity> {
        anyhow::bail!("not used by the watch tools")
    }
    async fn session_transcript(&self, _: &SessionTarget) -> anyhow::Result<Vec<TranscriptItem>> {
        anyhow::bail!("not used by the watch tools")
    }
    async fn session_tool_call(
        &self,
        _: &SessionTarget,
        _: &str,
    ) -> anyhow::Result<Option<ToolCallBodies>> {
        anyhow::bail!("not used by the watch tools")
    }
    async fn start_options(&self, _: &str, _: StartOptionsQuery) -> anyhow::Result<StartOptions> {
        anyhow::bail!("not used by the watch tools")
    }
    async fn terminate_session(&self, _: &SessionTarget) -> anyhow::Result<()> {
        anyhow::bail!("not used by the watch tools")
    }
    async fn refresh_worktree_disk_usage(
        &self,
        _: &str,
        _: Option<&str>,
    ) -> anyhow::Result<RefreshQueued> {
        anyhow::bail!("not used by the watch tools")
    }
}

async fn calling(host: &FakeWatches, tool: &str, args: Value) -> ToolOutcome {
    execute_project_home_watch_tool(host, &the_home(), tool, &args.to_string()).await
}

#[test]
fn the_watch_tools_are_offered_to_the_project_home_role() {
    let offered: Vec<String> = project_home_offered_catalog()
        .into_iter()
        .map(|def| def.name)
        .filter(|name| name.contains("SessionWatch"))
        .collect();
    assert_eq!(
        offered,
        project_home_watch_tool_defs()
            .into_iter()
            .map(|def| def.name)
            .collect::<Vec<_>>()
    );
}

#[test]
fn create_requires_ttl_seconds_in_its_schema() {
    let create = project_home_watch_tool_defs()
        .into_iter()
        .find(|def| def.name == CREATE_SESSION_WATCH)
        .expect("CreateSessionWatch is defined");
    let schema: Value = serde_json::from_str(&create.input_schema_json).unwrap();
    assert_eq!(schema["required"], json!(["ttl_seconds"]));
}

#[tokio::test]
async fn create_hands_the_port_what_was_asked_with_every_kind_by_default() {
    // Given
    let host = FakeWatches::default();

    // When
    let outcome = calling(
        &host,
        CREATE_SESSION_WATCH,
        json!({ "session_ids": ["s-1"], "ttl_seconds": 600 }),
    )
    .await;

    // Then
    assert!(!outcome.is_error, "{}", outcome.error_message);
    assert_eq!(
        host.asked(),
        vec![Asked::Create(WatchRequest {
            session_ids: vec!["s-1".to_string()],
            started_by_me: false,
            kinds: WATCH_KINDS.iter().map(|k| k.to_string()).collect(),
            ttl_seconds: 600,
        })]
    );
}

#[tokio::test]
async fn create_is_refused_before_the_port_for_bad_arguments() {
    // Given
    let host = FakeWatches::default();
    let cases = [
        (json!({ "session_ids": ["s-1"] }), "ttl_seconds"),
        (
            json!({ "session_ids": ["s-1"], "ttl_seconds": 0 }),
            "ttl_seconds",
        ),
        (
            json!({ "session_ids": ["s-1"], "ttl_seconds": 86_401 }),
            "86400",
        ),
        (
            json!({ "ttl_seconds": 600 }),
            "session_ids or started_by_me",
        ),
        (
            json!({ "session_ids": ["s-1"], "kinds": ["everything"], "ttl_seconds": 600 }),
            "everything",
        ),
    ];

    for (args, says) in cases {
        // When
        let outcome = calling(&host, CREATE_SESSION_WATCH, args.clone()).await;

        // Then
        assert!(
            outcome.is_error && outcome.error_message.contains(says),
            "{args}: answered {} {:?}, not a refusal saying {says:?}",
            outcome.result_json,
            outcome.error_message
        );
    }
    assert_eq!(host.asked(), Vec::<Asked>::new());
}

#[tokio::test]
async fn renew_and_cancel_name_the_watch_to_the_port() {
    // Given
    let host = FakeWatches::default();

    // When
    let renewed = calling(
        &host,
        RENEW_SESSION_WATCH,
        json!({ "watch_id": "w-1", "ttl_seconds": 60 }),
    )
    .await;
    let cancelled = calling(&host, CANCEL_SESSION_WATCH, json!({ "watch_id": "w-1" })).await;

    // Then
    assert!(
        !renewed.is_error && !cancelled.is_error,
        "{} {}",
        renewed.error_message,
        cancelled.error_message
    );
    assert_eq!(
        host.asked(),
        vec![
            Asked::Renew {
                watch_id: "w-1".to_string(),
                ttl_seconds: 60,
            },
            Asked::Cancel {
                watch_id: "w-1".to_string(),
            },
        ]
    );
}
