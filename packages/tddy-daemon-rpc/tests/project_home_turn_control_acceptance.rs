//! Acceptance: an orchestrator drives another session's turns the way `/yolo-pr-stack` drives a
//! subagent — `SendSessionMessage` hands a running session a new turn without taking its terminal,
//! and `ResumeSession` resumes a stopped one with an optional first message (`#orchestrators` 4/11).
//!
//! A running session's agent is stood in for by a shell script on its main terminal that puts the
//! terminal in raw mode, turns bracketed paste on as Claude Code's TUI does, announces itself and
//! then records every byte it is typed into a file — so a test reads exactly what reached the
//! agent's PTY. A resumed session's agent is a stub `claude` the host really spawns, recording the
//! same way and also the argv it was launched with.
//!
//! The caller is the project's main (`project-home`). A person watching a session in the browser
//! holds its control lease under a screen id of their own, and must keep it.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-10-session-turn-control.md.
//! Changeset: docs/dev/1-WIP/2026-11-10-session-turn-control.md

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use support::{
    a_host, a_host_running_claude_as, a_workspace_in, session_started_by, Host, InProcessPeers,
    HOST_A, HOST_B, THIS_PROJECT,
};
use tddy_core::pending_question::{write_pending_question, PendingQuestion};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

/// How long a stand-in agent is given to start, and a typed byte to reach it.
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long a test waits for something that must NOT happen to show up.
const QUIET_PERIOD: Duration = Duration::from_millis(500);
/// What a stand-in agent prints once its terminal is raw and it is recording.
const READY_MARKER: &str = "READY";
/// The screen a person watching the session in the browser holds control under.
const BROWSER_SCREEN: &str = "browser-screen-1";
const TEST_MODEL: &str = "claude-opus-4-8";
/// The message the tests hand a session's agent.
const MESSAGE: &str = "rebase onto the new base and re-run the tests";

/// What a claude-cli agent is typed for [`MESSAGE`]: the text as a bracketed paste (its TUI turned
/// bracketed paste on), then the submit key `claude.yaml` names, Enter.
fn a_pasted_and_submitted(text: &str) -> Vec<u8> {
    [
        b"\x1b[200~".as_slice(),
        text.as_bytes(),
        b"\x1b[201~",
        b"\r",
    ]
    .concat()
}

/// The shell that puts a PTY in raw mode, turns bracketed paste on, announces itself and then
/// appends every byte it is typed to `record`.
fn a_recording_agent_script(record: &Path) -> String {
    format!(
        "stty raw -echo\nprintf '\\033[?2004h{READY_MARKER}\\r\\n'\nexec cat >> '{}'\n",
        record.display()
    )
}

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

// ── the sessions ─────────────────────────────────────────────────────────────────────────────

impl Host {
    fn session_dir(&self, session_id: &str) -> PathBuf {
        unified_session_dir_path(self.sessions.path(), session_id)
    }

    /// Record `session_id` as a session of `session_type` whose process is alive (or, with
    /// `running: false`, has none).
    fn records_an_agent_session(&self, session_id: &str, session_type: &str, running: bool) {
        let dir = self.session_dir(session_id);
        let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
        meta.session_type = Some(session_type.to_string());
        meta.pid = running.then(std::process::id);
        meta.activity_status = Some(if running { "Running" } else { "Stopped" }.to_string());
        tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
    }

    /// A running session of the project on this host whose agent, of `session_type`, is a
    /// [`RecordingAgent`] on its main terminal.
    async fn a_running_session_of(&self, session_type: &str) -> (String, RecordingAgent) {
        let session_id = a_workspace_in(&self.daemon, THIS_PROJECT).await;
        self.records_an_agent_session(&session_id, session_type, true);
        let agent = RecordingAgent::on_the_main_terminal_of(self, &session_id, session_type).await;
        (session_id, agent)
    }

