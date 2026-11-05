//! Acceptance: one spec-configured headless workflow backend runs `claude`, `cursor`, `codex` and
//! any agent a YAML file declares (`#agent-tools 15/19`), with the argv, MCP registration, session
//! and error text the bespoke backends produced — and the behaviour changes the stack decided:
//! the agent sees no `TDDY_*` variable, a shell-run `tddy-tools` finds its session through
//! `.tddy/session.json` in the working directory, registration files are git-excluded and removed
//! with the turn, Codex reports progress and hands its tools the remote session, and one stderr rule
//! holds for every agent.
//!
//! Changeset: `docs/dev/1-WIP/2026-11-04-agent-tools-headless-driver.md`. The deadline and
//! process-group tests are in `tddy-agent-backend/tests/spec_headless_backend_acceptance.rs`.
//!
//! The host is a fixture whose environment names stub binaries; nothing of the real machine
//! resolves. Each stub records its argv (one entry per line), the `TDDY_*` variables it sees, the
//! session pointer and MCP registration it found, then prints one recorded run.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use serial_test::serial;
use tddy_actions::tool::ToolHost;
use tddy_coder::headless_from_spec::headless_backend_from_spec;
use tddy_core::backend::{BackendModel, BackendModels};
// `HarnessRoute` is the route body, which `tddy_coding_agent_specs::route` does not re-export.
use tddy_core::provider_route::HarnessRoute;
use tddy_core::{
    CodingBackend, GoalHints, GoalId, InvokeRequest, PermissionHint, RemoteToolEnv, SessionMode,
};

const PROMPT: &str = "Plan the feature";
const SYSTEM_PROMPT: &str = "You plan.";
const APPROVAL_TOOL: &str = "mcp__tddy-tools__approval_prompt";
/// The prompt a CLI with no system-prompt flag is handed, as a stub records it: one line per argv
/// entry, so the merged prompt's own lines are entries here.
const MERGED_PROMPT_LINES: [&str; 3] = [SYSTEM_PROMPT, "", PROMPT];
/// The first line of the hint `codex_openai_auth_remediation` appends to a sign-in failure.
const SIGN_IN_REMEDIATION: &str = "Fix: not signed in to OpenAI for Codex. `codex exec --json` does not print a login URL or start browser OAuth — only API errors.";
/// What the stub `agent --list-models` prints.
const CURSOR_MODEL_LISTING: &str =
    "Available models\n\nauto - Auto\ncomposer-2.5 - Composer 2.5 (current, default)\n";

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

/// `lark.yaml`: a headless agent no Rust names, read through `claude-stream-json`.
const LARK_YAML: &str = r#"schema_version: 1
id: lark
summary: Lark Code — a headless agent declared only in YAML
binary:
  command: lark
  env: TDDY_LARK_BINARY
deps: [tddy-tools]
reads: []
run:
  transport: pty
  argv: ["{binary}"]
  env: {}
extensions:
  coding_agent:
    product: Lark Code
    mcp: { kind: file, flag: --mcp-config, format: claude-mcp-json }
    tool_allow: unsupported
    tool_deny: unsupported
    permission_prompt: unsupported
    instructions:
      files: [AGENTS.md]
    hooks: { adapter: claude-settings-local }
    models:
      default: lark-1
      curated: [lark-1]
    headless:
      argv: ["{binary}", run, "{prompt}", { splice: slots }]
      adapter: claude-stream-json
      workflow_mcp: true
"#;

/// The shell lines every stub starts with: record argv, the `TDDY_*` variables it sees, the session
/// pointer in its cwd and (for `claude`/`lark`) the `--mcp-config` file it was handed.
fn recording_prologue(name: &str, record: &Path) -> String {
    let at = |what: &str| record.join(format!("{name}.{what}")).display().to_string();
    format!(
        r#"for a in "$@"; do printf '%s\n' "$a"; done > '{argv}'
env | grep '^TDDY_' | sort > '{env}'
[ -f .tddy/session.json ] && /bin/cat .tddy/session.json > '{pointer}'
[ -f .cursor/mcp.json ] && /bin/cat .cursor/mcp.json > '{registration}'
prev=""; for a in "$@"; do [ "$prev" = "--mcp-config" ] && /bin/cat "$a" > '{registration}'; prev="$a"; done
"#,
        argv = at("argv"),
        env = at("tddy-env"),
        pointer = at("pointer"),
        registration = at("registration"),
    )
}

