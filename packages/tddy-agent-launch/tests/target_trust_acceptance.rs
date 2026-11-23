//! Acceptance: a Claude start on a target meets neither the folder-trust nor the project-MCP
//! dialog, because the launch records both answers in the target's `~/.claude.json` first.
//!
//! The port is the real `SshShell` behind a stand-in `ssh` that runs the remote command on this
//! machine; a temp directory is the target's filesystem, and the stand-in's login gives it a
//! `HOME` of its own. The agent is a stub that stays up. Nothing opens a connection.
//!
//! The `acme` agent is Claude-shaped: it takes `--mcp-config` in Claude's format and its hooks
//! are Claude's per-session `--settings` file. The duplicated stand-in fixtures belong to the
//! `target-test-kit` node (docs/dev/todo/2026-11-16-remote-shell-test-fixtures-are-duplicated.md).
//!
//! The keys follow Claude Code 2.1.311's per-project record in `~/.claude.json`:
//! `hasTrustDialogAccepted`, and `disabledMcpjsonServers` (a listed server is "rejected").

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tddy_testing_commons::remote_target::write_executable;

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_actions::tool::{ResolvedTool, TemplateParams, ToolDef};
use tddy_agent_launch::remote_agent_launch::{launch_on_target, RemoteLaunch};
use tddy_agent_launch::spec_launch::{HostAgent, HostPtySession};
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_coding_agent_specs::CodingAgentSpec;
use tddy_remote_shell::SshShell;

const TARGET_ALIAS: &str = "buildbox";
const SESSION_ID: &str = "019d2c7e-51aa-7c40-8f0e-2b1d6a9c3e11";
const SESSION_TYPE: &str = "acme-cli";
const MODEL: &str = "acme-1";
const STARTUP_WINDOW: Duration = Duration::from_millis(400);
/// How long the agent may take to record its pid on the target.
const START_TIMEOUT: Duration = Duration::from_secs(60);

/// A machine reached over ssh, with a login of its own: a directory standing for its filesystem,
/// holding a clone of a repository with the session's worktree cut from it, an agent on its
/// login `PATH`, and the stand-in `ssh` that reaches it.
struct ATarget {
    root: PathBuf,
    _dir: tempfile::TempDir,
}

fn a_target_holding_a_clone_with_a_worktree() -> ATarget {
    let dir = tempfile::tempdir().expect("a directory standing for the target");
    let root = std::fs::canonicalize(dir.path()).expect("the target's directory resolves");
    let target = ATarget { root, _dir: dir };
    for path in [
        target.root.join("bin"),
        target.home(),
        target.agents_dir(),
        target.tmpdir(),
        target.worktree(),
    ] {
        std::fs::create_dir_all(path).expect("a directory of the target");
    }
    write_executable(
        &target.root.join("bin/ssh"),
        r#"#!/bin/sh
for command; do :; done
SHELL="$(dirname "$0")/login-shell"
export SHELL
exec 3<&0
/bin/sh -c "$command" <&3 3<&- &
remote=$!
trap 'kill -HUP "$remote" 2>/dev/null' HUP TERM
wait "$remote"
exit $?
"#,
    );
    write_executable(
        &target.root.join("bin/login-shell"),
        &format!(
            "#!/bin/sh\n[ \"$1\" = -lc ] || exit 64\n\
             HOME='{}'\nPATH='{}:/usr/bin:/bin'\nTMPDIR='{}'\nexport HOME PATH TMPDIR\n\
             exec /bin/sh -c \"$2\"\n",
            target.home().display(),
            target.agents_dir().display(),
            target.tmpdir().display()
        ),
    );
    write_executable(
        &target.agents_dir().join("acme"),
        "#!/bin/sh\nexec /bin/cat\n",
    );
    target
}

impl ATarget {
    fn shell(&self) -> Arc<SshShell> {
        Arc::new(SshShell::through(self.root.join("bin/ssh"), TARGET_ALIAS))
    }

    fn home(&self) -> PathBuf {
        self.root.join("home/dev")
    }