    /// `session_id`'s agent asks an `AskUserQuestion`, as `#home-inbox` 3/8 records it.
    fn asks_a_question(&self, session_id: &str) {
        write_pending_question(
            &self.session_dir(session_id),
            &PendingQuestion {
                kind: "select".to_string(),
                header: "Language".to_string(),
                question: "Which language?".to_string(),
                options: vec!["Rust".to_string(), "Go".to_string()],
                allow_other: true,
                request_id: "req-which-language".to_string(),
                tool_name: "AskUserQuestion".to_string(),
                tool_input_json: "{}".to_string(),
                questions: Vec::new(),
                at_unix_ms: 1_793_000_000_000,
            },
        )
        .unwrap();
    }

    /// A person watching `session_id` in the browser takes control, as the browser's claim does.
    async fn a_browser_takes_control_of(&self, session_id: &str) {
        self.daemon
            .connection()
            .terminal_control()
            .claim(session_id, BROWSER_SCREEN, true)
            .await;
    }

    async fn control_of(&self, session_id: &str) -> Option<String> {
        self.daemon
            .connection()
            .terminal_control()
            .holder_screen_id(session_id)
            .await
    }
}

/// A stand-in agent on a session's main terminal, and the file holding every byte typed into it.
struct RecordingAgent {
    _dir: tempfile::TempDir,
    record: PathBuf,
}

impl RecordingAgent {
    async fn on_the_main_terminal_of(host: &Host, session_id: &str, session_type: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("typed.bin");
        let script = dir.path().join("stand_in_agent.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\n{}", a_recording_agent_script(&record)),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let handle = host
            .terminals
            .start_with_options(
                session_id,
                host.worktree_of(session_id),
                TEST_MODEL,
                vec![script.to_str().unwrap().to_string()],
                session_type,
                None,
                None,
                Vec::new(),
                Vec::new(),
                None,
            )
            .await
            .expect("the main terminal starts");
        a_capture_showing(&handle.capture, READY_MARKER, PTY_STUB_OUTPUT).await;
        Self { _dir: dir, record }
    }

    fn typed_so_far(&self) -> Vec<u8> {
        std::fs::read(&self.record).unwrap_or_default()
    }

    /// Wait until the agent has been typed at least `expected.len()` bytes, and return them.
    async fn typed_after_waiting_for(&self, expected: &[u8]) -> Vec<u8> {
        typed_after_waiting_for(&self.record, expected).await
    }

    /// What the agent was typed once [`QUIET_PERIOD`] has passed: for asserting nothing was.
    async fn typed_after_a_quiet_period(&self) -> Vec<u8> {
        tokio::time::sleep(QUIET_PERIOD).await;
        self.typed_so_far()
    }
}

async fn typed_after_waiting_for(record: &Path, expected: &[u8]) -> Vec<u8> {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let typed = std::fs::read(record).unwrap_or_default();
        if typed.len() >= expected.len() || tokio::time::Instant::now() >= deadline {
            return typed;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
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
    /// `SendSessionMessage` from main to `session_id` on this host.
    async fn main_sends(&self, session_id: &str, text: &str) -> Answer {
        call(
            &self.a,
            &self.main,
            "SendSessionMessage",
            serde_json::json!({ "session_id": session_id, "text": text }),
        )
        .await
    }

    /// `ResumeSession` from main for `session_id` on this host, with no message.
    async fn main_resumes(&self, session_id: &str) -> Answer {
        call(
            &self.a,
            &self.main,
            "ResumeSession",
            serde_json::json!({ "session_id": session_id }),
        )
        .await
    }
}

// ── SendSessionMessage ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn main_sends_a_running_claude_cli_session_a_turn_without_taking_its_terminal() {
    // Given a running claude-cli session that a person watching it in the browser has control of,
    // and a subscriber to that session's lease changes
    let world = a_world().await;
    let (session, agent) = world.a.a_running_session_of("claude-cli").await;
    world.a.a_browser_takes_control_of(&session).await;
    let mut lease_changes = world.a.daemon.connection().terminal_control().subscribe();

    // When main sends it a message
    let sent = world.main_sends(&session, MESSAGE).await;

    // Then it is answered as delivered by terminal paste ...
    assert_eq!(
        sent.assert_answered(),
        serde_json::json!({ "session_id": session, "delivered_via": "terminal_paste" })
    );
    // ... the agent was typed the message as a paste, then Enter: a new turn ...
    let expected = a_pasted_and_submitted(MESSAGE);
    assert_eq!(
        agent.typed_after_waiting_for(&expected).await,
        expected,
        "the bytes the agent was typed"
    );
    // ... and the person still holds control: no lease change was broadcast
    assert_eq!(
        world.a.control_of(&session).await.as_deref(),
        Some(BROWSER_SCREEN)
    );
    assert!(
        tokio::time::timeout(QUIET_PERIOD, lease_changes.recv())
            .await
            .is_err(),
        "a lease change was broadcast"
    );
}

