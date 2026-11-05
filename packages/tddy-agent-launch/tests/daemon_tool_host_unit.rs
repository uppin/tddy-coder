//! Unit: the daemon's host finds a coding agent's binary with no daemon.yaml input, and refuses one
//! it cannot find naming the agent and every place it searched (`#agent-tools 19/19`).
//!
//! Before this node the daemon's host found `claude` through `resolve_claude_binary_path`, whose
//! last step returned daemon.yaml's `claude_cli.binary_path` (default `claude`) as-is, and read the
//! process's own `HOME` and `PATH` whatever host it was given. [`daemon_tool_host`] takes everything
//! from the host it wraps, so these tests run on a fixture host and the real machine's `claude`
//! (`~/.local/bin/claude` on a developer's laptop) cannot answer for it.
//!
//! Changeset: `docs/dev/1-WIP/2026-11-04-agent-tools-agent-config-removal.md`.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::{ToolError, ToolHost};
use tddy_agent_launch::host_coding_agents::daemon_tool_host;
use tddy_agent_launch::spec_launch::{HostCodingAgents, SpecLaunchError};

/// A host whose environment is `HOME` and `TDDY_TOOLS_BINARY` alone, whose `PATH` is `path`, over
/// the real filesystem.
struct AFixtureHost {
    home: PathBuf,
    tddy_tools: PathBuf,
    path: Vec<PathBuf>,
}

impl ToolHost for AFixtureHost {
    fn env_var(&self, name: &str) -> Option<String> {
        match name {
            "HOME" => Some(self.home.to_string_lossy().into_owned()),
            "TDDY_TOOLS_BINARY" => Some(self.tddy_tools.to_string_lossy().into_owned()),
            _ => None,
        }
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        self.path.clone()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// A machine laid out under one temporary root: a `$HOME`, a `tddy-tools`, and whatever `PATH`
/// directories a test adds.
struct AMachine {
    root: PathBuf,
    home: PathBuf,
    tddy_tools: PathBuf,
    path: Vec<PathBuf>,
    _dir: tempfile::TempDir,
}

fn a_machine() -> AMachine {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".local/bin")).unwrap();
    let tddy_tools = an_executable(&root.join("install"), "tddy-tools");
    AMachine {
        root,
        home,
        tddy_tools,
        path: Vec::new(),
        _dir: dir,
    }
}

fn an_executable(dir: &Path, name: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let file = dir.join(name);
    std::fs::write(&file, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    file
}

impl AMachine {
    /// `claude` installed where Claude's installer puts it, `$HOME/.local/bin`.
    fn with_claude_in_local_bin(self) -> Self {
        an_executable(&self.home.join(".local/bin"), "claude");
        self
    }

    /// The `PATH` directory `relative` (under the root), holding `claude` when `holds_claude`.
    fn with_path_dir(mut self, relative: &str, holds_claude: bool) -> Self {
        let dir = self.root.join(relative);
        std::fs::create_dir_all(&dir).unwrap();
        if holds_claude {
            an_executable(&dir, "claude");
        }
        self.path.push(dir);
        self
    }

    /// The built-in coding agents on the daemon's host over this machine.
    fn coding_agents(&self) -> HostCodingAgents {
        let registry =
            tddy_coding_agent_specs::load_agent_registry(&self.root.join("tddyhome/tools"))
                .expect("the built-in agents load");
        HostCodingAgents::new(
            registry,
            daemon_tool_host(Arc::new(AFixtureHost {
                home: self.home.clone(),
                tddy_tools: self.tddy_tools.clone(),
                path: self.path.clone(),
            })),
        )
    }

    fn claude_binary(&self) -> Result<PathBuf, SpecLaunchError> {
        self.coding_agents()
            .resolve("claude", None)
            .map(|agent| agent.tool.binary)
    }
}

#[test]
fn claude_is_found_in_the_hosts_local_bin_with_no_daemon_config() {
    // Given
    let machine = a_machine().with_claude_in_local_bin();

    // When
    let binary = machine.claude_binary();

    // Then
    assert_eq!(binary, Ok(machine.home.join(".local/bin/claude")));
}

#[test]
fn a_wrapper_shim_directory_on_the_hosts_path_is_passed_over_for_the_real_claude() {
    // Given — Superset's shim directory first on PATH, the real install after it
    let machine = a_machine()
        .with_path_dir("home/.superset/bin", true)
        .with_path_dir("opt/claude/bin", true);

    // When
    let binary = machine.claude_binary();

    // Then
    assert_eq!(binary, Ok(machine.root.join("opt/claude/bin/claude")));
}

#[test]
fn a_claude_found_nowhere_is_refused_naming_its_binary_env_and_every_place_searched() {
    // Given
    let machine = a_machine().with_path_dir("usr/local/bin", false);

    // When
    let refused = machine.claude_binary();

    // Then
    assert_eq!(
        refused,
        Err(SpecLaunchError::Tool(ToolError::Unresolvable {
            tool: "claude".to_string(),
            searched: format!(
                "TDDY_CLAUDE_BINARY (unset), PATH ({}:{})",
                machine.home.join(".local/bin").display(),
                machine.root.join("usr/local/bin").display(),
            ),
        }))
    );
}