    fn agents_dir(&self) -> PathBuf {
        self.root.join("opt/agents")
    }

    fn tmpdir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    /// The clone the session's worktree was cut from: where `~/.tddy/repos/<repo>` puts it.
    fn clone_root(&self) -> PathBuf {
        self.home().join(".tddy/repos/app")
    }

    fn worktree(&self) -> PathBuf {
        self.clone_root().join(".worktrees").join(SESSION_ID)
    }

    fn claude_json(&self) -> PathBuf {
        self.home().join(".claude.json")
    }

    /// The repository's project MCP servers, as its `.mcp.json` declares them.
    fn declares_mcp_servers(&self, names: &[&str]) {
        let servers: serde_json::Map<String, Value> = names
            .iter()
            .map(|name| {
                (
                    name.to_string(),
                    json!({ "command": format!("{name}-mcp") }),
                )
            })
            .collect();
        std::fs::write(
            self.worktree().join(".mcp.json"),
            serde_json::to_vec(&json!({ "mcpServers": servers })).expect("an .mcp.json encodes"),
        )
        .expect("the repository's .mcp.json is written");
    }

    /// A `~/.claude.json` the target already holds.
    fn holds_a_claude_json(&self, document: &Value) {
        std::fs::write(
            self.claude_json(),
            serde_json::to_vec_pretty(document).expect("a claude.json encodes"),
        )
        .expect("the target's claude.json is written");
    }

    /// The target's `~/.claude.json` now.
    fn claude_json_now(&self) -> Value {
        let bytes = std::fs::read(self.claude_json())
            .unwrap_or_else(|e| panic!("the start left no ~/.claude.json on the target: {e}"));
        serde_json::from_slice(&bytes).expect("the target's ~/.claude.json is JSON")
    }
}

/// The `acme` agent as this host resolves it, at `program`: Claude-shaped.
fn the_acme_agent(program: &Path) -> HostAgent {
    let def: ToolDef = serde_yaml::from_str(&format!(
        r#"schema_version: 1
id: acme
summary: a coding agent
binary:
  command: acme
deps: []
run:
  transport: pty
  argv:
    - "{{binary}}"
    - {{ if: model, then: ["--model", "{{model}}"] }}
extensions:
  coding_agent:
    mcp: {{ kind: file, flag: --mcp-config, format: claude-mcp-json }}
    tool_allow: {{ flag: --allowedTools }}
    tool_deny: {{ flag: --disallowedTools }}
    permission_prompt: {{ flag: --permission-prompt-tool }}
    instructions: {{ files: [CLAUDE.md] }}
    hooks: {{ adapter: claude-settings-local }}
    models: {{ default: {MODEL}, curated: [{MODEL}] }}
"#
    ))
    .expect("the tool file deserializes");
    HostAgent {
        spec: CodingAgentSpec::of(&def).expect("a valid coding agent"),
        tool: ResolvedTool {
            id: "acme".to_string(),
            command: "acme".to_string(),
            binary: program.to_path_buf(),
            reads: Vec::new(),
            network: Default::default(),
        },
        deps: Vec::new(),
        env_base: tddy_coding_agent_specs::AgentEnvBase::builtin(),
    }
}

/// A daemon about to start the session's agent on a target: its terminals and its session dir.
struct ADaemon {
    manager: Arc<CliSessionManager>,
    session_dir: PathBuf,
    _session_dir: tempfile::TempDir,
}

fn a_daemon() -> ADaemon {
    let dir = tempfile::tempdir().expect("a session directory");
    ADaemon {
        manager: Arc::new(CliSessionManager::new()),
        session_dir: std::fs::canonicalize(dir.path()).expect("the session directory resolves"),
        _session_dir: dir,
    }
}

