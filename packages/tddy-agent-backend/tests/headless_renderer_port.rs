//! The headless-renderer port: `ClaudeCodeBackend` and `HeadlessDriver` launching from a rendered
//! declaration and reading the child through the adapter it names.
//!
//! The renderer is a fixture — the spec-driven one lives in `tddy-coding-agent-specs`, which this
//! crate cannot name — so these tests pin what each consumer hands the port and what it does with
//! what comes back, independently of template rendering. The agent is a shell script that records
//! its argv and the file it was handed, then prints a recorded stream-json run.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-headless-agents-from-specs.md` (`#agent-tools 6/12`).

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tddy_agent_backend::adapters::{
    AdapterRegistry, HeadlessInvocation, HeadlessMcpServer, HeadlessRenderer, HeadlessTurn,
    CLAUDE_STREAM_JSON,
};
use tddy_agent_backend::{
    ClaudeCodeBackend, CodingBackend, ConversationDriver, HeadlessCommand, HeadlessDriver,
    HeadlessHarness, InvokeRequest, McpServerSpec, SessionMode,
};
use tddy_session_store::error::BackendError;
use tddy_workflow::{GoalHints, GoalId, PermissionHint};

const TURN_DEADLINE: Duration = Duration::from_secs(10);
const APPROVAL_TOOL: &str = "mcp__tddy-tools__approval_prompt";
const TDDY_TOOLS: &str = "/opt/tddy/bin/tddy-tools";
const A_STRUCTURED_RESULT: &str = "<structured-response goal=\"plan\">{}</structured-response>";

/// A fake agent in its own directory: records each invocation's argv (one line) and the contents of
/// `registration.json` beside it, prints `stream`, and exits with `exit_code`.
struct AFakeAgent {
    dir: tempfile::TempDir,
}

fn a_fake_agent(stream: &[serde_json::Value], exit_code: i32) -> AFakeAgent {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("stream.jsonl");
    let lines: Vec<String> = stream.iter().map(|event| event.to_string()).collect();
    std::fs::write(&output, lines.join("\n") + "\n").expect("write stream");
    let script = dir.path().join("agent");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{argv}'\ncat '{registration}' >> '{seen}' 2>/dev/null\ncat '{output}'\nexit {exit_code}\n",
            argv = dir.path().join("argv.log").display(),
            registration = dir.path().join("registration.json").display(),
            seen = dir.path().join("registration.seen").display(),
            output = output.display(),
        ),
    )
    .expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    AFakeAgent { dir }
}

impl AFakeAgent {
    fn program(&self) -> PathBuf {
        self.dir.path().join("agent")
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn invocations(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("argv.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// What `registration.json` held each time the agent started.
    fn registrations_seen(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("registration.seen")).unwrap_or_default()
    }
}

fn a_claude_stream(session: &str, result: &str) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({ "type": "system", "subtype": "init", "session_id": session }),
        serde_json::json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": result }] }, "session_id": session }),
        serde_json::json!({ "type": "result", "subtype": "success", "result": result, "session_id": session }),
    ]
}

/// A renderer that records every turn it is asked for and renders `["--rendered", <prompt>]` for
/// `program`, with `registration.json` (beside the program) as its one file.
#[derive(Debug)]
struct ARecordingRenderer {
    program: PathBuf,
    exit_ok_if_structured: bool,
    turns: Mutex<Vec<HeadlessTurn>>,
}

fn a_recording_renderer(
    agent: &AFakeAgent,
    exit_ok_if_structured: bool,
) -> Arc<ARecordingRenderer> {
    Arc::new(ARecordingRenderer {
        program: agent.program(),
        exit_ok_if_structured,
        turns: Mutex::new(Vec::new()),
    })
}

fn the_tool_server() -> HeadlessMcpServer {
    HeadlessMcpServer {
        name: "tddy-tools".to_string(),
        command: PathBuf::from(TDDY_TOOLS),
        args: vec!["--mcp".to_string()],
        env: BTreeMap::new(),
    }
}

impl ARecordingRenderer {
    fn turns(&self) -> Vec<HeadlessTurn> {
        self.turns.lock().expect("turns").clone()
    }
}

