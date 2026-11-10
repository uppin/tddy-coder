//! `coding_agent.route.{providers, shape}`: which model providers an agent can be routed to, and
//! how a route reaches it (`#agent-tools 12/12`).
//!
//! Feature doc: `docs/ft/daemon/session-agent-roster.md`. The types live in
//! `tddy_agent_backend::provider_route`, beside the route bodies that read them
//! (`#agent-tools 13/19`); they are re-exported here under the paths this crate published.

pub use tddy_agent_backend::provider_route::{ProviderKind, RouteShape, RouteSupport, RoutedAgent};
