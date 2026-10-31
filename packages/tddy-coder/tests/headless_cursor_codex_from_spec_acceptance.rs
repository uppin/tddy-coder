//! Acceptance: the `cursor` and `codex` workflow backends (`tddy-coder --agent cursor|codex`) are
//! rendered from `cursor.yaml`'s and `codex.yaml`'s `coding_agent.headless` sections, and stub CLIs
//! record exactly the argv the hand-built `build_cursor_cli_args` and `build_codex_exec_argv` pass
//! today — the resume and the thread id included — while a Codex auth failure still carries
//! today's sign-in remediation (`#agent-tools 9/12`).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-headless-cursor-codex-oz-from-specs.md`.
//!
//! The host is a fixture whose environment names stub `agent`, `codex` and `tddy-tools` binaries;
//! nothing of the real machine resolves.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolHost;
use tddy_coder::headless_from_spec::{codex_backend_from_spec, cursor_backend_from_spec};
use tddy_core::{
    CodexBackend, CodingBackend, GoalHints, GoalId, InvokeRequest, PermissionHint, SessionMode,
};

const PROMPT: &str = "Plan the feature";
const SYSTEM_PROMPT: &str = "You plan.";
/// The prompt both CLIs are handed: neither takes a system prompt, so it is merged in front.
const MERGED_PROMPT: &str = "You plan.\n\nPlan the feature";
/// The start of the hint `codex_openai_auth_remediation` appends to a sign-in failure today.
const SIGN_IN_REMEDIATION: &str = "Fix: not signed in to OpenAI for Codex.";

/// A host with nothing of the real machine: the env it is given, and executable files.
struct AFixtureHost {
    env: BTreeMap<String, String>,
}

