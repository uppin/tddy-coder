//! The daemon's [`RouteSupport`]: the harness route matrix (`tddy_core::provider_route`) answered
//! for the accounts service, which does not depend on the harness crates.

use tddy_accounts::RouteSupport;
use tddy_core::provider_route::{route_supported, Harness, RouteRefusal};

fn harness_named(id: &str) -> Result<Harness, String> {
    Harness::from_agent_or_session_type(id).ok_or_else(|| format!("unknown harness {id:?}"))
}

/// [`RouteSupport`] over [`tddy_core::provider_route::route_supported`].
pub struct HarnessRouteSupport;

impl RouteSupport for HarnessRouteSupport {
    fn harness_routable(&self, harness: &str) -> Result<(), String> {
        // A harness is routable when any account could run it; Cursor is the one that never is.
        // `route_supported` answers HarnessCannotBeRouted before it looks at the account.
        match route_supported(harness_named(harness)?, "", &[]) {
            Err(refusal @ RouteRefusal::HarnessCannotBeRouted { .. }) => Err(refusal.to_string()),
            _ => Ok(()),
        }
    }

    fn supports(&self, harness: &str, preset: &str, protocols: &[&str]) -> Result<(), String> {
        route_supported(harness_named(harness)?, preset, protocols).map_err(|r| r.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_is_the_harness_no_account_can_run() {
        let reason = HarnessRouteSupport
            .harness_routable("cursor-cli")
            .expect_err("cursor is never routed");
        assert!(reason.contains("Cursor"));
        assert_eq!(HarnessRouteSupport.harness_routable("claude-cli"), Ok(()));
    }

    #[test]
    fn claude_code_runs_only_on_an_account_serving_anthropic_messages() {
        assert_eq!(
            HarnessRouteSupport.supports("claude-cli", "meta", &["anthropic-messages"]),
            Ok(())
        );
        let reason = HarnessRouteSupport
            .supports("claude-cli", "baseten", &["openai-chat"])
            .expect_err("baseten serves no Anthropic Messages");
        assert!(reason.contains("Anthropic Messages"));
    }

    #[test]
    fn an_unknown_harness_is_named_in_the_refusal() {
        let reason = HarnessRouteSupport
            .harness_routable("stub")
            .expect_err("stub is no harness");
        assert!(reason.contains("\"stub\""));
        assert!(HarnessRouteSupport.supports("stub", "meta", &[]).is_err());
    }
}