#[tokio::test]
async fn a_message_to_a_stopped_session_is_refused_naming_resume_session() {
    // Given a claude-cli session whose agent has stopped
    let world = a_world().await;
    let session = a_workspace_in(&world.a.daemon, THIS_PROJECT).await;
    world
        .a
        .records_an_agent_session(&session, "claude-cli", false);

    // When main sends it a message
    let refused = world.main_sends(&session, MESSAGE).await;

    // Then it is refused, told to resume it
    refused.assert_refused_saying("ResumeSession");
}

#[tokio::test]
async fn a_message_to_a_session_waiting_on_a_question_is_refused_naming_the_answer_tools() {
    // Given a running claude-cli session whose agent asks a question
    let world = a_world().await;
    let (session, agent) = world.a.a_running_session_of("claude-cli").await;
    world.a.asks_a_question(&session);

    // When main sends it a message
    let refused = world.main_sends(&session, MESSAGE).await;

    // Then it is refused, told to answer the question instead, and the agent was typed nothing
    refused.assert_refused_saying("AnswerSessionQuestion");
    assert_eq!(
        agent.typed_after_a_quiet_period().await,
        Vec::<u8>::new(),
        "the bytes the agent was typed"
    );
}

#[tokio::test]
async fn a_message_to_an_agent_without_turn_input_is_refused_naming_the_agent() {
    // Given a running session of an agent whose spec declares no `turn_input` (codex)
    let world = a_world().await;
    let (session, agent) = world.a.a_running_session_of("codex-cli").await;

    // When main sends it a message
    let refused = world.main_sends(&session, MESSAGE).await;

    // Then it is refused naming the agent and what it lacks, and the agent was typed nothing
    refused
        .assert_refused_saying("codex")
        .assert_refused_saying("turn_input");
    assert_eq!(
        agent.typed_after_a_quiet_period().await,
        Vec::<u8>::new(),
        "the bytes the agent was typed"
    );
}

// ── ResumeSession ────────────────────────────────────────────────────────────────────────────

/// A stub `claude` the host really spawns: it appends its argv to `argv.txt` beside itself, then
/// records what it is typed as [`a_recording_agent_script`] does, into `typed.bin` beside itself.
struct AClaudeStub {
    dir: tempfile::TempDir,
}

