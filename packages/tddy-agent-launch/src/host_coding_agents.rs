//! The coding agents this daemon launches: the agent registry loaded from the built-in specs and
//! `<tddyhome>/tools`, resolved against the daemon's own process (`#agent-tools 2/12`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_actions::tool::{ProcessToolHost, ToolHost};
use tddy_coding_agent_specs::load_agent_registry;
use tddy_daemon_kernel::config::{is_wrapper_shim_dir, DaemonConfig};

use crate::spec_launch::HostCodingAgents;

/// The variable `tddy-tools.yaml` names for its binary override.
const TDDY_TOOLS_BINARY_ENV: &str = "TDDY_TOOLS_BINARY";

/// The coding agents of a daemon whose data directory is `tddy_data_dir`. A tool file under
/// `<tddyhome>/tools` that does not load refuses the daemon's start, naming the file.
pub fn host_coding_agents(
    _config: &DaemonConfig,
    tddy_data_dir: &Path,
) -> anyhow::Result<Arc<HostCodingAgents>> {
    let registry = load_agent_registry(&tddy_data_dir.join("tools"))
        .map_err(|e| anyhow::anyhow!("load the coding agent tools: {e}"))?;
    let agents = HostCodingAgents::try_new(
        registry,
        daemon_tool_host(Arc::new(ProcessToolHost)),
    )
    .map_err(|e| anyhow::anyhow!("load the coding agent tools: {e}"))?;
    Ok(Arc::new(agents))
}

/// The coding agents of a host that installed none: an empty registry, so every agent session is
/// refused for naming no spec rather than launched some other way (`#agent-tools 19/19`).
pub fn no_coding_agents() -> Arc<HostCodingAgents> {
    Arc::new(HostCodingAgents::new(
        tddy_actions::tool::ToolRegistry::default(),
        daemon_tool_host(Arc::new(ProcessToolHost)),
    ))
}

/// The daemon's host over `base` (`#agent-tools 19/19`), with no daemon.yaml input: the coding
/// agents' binaries are found where the daemon has always found them, but every place searched is
/// `base`'s — its environment, its `$HOME`, its `PATH` — and named in the refusal when none holds
/// the binary.
///
/// - A spec's `binary.env` (`TDDY_CLAUDE_BINARY`, `TDDY_TOOLS_BINARY`, …) set on `base` wins.
/// - `PATH` is `base`'s `$HOME/.local/bin` (where Claude's installer puts `claude`), then `base`'s
///   own `PATH` without wrapper-shim directories (`~/.superset/bin`), which cannot resolve inside a
///   jail.
/// - `tddy-tools` with no `TDDY_TOOLS_BINARY` is the sibling of the daemon binary `./install` lays
///   out, as `resolve_tddy_tools_path` finds it.
///
/// What none of these locates is refused by the spec's resolution
/// (`tool <id> is unavailable: searched <binary.env> (unset), PATH (<dirs>)`) — there is no
/// "configured value as-is" last step.
pub fn daemon_tool_host(base: Arc<dyn ToolHost>) -> Arc<dyn ToolHost> {
    Arc::new(DaemonToolHost { base })
}

/// `base` as the daemon's host: see [`daemon_tool_host`].
struct DaemonToolHost {
    base: Arc<dyn ToolHost>,
}

impl DaemonToolHost {
    /// Where `tddy-tools` sits beside the daemon, when `base` names none: an absolute location only,
    /// since a bare name is not one — it is what the `PATH` search finds.
    fn sibling_tddy_tools(&self) -> Option<String> {
        let located = tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(None);
        Path::new(&located).is_absolute().then_some(located)
    }
}

impl ToolHost for DaemonToolHost {
    fn env_var(&self, name: &str) -> Option<String> {
        self.base.env_var(name).or_else(|| match name {
            TDDY_TOOLS_BINARY_ENV => self.sibling_tddy_tools(),
            _ => None,
        })
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        let local_bin = self
            .base
            .env_var("HOME")
            .filter(|home| !home.is_empty())
            .map(|home| PathBuf::from(home).join(".local/bin"));
        local_bin
            .into_iter()
            .chain(
                self.base
                    .path_dirs()
                    .into_iter()
                    .filter(|dir| !is_wrapper_shim_dir(dir)),
            )
            .collect()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        self.base.is_executable_file(path)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        self.base.canonicalize(path)
    }
}

