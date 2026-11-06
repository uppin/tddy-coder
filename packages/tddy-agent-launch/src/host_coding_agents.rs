//! The coding agents this daemon launches: the agent registry loaded from the built-in specs and
//! `<tddyhome>/tools`, resolved against the daemon's own process (`#agent-tools 2/12`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_actions::tool::{ProcessToolHost, ToolHost};
use tddy_coding_agent_specs::load_agent_registry;
use tddy_daemon_kernel::config::{resolve_claude_binary_path, DaemonConfig, CLAUDE_BINARY_ENV};

use crate::spec_launch::HostCodingAgents;

/// The variable `tddy-tools.yaml` names for its binary override.
const TDDY_TOOLS_BINARY_ENV: &str = "TDDY_TOOLS_BINARY";

/// The coding agents of a daemon whose data directory is `tddy_data_dir`. A tool file under
/// `<tddyhome>/tools` that does not load refuses the daemon's start, naming the file.
pub fn host_coding_agents(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
) -> anyhow::Result<Arc<HostCodingAgents>> {
    let registry = load_agent_registry(&tddy_data_dir.join("tools"))
        .map_err(|e| anyhow::anyhow!("load the coding agent tools: {e}"))?;
    Ok(Arc::new(HostCodingAgents::new(
        registry,
        Arc::new(DaemonToolHost {
            config: config.clone(),
        }),
    )))
}

/// The daemon process as a [`ToolHost`], with `claude` and `tddy-tools` found the way the hand-built
/// launch found them: the host `claude` auto-resolution (`~/.local/bin`, then `PATH` without wrapper
/// shims) and the configured, `CARGO_BIN_EXE_` or sibling `tddy-tools` that `./install` relies on.
/// A binary those rules do not locate is left to the spec's own `PATH` search.
struct DaemonToolHost {
    config: DaemonConfig,
}

impl DaemonToolHost {
    fn located(&self, name: &str) -> Option<String> {
        let located = match name {
            CLAUDE_BINARY_ENV => resolve_claude_binary_path(&self.config),
            TDDY_TOOLS_BINARY_ENV => tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(
                self.config
                    .claude_cli
                    .as_ref()
                    .and_then(|claude_cli| claude_cli.tddy_tools_path.as_deref()),
            ),
            _ => return None,
        };
        // A bare name is not a location: it is what the search below finds.
        Path::new(&located).is_absolute().then_some(located)
    }
}

impl ToolHost for DaemonToolHost {
    fn env_var(&self, name: &str) -> Option<String> {
        ProcessToolHost.env_var(name).or_else(|| self.located(name))
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        ProcessToolHost.path_dirs()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        ProcessToolHost.is_executable_file(path)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        ProcessToolHost.canonicalize(path)
    }
}
