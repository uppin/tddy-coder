//! Acceptance: the `claude` workflow backend (`tddy-coder --agent claude`) is rendered from
//! `claude.yaml`'s `coding_agent.headless` section, and a stub `claude` records exactly the argv
//! the hand-built `build_claude_args` passes today — for the plan goal, for accept-edits, and for a
//! resume — while a `<tddyhome>/tools/claude.yaml` override changes it.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-headless-agents-from-specs.md` (`#agent-tools 6/12`).
//!
//! The goldens below are `build_claude_args`' output for the same requests, with the MCP config at
//! its new home: `<session_dir>/claude-mcp-config.json`, written by the MCP slot, rather than a
//! temp file. The host is a fixture whose environment names a stub `claude` and a stub
//! `tddy-tools`; nothing of the real machine resolves.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolHost;
use tddy_coder::claude_from_spec::claude_backend_from_spec;
use tddy_coding_agent_specs::builtin_specs;
use tddy_core::{
    CodingBackend, GoalHints, GoalId, InvokeRequest, InvokeResponse, PermissionHint, SessionMode,
};

const APPROVAL_TOOL: &str = "mcp__tddy-tools__approval_prompt";
const PROMPT: &str = "Plan the feature";

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

/// A `<tddyhome>` with a `tools/` directory, a session directory, a system prompt, and a stub
/// `claude` (plus a stub `tddy-tools`) that records its argv — one entry per line — and the MCP
/// config it was handed, then prints one stream-json result.
struct AWorkspace {
    dir: tempfile::TempDir,
}