impl HeadlessRenderer for ARecordingRenderer {
    fn agent_id(&self) -> &str {
        "claude"
    }

    fn tool_server(&self) -> Option<HeadlessMcpServer> {
        Some(the_tool_server())
    }

    fn render(&self, turn: &HeadlessTurn) -> Result<HeadlessInvocation, BackendError> {
        self.turns.lock().expect("turns").push(turn.clone());
        let adapter = AdapterRegistry::builtin()
            .get(CLAUDE_STREAM_JSON)
            .expect("claude-stream-json is registered");
        Ok(HeadlessInvocation {
            program: self.program.clone(),
            argv: vec!["--rendered".to_string(), turn.prompt.clone()],
            env: Vec::new(),
            files: vec![(
                self.program.with_file_name("registration.json"),
                b"{\"rendered\":true}".to_vec(),
            )],
            adapter,
            exit_ok_if_structured: self.exit_ok_if_structured,
        })
    }
}

fn hints(permission: PermissionHint, plan_mode: bool, exit_ok_if_structured: bool) -> GoalHints {
    GoalHints {
        display_name: "Plan".to_string(),
        permission,
        allowed_tools: vec!["Read".to_string()],
        default_model: None,
        agent_output: false,
        agent_cli_plan_mode: plan_mode,
        claude_nonzero_exit_ok_if_structured_response: exit_ok_if_structured,
    }
}

fn a_request(hints: GoalHints, session: SessionMode, agent: &AFakeAgent) -> InvokeRequest {
    InvokeRequest {
        prompt: "Plan the feature".to_string(),
        system_prompt: None,
        system_prompt_path: Some(agent.path().join("system-prompt.md")),
        goal_id: GoalId::new("plan"),
        submit_key: GoalId::new("plan"),
        hints,
        model: Some("opus".to_string()),
        session: Some(session),
        working_dir: Some(agent.path().to_path_buf()),
        debug: false,
        agent_output: false,
        agent_output_sink: None,
        progress_sink: None,
        conversation_output_path: None,
        inherit_stdin: false,
        extra_allowed_tools: None,
        socket_path: None,
        session_dir: Some(agent.path().join("session")),
        remote: None,
    }
}

/// The turn the backend should hand the renderer for `a_request(…)` with `permission_mode` and
/// `session`.
fn the_expected_turn(
    agent: &AFakeAgent,
    permission_mode: Option<&str>,
    session: SessionMode,
) -> HeadlessTurn {
    HeadlessTurn {
        prompt: "Plan the feature".to_string(),
        model: Some("opus".to_string()),
        session: Some(session),
        permission_mode: permission_mode.map(str::to_string),
        append_system_prompt_file: Some(agent.path().join("system-prompt.md")),
        allowed_tools: vec!["Read".to_string()],
        denied_tools: Vec::new(),
        permission_prompt_tool: Some(APPROVAL_TOOL.to_string()),
        mcp_server: Some(the_tool_server()),
        workdir: agent.path().to_path_buf(),
        scratch_dir: agent.path().join("session"),
    }
}

/// A backend whose own binary does not exist: only the rendered program can run.
fn a_backend_rendered_by(renderer: Arc<ARecordingRenderer>) -> ClaudeCodeBackend {
    ClaudeCodeBackend::with_path(PathBuf::from("/nonexistent/claude-is-never-run"))
        .with_headless_renderer(renderer)
}

#[tokio::test]
async fn a_claude_backend_with_a_renderer_runs_the_rendered_program_and_returns_the_adapters_result(
) {
    // Given
    let agent = a_fake_agent(&a_claude_stream("sess-1", "The plan is ready."), 0);
    let renderer = a_recording_renderer(&agent, false);
    let backend = a_backend_rendered_by(Arc::clone(&renderer));

    // When
    let response = backend
        .invoke(a_request(
            hints(PermissionHint::ReadOnly, true, false),
            SessionMode::Fresh("sess-1".to_string()),
            &agent,
        ))
        .await
        .expect("the rendered invocation succeeds");

    // Then
    assert_eq!(
        agent.invocations(),
        vec!["--rendered Plan the feature".to_string()]
    );
    assert_eq!(
        (
            response.output.as_str(),
            response.session_id.as_deref(),
            response.exit_code
        ),
        ("The plan is ready.", Some("sess-1"), 0)
    );
}

