//! The Codex route: a Responses-API provider declared through `-c` overrides, with the key in the
//! variable that provider names as its `env_key`.
//!
//! Feature: docs/ft/coder/1-WIP/PRD-2026-10-18-provider-route-tool-route.md

use tddy_agent_backend::provider_route::{
    resolve_route, Harness, RouteEndpoints, RouteRefusal, RouteSecret, RouteTarget,
    CODEX_ROUTE_KEY_ENV, CODEX_ROUTE_PROVIDER_ID,
};

fn on_meta(model: &str) -> RouteTarget {
    RouteTarget {
        preset: "meta".to_string(),
        endpoints: RouteEndpoints {
            anthropic_messages: Some("https://api.meta.ai".to_string()),
            openai_responses: Some("https://api.meta.ai/v1".to_string()),
            openai_chat: Some("https://api.meta.ai/v1".to_string()),
        },
        api_key: RouteSecret::new("a-meta-key"),
        model: model.to_string(),
    }
}

#[test]
fn codex_on_meta_declares_a_responses_provider_through_config_overrides() {
    for harness in [Harness::Codex, Harness::CodexAcp] {
        // When
        let route = resolve_route(harness, &on_meta("muse-wonder-1.0"))
            .unwrap_or_else(|refusal| panic!("{harness:?}: {refusal}"));

        // Then
        let id = CODEX_ROUTE_PROVIDER_ID;
        assert_eq!(
            route.args,
            vec![
                "-c".to_string(),
                format!("model_provider=\"{id}\""),
                "-c".to_string(),
                format!("model_providers.{id}.name=\"Meta\""),
                "-c".to_string(),
                format!("model_providers.{id}.base_url=\"https://api.meta.ai/v1\""),
                "-c".to_string(),
                format!("model_providers.{id}.env_key=\"{CODEX_ROUTE_KEY_ENV}\""),
                "-c".to_string(),
                format!("model_providers.{id}.wire_api=\"responses\""),
                "-m".to_string(),
                "muse-wonder-1.0".to_string(),
            ]
        );
        let secrets: Vec<(&str, &str)> = route
            .secret_env
            .iter()
            .map(|(k, v)| (k.as_str(), v.expose()))
            .collect();
        assert_eq!(secrets, vec![(CODEX_ROUTE_KEY_ENV, "a-meta-key")]);
    }
}

#[test]
fn codex_refuses_an_account_without_responses() {
    // Given
    let fireworks = RouteTarget {
        preset: "fireworks".to_string(),
        endpoints: RouteEndpoints {
            anthropic_messages: Some("https://api.fireworks.ai/inference".to_string()),
            openai_chat: Some("https://api.fireworks.ai/inference/v1".to_string()),
            openai_responses: None,
        },
        api_key: RouteSecret::new("k"),
        model: "kimi".to_string(),
    };

    // Then
    assert!(matches!(
        resolve_route(Harness::Codex, &fireworks),
        Err(RouteRefusal::ProtocolNotServed {
            harness: Harness::Codex,
            ..
        })
    ));
}
