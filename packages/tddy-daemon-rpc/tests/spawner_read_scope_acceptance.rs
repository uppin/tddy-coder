//! Acceptance: a session that spawns conversations (a grill-me session) reads the activity of the
//! sessions it started with `ReadSessionActivity` — so it learns the `request_id` of its child's
//! permission prompt and answers it with `AnswerSessionPermission` — and is refused any session it
//! did not start (`#home-inbox` 11/11).
//!
//! The world is `#home-inbox` 6/8's (`project_home_answer_permission_acceptance.rs`): one host with a
//! project-home session, a grill-me session, and a claude-cli child of each. The child's hook is the
//! real `AwaitQuestionAnswer`, held open in a task; the read and the answer are the real exec-tool
//! calls the grill-me agent makes.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! Changeset: docs/dev/1-WIP/2026-10-29-home-inbox-sweep.md

mod support;

use std::sync::Arc;
use std::time::Duration;

use support::{a_host, a_workspace_in, Host, InProcessPeers, HOST_A, THIS_PROJECT};
use tddy_core::pending_question::{write_pending_question, PendingQuestion};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::{Request, Status};
use tddy_service::proto::activity::{
    ActivityService as _, AwaitQuestionAnswerRequest, AwaitQuestionAnswerResponse,
    AwaitQuestionOutcome,
};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ListExecToolsRequest};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const HOOK_TOKEN: &str = "tok-child-hook";
const REQUEST_ID: &str = "req-cargo-publish";
/// How long a test gives the hook to open its wait, and a woken hook to return.
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);

struct World {
    a: Host,
    spawner: String,
    child_of_home: String,
    child_of_spawner: String,
    /// Keeps the orchestrators' stand-in terminals' scripts on disk.
    _terminals: tempfile::TempDir,
}

/// One host holding a project-home session and a spawn-capable (grill-me) session, each with a
/// claude-cli child. Both orchestrators are running: the daemon serves their terminals.
async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let spawner = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.with_recipe(&spawner, "grill-me");
    let child_of_home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_claude_cli_child_of(&child_of_home, &home);
    let child_of_spawner = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_claude_cli_child_of(&child_of_spawner, &spawner);
    let terminals = tempfile::tempdir().unwrap();
    for orchestrator in [&home, &spawner] {
        a.runs_a_terminal_for(orchestrator, &terminals).await;
    }
    World {
        a,
        spawner,
        child_of_home,
        child_of_spawner,
        _terminals: terminals,
    }
}

// ── the sessions ─────────────────────────────────────────────────────────────────────────────

impl Host {
    fn session_dir(&self, session_id: &str) -> std::path::PathBuf {
        unified_session_dir_path(self.sessions.path(), session_id)
    }

    fn with_recipe(&self, session_id: &str, recipe: &str) {
        let dir = self.session_dir(session_id);
        let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
        meta.recipe = Some(recipe.to_string());
        tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
    }

    /// Make `child` a claude-cli session whose hooks authenticate with [`HOOK_TOKEN`], started by
    /// `orchestrator` on this host.
    fn make_claude_cli_child_of(&self, child: &str, orchestrator: &str) {
        let dir = self.session_dir(child);
        let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
        meta.session_type = Some("claude-cli".to_string());
        meta.hook_token = Some(HOOK_TOKEN.to_string());
        tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
        let mut changeset = tddy_core::read_changeset(&dir).unwrap_or_default();
        changeset.orchestrator_session_id = Some(orchestrator.to_string());
        changeset.orchestrator_daemon_instance_id = None;
        tddy_core::write_changeset(&dir, &changeset).unwrap();
    }

    /// `child`'s agent asks for a permission to run `cargo publish`, as `#home-inbox` 3/8 records it.
    fn asks_for_a_permission(&self, child: &str) {
        write_pending_question(
            &self.session_dir(child),
            &PendingQuestion {
                kind: "permission".to_string(),
                question: "Run Bash: cargo publish".to_string(),
                request_id: REQUEST_ID.to_string(),
                tool_name: "Bash".to_string(),
                tool_input_json: r#"{"command":"cargo publish"}"#.to_string(),
                ..Default::default()
            },
        )
        .unwrap();
    }

