//! Acceptance: the session that started a claude-cli child answers the child's `AskUserQuestion`
//! with `AnswerSessionQuestion`, and the child's waiting hook wakes with those answers
//! (`#home-inbox` 8/9).
//!
//! The child's hook is the real `AwaitQuestionAnswer` of the daemon's own activity surface, held
//! open in a task; the answer is the real exec-tool call a home agent makes. An orchestrator is
//! present when the daemon serves its terminal, so the home runs a stand-in terminal that does
//! nothing but stay alive.
//!
//! The child's orchestrator and its pending question are written into its session directory as
//! `#start-parity` 2/9's start and `#home-inbox` 3/8's capture record them.
//!
//! Depends on `permission-answer` (`6/8`): the exec-tool dispatch, the port and the await handler
//! the question branch extends are its.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools,
//! docs/ft/daemon/claude-cli-session.md.
//! Changeset: docs/dev/1-WIP/2026-10-25-home-inbox-question-answer.md

mod support;

use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use support::{a_host, a_workspace_in, Host, InProcessPeers, HOST_A, THIS_PROJECT};
use tddy_core::pending_question::{
    write_pending_question, PendingQuestion, PendingQuestionItem, PendingQuestionOption,
};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::{Request, Status};
use tddy_service::proto::activity::{
    ActivityService as _, AwaitQuestionAnswerRequest, AwaitQuestionAnswerResponse,
    AwaitQuestionOutcome, QuestionAnswer,
};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const HOOK_TOKEN: &str = "tok-child-hook";
const REQUEST_ID: &str = "req-which-language";
/// How long a test gives the hook to open its wait, and a woken hook to return.
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);

struct World {
    a: Host,
    home: String,
    child: String,
    /// Keeps the home's stand-in terminal script on disk.
    _terminals: tempfile::TempDir,
}

/// One host holding a project-home session and a claude-cli child of it, whose `AskUserQuestion`
/// is recorded as pending. The home is running: the daemon serves its terminal.
async fn a_world_whose_child_asks_two_questions() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let child = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_claude_cli_child_of(&child, &home);
    a.asks_two_questions(&child);
    let terminals = tempfile::tempdir().unwrap();
    a.runs_a_terminal_for(&home, &terminals).await;
    World {
        a,
        home,
        child,
        _terminals: terminals,
    }
}

// ── the sessions ─────────────────────────────────────────────────────────────────────────────

impl Host {
    fn session_dir(&self, session_id: &str) -> std::path::PathBuf {
        unified_session_dir_path(self.sessions.path(), session_id)
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

    /// `child`'s agent asks an `AskUserQuestion` of two questions — which language (one of two),
    /// which checks to run (any of three) — as `#home-inbox` 3/8 records it.
    fn asks_two_questions(&self, child: &str) {
        let option = |label: &str| PendingQuestionOption {
            label: label.to_string(),
            description: String::new(),
        };
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
                tool_input_json: serde_json::json!({
                    "questions": [
                        {"question": "Which language?", "header": "Language", "multiSelect": false,
                         "options": [{"label": "Rust"}, {"label": "Go"}]},
                        {"question": "Which checks should run?", "header": "Checks",
                         "multiSelect": true,
                         "options": [{"label": "fmt"}, {"label": "clippy"}, {"label": "tests"}]}
                    ]
                })
                .to_string(),
                questions: vec![
                    PendingQuestionItem {
                        header: "Language".to_string(),
                        question: "Which language?".to_string(),
                        options: vec![option("Rust"), option("Go")],
                        multi_select: false,
                    },
                    PendingQuestionItem {
                        header: "Checks".to_string(),
                        question: "Which checks should run?".to_string(),
                        options: vec![option("fmt"), option("clippy"), option("tests")],
                        multi_select: true,
                    },
                ],
                at_unix_ms: 1_793_000_000_000,
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

    /// `child`'s `AskUserQuestion` hook, once it is waiting for answers to [`REQUEST_ID`].
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
        while pending.open_question_request(REQUEST_ID).is_none() {
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

/// What the tool call's `answers` argument and the woken hook both carry.
fn rust_with_fmt_and_clippy() -> Vec<QuestionAnswer> {
    vec![
        QuestionAnswer {
            question_index: 0,
            selected: vec!["Rust".to_string()],
            other: None,
        },
        QuestionAnswer {
            question_index: 1,
            selected: vec!["fmt".to_string(), "clippy".to_string()],
            other: None,
        },
    ]
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
}

impl World {
    /// `AnswerSessionQuestion` for [`REQUEST_ID`] on the child, called by the home: Rust, and fmt
    /// with clippy.
    async fn the_home_answers_rust_with_fmt_and_clippy(&self) -> Answer {
        let args = serde_json::json!({
            "session_id": self.child,
            "request_id": REQUEST_ID,
            "answers": [
                { "question_index": 0, "selected": ["Rust"] },
                { "question_index": 1, "selected": ["fmt", "clippy"] }
            ],
        });
        let result = self
            .a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.home.clone(),
                tool_name: "AnswerSessionQuestion".to_string(),
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

// ── the test ─────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn answer_session_question_from_the_childs_home_wakes_the_waiting_hook() {
    // Given a child of the home whose hook waits on its two questions
    let world = a_world_whose_child_asks_two_questions().await;
    let hook = world.a.the_hook_of(&world.child).await;

    // When the home answers both
    let answer = world
        .the_home_answers_rust_with_fmt_and_clippy()
        .await
        .assert_answered();

    // Then the hook wakes with those answers, naming the home
    assert_eq!(
        hook.woken_with().await,
        AwaitQuestionAnswerResponse {
            outcome: AwaitQuestionOutcome::Answered as i32,
            answered_by: world.home.clone(),
            answers: rust_with_fmt_and_clippy(),
            ..Default::default()
        },
        "how the hook's wait ended"
    );
    // And the tool says who answered which request
    assert_eq!(
        (
            answer["answered_by"].as_str(),
            answer["request_id"].as_str()
        ),
        (Some(world.home.as_str()), Some(REQUEST_ID)),
        "the tool's answer: {answer}"
    );
}