impl ADaemon {
    /// Start the agent on `target` in its session worktree, the clone root named.
    async fn starts_an_agent_on(&self, target: &ATarget) {
        let clone_root = target.clone_root();
        launch_on_target(
            &the_acme_agent(&target.agents_dir().join("acme")),
            &TemplateParams::new().with("model", MODEL),
            &RenderedSlots::default(),
            RemoteLaunch {
                shell: target.shell(),
                worktree: &target.worktree(),
                session_dir: &self.session_dir,
                pointer: None,
                startup_window: STARTUP_WINDOW,
                start_timeout: START_TIMEOUT,
                clone_root: Some(&clone_root),
                tmux: false,
            },
            HostPtySession {
                manager: &self.manager,
                session_id: SESSION_ID,
                session_type: SESSION_TYPE,
                model: MODEL,
                env: Vec::new(),
                os_user: None,
            },
            None,
        )
        .await
        .expect("the agent is started on the target, which holds it");
    }
}

#[tokio::test]
async fn a_claude_start_on_a_target_trusts_its_clone_root() {
    // Given a target whose repository declares two project MCP servers, and no ~/.claude.json
    let target = a_target_holding_a_clone_with_a_worktree();
    target.declares_mcp_servers(&["linear", "github"]);

    // When a Claude agent is started there
    a_daemon().starts_an_agent_on(&target).await;
    let claude_json = target.claude_json_now();
    let record = &claude_json["projects"][target.clone_root().display().to_string()];
    let mut rejected: Vec<String> =
        serde_json::from_value(record["disabledMcpjsonServers"].clone())
            .expect("the rejected servers are a list of names");
    rejected.sort();

    // Then the clone root is trusted, and every server the repository declares is rejected
    assert_eq!(
        (record["hasTrustDialogAccepted"].clone(), rejected),
        (
            json!(true),
            vec!["github".to_string(), "linear".to_string()]
        )
    );

    // And the file is the owner's alone: it sits beside the account's credentials
    let mode = std::fs::metadata(target.claude_json())
        .expect("the file exists")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

#[tokio::test]
async fn pre_trust_keeps_the_rest_of_claude_json() {
    // Given a target whose ~/.claude.json holds other settings, another project's record and a
    // record of the clone root of its own
    let target = a_target_holding_a_clone_with_a_worktree();
    target.declares_mcp_servers(&["linear"]);
    let root = target.clone_root().display().to_string();
    target.holds_a_claude_json(&json!({
        "numStartups": 7,
        "theme": "dark",
        "projects": {
            "/home/dev/other": { "allowedTools": ["Bash"], "hasTrustDialogAccepted": false },
            root.clone(): { "lastCost": 1.5, "allowedTools": [] },
        },
    }));

    // When a Claude agent is started there
    a_daemon().starts_an_agent_on(&target).await;

    // Then the clone root is trusted and its MCP answer recorded, and nothing else changed
    assert_eq!(
        target.claude_json_now(),
        json!({
            "numStartups": 7,
            "theme": "dark",
            "projects": {
                "/home/dev/other": { "allowedTools": ["Bash"], "hasTrustDialogAccepted": false },
                root: {
                    "lastCost": 1.5,
                    "allowedTools": [],
                    "hasTrustDialogAccepted": true,
                    "disabledMcpjsonServers": ["linear"],
                },
            },
        })
    );
}

#[tokio::test]
async fn a_second_start_does_not_rewrite_claude_json() {
    // Given a target whose first start wrote its ~/.claude.json, which is then aged by a day
    let target = a_target_holding_a_clone_with_a_worktree();
    target.declares_mcp_servers(&["linear"]);
    a_daemon().starts_an_agent_on(&target).await;
    let written = target.claude_json_now();
    let a_day_ago = SystemTime::now() - Duration::from_secs(24 * 60 * 60);
    std::fs::File::options()
        .write(true)
        .open(target.claude_json())
        .expect("the first start left a ~/.claude.json")
        .set_modified(a_day_ago)
        .expect("the file's mtime is set");

    // When a second session's agent is started on the same clone
    a_daemon().starts_an_agent_on(&target).await;
    let modified = std::fs::metadata(target.claude_json())
        .expect("the file exists")
        .modified()
        .expect("the file has an mtime");

    // Then the file was not written again: it holds the same document, with its old mtime
    assert_eq!((target.claude_json_now(), modified), (written, a_day_ago));
}