/// A `<tddyhome>` with `tools/` (holding `lark.yaml`), a session directory, a git checkout as the
/// repo, a system prompt, and stubs `claude`, `agent`, `codex`, `lark` and `tddy-tools`.
struct AWorkspace {
    dir: tempfile::TempDir,
}

fn a_workspace(codex: CodexRun) -> AWorkspace {
    let dir = tempfile::tempdir().expect("workspace");
    for sub in ["tools", "session", "repo", "bin", "record"] {
        std::fs::create_dir(dir.path().join(sub)).expect("workspace dir");
    }
    let git = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path().join("repo"))
        .status()
        .expect("git runs");
    assert!(git.success(), "the repo is a git checkout");
    std::fs::write(dir.path().join("system-prompt.md"), SYSTEM_PROMPT).expect("system prompt");
    std::fs::write(dir.path().join("tools").join("lark.yaml"), LARK_YAML).expect("lark.yaml");
    let record = dir.path().join("record");
    let stream_json_result = r#"printf '%s\n' '{"type":"result","subtype":"success","result":"The plan is ready.","session_id":"sess-1"}'"#;
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
            "claude",
            format!(
                "#!/bin/sh\n{}echo 'claude: a warning' >&2\n{stream_json_result}\n",
                recording_prologue("claude", &record)
            ),
        ),
        (
            "agent",
            format!(
                "#!/bin/sh\nif [ \"$1\" = --list-models ]; then printf '%s' '{CURSOR_MODEL_LISTING}'; exit 0; fi\n{}{}\n",
                recording_prologue("agent", &record),
                r#"printf '%s\n' '{"type":"system","subtype":"init","session_id":"chat-1"}'
printf '%s\n' '{"type":"result","subtype":"success","result":"The plan is ready.","session_id":"chat-1"}'"#
            ),
        ),
        (
            "codex",
            format!(
                "#!/bin/sh\n{}{codex_run}",
                recording_prologue("codex", &record)
            ),
        ),
        (
            "lark",
            format!(
                "#!/bin/sh\n{}{stream_json_result}\n",
                recording_prologue("lark", &record)
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

    fn session_dir(&self) -> PathBuf {
        self.dir.path().join("session")
    }

    fn socket(&self) -> PathBuf {
        self.dir.path().join("session").join("toolcall.sock")
    }

    fn bin(&self, name: &str) -> PathBuf {
        self.dir.path().join("bin").join(name)
    }

    fn tddy_tools(&self) -> PathBuf {
        std::fs::canonicalize(self.bin("tddy-tools")).expect("tddy-tools")
    }

    fn host(&self) -> Arc<dyn ToolHost> {
        let env = |name: &str| self.bin(name).to_string_lossy().into_owned();
        Arc::new(AFixtureHost {
            env: BTreeMap::from([
                ("TDDY_CLAUDE_BINARY".to_string(), env("claude")),
                ("TDDY_CURSOR_AGENT".to_string(), env("agent")),
                ("TDDY_CODEX_CLI".to_string(), env("codex")),
                ("TDDY_LARK_BINARY".to_string(), env("lark")),
                ("TDDY_TOOLS_BINARY".to_string(), env("tddy-tools")),
            ]),
        })
    }

    fn backend(&self, agent: &str) -> tddy_core::SpecHeadlessBackend {
        headless_backend_from_spec(agent, &self.tools_dir(), self.host())
            .unwrap_or_else(|e| panic!("the {agent} backend renders from its spec: {e:#}"))
    }

    fn recorded(&self, stub: &str, what: &str) -> Option<String> {
        std::fs::read_to_string(
            self.dir
                .path()
                .join("record")
                .join(format!("{stub}.{what}")),
        )
        .ok()
    }

    fn argv(&self, stub: &str) -> Vec<String> {
        self.recorded(stub, "argv")
            .unwrap_or_else(|| panic!("the {stub} stub ran"))
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn tddy_variables_seen(&self, stub: &str) -> String {
        self.recorded(stub, "tddy-env")
            .unwrap_or_else(|| panic!("the {stub} stub ran"))
    }

    fn pointer_seen(&self, stub: &str) -> Option<serde_json::Value> {
        self.recorded(stub, "pointer")
            .map(|text| serde_json::from_str(&text).expect("the pointer is JSON"))
    }

    fn registration_seen(&self, stub: &str) -> Option<serde_json::Value> {
        self.recorded(stub, "registration")
            .map(|text| serde_json::from_str(&text).expect("the registration is JSON"))
    }

    fn excluded(&self) -> Vec<String> {
        std::fs::read_to_string(self.repo().join(".git/info/exclude"))
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
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
            socket_path: Some(self.socket()),
            session_dir: Some(self.session_dir()),
            remote: None,
        }
    }
}

fn the_plan_goal() -> GoalHints {
    GoalHints {
        display_name: "Plan".to_string(),
        permission: PermissionHint::ReadOnly,
        allowed_tools: vec!["Read".to_string(), "Glob".to_string()],
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

fn a_remote() -> RemoteToolEnv {
    RemoteToolEnv {
        daemon_url: "https://daemon.example".to_string(),
        daemon_socket: None,
        session_id: "remote-session-1".to_string(),
        session_token: "remote-token".to_string(),
        daemon_instance_id: None,
        livekit_url: None,
        livekit_room: None,
        server_identity: None,
        livekit_token: None,
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// The pointer a turn in `workspace` writes for a session with no remote.
fn the_sessions_pointer(workspace: &AWorkspace) -> serde_json::Value {
    serde_json::json!({
        "socket": workspace.socket().to_string_lossy(),
        "repo_dir": workspace.repo().to_string_lossy(),
        "session_dir": workspace.session_dir().to_string_lossy(),
    })
}

// --- Any agent a spec declares ------------------------------------------------------------------

#[tokio::test]
async fn a_headless_agent_declared_only_in_yaml_runs_a_workflow_turn_through_its_named_stream_adapter(
) {
    // Given
    let workspace = a_workspace(CodexRun::Succeeds);
    let backend = workspace.backend("lark");
    let request = workspace.request(
        an_accept_edits_goal(),
        "lark-1",
        SessionMode::Fresh("unused".to_string()),
    );

    // When
    let response = backend.invoke(request).await.expect("the turn succeeds");

    // Then
    assert_eq!(
        (
            backend.name().to_string(),
            response.output,
            response.session_id,
            workspace.argv("lark")[..3].to_vec(),
        ),
        (
            "lark".to_string(),
            "The plan is ready.".to_string(),
            Some("sess-1".to_string()),
            strings(&["run", MERGED_PROMPT_LINES[0], MERGED_PROMPT_LINES[1]]),
        )
    );
}

#[tokio::test]
async fn a_spec_without_a_route_section_refuses_a_provider_route() {
    // Given
    let workspace = a_workspace(CodexRun::Succeeds);
    let backend = workspace.backend("lark");

    // When
    let refused = backend
        .with_provider_route(HarnessRoute::default())
        .map(|_| ())
        .map_err(|refusal| refusal.to_string());

    // Then
    assert_eq!(
        refused,
        Err(
            "Lark Code cannot be pointed at another provider; it always runs on its own vendor"
                .to_string()
        )
    );
}

// --- Parity with the bespoke backends -----------------------------------------------------------

#[tokio::test]
async fn the_claude_spec_turn_has_the_argv_mcp_config_and_session_id_the_claude_backend_produced() {
    // Given
    let workspace = a_workspace(CodexRun::Succeeds);
    let request = workspace.request(
        the_plan_goal(),
        "opus",
        SessionMode::Fresh("sess-1".to_string()),
    );

    // When
    let response = workspace
        .backend("claude")
        .invoke(request)
        .await
        .expect("the turn succeeds");

    // Then — `build_claude_args`' argv, the MCP config at `<session_dir>/claude-mcp-config.json`
    let mcp_config = workspace.session_dir().join("claude-mcp-config.json");
    let system_prompt = workspace.dir.path().join("system-prompt.md");
    assert_eq!(
        (
            response.session_id,
            workspace.argv("claude"),
            workspace.registration_seen("claude"),
        ),
        (
            Some("sess-1".to_string()),
            strings(&[
                "-p",
                PROMPT,
                "--output-format",
                "stream-json",
                "--verbose",
                "--permission-mode",
                "plan",
                "--model",
                "opus",
                "--session-id",
                "sess-1",
                "--append-system-prompt-file",
                &system_prompt.to_string_lossy(),
                "--allowedTools",
                "Read",
                "--allowedTools",
                "Glob",
                "--permission-prompt-tool",
                APPROVAL_TOOL,
                "--mcp-config",
                &mcp_config.to_string_lossy(),
            ]),
            Some(serde_json::json!({
                "mcpServers": {
                    "tddy-tools": {
                        "command": workspace.tddy_tools().to_string_lossy(),
                        "args": ["--mcp"],
                    }
                }
            })),
        )
    );
}

#[tokio::test]
async fn the_cursor_spec_turn_prepends_the_system_prompt_and_forces_trust_as_the_cursor_backend_did(
) {
    // Given
    let workspace = a_workspace(CodexRun::Succeeds);
    let request = workspace.request(
        the_plan_goal(),
        "composer-2.5",
        SessionMode::Resume("chat-1".to_string()),
    );

    // When
    let response = workspace
        .backend("cursor")
        .invoke(request)
        .await
        .expect("the turn succeeds");

    // Then — `build_cursor_cli_args`' argv, and the registration it read from the worktree
    assert_eq!(
        (
            response.output,
            workspace.argv("agent"),
            workspace.registration_seen("agent")
        ),
        (
            "The plan is ready.".to_string(),
            strings(&[
                "--plan",
                "--resume",
                "chat-1",
                "-p",
                MERGED_PROMPT_LINES[0],
                MERGED_PROMPT_LINES[1],
                MERGED_PROMPT_LINES[2],
                "--model",
                "composer-2.5",
                "--output-format",
                "stream-json",
                "--stream-partial-output",
                "--force",
                "--trust",
                "--approve-mcps",
            ]),
            Some(serde_json::json!({
                "mcpServers": {
                    "tddy-tools": {
                        "command": workspace.tddy_tools().to_string_lossy(),
                        "args": ["--mcp"],
                    }
                }
            })),
        )
    );
}

#[tokio::test]
async fn the_codex_spec_turn_runs_read_only_in_plan_mode_and_reports_the_auth_remediation_on_a_login_failure(
) {
    // Given
    let planning = a_workspace(CodexRun::Succeeds);
    let signed_out = a_workspace(CodexRun::IsNotSignedIn);
    let a_plan_turn = |workspace: &AWorkspace| {
        workspace.request(
            the_plan_goal(),
            "gpt-5.3-codex",
            SessionMode::Resume("th-1".to_string()),
        )
    };

    // When
    let planned = planning
        .backend("codex")
        .invoke(a_plan_turn(&planning))
        .await
        .map(|response| response.session_id)
        .map_err(|e| e.to_string());
    let refused = signed_out
        .backend("codex")
        .invoke(a_plan_turn(&signed_out))
        .await
        .map(|response| response.output)
        .map_err(|e| e.to_string());

    // Then
    let refusal = refused.expect_err("a signed-out codex fails the turn");
    let (failure, remediation) = refusal
        .split_once("\n\n")
        .expect("the remediation follows the failure after a blank line");
    assert_eq!(
        (
            planned,
            planning.argv("codex"),
            failure,
            remediation.lines().next()
        ),
        (
            Ok(Some("th-1".to_string())),
            strings(&[
                "exec",
                "-C",
                &planning.repo().to_string_lossy(),
                "-s",
                "read-only",
                "--json",
                "resume",
                "-m",
                "gpt-5.3-codex",
                "th-1",
                MERGED_PROMPT_LINES[0],
                MERGED_PROMPT_LINES[1],
                MERGED_PROMPT_LINES[2],
            ]),
            "codex exited with code 1: unexpected status 401 Unauthorized: Missing bearer or basic authentication",
            Some(SIGN_IN_REMEDIATION),
        )
    );
}

#[tokio::test]
async fn cursor_list_models_runs_the_spec_resolved_binary() {
    // Given — the stub `agent` is named only by the host's `TDDY_CURSOR_AGENT`
    let workspace = a_workspace(CodexRun::Succeeds);

    // When
    let models = workspace
        .backend("cursor")
        .list_models()
        .await
        .map_err(|e| e.to_string());

    // Then
    assert_eq!(
        models,
        Ok(BackendModels {
            models: vec![
                BackendModel {
                    id: "auto".to_string(),
                    label: "Auto".to_string()
                },
                BackendModel {
                    id: "composer-2.5".to_string(),
                    label: "Composer 2.5".to_string()
                },
            ],
            default_model: "composer-2.5".to_string(),
        })
    );
}

// --- The decided behaviour changes --------------------------------------------------------------

/// A variable this test process — standing in for the daemon — carries, which no agent may inherit.
const DAEMON_ONLY_VARIABLE: &str = "TDDY_DAEMON_ONLY_PROBE";

#[tokio::test]
#[serial]
async fn a_headless_agent_process_sees_no_tddy_variable_even_when_the_daemon_has_one() {
    // Given — the daemon's own environment has a `TDDY_*` variable, and the request a socket, a
    // repo, a session directory and a remote
    std::env::set_var(DAEMON_ONLY_VARIABLE, "the daemon's");
    let workspace = a_workspace(CodexRun::Succeeds);
    let mut request = workspace.request(
        an_accept_edits_goal(),
        "opus",
        SessionMode::Fresh("sess-1".to_string()),
    );
    request.remote = Some(a_remote());

    // When
    let outcome = workspace
        .backend("claude")
        .invoke(request)
        .await
        .map(|response| response.output)
        .map_err(|e| e.to_string());
    std::env::remove_var(DAEMON_ONLY_VARIABLE);

    // Then
    assert_eq!(
        (outcome, workspace.tddy_variables_seen("claude")),
        (Ok("The plan is ready.".to_string()), String::new())
    );
}

#[tokio::test]
async fn a_turn_points_its_working_directory_at_the_session_for_a_shell_run_tddy_tools() {
    // Given
    let workspace = a_workspace(CodexRun::Succeeds);
    let request = workspace.request(
        the_plan_goal(),
        "gpt-5.3-codex",
        SessionMode::Fresh("unused".to_string()),
    );

    // When
    workspace
        .backend("codex")
        .invoke(request)
        .await
        .expect("the turn succeeds");

    // Then — the agent found the pointer in its cwd; it is git-excluded, and gone after the turn
    assert_eq!(
        (
            workspace.pointer_seen("codex"),
            workspace.excluded(),
            workspace.repo().join(".tddy/session.json").exists(),
        ),
        (
            Some(the_sessions_pointer(&workspace)),
            vec!["/.tddy/session.json".to_string()],
            false
        )
    );
}

#[tokio::test]
async fn the_mcp_config_a_claude_turn_wrote_is_removed_when_the_turn_ends() {
    // Given
    let workspace = a_workspace(CodexRun::Succeeds);
    let request = workspace.request(
        an_accept_edits_goal(),
        "opus",
        SessionMode::Fresh("sess-1".to_string()),
    );

    // When
    workspace
        .backend("claude")
        .invoke(request)
        .await
        .expect("the turn succeeds");

    // Then — the agent was handed the registration, and it is gone with the turn
    assert_eq!(
        (
            workspace.registration_seen("claude").is_some(),
            workspace
                .session_dir()
                .join("claude-mcp-config.json")
                .exists(),
        ),
        (true, false)
    );
}

#[tokio::test]
async fn a_cursor_turn_git_excludes_the_mcp_config_it_writes_in_the_worktree_and_removes_it_after()
{
    // Given
    let workspace = a_workspace(CodexRun::Succeeds);
    let request = workspace.request(
        an_accept_edits_goal(),
        "composer-2.5",
        SessionMode::Fresh("unused".to_string()),
    );

    // When
    workspace
        .backend("cursor")
        .invoke(request)
        .await
        .expect("the turn succeeds");

    // Then
    assert_eq!(
        (
            workspace.registration_seen("agent").is_some(),
            workspace.excluded(),
            workspace.repo().join(".cursor/mcp.json").exists(),
        ),
        (
            true,
            vec![
                "/.cursor/mcp.json".to_string(),
                "/.tddy/session.json".to_string()
            ],
            false
        )
    );
}

#[tokio::test]
async fn a_codex_turn_reports_its_exit_as_progress_and_hands_its_tools_the_remote_session() {
    // Given — a codex backend with a progress callback, and a request with a remote and no sink
    let workspace = a_workspace(CodexRun::Succeeds);
    let progress: Arc<Mutex<Vec<String>>> = Arc::default();
    let backend = {
        let progress = Arc::clone(&progress);
        workspace.backend("codex").with_progress(move |event| {
            progress
                .lock()
                .expect("progress")
                .push(format!("{event:?}"))
        })
    };
    let mut request = workspace.request(
        an_accept_edits_goal(),
        "gpt-5.3-codex",
        SessionMode::Fresh("unused".to_string()),
    );
    request.remote = Some(a_remote());

    // When
    backend.invoke(request).await.expect("the turn succeeds");

    // Then — its tools reach the remote through the pointer, never through its environment, which
    // carries only Codex's OAuth capture
    let mut pointer = the_sessions_pointer(&workspace);
    pointer["remote"] = serde_json::json!({
        "TDDY_REMOTE_DAEMON_URL": "https://daemon.example",
        "TDDY_REMOTE_SESSION_ID": "remote-session-1",
        "TDDY_REMOTE_SESSION_TOKEN": "remote-token",
    });
    assert_eq!(
        (
            progress.lock().expect("progress").clone(),
            workspace.pointer_seen("codex"),
            workspace.tddy_variables_seen("codex"),
        ),
        (
            vec!["AgentExited { exit_code: 0, goal: \"plan\" }".to_string()],
            Some(pointer),
            // TODO(headless-driver): the one exception, owned by the Codex OAuth follow-up stack —
            // docs/dev/todo/2026-11-05-codex-oauth-out-env-in-headless-agent.md
            format!(
                "TDDY_CODEX_OAUTH_OUT={}/codex_oauth_authorize.url\n",
                workspace.session_dir().display()
            )
        )
    );
}

#[tokio::test]
async fn every_agents_response_keeps_its_stderr_when_it_left_any() {
    // Given — claude prints a warning on stderr next to a complete stream
    let workspace = a_workspace(CodexRun::Succeeds);
    let request = workspace.request(
        an_accept_edits_goal(),
        "opus",
        SessionMode::Fresh("sess-1".to_string()),
    );

    // When
    let response = workspace
        .backend("claude")
        .invoke(request)
        .await
        .expect("the turn succeeds");

    // Then — the bespoke claude backend dropped it whenever the stream had lines; one rule now
    assert_eq!(response.stderr, Some("claude: a warning\n".to_string()));
}
