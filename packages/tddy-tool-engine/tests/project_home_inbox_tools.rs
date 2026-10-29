//! `WaitProjectNotifications` and `ReadProjectNotifications`: the tools' own rules — their
//! definitions, how arguments reach the port, and what they answer (`#home-inbox` 7/8).
//!
//! The log is the daemon's: here the port is a fake that records what it was asked and answers a
//! page it was given, so only the tools' rules are read.
//!
//! Changeset: docs/dev/changesets/2026-10-29-home-inbox-home-wait.md

use std::sync::Mutex;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_tool_engine::project_home_disk_refresh::RefreshQueued;
use tddy_tool_engine::project_home_inbox_tools::{
    execute_project_home_inbox_tool, is_project_home_inbox_tool, project_home_inbox_tool_defs,
    NotificationsPage, ProjectHomeInboxPort, MAX_WAIT_MS, READ_PROJECT_NOTIFICATIONS,
    WAIT_PROJECT_NOTIFICATIONS,
};
use tddy_tool_engine::project_home_tools::{
    ListProjectSessionsResult, ListedSession, ProjectHomePort, TARGET_DAEMON_INSTANCE_ID,
};
use tddy_tool_engine::project_home_worktree_tools::WorktreeEntry;
use tddy_tool_engine::session_follow::{
    FollowPage, SessionActivity, ToolCallBodies, TranscriptItem, FOLLOW_PAGE_MAX,
};
use tddy_tool_engine::start_ledger::{StartHandle, StartLedgerRecord};
use tddy_tool_engine::start_options::{StartOptions, StartOptionsQuery};
use tddy_tool_engine::start_session_args::StartSessionArgs;
use tddy_tool_engine::ToolOutcome;
use tddy_worktree_service::session_git_ops::PushIdentity;
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget, TargetRecord};

const PROJECT: &str = "proj-a";
const HOME: &str = "home";

fn the_home() -> HomeCaller {
    HomeCaller {
        session_id: HOME.to_string(),
        project_id: PROJECT.to_string(),
    }
}

/// What the port was asked.
#[derive(Debug, Clone, PartialEq)]
enum Asked {
    Read {
        caller: String,
        cursor: String,
        limit: Option<usize>,
    },
    Wait {
        caller: String,
        cursor: String,
        timeout_ms: u64,
    },
}

/// A port that records each call and answers `page`, or refuses with `refusal`.
struct FakeInbox {
    asked: Mutex<Vec<Asked>>,
    page: NotificationsPage,
    refusal: Option<String>,
}

fn an_empty_page() -> NotificationsPage {
    NotificationsPage {
        page: FollowPage {
            items: Vec::new(),
            next_cursor: "0".to_string(),
            has_more: false,
            clamped_to: None,
        },
        lost: None,
        timed_out: None,
    }
}

fn a_host_answering(page: NotificationsPage) -> FakeInbox {
    FakeInbox {
        asked: Mutex::new(Vec::new()),
        page,
        refusal: None,
    }
}

impl FakeInbox {
    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }

    fn answer(&self) -> Result<NotificationsPage, String> {
        match &self.refusal {
            Some(reason) => Err(reason.clone()),
            None => Ok(self.page.clone()),
        }
    }
}

#[async_trait]
impl ProjectHomeInboxPort for FakeInbox {
    async fn read_notifications(
        &self,
        home: &HomeCaller,
        cursor: &str,
        limit: Option<usize>,
    ) -> Result<NotificationsPage, String> {
        self.asked.lock().unwrap().push(Asked::Read {
            caller: home.session_id.clone(),
            cursor: cursor.to_string(),
            limit,
        });
        self.answer()
    }

    async fn wait_notifications(
        &self,
        home: &HomeCaller,
        cursor: &str,
        timeout_ms: u64,
    ) -> Result<NotificationsPage, String> {
        self.asked.lock().unwrap().push(Asked::Wait {
            caller: home.session_id.clone(),
            cursor: cursor.to_string(),
            timeout_ms,
        });
        self.answer()
    }
}

