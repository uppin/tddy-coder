//! Unit tests: the agent a session on a target runs (`#tab-hardening` 4/4) — read from its spec with
//! its bare command, never resolved on this host while its `tddy-tools` still is
//! ([`resolve_session_agent_for_target`]) — and the question put to the target before anything is
//! cut ([`require_the_agent_on_the_target`]).

// Test-only: included via `mod` under `#[cfg(test)]`, so the filesystem-port guard reads
// the whole file as test code and the real-disk fixtures below are the tests' fixtures, not
// production I/O.
#![cfg(test)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::{ResolvedTool, ToolHost};
use tddy_remote_shell::{ExecOutput, InMemoryShell};
use tddy_rpc::{Code, Status};

use super::{
    require_the_agent_on_the_target, require_tmux_on_the_target, resolve_session_agent_for_target,
};
use crate::harness_from_spec::{cli_session_type, CliSessionAgent};
use crate::spec_launch::{HostAgent, HostCodingAgents};

/// What the probe of `claude` runs on the target, from its login's own directory.
const PROBE_OF_CLAUDE: &str =
    r#"cd '.' && exec "${SHELL:-/bin/sh}" -lc 'command -v '"'"'claude'"'"''"#;

/// A daemon host whose `PATH` is one directory holding exactly the commands it was given, and whose
/// `$HOME` is an empty directory: nothing of the real machine reaches resolution.
struct ADaemonHost {
    bin: PathBuf,
    home: PathBuf,
    _dir: tempfile::TempDir,
}

/// A daemon host holding `commands` — each an executable file named for it on its `PATH` — and
/// nothing else.
fn a_daemon_host_holding(commands: &[&str]) -> ADaemonHost {
    let dir = tempfile::tempdir().expect("a directory for the host");
    let root = std::fs::canonicalize(dir.path()).expect("the host's directory resolves");
    let bin = root.join("bin");
    let home = root.join("home");
    std::fs::create_dir_all(&bin).expect("the host's PATH directory");
    std::fs::create_dir_all(&home).expect("the host's home");
    for command in commands {
        let path = bin.join(command);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").expect("a command on the host");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("an executable command");
    }
    ADaemonHost {
        bin,
        home,
        _dir: dir,
    }
}

impl ADaemonHost {
    /// The built-in coding agents, installed on this host.
    fn coding_agents(&self) -> Arc<HostCodingAgents> {
        let no_overrides = tempfile::tempdir().expect("an empty tools dir");
        Arc::new(HostCodingAgents::new(
            tddy_coding_agent_specs::load_agent_registry(no_overrides.path())
                .expect("the built-in coding agents load"),
            Arc::new(TheHostsPath {
                bin: self.bin.clone(),
                home: self.home.clone(),
            }),
        ))
    }

    /// Where this host holds `command`.
    fn path_of(&self, command: &str) -> PathBuf {
        self.bin.join(command)
    }

    /// The agent a `session_type` session on a target runs, as this host settles it.
    fn target_agent_of(&self, session_type: &str) -> Result<CliSessionAgent, Status> {
        let agents = self.coding_agents();
        let kind = cli_session_type(&agents, session_type)
            .expect("the session type is a built-in agent's");
        resolve_session_agent_for_target(&agents, kind)
    }
}

/// The resolution host of an [`ADaemonHost`].
struct TheHostsPath {
    bin: PathBuf,
    home: PathBuf,
}

