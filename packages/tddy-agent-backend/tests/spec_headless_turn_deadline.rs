//! Acceptance: the generic headless workflow backend never hangs on a turn and cancels only its
//! own process group (`#agent-tools 15/19`).
//!
//! Changeset: `docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md`.
//!
//! The renderer is a fixture — the spec-driven one lives in `tddy-coding-agent-specs`, which this
//! crate cannot name; the parity tests against the real specs are in
//! `tddy-coder/tests/spec_headless_backend.rs`. The agents are shell scripts: a
//! *sleeper* that starts a grandchild and never ends, and a *quick* one that prints one
//! stream-json result after a short pause.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pretty_assertions::assert_eq;
use tddy_agent_backend::adapters::{
    AdapterRegistry, HeadlessInvocation, HeadlessMcpServer, HeadlessProtocol, HeadlessRenderer,
    HeadlessTurn, CLAUDE_STREAM_JSON,
};
use tddy_agent_backend::provider_route::{RouteSupport, RoutedAgent};
use tddy_agent_backend::{CodingBackend, InvokeRequest, SpecHeadlessBackend};
use tddy_session_store::error::BackendError;
use tddy_workflow::{GoalHints, GoalId, PermissionHint};

/// Long enough for any turn here that is not meant to hang; a turn that outlives it is a hang.
const NO_HANG: Duration = Duration::from_secs(20);

/// A renderer whose every turn runs `program` with no arguments, read as Claude's stream-json.
#[derive(Debug)]
struct ARenderer {
    agent: &'static str,
    program: PathBuf,
}

impl HeadlessRenderer for ARenderer {
    fn agent_id(&self) -> &str {
        self.agent
    }

    fn tool_server(&self) -> Option<HeadlessMcpServer> {
        None
    }

    fn render(&self, _turn: &HeadlessTurn) -> Result<HeadlessInvocation, BackendError> {
        Ok(HeadlessInvocation {
            program: self.program.clone(),
            argv: Vec::new(),
            env: Vec::new(),
            files: Vec::new(),
            adapter: AdapterRegistry::builtin()
                .get(CLAUDE_STREAM_JSON)
                .expect("claude-stream-json is registered"),
            exit_ok_if_structured: false,
        })
    }
}

/// A directory holding one stub agent, the pids it records, and the working directory it runs in.
struct AStubAgent {
    dir: tempfile::TempDir,
}

impl AStubAgent {
    /// An agent that records its pid and a grandchild's, then waits on the grandchild forever.
    fn that_never_ends() -> Self {
        Self::with_script(
            "echo $$ > \"$PIDS/agent.pid\"\nsleep 600 &\necho $! > \"$PIDS/grandchild.pid\"\nwait\n",
        )
    }

    /// An agent that prints one result after `pause_secs`.
    fn that_answers_after(pause_secs: u32, answer: &str) -> Self {
        Self::with_script(&format!(
            "echo $$ > \"$PIDS/agent.pid\"\nsleep {pause_secs}\nprintf '%s\\n' '{{\"type\":\"result\",\"subtype\":\"success\",\"result\":\"{answer}\",\"session_id\":\"sess-1\"}}'\n"
        ))
    }

    fn with_script(body: &str) -> Self {
        let dir = tempfile::tempdir().expect("agent dir");
        std::fs::create_dir(dir.path().join("work")).expect("workdir");
        let program = dir.path().join("agent");
        std::fs::write(
            &program,
            format!("#!/bin/sh\nPIDS='{}'\n{body}", dir.path().display()),
        )
        .expect("write agent");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Self { dir }
    }

    fn backend(&self) -> SpecHeadlessBackend {
        SpecHeadlessBackend::new(
            Arc::new(ARenderer {
                agent: "lark",
                program: self.dir.path().join("agent"),
            }),
            HeadlessProtocol::default(),
            RoutedAgent {
                id: "lark".to_string(),
                product: "Lark Code".to_string(),
                support: RouteSupport::default(),
                key_env: None,
            },
        )
    }

