//! Acceptance: a claude-cli child's hook waits for an orchestrator that runs on another host, and
//! the orchestrator's forwarded answer wakes it (`#home-inbox` 10/11).
//!
//! Two daemons run in this process and reach each other through the in-memory peer forwarder the
//! cross-host suites share. The child lives on `host-a` and its hook is the real
//! `AwaitQuestionAnswer` of that daemon's activity surface, held open in a task. Its orchestrator is
//! a project-home session on `host-b`, which is present when `host-b` serves its main terminal, so
//! the home runs a stand-in terminal that does nothing but stay alive. The answer is the real
//! exec-tool call the home's agent makes on `host-b`, addressed at `host-a`.
//!
//! Every wait is bounded: a hook that is never woken fails the test, it does not hang it.
//!
//! The child's orchestrator and its pending question are written into its session directory as
//! `#start-parity` 2/9's start and `#home-inbox` 3/8's capture record them.
//!
//! Depends on `permission-answer` (`6/11`) and `question-answer` (`8/11`): the answer tools, the
//! registry and the await handler are theirs. This node owns only the presence of a peer's terminal.
//!
//! Feature doc: docs/ft/daemon/1-WIP/PRD-2026-10-29-cross-host-answer-wait.md.
//! Changeset: docs/dev/1-WIP/2026-10-29-home-inbox-cross-host-wait.md

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use support::{
    a_host, a_workspace_in, Host, InProcessPeers, ACTIVITY_SERVICE, HOST_A, HOST_B, SILENT_HOST,
    THIS_PROJECT,
};
use tddy_core::pending_question::{
    write_pending_question, PendingQuestion, PendingQuestionItem, PendingQuestionOption,
};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::{Code, Request, RequestMetadata, RequestTransport, Status};
use tddy_service::proto::activity::{
    ActivityService as _, AwaitQuestionAnswerRequest, AwaitQuestionAnswerResponse,
    AwaitQuestionOutcome, IsSessionTerminalServedRequest, QuestionAnswer,
};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_session_activity::orchestrator_presence::PEER_PRESENCE_TIMEOUT;
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const HOOK_TOKEN: &str = "tok-child-hook";
const REQUEST_ID: &str = "req-cross-host";
/// The peer query the child's host makes (`ActivityService.IsSessionTerminalServed`).
const PRESENCE_QUERY: &str = "IsSessionTerminalServed";
/// How long a test gives a hook to open its wait, and a woken hook to return.
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);
/// How far past [`PEER_PRESENCE_TIMEOUT`] a hook that was told nobody is there may take: scheduling
/// slack, not part of the bound.
const SLACK: Duration = Duration::from_secs(2);
/// "At once": well inside the time a peer is given to answer, so it cannot be a timeout.
const AT_ONCE: Duration = Duration::from_millis(1500);

/// Two hosts and a child on the first.
struct World {
    peers: Arc<InProcessPeers>,
    /// Holds the child and its hook.
    a: Host,
    /// Holds the orchestrator.
    b: Host,
    child: String,
    /// Keeps the stand-in terminals' scripts on disk.
    scripts: tempfile::TempDir,
}

/// `host-a` holding a claude-cli child, and `host-b` holding a project-home session.
async fn two_hosts() -> (World, String) {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let home = a_workspace_in(&b.daemon, THIS_PROJECT).await;
    b.make_project_home(&home);
    let child = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let world = World {
        peers,
        a,
        b,
        child,
        scripts: tempfile::tempdir().unwrap(),
    };
    (world, home)
}

/// The child's orchestrator is the home on `host-b`, and `host-b` serves its terminal.
async fn a_world_whose_orchestrator_runs_on_the_other_host() -> (World, String) {
    let (world, home) = two_hosts().await;
    world
        .a
        .make_claude_cli_child_of(&world.child, &home, Some(HOST_B));
    world.b.runs_a_terminal_for(&home, &world.scripts).await;
    (world, home)
}

// ── the sessions ─────────────────────────────────────────────────────────────────────────────

impl Host {
    fn session_dir(&self, session_id: &str) -> std::path::PathBuf {
        unified_session_dir_path(self.sessions.path(), session_id)
    }