impl ToolHost for TheHostsPath {
    fn env_var(&self, name: &str) -> Option<String> {
        (name == "HOME").then(|| self.home.to_string_lossy().into_owned())
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        vec![self.bin.clone()]
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// `(spec id, command, binary)` of a target agent's own tool: what the launch on the target runs.
fn what_runs(agent: &CliSessionAgent) -> (String, String, PathBuf) {
    (
        agent.agent.spec.id.clone(),
        agent.agent.tool.command.clone(),
        agent.agent.tool.binary.clone(),
    )
}

/// The code and message of a refusal.
fn refusal(status: Status) -> (Code, String) {
    (status.code(), status.message().to_string())
}

/// The `claude-cli` agent of a session on a target, as [`resolve_session_agent_for_target`] hands
/// it over — the built-in spec, its bare command as its binary — built here, so the probe's tests
/// do not depend on the resolution.
fn claude_on_a_target() -> CliSessionAgent {
    let agents = a_daemon_host_holding(&["tddy-tools"]).coding_agents();
    let kind = cli_session_type(&agents, "claude-cli").expect("claude-cli is a built-in agent's");
    CliSessionAgent {
        kind,
        agent: HostAgent {
            spec: tddy_coding_agent_specs::embedded_spec("claude").expect("claude is built in"),
            tool: ResolvedTool {
                id: "claude".to_string(),
                command: "claude".to_string(),
                binary: PathBuf::from("claude"),
                reads: Vec::new(),
                network: Default::default(),
            },
            deps: Vec::new(),
            env_base: tddy_coding_agent_specs::AgentEnvBase::builtin(),
        },
        agents,
    }
}

// ── resolve_session_agent_for_target ─────────────────────────────────────────────────────────

#[test]
fn a_target_agent_is_its_specs_bare_command_on_a_host_without_the_agent() {
    // Given a daemon host with `tddy-tools` and no Claude Code
    let host = a_daemon_host_holding(&["tddy-tools"]);

    // When
    let agent = host
        .target_agent_of("claude-cli")
        .expect("a target's agent needs no copy on the daemon's host");

    // Then it is the claude spec, its binary the bare command the target's login PATH resolves,
    // for the session type it was asked for
    assert_eq!(
        (what_runs(&agent), agent.kind.name),
        (
            (
                "claude".to_string(),
                "claude".to_string(),
                PathBuf::from("claude")
            ),
            "claude-cli"
        )
    );
}

#[test]
fn a_target_agent_is_never_this_hosts_copy_of_the_agent() {
    // Given a daemon host that has Claude Code installed itself
    let host = a_daemon_host_holding(&["claude", "tddy-tools"]);
    assert_eq!(
        host.coding_agents()
            .resolve("claude", None)
            .map(|agent| agent.tool.binary)
            .ok(),
        Some(host.path_of("claude")),
        "the daemon's host must resolve its own Claude Code for this test"
    );

    // When
    let agent = host
        .target_agent_of("claude-cli")
        .expect("a target's agent is settled");

    // Then the target runs its own, by its bare command — never the host's path
    assert_eq!(agent.agent.tool.binary, PathBuf::from("claude"));
}

#[test]
fn a_target_agents_tddy_tools_is_the_one_on_this_host() {
    // Given a daemon host with `tddy-tools` and no Claude Code
    let host = a_daemon_host_holding(&["tddy-tools"]);

    // When
    let agent = host
        .target_agent_of("claude-cli")
        .expect("a target's agent is settled");

    // Then the tools the daemon runs itself are this host's
    assert_eq!(
        agent
            .agent
            .dep("tddy-tools")
            .map(|tool| tool.binary.clone()),
        Some(host.path_of("tddy-tools"))
    );
}

#[test]
fn a_target_agent_on_a_host_without_tddy_tools_is_refused_naming_it() {
    // Given a daemon host with neither the agent nor `tddy-tools`
    let host = a_daemon_host_holding(&[]);

    // When
    let refused = host
        .target_agent_of("claude-cli")
        .map(|agent| what_runs(&agent));

    // Then the missing dep is named — the agent itself is not looked for here
    assert_eq!(
        refused.map_err(|status| {
            let (code, message) = refusal(status);
            (code, message.starts_with("tool tddy-tools is unavailable"))
        }),
        Err((Code::FailedPrecondition, true))
    );
}

#[test]
fn a_target_cursor_agent_runs_the_command_its_spec_names_not_its_id() {
    // Given a daemon host with `tddy-tools` and no Cursor agent
    let host = a_daemon_host_holding(&["tddy-tools"]);

    // When
    let agent = host
        .target_agent_of("cursor-cli")
        .expect("a target's agent is settled");

    // Then the bare command is the spec's `binary.command`, `agent`
    assert_eq!(
        what_runs(&agent),
        (
            "cursor".to_string(),
            "agent".to_string(),
            PathBuf::from("agent")
        )
    );
}

// ── require_the_agent_on_the_target ──────────────────────────────────────────────────────────

#[tokio::test]
async fn a_target_whose_login_shell_finds_the_agent_admits_it_with_one_probe() {
    // Given a target whose login shell finds Claude Code
    let target = InMemoryShell::new("buildbox");
    target.answers(
        PROBE_OF_CLAUDE,
        ExecOutput::succeeded("/home/dev/.local/bin/claude\n"),
    );

    // When
    let admitted = require_the_agent_on_the_target(&target, &claude_on_a_target()).await;

    // Then exactly the one probe ran, from the login's own directory
    assert_eq!(
        (admitted.map_err(refusal), target.commands_run()),
        (Ok(()), vec![PROBE_OF_CLAUDE.to_string()])
    );
}

#[tokio::test]
async fn a_target_without_the_agent_refuses_failed_precondition_naming_the_command() {
    // Given a target whose login shell finds no Claude Code
    let target = InMemoryShell::new("buildbox");
    target.answers(PROBE_OF_CLAUDE, ExecOutput::exited(1, ""));

    // When
    let refused = require_the_agent_on_the_target(&target, &claude_on_a_target()).await;

    // Then
    assert_eq!(
        refused.map_err(|status| {
            let (code, message) = refusal(status);
            (
                code,
                message.contains("the agent `claude` is not installed on the target"),
            )
        }),
        Err((Code::FailedPrecondition, true))
    );
}

#[tokio::test]
async fn an_unreachable_target_refuses_unavailable_with_the_transports_reason() {
    // Given a target its transport cannot reach
    let target = InMemoryShell::new("buildbox");
    target.is_unreachable("Could not resolve hostname buildbox");

    // When
    let refused = require_the_agent_on_the_target(&target, &claude_on_a_target()).await;

    // Then the target, not the agent, is at fault
    assert_eq!(
        refused.map_err(|status| {
            let (code, message) = refusal(status);
            (
                code,
                message.contains("Could not resolve hostname buildbox"),
            )
        }),
        Err((Code::Unavailable, true))
    );
}

/// What the probe of `tmux` runs on the target, from its login's own directory.
const PROBE_OF_TMUX: &str = r#"cd '.' && exec "${SHELL:-/bin/sh}" -lc 'command -v '"'"'tmux'"'"''"#;

#[tokio::test]
async fn a_tmux_run_on_a_target_without_tmux_is_refused_naming_tmux() {
    // Given a target whose login shell finds no tmux
    let target = InMemoryShell::new("buildbox");
    target.answers(PROBE_OF_TMUX, ExecOutput::exited(1, ""));

    // When
    let refused = require_tmux_on_the_target(&target).await;

    // Then the start is refused as a precondition the caller can meet, naming tmux
    let refusal = refused.expect_err("a target without tmux refuses a tmux run");
    assert_eq!(
        (refusal.code(), refusal.message()),
        (
            Code::FailedPrecondition,
            "run_in_tmux needs tmux on the target: its login shell finds no `tmux` on its PATH"
        )
    );
}
