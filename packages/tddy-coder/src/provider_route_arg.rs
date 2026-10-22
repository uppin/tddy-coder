//! `--provider-route-path`: the provider route a session's backend runs on, loaded and applied.
//!
//! The daemon writes the route file owner-only (`tddy_core::provider_route_file`); this module is
//! where `tddy-coder` reads it and hands it to the backend. Nothing here falls back to the vendor: a
//! route that cannot be read or applied stops the run.

use std::path::Path;

use tddy_core::provider_route::HarnessRoute;
use tddy_core::AnyBackend;

/// The route the file at `path` holds; `None` when no path was given. A file another account can
/// read, or one that is not a route, is an error. The message names the path and never the
/// contents, which hold a key.
pub(crate) fn load_provider_route(path: Option<&Path>) -> anyhow::Result<Option<HarnessRoute>> {
    path.map(|path| {
        tddy_core::provider_route_file::load_route_file(path)
            .map_err(|reason| anyhow::anyhow!("--provider-route-path could not be used: {reason}"))
    })
    .transpose()
}

/// A specialized agent definition is its own endpoint, so a route cannot be applied to it: the
/// start is refused rather than run on the definition's endpoint instead of the account.
pub(crate) fn refuse_route_for_definition(
    agent: &str,
    route: Option<&HarnessRoute>,
) -> anyhow::Result<()> {
    if route.is_some() {
        anyhow::bail!(
            "agent {agent:?} is a specialized agent definition, which cannot run on a provider \
             account: its definition is its own endpoint"
        );
    }
    Ok(())
}

/// `backend` on `route`, or unchanged without one. A backend that cannot be routed is refused.
pub(crate) fn apply_provider_route(
    backend: AnyBackend,
    route: Option<HarnessRoute>,
) -> anyhow::Result<AnyBackend> {
    match route {
        Some(route) => backend
            .with_provider_route(route)
            .map_err(anyhow::Error::msg),
        None => Ok(backend),
    }
}