fn a_workspace() -> AWorkspace {
    let dir = tempfile::tempdir().expect("workspace");
    for sub in ["tools", "session", "repo", "bin"] {
        std::fs::create_dir(dir.path().join(sub)).expect("workspace dir");
    }
    std::fs::write(dir.path().join("system-prompt.md"), "You plan.").expect("system prompt");
    let claude = dir.path().join("bin").join("claude");
    std::fs::write(
        &claude,
        format!(
            r#"#!/bin/sh
for a in "$@"; do printf '%s\n' "$a"; done > '{argv}'
mcp=""; prev=""
for a in "$@"; do [ "$prev" = "--mcp-config" ] && mcp="$a"; prev="$a"; done
/bin/cat "$mcp" > '{mcp}' 2>/dev/null
printf '%s\n' '{{"type":"result","subtype":"success","result":"The plan is ready.","session_id":"sess-1"}}'
"#,
            argv = dir.path().join("argv.log").display(),
            mcp = dir.path().join("mcp.seen").display(),
        ),
    )
    .expect("stub claude");
    let tools = dir.path().join("bin").join("tddy-tools");
    std::fs::write(&tools, "#!/bin/sh\n").expect("stub tddy-tools");
    for file in [&claude, &tools] {
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    AWorkspace { dir }
}

impl AWorkspace {
    fn tools_dir(&self) -> PathBuf {
        self.dir.path().join("tools")
    }

    fn with_tool_override(self, file: &str, yaml: &str) -> Self {
        std::fs::write(self.tools_dir().join(file), yaml).expect("override");
        self
    }

    fn session_dir(&self) -> PathBuf {
        self.dir.path().join("session")
    }

    fn system_prompt(&self) -> PathBuf {
        self.dir.path().join("system-prompt.md")
    }

    fn tddy_tools(&self) -> PathBuf {
        std::fs::canonicalize(self.dir.path().join("bin").join("tddy-tools")).expect("tddy-tools")
    }

    fn host(&self) -> Arc<dyn ToolHost> {
        let bin = self.dir.path().join("bin");
        Arc::new(AFixtureHost {
            env: BTreeMap::from([
                (
                    "TDDY_CLAUDE_BINARY".to_string(),
                    bin.join("claude").to_string_lossy().into_owned(),
                ),
                (
                    "TDDY_TOOLS_BINARY".to_string(),
                    bin.join("tddy-tools").to_string_lossy().into_owned(),
                ),
            ]),
        })
    }

    fn mcp_config(&self) -> String {
        self.session_dir()
            .join("claude-mcp-config.json")
            .to_string_lossy()
            .into_owned()
    }

    fn argv(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("argv.log"))
            .expect("the stub ran")
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn mcp_config_seen(&self) -> serde_json::Value {
        serde_json::from_str(
            &std::fs::read_to_string(self.dir.path().join("mcp.seen")).expect("the stub ran"),
        )
        .expect("the MCP config is JSON")
    }

    fn request(&self, hints: GoalHints, session: SessionMode) -> InvokeRequest {
        InvokeRequest {
            prompt: PROMPT.to_string(),
            system_prompt: None,
            system_prompt_path: Some(self.system_prompt()),
            goal_id: GoalId::new("plan"),
            submit_key: GoalId::new("plan"),
            hints,
            model: Some("opus".to_string()),
            session: Some(session),
            working_dir: Some(self.dir.path().join("repo")),
            debug: false,
            agent_output: false,
            agent_output_sink: None,
            progress_sink: None,
            conversation_output_path: None,
            inherit_stdin: false,
            extra_allowed_tools: None,
            socket_path: None,
            session_dir: Some(self.session_dir()),
            remote: None,
        }
    }

    async fn invoke(&self, request: InvokeRequest) -> InvokeResponse {
        claude_backend_from_spec(&self.tools_dir(), self.host())
            .expect("the claude backend renders from its spec")
            .invoke(request)
            .await
            .expect("the invocation succeeds")
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

/// `build_claude_args`' argv for `PROMPT` on opus, with `middle` (permission mode and session) and
/// `allowed` tools, the workspace's system prompt and its MCP config.
fn todays_argv(
    workspace: &AWorkspace,
    after_verbose: &[&str],
    middle: &[&str],
    allowed: &[&str],
) -> Vec<String> {
    let mut argv: Vec<String> = ["-p", PROMPT, "--output-format", "stream-json", "--verbose"]
        .iter()
        .chain(after_verbose)
        .chain(middle)
        .map(|s| s.to_string())
        .collect();
    argv.push("--append-system-prompt-file".to_string());
    argv.push(workspace.system_prompt().to_string_lossy().into_owned());
    for tool in allowed {
        argv.push("--allowedTools".to_string());
        argv.push(tool.to_string());
    }
    argv.extend([
        "--permission-prompt-tool".to_string(),
        APPROVAL_TOOL.to_string(),
        "--mcp-config".to_string(),
        workspace.mcp_config(),
    ]);
    argv
}

#[tokio::test]
async fn tddy_coder_agent_claude_drives_a_stub_whose_argv_equals_todays_for_the_plan_goal() {
    // Given
    let workspace = a_workspace();
    let request = workspace.request(the_plan_goal(), SessionMode::Fresh("sess-1".to_string()));

    // When
    let response = workspace.invoke(request).await;

    // Then
    assert_eq!(response.output, "The plan is ready.");
    assert_eq!(
        workspace.argv(),
        todays_argv(
            &workspace,
            &[],
            &[
                "--permission-mode",
                "plan",
                "--model",
                "opus",
                "--session-id",
                "sess-1"
            ],
            &["Read", "Glob"],
        )
    );
    assert_eq!(
        workspace.mcp_config_seen(),
        serde_json::json!({
            "mcpServers": {
                "tddy-tools": {
                    "command": workspace.tddy_tools().to_string_lossy(),
                    "args": ["--mcp"],
                }
            }
        })
    );
}

#[tokio::test]
async fn the_same_for_accept_edits_and_for_resume() {
    // Given
    let workspace = a_workspace();
    let request = workspace.request(
        an_accept_edits_goal(),
        SessionMode::Resume("sess-1".to_string()),
    );

    // When
    workspace.invoke(request).await;

    // Then
    assert_eq!(
        workspace.argv(),
        todays_argv(
            &workspace,
            &[],
            &[
                "--permission-mode",
                "acceptEdits",
                "--model",
                "opus",
                "--resume",
                "sess-1"
            ],
            &["Edit"],
        )
    );
}

#[tokio::test]
async fn a_tddyhome_override_of_claude_yaml_changes_the_headless_argv() {
    // Given — the built-in claude.yaml with one more flag after `--verbose` in its headless argv
    let (_, builtin) = builtin_specs()
        .into_iter()
        .find(|(path, _)| path.ends_with("claude.yaml"))
        .expect("claude.yaml is built in");
    let headless_verbose = "        - --verbose\n";
    assert_eq!(builtin.matches(headless_verbose).count(), 1);
    let overridden = builtin.replace(
        headless_verbose,
        "        - --verbose\n        - --include-partial-messages\n",
    );
    let workspace = a_workspace().with_tool_override("claude.yaml", &overridden);
    let request = workspace.request(the_plan_goal(), SessionMode::Fresh("sess-1".to_string()));

    // When
    workspace.invoke(request).await;

    // Then
    assert_eq!(
        workspace.argv(),
        todays_argv(
            &workspace,
            &["--include-partial-messages"],
            &[
                "--permission-mode",
                "plan",
                "--model",
                "opus",
                "--session-id",
                "sess-1"
            ],
            &["Read", "Glob"],
        )
    );
}