    fn request(&self) -> InvokeRequest {
        InvokeRequest {
            prompt: "Plan the feature".to_string(),
            system_prompt: None,
            system_prompt_path: None,
            goal_id: GoalId::new("plan"),
            submit_key: GoalId::new("plan"),
            hints: GoalHints {
                display_name: "Plan".to_string(),
                permission: PermissionHint::ReadOnly,
                allowed_tools: Vec::new(),
                default_model: None,
                agent_output: false,
                agent_cli_plan_mode: false,
                claude_nonzero_exit_ok_if_structured_response: false,
            },
            model: None,
            session: None,
            working_dir: Some(self.dir.path().join("work")),
            debug: false,
            agent_output: false,
            agent_output_sink: None,
            progress_sink: None,
            conversation_output_path: None,
            inherit_stdin: false,
            extra_allowed_tools: None,
            socket_path: None,
            session_dir: Some(self.dir.path().to_path_buf()),
            remote: None,
        }
    }

    /// The pid the agent recorded in `file`, once it has.
    async fn recorded_pid(&self, file: &str) -> u32 {
        let path = self.dir.path().join(file);
        let started = Instant::now();
        loop {
            if let Some(pid) = read_pid(&path) {
                return pid;
            }
            assert!(started.elapsed() < NO_HANG, "the agent never wrote {file}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

fn read_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Whether `pid` is a live process: `ps` lists it, and not as a zombie.
fn is_running(pid: u32) -> bool {
    let output = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .expect("ps runs");
    let state = String::from_utf8_lossy(&output.stdout);
    let state = state.trim();
    !state.is_empty() && !state.starts_with('Z')
}

/// Whether `pid` is gone within a few seconds — a kill is asynchronous.
async fn is_gone_soon(pid: u32) -> bool {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        if !is_running(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
async fn a_turn_that_never_ends_is_cancelled_at_its_deadline_and_its_whole_process_group_is_killed()
{
    // Given — a deadline long enough for the stub to record its pids on a loaded machine
    let agent = AStubAgent::that_never_ends();
    let backend = agent.backend().with_turn_deadline(Duration::from_secs(3));

    // When
    let outcome = tokio::time::timeout(NO_HANG, backend.invoke(agent.request()))
        .await
        .expect("the turn ends at its deadline, not never")
        .map(|response| response.output)
        .map_err(|e| e.to_string());

    // Then
    let agent_pid = agent.recorded_pid("agent.pid").await;
    let grandchild_pid = agent.recorded_pid("grandchild.pid").await;
    assert_eq!(
        (
            outcome,
            is_gone_soon(agent_pid).await,
            is_gone_soon(grandchild_pid).await
        ),
        (
            Err(
                "lark turn did not end within its 3s deadline; its process group was killed"
                    .to_string()
            ),
            true,
            true
        )
    );
}

#[tokio::test]
async fn two_concurrent_turns_cancel_only_their_own_process() {
    // Given — a turn that never ends, and one that answers after six seconds (well after the
    // cancel and the checks below, even on a loaded machine), running together
    let hanging = AStubAgent::that_never_ends();
    let answering = AStubAgent::that_answers_after(6, "The plan is ready.");
    let hanging_turn = {
        let backend = hanging.backend();
        let request = hanging.request();
        tokio::spawn(async move { backend.invoke(request).await })
    };
    let answering_turn = {
        let backend = answering.backend();
        let request = answering.request();
        tokio::spawn(async move { backend.invoke(request).await })
    };
    let hanging_pid = hanging.recorded_pid("agent.pid").await;
    let hanging_grandchild = hanging.recorded_pid("grandchild.pid").await;
    let answering_pid = answering.recorded_pid("agent.pid").await;

    // When — the hanging turn is cancelled: its future is dropped
    hanging_turn.abort();

    // Then — its whole group is killed while the other turn's agent runs on and answers
    let hanging_killed = (
        is_gone_soon(hanging_pid).await,
        is_gone_soon(hanging_grandchild).await,
    );
    let answering_still_running = is_running(answering_pid);
    let answer = tokio::time::timeout(NO_HANG, answering_turn)
        .await
        .expect("the answering turn ends")
        .expect("the answering turn did not panic")
        .map(|response| response.output)
        .map_err(|e| e.to_string());
    assert_eq!(
        (hanging_killed, answering_still_running, answer),
        ((true, true), true, Ok("The plan is ready.".to_string()))
    );
}