#[tokio::test]
async fn a_claude_backend_maps_the_plan_goal_onto_the_plan_permission_mode_and_a_fresh_session() {
    // Given
    let agent = a_fake_agent(&a_claude_stream("sess-1", "ok"), 0);
    let renderer = a_recording_renderer(&agent, false);
    let backend = a_backend_rendered_by(Arc::clone(&renderer));

    // When
    backend
        .invoke(a_request(
            hints(PermissionHint::ReadOnly, true, false),
            SessionMode::Fresh("sess-1".to_string()),
            &agent,
        ))
        .await
        .expect("the rendered invocation succeeds");

    // Then
    assert_eq!(
        renderer.turns(),
        vec![the_expected_turn(
            &agent,
            Some("plan"),
            SessionMode::Fresh("sess-1".to_string())
        )]
    );
}

#[tokio::test]
async fn a_claude_backend_maps_accept_edits_onto_its_permission_mode_and_keeps_a_resume_a_resume() {
    // Given
    let agent = a_fake_agent(&a_claude_stream("sess-1", "ok"), 0);
    let renderer = a_recording_renderer(&agent, false);
    let backend = a_backend_rendered_by(Arc::clone(&renderer));

    // When
    backend
        .invoke(a_request(
            hints(PermissionHint::AcceptEdits, false, false),
            SessionMode::Resume("sess-1".to_string()),
            &agent,
        ))
        .await
        .expect("the rendered invocation succeeds");

    // Then
    assert_eq!(
        renderer.turns(),
        vec![the_expected_turn(
            &agent,
            Some("acceptEdits"),
            SessionMode::Resume("sess-1".to_string())
        )]
    );
}

#[tokio::test]
async fn a_read_only_goal_without_plan_mode_leaves_the_permission_mode_unset() {
    // Given
    let agent = a_fake_agent(&a_claude_stream("sess-1", "ok"), 0);
    let renderer = a_recording_renderer(&agent, false);
    let backend = a_backend_rendered_by(Arc::clone(&renderer));

    // When
    backend
        .invoke(a_request(
            hints(PermissionHint::ReadOnly, false, false),
            SessionMode::Fresh("sess-1".to_string()),
            &agent,
        ))
        .await
        .expect("the rendered invocation succeeds");

    // Then
    assert_eq!(
        renderer.turns(),
        vec![the_expected_turn(
            &agent,
            None,
            SessionMode::Fresh("sess-1".to_string())
        )]
    );
}

#[tokio::test]
async fn the_rendered_files_are_written_before_the_agent_starts() {
    // Given
    let agent = a_fake_agent(&a_claude_stream("sess-1", "ok"), 0);
    let backend = a_backend_rendered_by(a_recording_renderer(&agent, false));

    // When
    backend
        .invoke(a_request(
            hints(PermissionHint::ReadOnly, true, false),
            SessionMode::Fresh("sess-1".to_string()),
            &agent,
        ))
        .await
        .expect("the rendered invocation succeeds");

    // Then
    assert_eq!(agent.registrations_seen(), "{\"rendered\":true}");
}

#[tokio::test]
async fn a_non_zero_exit_with_a_structured_result_succeeds_when_the_declaration_allows_it() {
    // Given — the goal hint says no; the declaration says yes, and the declaration decides
    let agent = a_fake_agent(&a_claude_stream("sess-1", A_STRUCTURED_RESULT), 1);
    let backend = a_backend_rendered_by(a_recording_renderer(&agent, true));

    // When
    let response = backend
        .invoke(a_request(
            hints(PermissionHint::ReadOnly, true, false),
            SessionMode::Fresh("sess-1".to_string()),
            &agent,
        ))
        .await;

    // Then
    assert_eq!(
        response
            .map(|r| (r.output, r.exit_code))
            .map_err(|e| e.to_string()),
        Ok((A_STRUCTURED_RESULT.to_string(), 1))
    );
}