    /// Make `child` a claude-cli session whose hooks authenticate with [`HOOK_TOKEN`], started by
    /// `orchestrator` on `orchestrator_host` — `None` is the child's own host.
    fn make_claude_cli_child_of(
        &self,
        child: &str,
        orchestrator: &str,
        orchestrator_host: Option<&str>,
    ) {
        let dir = self.session_dir(child);
        let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
        meta.session_type = Some("claude-cli".to_string());
        meta.hook_token = Some(HOOK_TOKEN.to_string());
        tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
        let mut changeset = tddy_core::read_changeset(&dir).unwrap_or_default();
        changeset.orchestrator_session_id = Some(orchestrator.to_string());
        changeset.orchestrator_daemon_instance_id = orchestrator_host.map(str::to_string);
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

    /// `child`'s agent asks an `AskUserQuestion` of two questions, as `#home-inbox` 3/8 records it.
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

    /// Start a stand-in terminal as `session_id`'s main one, which only stays alive: this daemon now
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
}

// ── the child's hook ─────────────────────────────────────────────────────────────────────────

/// A child's hook, in `AwaitQuestionAnswer`.
struct ARunningHook {
    running: tokio::task::JoinHandle<Result<AwaitQuestionAnswerResponse, Status>>,
}

/// What a hook did first: it opened a wait, or it was told something at once.
enum HookStart {
    Waiting(ARunningHook),
    Answered {
        response: AwaitQuestionAnswerResponse,
        after: Duration,
    },
}

impl Host {
    /// `child`'s hook, started as the daemon starts it and followed until it has either opened its
    /// wait (the question is registered for an answer) or returned.
    async fn starts_the_hook_of(&self, child: &str, kind_of_question: Question) -> HookStart {
        let service = self.daemon.connection().activity_service();
        let request = AwaitQuestionAnswerRequest {
            session_id: child.to_string(),
            hook_token: HOOK_TOKEN.to_string(),
            os_user: "testdev".to_string(),
            request_id: REQUEST_ID.to_string(),
            permission_suggestions_json: String::new(),
        };
        let started = Instant::now();
        let running = tokio::spawn(async move {
            service
                .await_question_answer(Request::direct(request))
                .await
                .map(|response| response.into_inner())
        });
        let pending = self.daemon.connection().pending_answers();
        let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT + PEER_PRESENCE_TIMEOUT;
        loop {
            let is_waiting = match kind_of_question {
                Question::Permission => pending.open_request(REQUEST_ID).is_some(),
                Question::AskUserQuestion => pending.open_question_request(REQUEST_ID).is_some(),
            };
            if is_waiting {
                return HookStart::Waiting(ARunningHook { running });
            }
            if running.is_finished() {
                let response = running
                    .await
                    .expect("the hook ran to completion")
                    .expect("the hook is answered, not refused");
                return HookStart::Answered {
                    response,
                    after: started.elapsed(),
                };
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the hook of {child} neither opened its wait nor returned"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// `child`'s hook, which must be waiting for its orchestrator: told `NO_ORCHESTRATOR` instead is
    /// the failure these tests exist to catch.
    async fn the_waiting_hook_of(&self, child: &str, kind_of_question: Question) -> ARunningHook {
        match self.starts_the_hook_of(child, kind_of_question).await {
            HookStart::Waiting(hook) => hook,
            HookStart::Answered { response, .. } => panic!(
                "the hook of {child} was told {:?} at once instead of waiting for the \
                 orchestrator on another host",
                AwaitQuestionOutcome::try_from(response.outcome)
            ),
        }
    }

    /// `child`'s hook, which must be told at once — or within the presence bound — that there is
    /// nobody to wait for: how it was told, and how long that took.
    async fn the_hook_of_told_nobody_is_there(
        &self,
        child: &str,
        kind_of_question: Question,
    ) -> Duration {
        match self.starts_the_hook_of(child, kind_of_question).await {
            HookStart::Answered { response, after } => {
                assert_eq!(
                    AwaitQuestionOutcome::try_from(response.outcome),
                    Ok(AwaitQuestionOutcome::NoOrchestrator),
                    "how the hook's wait ended"
                );
                after
            }
            HookStart::Waiting(_) => panic!("the hook of {child} waited for an orchestrator"),
        }
    }
}

#[derive(Clone, Copy)]
enum Question {
    Permission,
    AskUserQuestion,
}

impl ARunningHook {
    async fn woken_with(self) -> AwaitQuestionAnswerResponse {
        tokio::time::timeout(WAIT_TIMEOUT, self.running)
            .await
            .expect("the hook was woken")
            .expect("the hook's wait ran to completion")
            .expect("the hook's wait is answered")
    }
}

// ── what a tool call answered ────────────────────────────────────────────────────────────────

/// What came back from a tool call as the agent sees it: an answer, or why there is none.
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
    /// `tool_name` called by `caller` on `host`, with `args`.
    async fn calls(
        &self,
        host: &Host,
        caller: &str,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Answer {
        let result = host
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

    /// The home on `host-b` allows the child's permission prompt, forwarded to the child's host.
    async fn the_home_on_b_allows_the_permission(&self, home: &str) -> Answer {
        self.calls(
            &self.b,
            home,
            "AnswerSessionPermission",
            serde_json::json!({
                "session_id": self.child,
                "request_id": REQUEST_ID,
                "option_id": "allow_once",
                "daemon_instance_id": HOST_A,
            }),
        )
        .await
    }

    /// The home on `host-b` answers the child's two questions, forwarded to the child's host.
    async fn the_home_on_b_answers_rust_with_fmt_and_clippy(&self, home: &str) -> Answer {
        self.calls(
            &self.b,
            home,
            "AnswerSessionQuestion",
            serde_json::json!({
                "session_id": self.child,
                "request_id": REQUEST_ID,
                "answers": [
                    { "question_index": 0, "selected": ["Rust"] },
                    { "question_index": 1, "selected": ["fmt", "clippy"] }
                ],
                "daemon_instance_id": HOST_A,
            }),
        )
        .await
    }

    /// How many times the child's host asked `peer` whether it serves a terminal.
    fn presence_queries_to(&self, peer: &str) -> usize {
        self.peers
            .times_asked(peer, ACTIVITY_SERVICE, PRESENCE_QUERY)
    }
}

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

// ── the tests ────────────────────────────────────────────────────────────────────────────────

// Criterion 1.
#[tokio::test]
async fn a_forwarded_permission_answer_wakes_a_hook_whose_orchestrator_runs_on_another_host() {
    // Given a child on host-a whose home on host-b serves its terminal, and whose hook waits
    let (world, home) = a_world_whose_orchestrator_runs_on_the_other_host().await;
    world.a.asks_for_a_permission(&world.child);
    let hook = world
        .a
        .the_waiting_hook_of(&world.child, Question::Permission)
        .await;

    // When the home on host-b allows it, the call forwarded to host-a
    let answer = world
        .the_home_on_b_allows_the_permission(&home)
        .await
        .assert_answered();

    // Then the hook wakes with the allow, naming the home
    assert_eq!(
        hook.woken_with().await,
        AwaitQuestionAnswerResponse {
            outcome: AwaitQuestionOutcome::Answered as i32,
            option_id: "allow_once".to_string(),
            answered_by: home.clone(),
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
        (Some(home.as_str()), Some(REQUEST_ID)),
        "the tool's answer: {answer}"
    );
}

// Criterion 2.
#[tokio::test]
async fn a_forwarded_question_answer_wakes_a_hook_whose_orchestrator_runs_on_another_host() {
    // Given a child on host-a whose home on host-b serves its terminal, and whose hook waits on
    // two questions
    let (world, home) = a_world_whose_orchestrator_runs_on_the_other_host().await;
    world.a.asks_two_questions(&world.child);
    let hook = world
        .a
        .the_waiting_hook_of(&world.child, Question::AskUserQuestion)
        .await;

    // When the home on host-b answers both, the call forwarded to host-a
    world
        .the_home_on_b_answers_rust_with_fmt_and_clippy(&home)
        .await
        .assert_answered();

    // Then the hook wakes with those answers, naming the home
    assert_eq!(
        hook.woken_with().await,
        AwaitQuestionAnswerResponse {
            outcome: AwaitQuestionOutcome::Answered as i32,
            answered_by: home.clone(),
            answers: rust_with_fmt_and_clippy(),
            ..Default::default()
        },
        "how the hook's wait ended"
    );
}

// Criterion 3.
#[tokio::test]
async fn a_hook_is_told_nobody_is_there_at_once_when_the_other_host_does_not_serve_the_orchestrator(
) {
    // Given a child on host-a whose home is on host-b, which serves no terminal for it
    let (world, home) = two_hosts().await;
    world
        .a
        .make_claude_cli_child_of(&world.child, &home, Some(HOST_B));
    world.a.asks_for_a_permission(&world.child);

    // When its hook asks for an answer
    let told_after = world
        .a
        .the_hook_of_told_nobody_is_there(&world.child, Question::Permission)
        .await;

    // Then it is told at once, not at the end of a timeout
    assert!(
        told_after < AT_ONCE,
        "the hook was told after {told_after:?}, not at once"
    );
    // And that was host-b's answer: host-a asked it
    assert_eq!(
        world.presence_queries_to(HOST_B),
        1,
        "how often host-a asked host-b whether it serves the home's terminal"
    );
}

// Criterion 4.
#[tokio::test]
async fn a_hook_is_told_nobody_is_there_within_the_bound_when_the_other_host_never_answers() {
    // Given a child on host-a whose orchestrator runs on host-c, which takes every call and never
    // answers it
    let (world, home) = two_hosts().await;
    world.peers.never_answers(SILENT_HOST);
    world
        .a
        .make_claude_cli_child_of(&world.child, &home, Some(SILENT_HOST));
    world.a.asks_for_a_permission(&world.child);

    // When its hook asks for an answer
    let told_after = world
        .a
        .the_hook_of_told_nobody_is_there(&world.child, Question::Permission)
        .await;

    // Then it is told nobody is there, no later than the presence bound
    assert!(
        told_after <= PEER_PRESENCE_TIMEOUT + SLACK,
        "the hook was told after {told_after:?}, past the {PEER_PRESENCE_TIMEOUT:?} bound"
    );
    // And host-a did ask host-c: the answer is the bound running out, not a refusal to ask
    assert_eq!(
        world.presence_queries_to(SILENT_HOST),
        1,
        "how often host-a asked host-c whether it serves the home's terminal"
    );
}

// Criterion 4, a peer that is on the roster with nothing behind it answering an error at once.
#[tokio::test]
async fn a_hook_is_told_nobody_is_there_at_once_when_asking_the_other_host_fails() {
    // Given a child on host-a whose orchestrator runs on host-c, which is on the roster with no
    // daemon behind it
    let (world, home) = two_hosts().await;
    world
        .a
        .make_claude_cli_child_of(&world.child, &home, Some(SILENT_HOST));
    world.a.asks_for_a_permission(&world.child);

    // When its hook asks for an answer
    let told_after = world
        .a
        .the_hook_of_told_nobody_is_there(&world.child, Question::Permission)
        .await;

    // Then it is told at once, and host-a did ask
    assert!(
        told_after < AT_ONCE,
        "the hook was told after {told_after:?}, not at once"
    );
    assert_eq!(
        world.presence_queries_to(SILENT_HOST),
        1,
        "how often host-a asked host-c whether it serves the home's terminal"
    );
}

// Criterion 5 — a guard: this holds before this node and must hold after it.
#[tokio::test]
async fn an_orchestrator_on_the_childs_own_host_is_waited_for_and_no_peer_is_asked() {
    // Given a child on host-a whose home is on host-a too, serving its terminal; the changeset names
    // the host, as `#start-parity` records it
    let (world, _) = two_hosts().await;
    let home = a_workspace_in(&world.a.daemon, THIS_PROJECT).await;
    world.a.make_project_home(&home);
    world
        .a
        .make_claude_cli_child_of(&world.child, &home, Some(HOST_A));
    world.a.runs_a_terminal_for(&home, &world.scripts).await;
    world.a.asks_for_a_permission(&world.child);
    let hook = world
        .a
        .the_waiting_hook_of(&world.child, Question::Permission)
        .await;

    // When the home answers on its own host, with no host named
    let answer = world
        .calls(
            &world.a,
            &home,
            "AnswerSessionPermission",
            serde_json::json!({
                "session_id": world.child,
                "request_id": REQUEST_ID,
                "option_id": "reject_once",
            }),
        )
        .await
        .assert_answered();

    // Then the hook wakes with the rejection, naming the home
    assert_eq!(
        hook.woken_with().await,
        AwaitQuestionAnswerResponse {
            outcome: AwaitQuestionOutcome::Answered as i32,
            option_id: "reject_once".to_string(),
            answered_by: home.clone(),
            ..Default::default()
        },
        "how the hook's wait ended; the tool said {answer}"
    );
    // And nobody was asked whether a terminal is served: the daemon knows its own
    assert_eq!(
        [HOST_A, HOST_B, SILENT_HOST].map(|host| world.presence_queries_to(host)),
        [0, 0, 0],
        "presence queries made to host-a, host-b, host-c"
    );
}

// The peer query is for peers: a daemon answers it only to a call that arrived from one.
#[tokio::test]
async fn the_presence_query_is_refused_unless_it_arrives_from_a_peer() {
    // Given a daemon serving a home's terminal
    let (world, home) = two_hosts().await;
    world.b.runs_a_terminal_for(&home, &world.scripts).await;
    let service = world.b.daemon.connection().activity_service();

    // When the question arrives over each transport a peer's call does not
    let mut refusals = Vec::new();
    for transport in [
        RequestTransport::Http,
        RequestTransport::UnixSocket,
        RequestTransport::InProcess,
    ] {
        let refused = service
            .is_session_terminal_served(Request::with_metadata(
                IsSessionTerminalServedRequest {
                    session_id: home.clone(),
                },
                RequestMetadata::over(transport),
            ))
            .await;
        refusals.push((
            transport,
            refused.as_ref().err().map(|status| status.code()),
            refused
                .err()
                .map(|status| status.message().contains("peer")),
        ));
    }

    // Then each is refused as permission denied, saying the RPC is for peers only
    assert_eq!(
        refusals,
        [
            RequestTransport::Http,
            RequestTransport::UnixSocket,
            RequestTransport::InProcess
        ]
        .map(|transport| (transport, Some(Code::PermissionDenied), Some(true))),
        "how each transport was answered: (transport, refusal code, names peers)"
    );
}