#[async_trait]
impl ProjectHomePort for FakeInbox {
    async fn target_records(&self) -> anyhow::Result<Vec<TargetRecord>> {
        anyhow::bail!("not used by the inbox tools")
    }
    async fn list_sessions(&self, _: &str, _: &str) -> anyhow::Result<Vec<ListedSession>> {
        anyhow::bail!("not used by the inbox tools")
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
        anyhow::bail!("not used by the inbox tools")
    }
    async fn get_session_start(&self, _: &str, _: &str) -> anyhow::Result<StartLedgerRecord> {
        anyhow::bail!("not used by the inbox tools")
    }
    async fn cancel_session_start(&self, _: &str, _: &str) -> anyhow::Result<()> {
        anyhow::bail!("not used by the inbox tools")
    }
    async fn push_identity(&self, _: &SessionTarget) -> anyhow::Result<PushIdentity> {
        anyhow::bail!("not used by the inbox tools")
    }
    async fn list_worktrees(&self, _: &str) -> anyhow::Result<Vec<WorktreeEntry>> {
        anyhow::bail!("not used by the inbox tools")
    }
    async fn session_activity(&self, _: &SessionTarget) -> anyhow::Result<SessionActivity> {
        anyhow::bail!("not used by the inbox tools")
    }
    async fn session_transcript(&self, _: &SessionTarget) -> anyhow::Result<Vec<TranscriptItem>> {
        anyhow::bail!("not used by the inbox tools")
    }
    async fn session_tool_call(
        &self,
        _: &SessionTarget,
        _: &str,
    ) -> anyhow::Result<Option<ToolCallBodies>> {
        anyhow::bail!("not used by the inbox tools")
    }
    async fn start_options(&self, _: &str, _: StartOptionsQuery) -> anyhow::Result<StartOptions> {
        anyhow::bail!("not used by the inbox tools")
    }
    async fn terminate_session(&self, _: &SessionTarget) -> anyhow::Result<()> {
        anyhow::bail!("not used by the inbox tools")
    }
    async fn refresh_worktree_disk_usage(
        &self,
        _: &str,
        _: Option<&str>,
    ) -> anyhow::Result<RefreshQueued> {
        anyhow::bail!("not used by the inbox tools")
    }
}

async fn calling(host: &FakeInbox, tool: &str, args: Value) -> ToolOutcome {
    execute_project_home_inbox_tool(host, &the_home(), tool, &args.to_string()).await
}

fn answer_of(outcome: &ToolOutcome) -> Value {
    assert!(!outcome.is_error, "{}", outcome.error_message);
    serde_json::from_str(&outcome.result_json).expect("the answer is JSON")
}

fn schema_of(tool: &str) -> Value {
    let def = project_home_inbox_tool_defs()
        .into_iter()
        .find(|def| def.name == tool)
        .unwrap_or_else(|| panic!("{tool} is defined"));
    serde_json::from_str(&def.input_schema_json).expect("the schema is JSON")
}

// ── the definitions ──────────────────────────────────────────────────────────────────────────

#[test]
fn the_two_tools_are_defined_and_recognised_by_name() {
    // Given the tools' definitions
    let names: Vec<String> = project_home_inbox_tool_defs()
        .into_iter()
        .map(|def| def.name)
        .collect();

    // Then they are the wait and the read, and nothing else is an inbox tool
    assert_eq!(
        names,
        vec![WAIT_PROJECT_NOTIFICATIONS, READ_PROJECT_NOTIFICATIONS]
    );
    assert_eq!(
        (
            is_project_home_inbox_tool(WAIT_PROJECT_NOTIFICATIONS),
            is_project_home_inbox_tool(READ_PROJECT_NOTIFICATIONS),
            is_project_home_inbox_tool("ReadSessionActivity"),
        ),
        (true, true, false)
    );
}

#[test]
fn the_wait_schema_bounds_timeout_ms_at_20000() {
    // Given the wait's schema
    let schema = schema_of(WAIT_PROJECT_NOTIFICATIONS);

    // Then its timeout is bounded at the remote-call clamp
    assert_eq!(MAX_WAIT_MS, 20_000);
    assert_eq!(schema["properties"]["timeout_ms"]["maximum"], json!(20_000));
}

#[test]
fn the_read_schema_bounds_limit_at_the_follow_page_maximum() {
    // Given the read's schema
    let schema = schema_of(READ_PROJECT_NOTIFICATIONS);

    // Then its limit is bounded at 200, as the follow tools' are
    assert_eq!(FOLLOW_PAGE_MAX, 200);
    assert_eq!(schema["properties"]["limit"]["maximum"], json!(200));
}

#[test]
fn both_tools_take_a_daemon_instance_id_and_a_cursor_and_require_nothing() {
    for tool in [WAIT_PROJECT_NOTIFICATIONS, READ_PROJECT_NOTIFICATIONS] {
        // Given the tool's schema
        let schema = schema_of(tool);

        // Then it names the host, takes a cursor, and every argument is optional
        assert_eq!(
            (
                schema["properties"][TARGET_DAEMON_INSTANCE_ID]["type"].as_str(),
                schema["properties"]["cursor"]["type"].as_str(),
                schema.get("required").is_none(),
            ),
            (Some("string"), Some("string"), true),
            "{tool}'s schema: {schema}"
        );
    }
}

// ── the wait ─────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_wait_passes_its_cursor_and_timeout_and_the_caller_to_the_port() {
    // Given a host that answers an empty page
    let host = a_host_answering(an_empty_page());

    // When the home agent waits from cursor 7 for two seconds
    calling(
        &host,
        WAIT_PROJECT_NOTIFICATIONS,
        json!({ "cursor": "7", "timeout_ms": 2_000 }),
    )
    .await;

    // Then the port was asked once, for that caller, cursor and timeout
    assert_eq!(
        host.asked(),
        vec![Asked::Wait {
            caller: HOME.to_string(),
            cursor: "7".to_string(),
            timeout_ms: 2_000
        }]
    );
}