    /// Start a stand-in terminal as `session_id`'s main one, which only stays alive: the daemon now
    /// serves it, so the session is a present orchestrator.
    async fn runs_a_terminal_for(&self, session_id: &str, scripts: &tempfile::TempDir) {
        use std::os::unix::fs::PermissionsExt;
        let script = scripts.path().join(format!("{session_id}.sh"));
        std::fs::write(&script, "#!/bin/sh\nexec sleep 600\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        self.terminals
            .start(
                session_id,
                self.worktree_of(session_id),
                "claude-opus-4-8",
                script.to_str().unwrap(),
                None,
                None,
            )
            .await
            .expect("the orchestrator's terminal starts");
    }

    /// `child`'s `PermissionRequest` hook, once it is waiting for an answer to [`REQUEST_ID`].
    async fn the_hook_of(&self, child: &str) -> AWaitingHook {
        let service = self.daemon.connection().activity_service();
        let request = AwaitQuestionAnswerRequest {
            session_id: child.to_string(),
            hook_token: HOOK_TOKEN.to_string(),
            os_user: "testdev".to_string(),
            request_id: REQUEST_ID.to_string(),
            permission_suggestions_json: String::new(),
        };
        let waiting = tokio::spawn(async move {
            service
                .await_question_answer(Request::direct(request))
                .await
                .map(|response| response.into_inner())
        });
        let pending = self.daemon.connection().pending_answers();
        let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
        while pending.open_request(REQUEST_ID).is_none() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the hook of {child} never opened its wait"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        AWaitingHook { waiting }
    }
}

/// A child's hook, held in `AwaitQuestionAnswer`.
struct AWaitingHook {
    waiting: tokio::task::JoinHandle<Result<AwaitQuestionAnswerResponse, Status>>,
}

impl AWaitingHook {
    async fn woken_with(self) -> AwaitQuestionAnswerResponse {
        tokio::time::timeout(WAIT_TIMEOUT, self.waiting)
            .await
            .expect("the hook was woken")
            .expect("the hook's wait ran to completion")
            .expect("the hook's wait is answered")
    }
}

// ── what a tool call answered ────────────────────────────────────────────────────────────────

/// What came back from a tool call as the agent sees it: an answer, or why there is none. A call
/// the daemon refuses outright is a refusal here too, so a tool it does not serve reads as one.
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

impl World {
    /// `tool_name` with `args`, called by `caller`'s agent.
    async fn calls(&self, caller: &str, tool_name: &str, args: serde_json::Value) -> Answer {
        let result = self
            .a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: caller.to_string(),
                tool_name: tool_name.to_string(),
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

    /// `caller` reads `target`'s activity, the question it waits on included.
    async fn reads_the_activity_of(&self, caller: &str, target: &str) -> Answer {
        self.calls(
            caller,
            "ReadSessionActivity",
            serde_json::json!({ "session_id": target }),
        )
        .await
    }

    /// The exec tools `session_id`'s agent is offered.
    async fn tools_offered_to(&self, session_id: &str) -> Vec<String> {
        self.a
            .daemon
            .list_exec_tools(Request::direct(ListExecToolsRequest {
                session_token: TEST_TOKEN.to_string(),
                daemon_instance_id: String::new(),
                session_id: session_id.to_string(),
            }))
            .await
            .expect("the session's tools are listed")
            .into_inner()
            .tools
            .into_iter()
            .map(|tool| tool.name)
            .collect()
    }
}

fn answered(option_id: &str, by: &str, message: &str) -> AwaitQuestionAnswerResponse {
    AwaitQuestionAnswerResponse {
        outcome: AwaitQuestionOutcome::Answered as i32,
        option_id: option_id.to_string(),
        cancelled: false,
        message: message.to_string(),
        answered_by: by.to_string(),
        ..Default::default()
    }
}

// ── the tests ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_grill_me_spawner_is_offered_read_session_activity() {
    // Given a grill-me session
    let world = a_world().await;

    // When its agent's tools are listed
    let offered = world.tools_offered_to(&world.spawner).await;

    // Then it may read a session's activity beside answering its prompts
    assert!(
        offered.iter().any(|tool| tool == "ReadSessionActivity"),
        "the spawner's tools: {offered:?}"
    );
}

#[tokio::test]
async fn a_grill_me_spawner_reads_its_childs_pending_request_id_and_answers_it() {
    // Given a child the grill-me session started, whose hook waits on Claude's permission prompt
    let world = a_world().await;
    world.a.asks_for_a_permission(&world.child_of_spawner);
    let hook = world.a.the_hook_of(&world.child_of_spawner).await;

    // When the grill-me session reads its child's activity
    let activity = world
        .reads_the_activity_of(&world.spawner, &world.child_of_spawner)
        .await
        .assert_answered();

    // Then it sees the permission the child waits on, with its request_id
    let request_id = activity["pending_question"]["request_id"]
        .as_str()
        .unwrap_or_else(|| panic!("no pending request_id in {activity}"))
        .to_string();
    assert_eq!(request_id, REQUEST_ID, "the pending question's request_id");

    // And when it answers that request, the child's hook wakes with the answer, naming it
    world
        .calls(
            &world.spawner,
            "AnswerSessionPermission",
            serde_json::json!({
                "session_id": world.child_of_spawner,
                "request_id": request_id,
                "option_id": "allow_once",
            }),
        )
        .await
        .assert_answered();
    assert_eq!(
        hook.woken_with().await,
        answered("allow_once", &world.spawner, ""),
        "how the hook's wait ended"
    );
}

#[tokio::test]
async fn a_grill_me_spawner_reading_a_session_it_did_not_start_is_refused_as_permission_denied() {
    // Given a child the project-home session started, waiting on a permission prompt
    let world = a_world().await;
    world.a.asks_for_a_permission(&world.child_of_home);

    // When the grill-me session reads that child's activity
    let refused = world
        .reads_the_activity_of(&world.spawner, &world.child_of_home)
        .await;

    // Then it is refused as permission denied, naming the spawner and why
    refused
        .assert_refused_saying("PermissionDenied")
        .assert_refused_saying(&world.spawner)
        .assert_refused_saying("started");
    // And it was refused for its scope, not for want of the tool: its own child it may read
    world
        .reads_the_activity_of(&world.spawner, &world.child_of_spawner)
        .await
        .assert_answered();
}

#[tokio::test]
async fn a_grill_me_spawner_reading_a_child_whose_changeset_is_unreadable_is_refused() {
    // Given a child the grill-me session started, whose changeset can no longer be parsed
    let world = a_world().await;
    std::fs::write(
        world
            .a
            .session_dir(&world.child_of_spawner)
            .join("changeset.yaml"),
        "orchestrator_session_id: [not, a, string\n",
    )
    .unwrap();

    // When the grill-me session reads that child's activity
    let refused = world
        .reads_the_activity_of(&world.spawner, &world.child_of_spawner)
        .await;

    // Then it is refused, as it cannot be shown to have started it: the scope fails closed
    refused
        .assert_refused_saying("PermissionDenied")
        .assert_refused_saying(&world.spawner)
        .assert_refused_saying("started");
}
