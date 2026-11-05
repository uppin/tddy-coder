//! The one headless workflow backend (`#agent-tools 15/19`): every headless coding agent's
//! workflow turn — `claude -p`, `agent -p`, `codex exec --json`, `oz agent run`, or any agent a
//! YAML file declares — runs through [`SpecHeadlessBackend`], configured by the agent's spec.
//!
//! Changeset: `docs/dev/1-WIP/2026-11-04-agent-tools-headless-driver.md`.
//!
//! What differs between agents is data, never a Rust branch on an agent's name:
//!
//! - the argv, the files it writes and the adapter its stdout is read through come from the spec's
//!   renderer ([`HeadlessRenderer`]);
//! - how the system prompt is delivered, which params a goal's hints bind, whether the approval
//!   tool and the tool server are registered, whether a route replaces the model, who mints a fresh
//!   session and where registration files go come from the spec's protocol fields
//!   ([`HeadlessProtocol`]);
//! - whether a provider route can point the agent elsewhere comes from its [`RoutedAgent`].
//!
//! A turn's child runs in **its own process group**, under a **turn deadline**: a cancel (the
//! turn's future dropped) or the deadline kills that group and no other, so two concurrent turns
//! never reach each other's processes. The agent process sees **no `TDDY_*` variable**: a
//! `tddy-tools` it runs — over MCP or from its shell — finds the session through the
//! [`crate::session_pointer`] file the turn writes in its working directory.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::adapters::headless::{HeadlessModelListing, HeadlessProtocol};
use crate::adapters::HeadlessRenderer;
use crate::error::BackendError;
use crate::provider_route::by_spec::RouteRefusal;
use crate::provider_route::{HarnessRoute, RoutedAgent};
use crate::stream::ProgressEvent;

use super::{BackendModels, CodingBackend, InvokeRequest, InvokeResponse};

/// How long a workflow turn may run before it is cancelled and its process group killed, when the
/// backend is given no other deadline ([`SpecHeadlessBackend::with_turn_deadline`]).
pub const DEFAULT_TURN_DEADLINE: Duration = Duration::from_secs(4 * 60 * 60);

/// How long a model listing (`models.list_command`) may run.
pub const MODEL_LISTING_DEADLINE: Duration = Duration::from_secs(30);

type ProgressCallback = Option<Arc<Mutex<Box<dyn FnMut(&ProgressEvent) + Send>>>>;

/// A headless coding agent's workflow backend, configured by its spec.
#[derive(Clone)]
pub struct SpecHeadlessBackend {
    renderer: Arc<dyn HeadlessRenderer>,
    protocol: HeadlessProtocol,
    agent: RoutedAgent,
    provider_route: Option<HarnessRoute>,
    model_listing: Option<HeadlessModelListing>,
    progress_callback: ProgressCallback,
    turn_deadline: Duration,
}

impl std::fmt::Debug for SpecHeadlessBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpecHeadlessBackend")
            .field("renderer", &self.renderer)
            .field("protocol", &self.protocol)
            .field("agent", &self.agent)
            .field("provider_route", &self.provider_route)
            .field("model_listing", &self.model_listing)
            .field("turn_deadline", &self.turn_deadline)
            .finish_non_exhaustive()
    }
}

impl SpecHeadlessBackend {
    /// A backend rendering every turn from `renderer`, run as `protocol` says, routable as `agent`
    /// says, with [`DEFAULT_TURN_DEADLINE`], no provider route, no model listing (the curated
    /// catalogue answers `list_models`) and no progress callback.
    pub fn new(
        renderer: Arc<dyn HeadlessRenderer>,
        protocol: HeadlessProtocol,
        agent: RoutedAgent,
    ) -> Self {
        Self {
            renderer,
            protocol,
            agent,
            provider_route: None,
            model_listing: None,
            progress_callback: None,
            turn_deadline: DEFAULT_TURN_DEADLINE,
        }
    }

    /// Every turn is cancelled, and its process group killed, once it has run for `deadline`.
    #[must_use]
    pub fn with_turn_deadline(mut self, deadline: Duration) -> Self {
        self.turn_deadline = deadline;
        self
    }

    /// `list_models` runs `listing` (the spec's `models.list_command`, rendered with the
    /// spec-resolved binary) under [`MODEL_LISTING_DEADLINE`] and reads it through its adapter.
    #[must_use]
    pub fn with_model_listing(mut self, listing: HeadlessModelListing) -> Self {
        self.model_listing = Some(listing);
        self
    }

    /// Progress events go to `f` for a request that names no `progress_sink`.
    #[must_use]
    pub fn with_progress<F>(mut self, f: F) -> Self
    where
        F: FnMut(&ProgressEvent) + Send + 'static,
    {
        self.progress_callback = Some(Arc::new(Mutex::new(Box::new(f))));
        self
    }

    /// Every turn runs on `route`: its environment on the child and its argv spliced as
    /// `route_args`, the request's model replaced as the protocol's `route_model` says. Refused,
    /// naming the agent's product, for an agent whose spec declares no provider
    /// (`provider_route::by_spec::agent_routable`) — never silently run on the vendor.
    pub fn with_provider_route(self, route: HarnessRoute) -> Result<Self, RouteRefusal> {
        // TODO(headless-driver): implement — `by_spec::agent_routable(&self.agent)?`, then store.
        let _ = route;
        todo!("headless-driver: SpecHeadlessBackend::with_provider_route")
    }

    /// The protocol this backend runs its turns with.
    pub fn protocol(&self) -> &HeadlessProtocol {
        &self.protocol
    }
}

#[async_trait::async_trait]
impl CodingBackend for SpecHeadlessBackend {
    /// One workflow turn: the request mapped onto a `HeadlessTurn` by the protocol, rendered, its
    /// files and the session pointer written in the working directory (git-excluded) and removed
    /// when the turn ends, the program spawned in its own process group with no inherited `TDDY_*`
    /// variable, its stdout read through the adapter, and its exit judged by the adapter — all under
    /// the turn deadline, with a bounded stderr grace.
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        // TODO(headless-driver): implement.
        let _ = request;
        todo!("headless-driver: SpecHeadlessBackend::invoke")
    }

    /// The spec id (`claude`, `cursor`, `codex`, `oz`, or a YAML-only agent's), so the workflow
    /// engine's codex thread-id swap and the context-glob and model tables keep keying on it.
    fn name(&self) -> &str {
        // TODO(headless-driver): implement — `self.renderer.agent_id()`.
        todo!("headless-driver: SpecHeadlessBackend::name")
    }

    /// The spec's model listing when it declares one, else the curated catalogue for
    /// [`CodingBackend::name`].
    async fn list_models(&self) -> Result<BackendModels, BackendError> {
        // TODO(headless-driver): implement.
        todo!("headless-driver: SpecHeadlessBackend::list_models")
    }
}