impl AClaudeStub {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("claude");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho \"$@\" >> '{}'\n{}",
                dir.path().join("argv.txt").display(),
                a_recording_agent_script(&dir.path().join("typed.bin"))
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }

    fn binary(&self) -> PathBuf {
        self.dir.path().join("claude")
    }

    fn typed(&self) -> PathBuf {
        self.dir.path().join("typed.bin")
    }

    /// Every argv the stub was launched with, one per spawn, oldest first.
    fn launches(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("argv.txt"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

/// One host whose `claude` is [`AClaudeStub`], its main, and a claude-cli session main started and
/// then terminated: a stopped session with a conversation to resume.
struct AResumeWorld {
    a: Host,
    main: String,
    stopped: String,
    claude: AClaudeStub,
}

async fn a_claude_cli_session_main_started_and_terminated() -> AResumeWorld {
    let peers = Arc::new(InProcessPeers::default());
    let claude = AClaudeStub::new();
    let a = a_host_running_claude_as(HOST_A, &peers, &claude.binary());
    let main = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&main);
    let started = call(
        &a,
        &main,
        "StartSession",
        serde_json::json!({ "session_type": "claude-cli", "model": TEST_MODEL }),
    )
    .await;
    started.assert_answered();
    let stopped = session_started_by(&a.daemon, &main, &started.text).await;
    let handle = a
        .terminals
        .get(&stopped)
        .await
        .expect("the session's agent was spawned");
    a_capture_showing(&handle.capture, READY_MARKER, PTY_STUB_OUTPUT).await;
    call(
        &a,
        &main,
        "TerminateSession",
        serde_json::json!({ "session_id": stopped }),
    )
    .await
    .assert_answered();
    let world = AResumeWorld {
        a,
        main,
        stopped,
        claude,
    };
    world.until_the_session_is_listed_stopped().await;
    world
}

impl AResumeWorld {
    /// Wait until `ListProjectSessions` reports the terminated session as no longer active.
    async fn until_the_session_is_listed_stopped(&self) {
        let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
        loop {
            let listed = call(
                &self.a,
                &self.main,
                "ListProjectSessions",
                serde_json::json!({}),
            )
            .await
            .assert_answered();
            let active = listed["sessions"]
                .as_array()
                .expect("a sessions array")
                .iter()
                .find(|row| row["session_id"] == self.stopped)
                .map(|row| row["is_active"] == true);
            if active == Some(false) {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "session {} is still listed as {listed}",
                self.stopped
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

#[tokio::test]
async fn resume_session_with_a_message_resumes_it_and_delivers_the_message() {
    // Given a claude-cli session main started, whose agent was terminated
    let world = a_claude_cli_session_main_started_and_terminated().await;

    // When main resumes it with a message
    let resumed = call(
        &world.a,
        &world.main,
        "ResumeSession",
        serde_json::json!({ "session_id": world.stopped, "message": MESSAGE }),
    )
    .await;

    // Then it is answered as resumed ...
    assert_eq!(
        resumed.assert_answered(),
        serde_json::json!({ "session_id": world.stopped, "outcome": "resumed" })
    );
    // ... claude was launched again, resuming the session's own conversation ...
    let launches = world.claude.launches();
    assert_eq!(launches.len(), 2, "claude's launches: {launches:?}");
    assert!(
        launches[1].contains(&format!("--resume {}", world.stopped)),
        "the second launch resumes the conversation: {}",
        launches[1]
    );
    // ... and the resumed agent was typed the message as its first turn
    let expected = a_pasted_and_submitted(MESSAGE);
    assert_eq!(
        typed_after_waiting_for(&world.claude.typed(), &expected).await,
        expected,
        "the bytes the resumed agent was typed"
    );
}

#[tokio::test]
async fn resume_session_on_a_running_session_answers_running() {
    // Given a running claude-cli session
    let world = a_world().await;
    let (session, agent) = world.a.a_running_session_of("claude-cli").await;

    // When main resumes it
    let resumed = world.main_resumes(&session).await;

    // Then it is answered as already running, and its agent is left alone
    assert_eq!(
        resumed.assert_answered(),
        serde_json::json!({ "session_id": session, "outcome": "running" })
    );
    assert_eq!(
        agent.typed_after_a_quiet_period().await,
        Vec::<u8>::new(),
        "the bytes the agent was typed"
    );
}

// ── across hosts ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn both_tools_reach_a_session_on_another_host() {
    // Given a running claude-cli session on host-b; main is on host-a
    let world = a_world().await;
    let (on_b, agent) = world.b.a_running_session_of("claude-cli").await;
    let naming_host_b = |extra: serde_json::Value| {
        let mut args = serde_json::json!({ "session_id": on_b, "daemon_instance_id": HOST_B });
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        args
    };

    // When main sends it a message and asks to resume it, naming host-b
    let sent = call(
        &world.a,
        &world.main,
        "SendSessionMessage",
        naming_host_b(serde_json::json!({ "text": MESSAGE })),
    )
    .await;
    let resumed = call(
        &world.a,
        &world.main,
        "ResumeSession",
        naming_host_b(serde_json::json!({})),
    )
    .await;

    // Then host-b's agent was typed the message, and host-b answers that the session is running
    assert_eq!(
        sent.assert_answered()["delivered_via"],
        serde_json::json!("terminal_paste")
    );
    let expected = a_pasted_and_submitted(MESSAGE);
    assert_eq!(
        agent.typed_after_waiting_for(&expected).await,
        expected,
        "the bytes host-b's agent was typed"
    );
    assert_eq!(
        resumed.assert_answered()["outcome"],
        serde_json::json!("running")
    );
}
