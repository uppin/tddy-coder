//! The daemon's [`RouteSupport`]: the harness route matrix (`tddy_core::provider_route`) answered
//! for the accounts service, which does not depend on the harness crates.

use tddy_accounts::RouteSupport;

/// [`RouteSupport`] over [`tddy_core::provider_route::route_supported`].
pub struct HarnessRouteSupport;

impl RouteSupport for HarnessRouteSupport {
    fn harness_routable(&self, harness: &str) -> Result<(), String> {
        let _ = harness;
        // TODO(claude-route): implement
        todo!()
    }

    fn supports(&self, harness: &str, preset: &str, protocols: &[&str]) -> Result<(), String> {
        let _ = (harness, preset, protocols);
        // TODO(claude-route): implement
        todo!()
    }
}
