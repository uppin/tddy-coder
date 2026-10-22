//! The daemon's [`RouteSupport`]: the harness route matrix (`tddy_core::provider_route`) answered
//! for the accounts service, which does not depend on the harness crates.

use tddy_accounts::RouteSupport;
use tddy_core::provider_route::{harness_routable, route_supported, Harness};

fn harness_named(id: &str) -> Result<Harness, String> {
    Harness::from_agent_or_session_type(id).ok_or_else(|| format!("unknown harness {id:?}"))
}

/// [`RouteSupport`] over [`tddy_core::provider_route`]: a harness this build cannot route yet is
/// reported unroutable, so no account is offered for a start that would be refused.
pub struct HarnessRouteSupport;

impl RouteSupport for HarnessRouteSupport {
    fn harness_routable(&self, harness: &str) -> Result<(), String> {
        harness_routable(harness_named(harness)?).map_err(|refusal| refusal.to_string())
    }

    fn supports(&self, harness: &str, preset: &str, protocols: &[&str]) -> Result<(), String> {
        let harness = harness_named(harness)?;
        harness_routable(harness)
            .and_then(|()| route_supported(harness, preset, protocols))
            .map_err(|refusal| refusal.to_string())
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
    fn muse_code_runs_only_on_a_meta_account() {
        assert_eq!(HarnessRouteSupport.harness_routable("muse-cli"), Ok(()));
        assert_eq!(
            HarnessRouteSupport.supports("muse-cli", "meta", &["openai-responses", "openai-chat"]),
            Ok(())
        );
        let reason = HarnessRouteSupport
            .supports(
                "muse-cli",
                "fireworks",
                &["openai-responses", "openai-chat"],
            )
            .expect_err("Muse Code runs on Meta only");
        assert!(reason.contains("Meta only"), "{reason}");
    }

    #[test]
    fn codex_runs_only_on_an_account_serving_openai_responses() {
        for harness in ["codex", "codex-acp"] {
            assert_eq!(HarnessRouteSupport.harness_routable(harness), Ok(()));
            assert_eq!(
                HarnessRouteSupport.supports(harness, "meta", &["openai-responses"]),
                Ok(())
            );
            assert!(HarnessRouteSupport
                .supports(harness, "baseten", &["openai-chat"])
                .is_err());
        }
    }

    #[test]
    fn opencode_runs_on_an_account_serving_an_openai_protocol() {
        for harness in ["opencode", "opencode-cli"] {
            assert_eq!(HarnessRouteSupport.harness_routable(harness), Ok(()));
            assert_eq!(
                HarnessRouteSupport.supports(harness, "meta", &["openai-responses", "openai-chat"]),
                Ok(())
            );
        }
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
