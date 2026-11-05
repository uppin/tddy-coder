//! The one workflow backend built from a coding agent's spec (`#agent-tools 16/19`): a
//! [`SpecBackend`] runs an agent's turns over the transport its spec declares — the generic
//! headless driver ([`SpecHeadlessBackend`], `#agent-tools 15/19`) or ACP over the shared
//! [`AcpDriver`] ([`SpecAcpBackend`]). It replaces `AnyBackend`, whose variants named agents: here
//! the agent is data, and `tddy-coder --agent <id>` builds one for any registry agent — `oz`, and an
//! agent dropped into `<tddyhome>/tools/`, included.
//!
//! Changeset: `docs/dev/1-WIP/2026-11-04-agent-tools-spec-backend.md`.
//!
//! **Transport rule.** A spec that declares both a `coding_agent.headless` and a `coding_agent.acp`
//! section runs **headless** unless the session or the request asks for ACP
//! ([`SpecTransport::chosen`]). A spec with neither is refused naming the agent — never run as
//! another agent.
//!
//! **No hangs.** An ACP agent that never answers `initialize` / `session/new` / `session/load` is
//! refused at [`SpecAcpBackend`]'s session-open deadline, naming the agent and the deadline.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::AcpHooksAdapter;
use crate::error::BackendError;
use crate::provider_route::by_spec::RouteRefusal;
use crate::provider_route::{HarnessRoute, RoutedAgent};

use super::acp_driver::{AcpAgentCommand, DriverSlot, SESSION_OPEN_DEADLINE};
use super::spec_headless::SpecHeadlessBackend;
use super::{BackendModels, CodingBackend, InvokeRequest, InvokeResponse, SharedBackend};

/// The way a spec-built backend takes a turn without a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpecTransport {
    /// `coding_agent.headless`: one CLI run per turn, read through its stream adapter.
    Headless,
    /// `coding_agent.acp`: an Agent Client Protocol session over the agent's stdio.
    Acp,
}

impl SpecTransport {
    /// The spelling a refusal and a log line use: `headless`, `acp`.
    pub fn as_str(self) -> &'static str {
        match self {
            SpecTransport::Headless => "headless",
            SpecTransport::Acp => "acp",
        }
    }

    /// The transport agent `agent` runs on, from which sections its spec declares and what the
    /// session or request `asked` for:
    ///
    /// - asked for one the spec declares → that one;
    /// - asked for nothing, both declared → [`SpecTransport::Headless`];
    /// - asked for nothing, one declared → that one;
    /// - asked for one the spec does not declare → [`SpecBackendRefusal::TransportNotDeclared`];
    /// - neither declared → [`SpecBackendRefusal::NoTurnTransport`].
    pub fn chosen(
        agent: &str,
        declares_headless: bool,
        declares_acp: bool,
        asked: Option<SpecTransport>,
    ) -> Result<SpecTransport, SpecBackendRefusal> {
        // TODO(spec-backend): implement.
        let _ = (agent, declares_headless, declares_acp, asked);
        todo!("spec-backend: SpecTransport::chosen")
    }
}

impl std::fmt::Display for SpecTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why no spec backend can be built for an agent. Each names the agent; none falls back to another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecBackendRefusal {
    /// The spec declares neither `coding_agent.headless` nor `coding_agent.acp` — a PTY-only agent
    /// (`muse`, `warp`) cannot take a workflow turn.
    NoTurnTransport { agent: String },
    /// The session or request asked for a transport the spec does not declare.
    TransportNotDeclared { agent: String, asked: SpecTransport },
}

impl std::fmt::Display for SpecBackendRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpecBackendRefusal::NoTurnTransport { agent } => write!(
                f,
                "agent `{agent}` declares no `coding_agent.headless` or `coding_agent.acp` \
                 section, so it cannot run a workflow turn"
            ),
            SpecBackendRefusal::TransportNotDeclared { agent, asked } => write!(
                f,
                "agent `{agent}` was asked to run over {asked}, which its spec does not declare"
            ),
        }
    }
}

impl std::error::Error for SpecBackendRefusal {}

/// An ACP coding agent's workflow backend, configured by its spec's `coding_agent.acp` section:
/// the rendered command, the hooks adapter the spec names and the deps that adapter reads, over
/// the shared [`AcpDriver`](super::acp_driver::AcpDriver). Any agent a YAML file declares with an
/// `acp` section runs through it; nothing here names an agent.
pub struct SpecAcpBackend {
    /// What a route body and every refusal know of the agent; `agent.id` is [`CodingBackend::name`].
    agent: RoutedAgent,
    /// The command, rendered from `coding_agent.acp.command`.
    command: AcpAgentCommand,
    /// The hooks adapter the spec names, when it names one.
    hooks: Option<std::sync::Arc<dyn AcpHooksAdapter>>,
    /// The resolved deps' binaries the hooks adapter reads, by tool id.
    deps: BTreeMap<String, PathBuf>,
    /// The provider route the agent runs on; `None` for the vendor's own login.
    provider_route: Option<HarnessRoute>,
    /// How long one session open (`initialize` + `session/new` or `session/load`) may take.
    session_open_deadline: Duration,
    /// The driver, started on first use.
    driver: DriverSlot,
}

