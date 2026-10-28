//! Acceptance: the session that started a claude-cli child answers the child's permission prompt
//! with `AnswerSessionPermission`, and the child's waiting hook wakes with that answer
//! (`#home-inbox` 6/8).
//!
//! The child's hook is the real `AwaitQuestionAnswer` of the daemon's own activity surface, held
//! open in a task; the answer is the real exec-tool call a home agent makes. An orchestrator is
//! present when the daemon serves its terminal, so each orchestrator here runs a stand-in terminal
//! that does nothing but stay alive.
//!
//! The child's orchestrator and its pending question are written into its session directory as
//! `#start-parity` 2/9's start and `#home-inbox` 3/8's capture record them, so these tests depend on
//! this node alone.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools,
//! docs/ft/daemon/claude-cli-session.md.
//! Changeset: docs/dev/1-WIP/2026-10-25-home-inbox-permission-answer.md

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
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const HOOK_TOKEN: &str = "tok-child-hook";
const REQUEST_ID: &str = "req-cargo-publish";
/// How long a test gives the hook to open its wait, and a woken hook to return.
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);

struct World {
    a: Host,
    home: String,
    other_home: String,
    spawner: String,
    child_of_home: String,
    child_of_spawner: String,
    /// Keeps the orchestrators' stand-in terminals' scripts on disk.
    _terminals: tempfile::TempDir,
}

/// One host holding two project-home sessions, a spawn-capable (grill-me) session, and a claude-cli
/// child of the first home and of the spawner. Both orchestrators are running: the daemon serves
/// their terminals.
async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let other_home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&other_home);
    let spawner = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.with_recipe(&spawner, "grill-me");
    let child_of_home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_claude_cli_child_of(&child_of_home, &home);
    let child_of_spawner = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_claude_cli_child_of(&child_of_spawner, &spawner);
    let terminals = tempfile::tempdir().unwrap();
    for orchestrator in [&home, &other_home, &spawner] {
        a.runs_a_terminal_for(orchestrator, &terminals).await;
    }
    World {
        a,
        home,
        other_home,
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

    /// `child`'s agent asks a question with `AskUserQuestion`, as `#home-inbox` 3/8 records it.
    fn asks_a_question(&self, child: &str) {
        write_pending_question(
            &self.session_dir(child),
            &PendingQuestion {
                kind: "select".to_string(),
                header: "Language".to_string(),
                question: "Which language?".to_string(),
                options: vec!["Rust".to_string(), "Go".to_string()],
                allow_other: true,
                request_id: REQUEST_ID.to_string(),
                tool_name: "AskUserQuestion".to_string(),
                tool_input_json: r#"{"questions":[{"question":"Which language?"}]}"#.to_string(),
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

impl World {
    /// `AnswerSessionPermission` for [`REQUEST_ID`] on `child`, called by `caller`: `option_id`,
    /// with `message`.
    async fn answers(
        &self,
        caller: &str,
        child: &str,
        option_id: &str,
        message: Option<&str>,
    ) -> Answer {
        let mut args = serde_json::json!({
            "session_id": child,
            "request_id": REQUEST_ID,
            "option_id": option_id,
        });
        if let Some(message) = message {
            args["message"] = serde_json::json!(message);
        }
        let result = self
            .a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: caller.to_string(),
                tool_name: "AnswerSessionPermission".to_string(),
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
}

// ── the tests ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn answer_session_permission_from_the_childs_home_wakes_the_waiting_hook() {
    // Given a child of the home whose hook waits on Claude's permission prompt
    let world = a_world().await;
    world.a.asks_for_a_permission(&world.child_of_home);
    let hook = world.a.the_hook_of(&world.child_of_home).await;

    // When the home allows it once
    let answer = world
        .answers(&world.home, &world.child_of_home, "allow_once", None)
        .await
        .assert_answered();

    // Then the hook wakes with the allow, naming the home
    assert_eq!(
        hook.woken_with().await,
        answered("allow_once", &world.home, ""),
        "how the hook's wait ended"
    );
    // And the tool says what it recorded
    assert_eq!(
        (
            answer["answered_by"].as_str(),
            answer["option_id"].as_str(),
            answer["request_id"].as_str()
        ),
        (
            Some(world.home.as_str()),
            Some("allow_once"),
            Some(REQUEST_ID)
        ),
        "the tool's answer: {answer}"
    );
}

#[tokio::test]
async fn a_caller_that_is_not_the_childs_orchestrator_is_refused_as_permission_denied() {
    // Given a child of the home whose hook waits on Claude's permission prompt
    let world = a_world().await;
    world.a.asks_for_a_permission(&world.child_of_home);
    let hook = world.a.the_hook_of(&world.child_of_home).await;

    // When another home session of the same project answers it
    let refused = world
        .answers(&world.other_home, &world.child_of_home, "allow_once", None)
        .await;

    // Then it is refused as permission denied, naming the caller
    refused
        .assert_refused_saying("permission denied")
        .assert_refused_saying(&world.other_home);
    // And the hook kept waiting: it is woken by its own home's answer, not the refused one
    world
        .answers(&world.home, &world.child_of_home, "reject_once", None)
        .await
        .assert_answered();
    assert_eq!(
        hook.woken_with().await,
        answered("reject_once", &world.home, ""),
        "how the hook's wait ended"
    );
}

#[tokio::test]
async fn a_spawn_capable_orchestrator_answers_its_own_child() {
    // Given a child a grill-me session started, whose hook waits on Claude's permission prompt
    let world = a_world().await;
    world.a.asks_for_a_permission(&world.child_of_spawner);
    let hook = world.a.the_hook_of(&world.child_of_spawner).await;

    // When the grill-me session rejects it, saying why
    world
        .answers(
            &world.spawner,
            &world.child_of_spawner,
            "reject_once",
            Some("not before the release notes are written"),
        )
        .await
        .assert_answered();

    // Then the hook wakes with the rejection and its message, naming the grill-me session
    assert_eq!(
        hook.woken_with().await,
        answered(
            "reject_once",
            &world.spawner,
            "not before the release notes are written"
        ),
        "how the hook's wait ended"
    );
}

#[tokio::test]
async fn a_second_answer_is_refused_naming_who_answered_first() {
    // Given a child whose home has already allowed its permission prompt
    let world = a_world().await;
    world.a.asks_for_a_permission(&world.child_of_home);
    let hook = world.a.the_hook_of(&world.child_of_home).await;
    world
        .answers(&world.home, &world.child_of_home, "allow_once", None)
        .await
        .assert_answered();

    // When the home answers it again, the other way
    let again = world
        .answers(&world.home, &world.child_of_home, "reject_once", None)
        .await;

    // Then the second answer is refused, naming who answered first, and the hook got the first
    again
        .assert_refused_saying("already answered")
        .assert_refused_saying(&world.home);
    assert_eq!(
        hook.woken_with().await,
        answered("allow_once", &world.home, ""),
        "how the hook's wait ended"
    );
}

#[tokio::test]
async fn a_question_question_is_refused_naming_answer_session_question() {
    // Given a child of the home that asks a question rather than for a permission
    let world = a_world().await;
    world.a.asks_a_question(&world.child_of_home);

    // When the home answers it as a permission
    let refused = world
        .answers(&world.home, &world.child_of_home, "allow_once", None)
        .await;

    // Then it is refused, pointing at the tool that answers questions
    refused.assert_refused_saying("AnswerSessionQuestion");
}
