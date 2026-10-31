//! ACP hooks adapters: the Rust code a declared ACP agent's driver runs with, by name
//! (`#agent-tools 11/12`).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-acp-agents-from-specs.md`.
//!
//! A spec's `coding_agent.acp` section declares *how the agent is launched*. What the driver does
//! around a session — run `codex login` and retry once on an auth-like error, hold a route's
//! owner-only config file for the agent's lifetime — is runtime behaviour that cannot be YAML, so
//! the section only **names** an adapter (`hooks_adapter: codex-acp-login`). Neither
//! [`super::AdapterRegistry`] (stream parsers) nor [`super::LaunchAdapters`] (PTY output readers)
//! has this shape: an ACP hooks adapter acts on the command before it is spawned and hands the
//! driver its [`AcpAgentHooks`], so it has a registry of its own ([`AcpHooksAdapters`]).

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use crate::backend::acp_driver::{AcpAgentCommand, AcpAgentHooks};
use crate::error::BackendError;
use crate::provider_route::HarnessRoute;

pub use super::codex_acp_login::{CodexAcpLogin, CODEX_ACP_LOGIN};
pub use super::opencode_route_file::{OpenCodeRouteFile, OPENCODE_ROUTE_FILE};

/// What one ACP launch hands its hooks adapter, before the agent is spawned.
pub struct AcpHooksLaunch<'a> {
    /// The command about to be spawned. An adapter may add to its environment
    /// (`OPENCODE_CONFIG` naming a route file it wrote).
    pub command: &'a mut AcpAgentCommand,
    /// The provider route the launch runs on; `None` for the vendor's own login.
    pub route: Option<&'a HarnessRoute>,
    /// The launch's resolved deps' binaries, by tool id (`codex` → the binary `codex login` runs).
    pub deps: &'a BTreeMap<String, PathBuf>,
}

/// Brackets one `session/new` (or `session/load`) of a driver whose hooks need to know which
/// request is opening — the session directory a `codex login` writes its authorize URL to — and
/// report a failure of their own in place of the session error it followed.
#[async_trait::async_trait]
pub trait AcpOpenScope: Send + Sync {
    /// Before the open: the session directory of the request it serves.
    async fn begin_open(&self, session_dir: Option<PathBuf>);
    /// After the open: the hooks' own failure during it (a login that failed), if any.
    async fn end_open(&self) -> Option<BackendError>;
}

/// The hooks one launch's driver runs with.
pub struct PreparedAcpHooks {
    /// What the driver calls around a session.
    pub hooks: Arc<dyn AcpAgentHooks>,
    /// How a backend brackets each open for these hooks; `None` for hooks that need no bracket.
    pub opens: Option<Arc<dyn AcpOpenScope>>,
}

/// One ACP agent's runtime behaviour around the shared driver.
pub trait AcpHooksAdapter: Send + Sync + fmt::Debug {
    /// The name a spec's `coding_agent.acp.hooks_adapter` names it by.
    fn name(&self) -> &str;

    /// Prepare `launch`'s command and the hooks its driver runs with. Refused when the launch
    /// lacks what the adapter needs (a dep it names), never run without it.
    fn prepare(&self, launch: AcpHooksLaunch<'_>) -> Result<PreparedAcpHooks, BackendError>;
}

/// The ACP hooks adapters this build has, by name.
#[derive(Clone, Default)]
pub struct AcpHooksAdapters {
    adapters: BTreeMap<String, Arc<dyn AcpHooksAdapter>>,
}

impl fmt::Debug for AcpHooksAdapters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.adapters.keys()).finish()
    }
}

impl AcpHooksAdapters {
    /// Every ACP hooks adapter this build registers: [`CODEX_ACP_LOGIN`] ([`CodexAcpLogin`]) and
    /// [`OPENCODE_ROUTE_FILE`] ([`OpenCodeRouteFile`]).
    pub fn builtin() -> Self {
        // TODO(acp-agents): implement — register `CodexAcpLogin` and `OpenCodeRouteFile` under
        // their `name()`.
        todo!("acp-agents: AcpHooksAdapters::builtin")
    }

    /// The adapter registered as `name`.
    pub fn get(&self, name: &str) -> Option<Arc<dyn AcpHooksAdapter>> {
        self.adapters.get(name).cloned()
    }

    /// Whether an adapter is registered as `name`.
    pub fn contains(&self, name: &str) -> bool {
        self.adapters.contains_key(name)
    }

    /// The registered names, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.adapters.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn the_builtin_acp_hooks_adapters_are_codex_acp_login_and_opencode_route_file() {
        // Given
        let adapters = AcpHooksAdapters::builtin();

        // When
        let names: Vec<&str> = adapters.names().collect();

        // Then
        assert_eq!(names, vec![CODEX_ACP_LOGIN, OPENCODE_ROUTE_FILE]);
    }

    #[test]
    fn each_builtin_acp_hooks_adapter_answers_to_its_name_and_an_unregistered_name_is_absent() {
        // Given
        let adapters = AcpHooksAdapters::builtin();

        // When
        let found = (
            adapters
                .get(CODEX_ACP_LOGIN)
                .map(|adapter| adapter.name().to_string()),
            adapters
                .get(OPENCODE_ROUTE_FILE)
                .map(|adapter| adapter.name().to_string()),
            adapters.contains("claude-acp-login"),
        );

        // Then
        assert_eq!(
            found,
            (
                Some(CODEX_ACP_LOGIN.to_string()),
                Some(OPENCODE_ROUTE_FILE.to_string()),
                false,
            )
        );
    }
}
