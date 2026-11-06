//! What a harness is given to run on a model-provider account, and when it is refused.
//!
//! The route is pure data, so these tests read it directly: no process is spawned. What they pin is
//! the contract every launcher relies on — Claude Code is pointed at the account's Anthropic
//! Messages root with the account's key, and **every** model it might pick is pinned, so no request
//! can fall through to Anthropic.
//!
//! Feature: docs/ft/daemon/provider-accounts.md (Running a harness on an account)

mod route_agents;

use route_agents::{claude_named, codex_named, cursor_named, muse_named, opencode_named};
use tddy_agent_backend::provider_route::{
    agent_routable, choose_route, resolve_route, route_supported, RouteChoice, RouteEndpoints,
    RouteRefusal, RouteSecret, RouteSources, RouteTarget,
};

const META_KEY: &str = "a-meta-key";

fn a_meta_account_running(model: &str) -> RouteTarget {
    RouteTarget {
        preset: "meta".to_string(),
        endpoints: RouteEndpoints {
            anthropic_messages: Some("https://api.meta.ai".to_string()),
            openai_responses: Some("https://api.meta.ai/v1".to_string()),
            openai_chat: Some("https://api.meta.ai/v1".to_string()),
        },
        api_key: RouteSecret::new(META_KEY),
        model: model.to_string(),
    }
}

fn a_baseten_account() -> RouteTarget {
    RouteTarget {
        preset: "baseten".to_string(),
        endpoints: RouteEndpoints {
            openai_chat: Some("https://inference.baseten.co/v1".to_string()),
            ..RouteEndpoints::default()
        },
        api_key: RouteSecret::new("a-baseten-key"),
        model: "moonshotai/Kimi-K2".to_string(),
    }
}

fn value_of<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

#[test]
fn claude_code_on_meta_gets_the_messages_root_the_key_and_every_alias_pinned() {
    for harness in ["claude", "claude-acp", "claude-cli"].map(claude_named) {
        // When
        let route = resolve_route(&harness, &a_meta_account_running("muse-wonder-1.0"))
            .unwrap_or_else(|refusal| panic!("{} routes on Meta: {refusal}", harness.id));

        // Then
        assert_eq!(
            value_of(&route.env, "ANTHROPIC_BASE_URL"),
            Some("https://api.meta.ai")
        );
        for alias in [
            "ANTHROPIC_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "CLAUDE_CODE_SUBAGENT_MODEL",
        ] {
            assert_eq!(
                value_of(&route.env, alias),
                Some("muse-wonder-1.0"),
                "{} {alias}",
                harness.id
            );
        }
        let secrets: Vec<(&str, &str)> = route
            .secret_env
            .iter()
            .map(|(k, v)| (k.as_str(), v.expose()))
            .collect();
        assert_eq!(secrets, vec![("ANTHROPIC_AUTH_TOKEN", META_KEY)]);
        assert!(
            route.env.iter().all(|(_, v)| v != META_KEY),
            "the key is never plain environment"
        );
        assert!(
            route.args.iter().all(|a| !a.contains(META_KEY)),
            "nor an argument"
        );
    }
}

#[test]
fn claude_code_refuses_an_account_without_anthropic_messages() {
    // When
    let refusal =
        resolve_route(&claude_named("claude"), &a_baseten_account()).expect_err("refused");

    // Then
    assert!(matches!(
        &refusal,
        RouteRefusal::ProtocolNotServed { agent, .. } if agent == "claude"
    ));
    assert!(refusal.to_string().contains("Anthropic Messages"));
}

#[test]
fn cursor_cannot_be_routed_whatever_the_account() {
    for harness in ["cursor", "cursor-cli"].map(cursor_named) {
        // When
        let refusal = resolve_route(&harness, &a_meta_account_running("muse-spark-1.3"))
            .expect_err("cursor is never routed");
        let support = route_supported(&harness, "meta", &["anthropic-messages", "openai-chat"]);
        let cannot_be_routed = RouteRefusal::AgentCannotBeRouted {
            agent: harness.id.clone(),
            product: "Cursor".to_string(),
        };

        // Then
        assert_eq!(refusal, cannot_be_routed);
        assert_eq!(support, Err(cannot_be_routed));
        assert!(refusal.to_string().contains("Cursor"));
    }
}

#[test]
fn the_matrix_answers_without_building_a_route() {
    // Then
    assert_eq!(
        route_supported(
            &claude_named("claude-cli"),
            "fireworks",
            &["anthropic-messages", "openai-chat"]
        ),
        Ok(())
    );
    assert!(matches!(
        route_supported(&claude_named("claude-cli"), "baseten", &["openai-chat"]),
        Err(RouteRefusal::ProtocolNotServed { .. })
    ));
}

#[test]
fn a_session_choice_beats_an_agent_definition_which_beats_a_default() {
    // Given
    let all_three = RouteSources {
        session: Some(RouteChoice::Account("session-account".to_string())),
        agent_definition: Some(RouteChoice::Account("profile-account".to_string())),
        user_default: Some("default-account".to_string()),
    };
    let no_session = RouteSources {
        session: None,
        ..all_three.clone()
    };
    let only_default = RouteSources {
        session: None,
        agent_definition: None,
        ..all_three.clone()
    };

    // Then
    assert_eq!(choose_route(&all_three).as_deref(), Some("session-account"));
    assert_eq!(
        choose_route(&no_session).as_deref(),
        Some("profile-account")
    );
    assert_eq!(
        choose_route(&only_default).as_deref(),
        Some("default-account")
    );
    assert_eq!(choose_route(&RouteSources::default()), None);
}

#[test]
fn an_explicit_built_in_choice_overrides_every_lower_level() {
    // Given
    let built_in_session = RouteSources {
        session: Some(RouteChoice::BuiltIn),
        agent_definition: Some(RouteChoice::Account("profile-account".to_string())),
        user_default: Some("default-account".to_string()),
    };
    let built_in_profile = RouteSources {
        session: None,
        agent_definition: Some(RouteChoice::BuiltIn),
        user_default: Some("default-account".to_string()),
    };

    // Then
    assert_eq!(choose_route(&built_in_session), None);
    assert_eq!(choose_route(&built_in_profile), None);
}

#[test]
fn a_route_can_be_logged_without_its_key() {
    // Given
    let route = resolve_route(
        &claude_named("claude-cli"),
        &a_meta_account_running("muse-spark-1.3"),
    )
    .expect("routes");

    // Then
    assert!(!format!("{route:?}").contains(META_KEY));
}

#[test]
fn every_agent_but_cursor_and_warp_answers_routable() {
    // Then Claude Code, Codex, OpenCode and Muse Code are routable; Cursor and Warp never are
    let routable = [
        claude_named("claude"),
        claude_named("claude-acp"),
        claude_named("claude-cli"),
        codex_named("codex"),
        codex_named("codex-acp"),
        opencode_named("opencode"),
        opencode_named("opencode-cli"),
        muse_named("muse-cli"),
    ];
    for agent in &routable {
        assert_eq!(agent_routable(agent), Ok(()), "{}", agent.id);
    }
    for id in ["cursor", "cursor-cli", "warp-cli"] {
        let agent = cursor_named(id);
        assert_eq!(
            agent_routable(&agent),
            Err(RouteRefusal::AgentCannotBeRouted {
                agent: id.to_string(),
                product: "Cursor".to_string(),
            })
        );
    }
}
