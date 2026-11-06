//! ACP backend for Claude: `claude-agent-acp` over stdio, on the shared [`AcpDriver`].
//!
//! Spawns the agent (or `tddy-acp-stub` in tests) and speaks JSON-RPC 2.0 over its stdin and stdout
//! through the driver. What is Claude's own is its provider route, which reaches the agent as its
//! environment; the session, the prompt, the cancel and the permission answers are the driver's.

use super::acp_driver::{self, AcpAgentCommand, AcpDriver, AllowOnce, DriverSlot, NoHooks};
use super::{InvokeRequest, InvokeResponse};
use crate::error::BackendError;
use crate::provider_route::HarnessRoute;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Backend that invokes an ACP agent subprocess via the Agent Client Protocol.
///
/// The driver spawns the agent when the first request needs it, so it starts on first use rather
/// than at construction: a route set by [`ClaudeAcpBackend::with_provider_route`] has to be known
/// before the agent exists.
pub struct ClaudeAcpBackend {
    /// The command the agent is spawned with; a route's environment is added to it when the driver
    /// starts.
    command: AcpAgentCommand,
    /// The provider route the agent runs on (`with_provider_route`); `None` for the vendor.
    provider_route: Option<HarnessRoute>,
    driver: DriverSlot,
}

impl std::fmt::Debug for ClaudeAcpBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaudeAcpBackend")
            .field("agent_path", &self.command.program)
            .field("agent_args", &self.command.args)
            .finish()
    }
}

// The workflow backend is built from `claude-acp.yaml` (`tddy_coder::acp_from_spec`) with
// [`ClaudeAcpBackend::with_command`]. This default still answers the model listing of
// `model_catalog` until that reads the spec too.
// TODO(acp-agents): retire this default once `model_catalog` lists models from the spec.
impl Default for ClaudeAcpBackend {
    fn default() -> Self {
        Self::with_command(AcpAgentCommand {
            program: PathBuf::from("bunx"),
            args: vec!["claude-agent-acp".to_string()],
            env: Vec::new(),
            cwd: None,
        })
    }
}

impl ClaudeAcpBackend {
    /// Create a new backend using the default agent (bunx claude-agent-acp).
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a backend that spawns the given binary (for tests with tddy-acp-stub).
    #[must_use]
    pub fn with_agent_path(path: PathBuf) -> Self {
        Self::with_command(AcpAgentCommand {
            program: path,
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
        })
    }

    /// Create a backend that spawns `command`, as `claude-acp.yaml`'s `coding_agent.acp` renders it.
    #[must_use]
    pub fn with_command(command: AcpAgentCommand) -> Self {
        Self {
            command,
            provider_route: None,
            driver: Mutex::new(None),
        }
    }

    /// Run the agent on `route`. A driver started before the route was set runs the vendor: it is
    /// dropped here, which ends its child, and the next request starts one on the route.
    pub(super) fn set_provider_route(&mut self, route: HarnessRoute) {
        self.provider_route = Some(route);
        self.driver = Mutex::new(None);
    }

    /// The driver, started on first use. The route's environment is given to the agent child.
    fn driver(&self) -> Result<Arc<AcpDriver>, BackendError> {
        acp_driver::started(&self.driver, || {
            let mut command = self.command.clone();
            if let Some(route) = &self.provider_route {
                command.env.extend(acp_driver::route_env(route));
            }
            AcpDriver::spawn(command, Arc::new(AllowOnce), Arc::new(NoHooks))
        })
    }
}

#[async_trait::async_trait]
impl super::CodingBackend for ClaudeAcpBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        let driver = self.driver()?;
        let open = acp_driver::open_for_request(&request, self.provider_route.is_some())?;
        let session = driver.open_session(open).await?;
        acp_driver::answer(&driver, session, request.prompt, request.progress_sink).await
    }

    fn name(&self) -> &str {
        "claude-acp"
    }

    async fn list_models(&self) -> Result<super::BackendModels, BackendError> {
        self.driver()?.list_models().await
    }
}
