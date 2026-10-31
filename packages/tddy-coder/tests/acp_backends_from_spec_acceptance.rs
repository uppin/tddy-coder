//! Acceptance: the `claude-acp`, `codex-acp` and `opencode` workflow backends
//! (`tddy-coder --agent claude-acp|codex-acp|opencode`) are built from `claude-acp.yaml`'s,
//! `codex-acp.yaml`'s and `opencode.yaml`'s `coding_agent.acp` sections, and each spawns the
//! command its hand-built constructor spawns today — `bunx claude-agent-acp`, `codex-acp` beside
//! `codex`, `opencode acp` — then answers a turn over ACP (`#agent-tools 11/12`).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-acp-agents-from-specs.md`.
//!
//! The host is a fixture: its `PATH` is one directory holding a `bunx`, and its environment names
//! stub `codex`, `opencode` and `tddy-tools` binaries. Every agent binary is a shell wrapper that
//! records its argv, then `exec`s `tddy-acp-stub` scripted to answer one turn. Needs
//! `target/debug/tddy-acp-stub` (`cargo build -p tddy-acp-stub`).

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolHost;
use tddy_coder::acp_from_spec::{
    claude_acp_backend_from_spec, codex_acp_backend_from_spec, opencode_acp_backend_from_spec,
};
use tddy_core::{CodingBackend, GoalHints, GoalId, InvokeRequest, PermissionHint, SessionMode};

const PROMPT: &str = "Plan the feature";
const ANSWER: &str = "The plan is ready.";
/// The model `tddy-acp-stub` advertises.
const STUB_MODEL: &str = "stub-model";

/// A host with nothing of the real machine: one `PATH` directory, the env it is given, and
/// executable files.
struct AFixtureHost {
    path: PathBuf,
    env: BTreeMap<String, String>,
}

impl ToolHost for AFixtureHost {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        vec![self.path.clone()]
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// `tddy-acp-stub` in the target directory cargo builds into: `CARGO_TARGET_DIR` when set (a
/// relative one read from the workspace root), else the workspace's `target/`.
fn the_acp_stub() -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(|dir| workspace.join(dir))
        .unwrap_or_else(|| workspace.join("target"));
    let stub = target.join("debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

/// A `<tddyhome>` with an empty `tools/`, a session directory, a repo, and a `bin/` of wrappers:
/// `bunx`, `codex-acp` and `opencode` record their argv to `<name>.argv` and run the stub, scripted
/// to answer [`ANSWER`]; `codex` and `tddy-tools` do nothing.
struct AWorkspace {
    dir: tempfile::TempDir,
}

fn a_workspace() -> AWorkspace {
    let workspace = AWorkspace {
        dir: tempfile::tempdir().expect("workspace"),
    };
    for sub in ["tools", "session", "repo", "bin"] {
        std::fs::create_dir(workspace.dir.path().join(sub)).expect("workspace dir");
    }
    let scenario = workspace.dir.path().join("scenario.json");
    std::fs::write(
        &scenario,
        serde_json::json!({ "responses": [{ "chunks": [ANSWER] }] }).to_string(),
    )
    .expect("write scenario");
    for agent in ["bunx", "codex-acp", "opencode"] {
        workspace.write_bin(
            agent,
            &format!(
                r#"#!/bin/sh
for a in "$@"; do printf '%s\n' "$a"; done > '{argv}'
TDDY_ACP_SCENARIO='{scenario}' exec '{stub}'
"#,
                argv = workspace.record(agent),
                scenario = scenario.display(),
                stub = the_acp_stub().display(),
            ),
        );
    }
    for quiet in ["codex", "tddy-tools"] {
        workspace.write_bin(quiet, "#!/bin/sh\n");
    }
    workspace
}

impl AWorkspace {
    fn write_bin(&self, name: &str, script: &str) {
        let path = self.bin(name);
        std::fs::write(&path, script).expect("write binary");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    fn bin(&self, name: &str) -> PathBuf {
        self.dir.path().join("bin").join(name)
    }

    fn record(&self, agent: &str) -> String {
        self.dir
            .path()
            .join(format!("{agent}.argv"))
            .display()
            .to_string()
    }

    fn tools_dir(&self) -> PathBuf {
        self.dir.path().join("tools")
    }

    fn repo(&self) -> PathBuf {
        self.dir.path().join("repo")
    }

    /// No `TDDY_CLAUDE_AGENT_ACP_CLI` and no `TDDY_CODEX_ACP_CLI`: `claude-acp` runs through `bunx`
    /// on the `PATH`, `codex-acp` is found beside `codex`.
    fn host(&self) -> Arc<dyn ToolHost> {
        let named = |name: &str| self.bin(name).display().to_string();
        Arc::new(AFixtureHost {
            path: self.dir.path().join("bin"),
            env: BTreeMap::from([
                ("TDDY_CODEX_CLI".to_string(), named("codex")),
                ("TDDY_OPENCODE_CLI".to_string(), named("opencode")),
                ("TDDY_TOOLS_BINARY".to_string(), named("tddy-tools")),
            ]),
        })
    }

    fn argv(&self, agent: &str) -> Vec<String> {
        std::fs::read_to_string(self.record(agent))
            .unwrap_or_else(|e| panic!("the {agent} wrapper ran: {e}"))
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn request(&self, session_id: &str) -> InvokeRequest {
        InvokeRequest {
            prompt: PROMPT.to_string(),
            system_prompt: None,
            system_prompt_path: None,
            goal_id: GoalId::new("plan"),
            submit_key: GoalId::new("plan"),
            hints: the_plan_goal(),
            model: Some(STUB_MODEL.to_string()),
            session: Some(SessionMode::Fresh(session_id.to_string())),
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

/// One turn of `backend`, its answer.
async fn one_turn(backend: &dyn CodingBackend, request: InvokeRequest) -> String {
    let name = backend.name().to_string();
    backend
        .invoke(request)
        .await
        .unwrap_or_else(|e| panic!("the {name} turn succeeds: {e}"))
        .output
}

#[tokio::test]
async fn tddy_coder_agent_claude_acp_codex_acp_and_opencode_launch_from_specs() {
    // Given
    let workspace = a_workspace();
    let claude_acp = claude_acp_backend_from_spec(&workspace.tools_dir(), workspace.host())
        .expect("the claude-acp backend renders from its spec");
    let codex_acp = codex_acp_backend_from_spec(&workspace.tools_dir(), workspace.host())
        .expect("the codex-acp backend renders from its spec");
    let opencode = opencode_acp_backend_from_spec(&workspace.tools_dir(), workspace.host())
        .expect("the opencode backend renders from its spec");

    // When
    let answers = (
        one_turn(&claude_acp, workspace.request("tddy-claude-acp")).await,
        one_turn(&codex_acp, workspace.request("tddy-codex-acp")).await,
        one_turn(&opencode, workspace.request("tddy-opencode")).await,
    );

    // Then — `bunx claude-agent-acp`, `codex-acp` beside `codex` with no route args, `opencode acp`
    assert_eq!(
        (
            answers,
            workspace.argv("bunx"),
            workspace.argv("codex-acp"),
            workspace.argv("opencode"),
        ),
        (
            (ANSWER.to_string(), ANSWER.to_string(), ANSWER.to_string()),
            vec!["claude-agent-acp".to_string()],
            Vec::<String>::new(),
            vec!["acp".to_string()],
        )
    );
}
