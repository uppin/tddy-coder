//! The daemon's [`RouteSupport`]: the route matrix (`tddy_core::provider_route`) answered for the
//! accounts service, which does not depend on the harness crates. An agent's support is read from
//! its spec in the host's registry — loaded once, at start, never per RPC.

use std::sync::Arc;

use tddy_accounts::RouteSupport;
use tddy_coding_agent_specs::{routed_agent, CodingAgentRegistry};
use tddy_core::provider_route::{agent_routable, route_supported, RoutedAgent};

/// [`RouteSupport`] over [`tddy_core::provider_route`] and the host's coding agents: an agent this
/// build cannot route is reported unroutable, so no account is offered for a start that would be
/// refused. A host with no coding agents, or an id its registry does not hold, refuses.
pub struct HarnessRouteSupport {
    agents: Option<Arc<CodingAgentRegistry>>,
}

impl HarnessRouteSupport {
    /// The matrix of `agents`; `None` for a host with no coding agents installed.
    pub fn new(agents: Option<Arc<CodingAgentRegistry>>) -> Self {
        Self { agents }
    }

    /// The agent `id` names: an agent id (`claude`) or the CLI session type that runs one
    /// (`claude-cli`) — the two forms a vault's `default_for` and a harness profile write.
    fn agent_named(&self, id: &str) -> Result<RoutedAgent, String> {
        let agents = self.agents.as_ref().ok_or_else(|| {
            format!("this host has no coding agents installed: cannot route {id:?}")
        })?;
        agents
            .get(id)
            .or_else(|| agents.for_session_type(id))
            .map(routed_agent)
            .ok_or_else(|| format!("unknown harness {id:?}"))
    }
}

impl RouteSupport for HarnessRouteSupport {
    fn harness_routable(&self, harness: &str) -> Result<(), String> {
        agent_routable(&self.agent_named(harness)?).map_err(|refusal| refusal.to_string())
    }

    fn supports(&self, harness: &str, preset: &str, protocols: &[&str]) -> Result<(), String> {
        let agent = self.agent_named(harness)?;
        agent_routable(&agent)
            .and_then(|()| route_supported(&agent, preset, protocols))
            .map_err(|refusal| refusal.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The matrix of the built-in coding agents.
    fn support() -> HarnessRouteSupport {
        HarnessRouteSupport::new(Some(Arc::new(
            CodingAgentRegistry::builtin().expect("the built-ins load"),
        )))
    }

    #[test]
    fn cursor_is_the_harness_no_account_can_run() {
        let reason = support()
            .harness_routable("cursor-cli")
            .expect_err("cursor is never routed");
        assert!(reason.contains("Cursor"));
        assert_eq!(support().harness_routable("claude-cli"), Ok(()));
    }

    #[test]
    fn claude_code_runs_only_on_an_account_serving_anthropic_messages() {
        assert_eq!(
            support().supports("claude-cli", "meta", &["anthropic-messages"]),
            Ok(())
        );
        let reason = support()
            .supports("claude-cli", "baseten", &["openai-chat"])
            .expect_err("baseten serves no Anthropic Messages");
        assert!(reason.contains("Anthropic Messages"));
    }

    #[test]
    fn muse_code_runs_only_on_a_meta_account() {
        assert_eq!(support().harness_routable("muse-cli"), Ok(()));
        assert_eq!(
            support().supports("muse-cli", "meta", &["openai-responses", "openai-chat"]),
            Ok(())
        );
        let reason = support()
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
            assert_eq!(support().harness_routable(harness), Ok(()));
            assert_eq!(
                support().supports(harness, "meta", &["openai-responses"]),
                Ok(())
            );
            assert!(support()
                .supports(harness, "baseten", &["openai-chat"])
                .is_err());
        }
    }

    #[test]
    fn opencode_runs_on_an_account_serving_an_openai_protocol() {
        for harness in ["opencode", "opencode-cli"] {
            assert_eq!(support().harness_routable(harness), Ok(()));
            assert_eq!(
                support().supports(harness, "meta", &["openai-responses", "openai-chat"]),
                Ok(())
            );
        }
    }

    #[test]
    fn an_unknown_harness_is_named_in_the_refusal() {
        let reason = support()
            .harness_routable("stub")
            .expect_err("stub is no harness");
        assert!(reason.contains("\"stub\""));
        assert!(support().supports("stub", "meta", &[]).is_err());
    }

    #[test]
    fn the_session_type_and_the_agent_id_name_the_same_support() {
        // When
        let answers = (
            support().supports("claude-cli", "meta", &["anthropic-messages"]),
            support().supports("claude", "meta", &["anthropic-messages"]),
            support().supports("claude-acp", "meta", &["anthropic-messages"]),
        );

        // Then — the vault's `default_for` is read under either spelling
        assert_eq!(answers, (Ok(()), Ok(()), Ok(())));
    }

    #[test]
    fn a_host_with_no_coding_agents_routes_nothing() {
        // When
        let reason = HarnessRouteSupport::new(None)
            .harness_routable("claude-cli")
            .expect_err("no registry, no route");

        // Then
        assert!(reason.contains("no coding agents installed"), "{reason}");
    }
}