impl std::fmt::Debug for SpecAcpBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpecAcpBackend")
            .field("agent", &self.agent)
            .field("program", &self.command.program)
            .field("args", &self.command.args)
            .field("hooks", &self.hooks)
            .field("deps", &self.deps)
            .field("on_route", &self.provider_route.is_some())
            .field("session_open_deadline", &self.session_open_deadline)
            .field(
                "started",
                &self.driver.lock().map(|d| d.is_some()).unwrap_or(false),
            )
            .finish()
    }
}

impl SpecAcpBackend {
    /// A backend spawning `command` for `agent`, with no hooks adapter, no deps, no provider route
    /// and [`SESSION_OPEN_DEADLINE`].
    pub fn new(agent: RoutedAgent, command: AcpAgentCommand) -> Self {
        Self {
            agent,
            command,
            hooks: None,
            deps: BTreeMap::new(),
            provider_route: None,
            session_open_deadline: SESSION_OPEN_DEADLINE,
            driver: std::sync::Mutex::new(None),
        }
    }

    /// The driver runs with `hooks` (the adapter the spec's `acp.hooks_adapter` names), which read
    /// the resolved `deps`.
    #[must_use]
    pub fn with_hooks(
        mut self,
        hooks: std::sync::Arc<dyn AcpHooksAdapter>,
        deps: BTreeMap<String, PathBuf>,
    ) -> Self {
        self.hooks = Some(hooks);
        self.deps = deps;
        self
    }

    /// A session open that has not answered within `deadline` is refused, naming the agent and the
    /// deadline, and the agent child is ended.
    #[must_use]
    pub fn with_session_open_deadline(mut self, deadline: Duration) -> Self {
        self.session_open_deadline = deadline;
        self
    }

    /// Every turn runs on `route`: its environment on the agent child, then the hooks adapter's
    /// preparation. Refused, naming the agent's product, for an agent whose spec declares no
    /// provider (`provider_route::by_spec::agent_routable`) — never silently run on the vendor.
    pub fn with_provider_route(self, route: HarnessRoute) -> Result<Self, RouteRefusal> {
        // TODO(spec-backend): implement — `by_spec::agent_routable(&self.agent)?`, then store it and
        // empty the driver slot, as the bespoke ACP backends' `set_provider_route` did.
        let _ = route;
        todo!("spec-backend: SpecAcpBackend::with_provider_route")
    }
}

#[async_trait::async_trait]
impl CodingBackend for SpecAcpBackend {
    /// One turn: the driver started on first use (route env, then the hooks adapter's
    /// preparation), a session opened for the request within the session-open deadline (bracketed
    /// by the hooks' open scope when they have one), and one prompt answered. The request's system
    /// prompt is merged into the prompt, never dropped.
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        // TODO(spec-backend): implement.
        let _ = request;
        todo!("spec-backend: SpecAcpBackend::invoke")
    }

    /// The spec id.
    fn name(&self) -> &str {
        // TODO(spec-backend): implement — `&self.agent.id`.
        todo!("spec-backend: SpecAcpBackend::name")
    }

    /// The models a fresh session would offer, from the driver.
    async fn list_models(&self) -> Result<BackendModels, BackendError> {
        // TODO(spec-backend): implement.
        todo!("spec-backend: SpecAcpBackend::list_models")
    }
}

/// A coding agent's workflow backend, built from its spec over the transport
/// [`SpecTransport::chosen`] picks. Replaces `AnyBackend`.
#[derive(Debug)]
pub enum SpecBackend {
    /// `coding_agent.headless`, on the generic headless driver (`#agent-tools 15/19`).
    Headless(SpecHeadlessBackend),
    /// `coding_agent.acp`, on the shared ACP driver.
    Acp(SpecAcpBackend),
}

impl SpecBackend {
    /// The transport this backend runs its turns over.
    pub fn transport(&self) -> SpecTransport {
        // TODO(spec-backend): implement.
        todo!("spec-backend: SpecBackend::transport")
    }

    /// This backend on `route`. Refused (n13's agent-keyed [`RouteRefusal`]) for an agent whose spec
    /// has no `coding_agent.route` section.
    pub fn with_provider_route(self, route: HarnessRoute) -> Result<Self, RouteRefusal> {
        // TODO(spec-backend): implement — delegate to the variant's `with_provider_route`.
        let _ = route;
        todo!("spec-backend: SpecBackend::with_provider_route")
    }
}

#[async_trait::async_trait]
impl CodingBackend for SpecBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        // TODO(spec-backend): implement — delegate to the variant.
        let _ = request;
        todo!("spec-backend: SpecBackend::invoke")
    }

    /// The spec id, whichever transport runs it.
    fn name(&self) -> &str {
        // TODO(spec-backend): implement — delegate to the variant.
        todo!("spec-backend: SpecBackend::name")
    }

    async fn list_models(&self) -> Result<BackendModels, BackendError> {
        // TODO(spec-backend): implement — delegate to the variant.
        todo!("spec-backend: SpecBackend::list_models")
    }
}

impl SharedBackend {
    /// The simulated-response test backend (`--agent stub`), shared: what tests built as
    /// `SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()))` before `AnyBackend` was
    /// deleted. Its `name()` is `stub`.
    pub fn stub() -> Self {
        // TODO(spec-backend): implement — `Self::from_arc(Arc::new(StubBackend::new()))`.
        todo!("spec-backend: SharedBackend::stub")
    }
}