#[tokio::test]
async fn a_non_zero_exit_with_a_structured_result_fails_when_the_declaration_does_not_allow_it() {
    // Given — the goal hint says yes; the declaration says no, and the declaration decides
    let agent = a_fake_agent(&a_claude_stream("sess-1", A_STRUCTURED_RESULT), 1);
    let backend = a_backend_rendered_by(a_recording_renderer(&agent, false));

    // When
    let response = backend
        .invoke(a_request(
            hints(PermissionHint::ReadOnly, true, true),
            SessionMode::Fresh("sess-1".to_string()),
            &agent,
        ))
        .await;

    // Then
    assert_eq!(
        response.map(|r| r.exit_code).map_err(|e| e.to_string()),
        Err("backend invocation failed: Claude Code CLI exited with code 1".to_string())
    );
}

fn a_rendered_driver(agent: &AFakeAgent, renderer: Arc<ARecordingRenderer>) -> HeadlessDriver {
    HeadlessDriver::new(
        HeadlessHarness::Claude,
        HeadlessCommand {
            program: PathBuf::from("/nonexistent/claude-is-never-run"),
            cwd: Some(agent.path().to_path_buf()),
            model: Some("sonnet".to_string()),
            permission_prompt_tool: Some(APPROVAL_TOOL.to_string()),
            mcp_server: Some(McpServerSpec {
                name: "tddy-tools".to_string(),
                command: PathBuf::from(TDDY_TOOLS),
                args: vec!["--mcp".to_string()],
                env: vec![("TDDY_CONVERSATION_ID".to_string(), "conv-1".to_string())],
            }),
            scratch_dir: Some(agent.path().join("conv-1")),
            renderer: Some(renderer),
            ..HeadlessCommand::default()
        },
    )
}

/// The turn a rendered driver should hand its renderer for `prompt`, resuming `resume`.
fn the_expected_driver_turn(
    agent: &AFakeAgent,
    prompt: &str,
    resume: Option<&str>,
) -> HeadlessTurn {
    HeadlessTurn {
        prompt: prompt.to_string(),
        model: Some("sonnet".to_string()),
        session: resume.map(|id| SessionMode::Resume(id.to_string())),
        permission_mode: None,
        append_system_prompt_file: None,
        allowed_tools: Vec::new(),
        denied_tools: Vec::new(),
        permission_prompt_tool: Some(APPROVAL_TOOL.to_string()),
        mcp_server: Some(HeadlessMcpServer {
            env: BTreeMap::from([("TDDY_CONVERSATION_ID".to_string(), "conv-1".to_string())]),
            ..the_tool_server()
        }),
        workdir: agent.path().to_path_buf(),
        scratch_dir: agent.path().join("conv-1"),
    }
}

async fn a_turn(driver: &HeadlessDriver, prompt: &str) -> tddy_agent_backend::AcpTurnEnd {
    tokio::time::timeout(TURN_DEADLINE, driver.prompt_turn(prompt.to_string(), None))
        .await
        .expect("the turn ended in time")
        .expect("the turn succeeds")
}

#[tokio::test]
async fn a_headless_driver_with_a_renderer_runs_each_turns_rendered_argv_and_resumes_its_session() {
    // Given
    let agent = a_fake_agent(&a_claude_stream("cl-1", "Fixed it."), 0);
    let renderer = a_recording_renderer(&agent, false);
    let driver = a_rendered_driver(&agent, Arc::clone(&renderer));

    // When
    let first = a_turn(&driver, "Fix the bug").await;
    a_turn(&driver, "Now add a test").await;

    // Then
    assert_eq!(first.text, "Fixed it.");
    assert_eq!(
        agent.invocations(),
        vec![
            "--rendered Fix the bug".to_string(),
            "--rendered Now add a test".to_string(),
        ]
    );
    assert_eq!(
        renderer.turns(),
        vec![
            the_expected_driver_turn(&agent, "Fix the bug", None),
            the_expected_driver_turn(&agent, "Now add a test", Some("cl-1")),
        ]
    );
}