impl ToolHost for AFixtureHost {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// How the stub `codex` behaves.
#[derive(Clone, Copy)]
enum CodexRun {
    /// Prints a thread and the agent's message, and exits 0.
    Succeeds,
    /// Prints a 401 sign-in failure, and exits 1.
    IsNotSignedIn,
}

/// A `<tddyhome>` with a `tools/` directory, a session directory, a repo, a system prompt, and
/// stubs `agent` and `codex` (plus a stub `tddy-tools`) that record their argv — one entry per line
/// — then print one recorded run.
struct AWorkspace {
    dir: tempfile::TempDir,
}

fn a_workspace(codex: CodexRun) -> AWorkspace {
    let dir = tempfile::tempdir().expect("workspace");
    for sub in ["tools", "session", "repo", "bin"] {
        std::fs::create_dir(dir.path().join(sub)).expect("workspace dir");
    }
    std::fs::write(dir.path().join("system-prompt.md"), SYSTEM_PROMPT).expect("system prompt");
    let record = |name: &str| dir.path().join(name).display().to_string();
    let codex_run = match codex {
        CodexRun::Succeeds => concat!(
            r#"printf '%s\n' '{"type":"thread.started","thread_id":"th-1"}'"#,
            "\n",
            r#"printf '%s\n' '{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"The plan is ready."}}'"#,
            "\n",
        ),
        CodexRun::IsNotSignedIn => concat!(
            r#"printf '%s\n' '{"type":"error","message":"unexpected status 401 Unauthorized: Missing bearer or basic authentication"}'"#,
            "\n",
            "exit 1\n",
        ),
    };
    let scripts = [
        (
            "agent",
            format!(
                r#"#!/bin/sh
for a in "$@"; do printf '%s\n' "$a"; done > '{argv}'
printf '%s\n' '{{"type":"system","subtype":"init","session_id":"chat-1"}}'
printf '%s\n' '{{"type":"result","subtype":"success","result":"The plan is ready.","session_id":"chat-1"}}'
"#,
                argv = record("agent.argv"),
            ),
        ),
        (
            "codex",
            format!(
                "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{argv}'\n{codex_run}",
                argv = record("codex.argv"),
            ),
        ),
        ("tddy-tools", "#!/bin/sh\n".to_string()),
    ];
    for (name, script) in scripts {
        let path = dir.path().join("bin").join(name);
        std::fs::write(&path, script).expect("write stub");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    AWorkspace { dir }
}

impl AWorkspace {
    fn tools_dir(&self) -> PathBuf {
        self.dir.path().join("tools")
    }

    fn repo(&self) -> PathBuf {
        self.dir.path().join("repo")
    }

    fn bin(&self, name: &str) -> PathBuf {
        self.dir.path().join("bin").join(name)
    }

    fn host(&self) -> Arc<dyn ToolHost> {
        let env = |name: &str| self.bin(name).to_string_lossy().into_owned();
        Arc::new(AFixtureHost {
            env: BTreeMap::from([
                ("TDDY_CURSOR_AGENT".to_string(), env("agent")),
                ("TDDY_CODEX_CLI".to_string(), env("codex")),
                ("TDDY_TOOLS_BINARY".to_string(), env("tddy-tools")),
            ]),
        })
    }

    fn argv(&self, cli: &str) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join(format!("{cli}.argv")))
            .unwrap_or_else(|e| panic!("the {cli} stub ran: {e}"))
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn request(&self, hints: GoalHints, model: &str, session: SessionMode) -> InvokeRequest {
        InvokeRequest {
            prompt: PROMPT.to_string(),
            system_prompt: None,
            system_prompt_path: Some(self.dir.path().join("system-prompt.md")),
            goal_id: GoalId::new("plan"),
            submit_key: GoalId::new("plan"),
            hints,
            model: Some(model.to_string()),
            session: Some(session),
            working_dir: Some(self.repo()),
            debug: false,
            agent_output: false,
            agent_output_sink: None,
            progress_sink: None,
            conversation_output_path: None,
            inherit_stdin: false,
            extra_allowed_tools: None,
            socket_path: None,
            session_dir: Some(self.dir.path().join("session")),
            remote: None,
        }
    }
}

fn the_plan_goal() -> GoalHints {
    GoalHints {
        display_name: "Plan".to_string(),
        permission: PermissionHint::ReadOnly,
        allowed_tools: vec!["Read".to_string()],
        default_model: None,
        agent_output: false,
        agent_cli_plan_mode: true,
        claude_nonzero_exit_ok_if_structured_response: true,
    }
}

fn an_accept_edits_goal() -> GoalHints {
    GoalHints {
        display_name: "Green".to_string(),
        permission: PermissionHint::AcceptEdits,
        allowed_tools: vec!["Edit".to_string()],
        default_model: None,
        agent_output: false,
        agent_cli_plan_mode: false,
        claude_nonzero_exit_ok_if_structured_response: false,
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[tokio::test]
async fn tddy_coder_agent_cursor_drives_a_stub_whose_argv_equals_todays_including_resume() {
    // Given
    let workspace = a_workspace(CodexRun::Succeeds);
    let backend = cursor_backend_from_spec(&workspace.tools_dir(), workspace.host())
        .expect("the cursor backend renders from its spec");
    let request = workspace.request(
        the_plan_goal(),
        "composer-2.5",
        SessionMode::Resume("chat-1".to_string()),
    );

    // When
    let response = backend
        .invoke(request)
        .await
        .expect("the invocation succeeds");

    // Then — `build_cursor_cli_args`' argv for the same request
    let registration: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(workspace.repo().join(".cursor/mcp.json"))
            .expect("the registration is in the repo"),
    )
    .expect("the registration is JSON");
    assert_eq!(
        (response.output, workspace.argv("agent"), registration),
        (
            "The plan is ready.".to_string(),
            strings(&[
                "--plan",
                "--resume",
                "chat-1",
                "-p",
                MERGED_PROMPT,
                "--model",
                "composer-2.5",
                "--output-format",
                "stream-json",
                "--stream-partial-output",
                "--force",
                "--trust",
                "--approve-mcps",
            ]),
            serde_json::json!({
                "mcpServers": {
                    "tddy-tools": {
                        "command": std::fs::canonicalize(workspace.bin("tddy-tools"))
                            .expect("tddy-tools")
                            .to_string_lossy(),
                        "args": ["--mcp"],
                    }
                }
            }),
        )
    );
}

#[tokio::test]
async fn tddy_coder_agent_codex_drives_a_stub_whose_argv_equals_todays_including_the_thread_id() {
    // Given
    let workspace = a_workspace(CodexRun::Succeeds);
    let backend = codex_backend_from_spec(&workspace.tools_dir(), workspace.host())
        .expect("the codex backend renders from its spec");
    let request = workspace.request(
        an_accept_edits_goal(),
        "gpt-5.3-codex",
        SessionMode::Resume("th-1".to_string()),
    );

    // When
    let response = backend
        .invoke(request)
        .await
        .expect("the invocation succeeds");

    // Then — `build_codex_exec_argv`' argv for the same request
    assert_eq!(
        (
            response.output,
            response.session_id,
            workspace.argv("codex")
        ),
        (
            "The plan is ready.".to_string(),
            Some("th-1".to_string()),
            strings(&[
                "exec",
                "-C",
                &workspace.repo().to_string_lossy(),
                "-s",
                "workspace-write",
                "--json",
                "resume",
                "-m",
                "gpt-5.3-codex",
                "th-1",
                MERGED_PROMPT,
            ]),
        )
    );
}

/// A preserved-behaviour pin: no login retry exists on this path today, only the hint. It fails
/// only if the spec path loses what the hand-built path appends.
#[tokio::test]
async fn a_codex_auth_error_still_carries_todays_sign_in_remediation_through_the_spec() {
    // Given
    let workspace = a_workspace(CodexRun::IsNotSignedIn);
    let request = || {
        workspace.request(
            the_plan_goal(),
            "gpt-5.3-codex",
            SessionMode::Fresh("unused".to_string()),
        )
    };
    let todays = CodexBackend::with_path(workspace.bin("codex"))
        .invoke(request())
        .await
        .map(|_| ())
        .map_err(|e| e.to_string());
    let backend = codex_backend_from_spec(&workspace.tools_dir(), workspace.host())
        .expect("the codex backend renders from its spec");

    // When
    let from_spec = backend
        .invoke(request())
        .await
        .map(|_| ())
        .map_err(|e| e.to_string());

    // Then — the positive control first: today's path appends the hint
    assert_eq!(
        (
            todays.map_err(|e| e.contains(SIGN_IN_REMEDIATION)),
            from_spec.map_err(|e| e.contains(SIGN_IN_REMEDIATION)),
        ),
        (Err(true), Err(true))
    );
}
