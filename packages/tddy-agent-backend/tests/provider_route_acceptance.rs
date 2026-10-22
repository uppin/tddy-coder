//! What a harness is given to run on a model-provider account, and when it is refused.
//!
//! The route is pure data, so these tests read it directly: no process is spawned. What they pin is
//! the contract every launcher relies on — Claude Code is pointed at the account's Anthropic
//! Messages root with the account's key, and **every** model it might pick is pinned, so no request
//! can fall through to Anthropic.
//!
//! Feature: docs/ft/daemon/provider-accounts.md (Running a harness on an account)

use tddy_agent_backend::provider_route::{
    choose_route, harness_routable, resolve_route, route_supported, Harness, RouteChoice,
    RouteEndpoints, RouteRefusal, RouteSecret, RouteSources, RouteTarget,
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
    for harness in [Harness::ClaudeCli, Harness::Claude, Harness::ClaudeAcp] {
        // When
        let route = resolve_route(harness, &a_meta_account_running("muse-wonder-1.0"))
            .unwrap_or_else(|refusal| panic!("{harness:?} routes on Meta: {refusal}"));

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
                "{harness:?} {alias}"
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
    let refusal = resolve_route(Harness::ClaudeCli, &a_baseten_account()).expect_err("refused");

    // Then
    assert!(matches!(
        refusal,
        RouteRefusal::ProtocolNotServed {
            harness: Harness::ClaudeCli,
            ..
        }
    ));
    assert!(refusal.to_string().contains("Anthropic Messages"));
}

#[test]
fn cursor_cannot_be_routed_whatever_the_account() {
    for harness in [Harness::Cursor, Harness::CursorCli] {
        // When
        let refusal = resolve_route(harness, &a_meta_account_running("muse-spark-1.3"))
            .expect_err("cursor is never routed");
        let support = route_supported(harness, "meta", &["anthropic-messages", "openai-chat"]);

        // Then
        assert_eq!(refusal, RouteRefusal::HarnessCannotBeRouted { harness });
        assert_eq!(
            support,
            Err(RouteRefusal::HarnessCannotBeRouted { harness })
        );
        assert!(refusal.to_string().contains("Cursor"));
    }
}

#[test]
fn the_matrix_answers_without_building_a_route() {
    // Then
    assert_eq!(
        route_supported(
            Harness::ClaudeCli,
            "fireworks",
            &["anthropic-messages", "openai-chat"]
        ),
        Ok(())
    );
    assert!(matches!(
        route_supported(Harness::ClaudeCli, "baseten", &["openai-chat"]),
        Err(RouteRefusal::ProtocolNotServed { .. })
    ));
}

#[test]
fn every_harness_id_names_its_harness_and_back() {
    for id in [
        "claude",
        "claude-acp",
        "claude-cli",
        "codex",
        "codex-acp",
        "opencode",
        "opencode-cli",
        "muse-cli",
        "cursor",
        "cursor-cli",
    ] {
        // When
        let harness = Harness::from_agent_or_session_type(id).unwrap_or_else(|| panic!("{id}"));

        // Then
        assert_eq!(harness.id(), id);
    }
    assert_eq!(Harness::from_agent_or_session_type("stub"), None);
    assert_eq!(Harness::from_agent_or_session_type("tool"), None);
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
        Harness::ClaudeCli,
        &a_meta_account_running("muse-spark-1.3"),
    )
    .expect("routes");

    // Then
    assert!(!format!("{route:?}").contains(META_KEY));
}

#[test]
fn every_harness_but_cursor_answers_routable() {
    // Then Claude Code, Codex, OpenCode and Muse Code are routable, and Cursor never is
    for harness in [
        Harness::Claude,
        Harness::ClaudeAcp,
        Harness::ClaudeCli,
        Harness::Codex,
        Harness::CodexAcp,
        Harness::OpenCode,
        Harness::OpenCodeCli,
        Harness::MuseCli,
    ] {
        assert_eq!(harness_routable(harness), Ok(()));
    }
    for harness in [Harness::Cursor, Harness::CursorCli] {
        assert_eq!(
            harness_routable(harness),
            Err(RouteRefusal::HarnessCannotBeRouted { harness })
        );
    }
}