#[tokio::test]
async fn a_wait_above_20000_ms_is_capped_before_the_port_is_asked() {
    // Given a host that answers an empty page
    let host = a_host_answering(an_empty_page());

    // When the home agent asks to wait a minute
    calling(
        &host,
        WAIT_PROJECT_NOTIFICATIONS,
        json!({ "timeout_ms": 60_000 }),
    )
    .await;

    // Then the port is asked for 20 s, from no cursor
    assert_eq!(
        host.asked(),
        vec![Asked::Wait {
            caller: HOME.to_string(),
            cursor: String::new(),
            timeout_ms: 20_000
        }]
    );
}

#[tokio::test]
async fn a_wait_that_names_no_timeout_holds_for_the_cap() {
    // Given a host that answers an empty page
    let host = a_host_answering(an_empty_page());

    // When the home agent waits without a timeout
    calling(&host, WAIT_PROJECT_NOTIFICATIONS, json!({})).await;

    // Then the port is asked for the cap: a wait is for waiting
    assert_eq!(
        host.asked(),
        vec![Asked::Wait {
            caller: HOME.to_string(),
            cursor: String::new(),
            timeout_ms: MAX_WAIT_MS
        }]
    );
}

#[tokio::test]
async fn a_timed_out_wait_answers_the_page_with_timed_out_and_no_lost() {
    // Given a host whose wait timed out
    let host = a_host_answering(NotificationsPage {
        timed_out: Some(true),
        ..an_empty_page()
    });

    // When the home agent waits
    let answer = answer_of(&calling(&host, WAIT_PROJECT_NOTIFICATIONS, json!({})).await);

    // Then the answer is the page, flattened, saying it timed out
    assert_eq!(
        answer,
        json!({ "items": [], "next_cursor": "0", "has_more": false, "timed_out": true })
    );
}

#[tokio::test]
async fn a_timeout_that_is_not_a_number_is_refused_before_the_port_is_asked() {
    // Given a host
    let host = a_host_answering(an_empty_page());

    // When the home agent waits "soon"
    let outcome = calling(
        &host,
        WAIT_PROJECT_NOTIFICATIONS,
        json!({ "timeout_ms": "soon" }),
    )
    .await;

    // Then the call is refused, naming the tool, and the port was not asked
    assert!(
        outcome.is_error && outcome.error_message.contains(WAIT_PROJECT_NOTIFICATIONS),
        "{}",
        outcome.error_message
    );
    assert_eq!(host.asked(), Vec::new());
}

// ── the read ─────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_read_passes_its_cursor_and_limit_and_the_caller_to_the_port() {
    // Given a host that answers an empty page
    let host = a_host_answering(an_empty_page());

    // When the home agent reads 25 records after cursor 3
    calling(
        &host,
        READ_PROJECT_NOTIFICATIONS,
        json!({ "cursor": "3", "limit": 25 }),
    )
    .await;

    // Then the port was asked once, for that caller, cursor and limit
    assert_eq!(
        host.asked(),
        vec![Asked::Read {
            caller: HOME.to_string(),
            cursor: "3".to_string(),
            limit: Some(25)
        }]
    );
}

#[tokio::test]
async fn a_read_that_lost_records_answers_how_many() {
    // Given a host whose read lost four records and clamped the limit
    let host = a_host_answering(NotificationsPage {
        lost: Some(4),
        page: FollowPage {
            clamped_to: Some(200),
            ..an_empty_page().page
        },
        ..an_empty_page()
    });

    // When the home agent reads
    let answer =
        answer_of(&calling(&host, READ_PROJECT_NOTIFICATIONS, json!({ "limit": 900 })).await);

    // Then the answer carries lost and clamped_to, and no timed_out
    assert_eq!(
        answer,
        json!({
            "items": [],
            "next_cursor": "0",
            "has_more": false,
            "clamped_to": 200,
            "lost": 4
        })
    );
}

#[tokio::test]
async fn a_refusal_of_the_port_comes_back_as_an_error_naming_why() {
    // Given a host that refuses the cursor
    let host = FakeInbox {
        refusal: Some("invalid cursor \"abc\"".to_string()),
        ..a_host_answering(an_empty_page())
    };

    // When the home agent reads from it
    let outcome = calling(
        &host,
        READ_PROJECT_NOTIFICATIONS,
        json!({ "cursor": "abc" }),
    )
    .await;

    // Then the outcome is an error carrying the reason
    assert!(
        outcome.is_error && outcome.error_message.contains("invalid cursor"),
        "{}",
        outcome.error_message
    );
}

#[tokio::test]
async fn a_name_that_is_not_an_inbox_tool_is_refused() {
    // Given a host
    let host = a_host_answering(an_empty_page());

    // When another tool's name is run through the inbox executor
    let outcome = calling(&host, "ReadSessionActivity", json!({})).await;

    // Then it is refused and the port was not asked
    assert!(outcome.is_error, "{}", outcome.error_message);
    assert_eq!(host.asked(), Vec::new());
}
