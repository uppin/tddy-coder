//! OpenCode as a tool-session agent: `opencode acp` driven over the Agent Client Protocol, the same
//! way [`super::CodexAcpBackend`] drives `codex-acp` and [`super::ClaudeAcpBackend`] drives
//! `claude-agent-acp`, on the shared [`AcpDriver`](super::acp_driver::AcpDriver).
//!
//! # Provider routes
//!
//! On a provider route the agent reads its provider from the configuration file `OPENCODE_CONFIG`
//! names, and that file reads the account's key from the environment. The `opencode-route-file`
//! hooks adapter ([`crate::adapters::OpenCodeRouteFile`]) writes the route's files owner-only into a
//! directory and names the first in `OPENCODE_CONFIG`; the driver holds them, so each file lives as
//! long as the agent child and is removed when the driver ends.
//!
//! The route's arguments (`--model <provider>/<model>`) are not passed to `opencode acp`. The
//! generated configuration already selects that model on the provider it declares. The flag could
//! not be checked against a real `opencode acp` here: the `opencode` on the development machine is
//! a wrapper that exits with status 127 when no OpenCode is on `PATH`, so `opencode acp --help` never
//! ran. Passing an argument the agent may not take would fail the whole launch.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::acp_driver::{self, AcpAgentCommand, AcpDriver, AllowOnce, DriverSlot};
use super::{BackendModels, InvokeRequest, InvokeResponse};
use crate::adapters::{AcpHooksAdapter, AcpHooksLaunch, OpenCodeRouteFile};
use crate::error::BackendError;
use crate::provider_route::HarnessRoute;

/// The message for an `opencode` that cannot be found: where to get one.
fn missing_opencode(error: BackendError, program: &Path) -> BackendError {
    match error {
        BackendError::BinaryNotFound(_) => {
            let p = program.display();
            BackendError::BinaryNotFound(format!(
                "{p} — install OpenCode on PATH, or set TDDY_OPENCODE_CLI / --opencode-cli-path / coder-config `opencode_cli_path`"
            ))
        }
        other => other,
    }
}

/// `opencode acp` behind [`CodingBackend`](super::CodingBackend).
///
/// The driver starts the agent on the first request rather than at construction, so a route set by
/// [`OpenCodeAcpBackend::with_provider_route`] is known before the agent exists.
pub struct OpenCodeAcpBackend {
    /// The command the agent is spawned with, as its spec renders it.
    command: AcpAgentCommand,
    /// The hooks adapter the driver runs with, by the name the spec gives (`opencode-route-file`).
    hooks: Arc<dyn AcpHooksAdapter>,
    /// The resolved deps the hooks adapter reads; none for OpenCode.
    deps: BTreeMap<String, PathBuf>,
    /// The provider route the agent runs on (`with_provider_route`); `None` for the person's own setup.
    provider_route: Option<HarnessRoute>,
    driver: DriverSlot,
}

impl std::fmt::Debug for OpenCodeAcpBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenCodeAcpBackend")
            .field("agent_path", &self.command.program)
            .finish()
    }
}

impl OpenCodeAcpBackend {
    /// Default executable on `PATH`.
    pub const DEFAULT_CLI_BINARY: &'static str = "opencode";

    #[must_use]
    pub fn new() -> Self {
        Self::with_agent_path(PathBuf::from(Self::DEFAULT_CLI_BINARY))
    }

    /// Run the given binary (`opencode`, or a stub agent in tests) as `<binary> acp`.
    #[must_use]
    pub fn with_agent_path(path: PathBuf) -> Self {
        Self::with_launch(
            AcpAgentCommand {
                program: path,
                args: vec!["acp".to_string()],
                env: Vec::new(),
                cwd: None,
            },
            Arc::new(OpenCodeRouteFile),
            BTreeMap::new(),
        )
    }

    /// The backend that spawns `command` (rendered from `opencode.yaml`'s `coding_agent.acp`), its
    /// driver run with the hooks adapter the spec names and the resolved `deps` that adapter reads.
    #[must_use]
    pub fn with_launch(
        command: AcpAgentCommand,
        hooks: Arc<dyn AcpHooksAdapter>,
        deps: BTreeMap<String, PathBuf>,
    ) -> Self {
        Self {
            command,
            hooks,
            deps,
            provider_route: None,
            driver: Mutex::new(None),
        }
    }

    /// Run the agent on `route`. A driver started before the route was set runs the person's own
    /// setup: it is dropped here, which ends its child, and the next request starts one on the route.
    pub(super) fn set_provider_route(&mut self, route: HarnessRoute) {
        self.provider_route = Some(route);
        self.driver = Mutex::new(None);
    }

    /// The driver, started on first use. On a route the child gets the route's environment; the
    /// hooks adapter then names the route's configuration file and holds it.
    fn driver(&self) -> Result<Arc<AcpDriver>, BackendError> {
        acp_driver::started(&self.driver, || {
            let mut command = self.command.clone();
            if let Some(route) = &self.provider_route {
                command.env.extend(acp_driver::route_env(route));
            }
            let prepared = self.hooks.prepare(AcpHooksLaunch {
                command: &mut command,
                route: self.provider_route.as_ref(),
                deps: &self.deps,
            })?;
            let program = command.program.clone();
            AcpDriver::spawn(command, Arc::new(AllowOnce), prepared.hooks)
                .map_err(|e| missing_opencode(e, &program))
        })
    }
}

impl Default for OpenCodeAcpBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl super::CodingBackend for OpenCodeAcpBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        let prompt = super::codex::merge_codex_prompt(&request)?;
        let driver = self.driver()?;
        let open = acp_driver::open_for_request(&request, self.provider_route.is_some())?;
        let session = driver.open_session(open).await?;
        acp_driver::answer(&driver, session, prompt, request.progress_sink).await
    }

    fn name(&self) -> &str {
        "opencode"
    }

    async fn list_models(&self) -> Result<BackendModels, BackendError> {
        self.driver()?.list_models().await
    }
}
