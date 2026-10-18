//! The OpenCode route: a generated `opencode.json` that declares the account as a provider and
//! selects the model on it, with the key read from the environment rather than written to the file.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-opencode-cli.md

use serde_json::Value;
use tddy_agent_backend::provider_route::{
    opencode_config_for, resolve_route, Harness, RouteEndpoints, RouteSecret, RouteTarget,
    OPENCODE_ROUTE_KEY_ENV, OPENCODE_ROUTE_PROVIDER_ID,
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

fn on_baseten(model: &str) -> RouteTarget {
    RouteTarget {
        preset: "baseten".to_string(),
        endpoints: RouteEndpoints {
            openai_chat: Some("https://inference.baseten.co/v1".to_string()),
            ..RouteEndpoints::default()
        },
        api_key: RouteSecret::new("a-baseten-key"),
        model: model.to_string(),
    }
}

fn config_of(target: &RouteTarget) -> Value {
    let file = opencode_config_for(target);
    assert_eq!(file.name, "opencode.json");
    serde_json::from_str(&file.contents).expect("valid JSON")
}

#[test]
fn opencode_on_meta_prefers_responses_and_carries_reasoning() {
    // When
    let config = config_of(&on_meta("muse-spark-1.3"));

    // Then
    let provider = &config["provider"][OPENCODE_ROUTE_PROVIDER_ID];
    assert_eq!(provider["npm"], "@ai-sdk/openai");
    assert_eq!(provider["options"]["baseURL"], "https://api.meta.ai/v1");
    assert_eq!(
        provider["options"]["apiKey"],
        format!("{{env:{OPENCODE_ROUTE_KEY_ENV}}}")
    );
    let model = &provider["models"]["muse-spark-1.3"];
    assert_eq!(
        model["options"]["include"][0],
        "reasoning.encrypted_content"
    );
    assert_eq!(
        config["model"],
        format!("{OPENCODE_ROUTE_PROVIDER_ID}/muse-spark-1.3")
    );
}

#[test]
fn opencode_on_baseten_uses_chat_completions() {
    // When
    let config = config_of(&on_baseten("moonshotai/Kimi-K2"));

    // Then
    let provider = &config["provider"][OPENCODE_ROUTE_PROVIDER_ID];
    assert_eq!(provider["npm"], "@ai-sdk/openai-compatible");
    assert_eq!(
        provider["options"]["baseURL"],
        "https://inference.baseten.co/v1"
    );
}

#[test]
fn the_generated_config_holds_no_key() {
    // When
    let file = opencode_config_for(&on_meta("muse-spark-1.3"));

    // Then
    assert!(!file.contents.contains("a-meta-key"));
}

#[test]
fn an_opencode_route_carries_the_config_the_key_and_the_model_argument() {
    for harness in [Harness::OpenCodeCli, Harness::OpenCode] {
        // When
        let route = resolve_route(harness, &on_meta("muse-wonder-1.0"))
            .unwrap_or_else(|refusal| panic!("{harness:?}: {refusal}"));

        // Then
        assert_eq!(
            route.files,
            vec![opencode_config_for(&on_meta("muse-wonder-1.0"))]
        );
        let secrets: Vec<(&str, &str)> = route
            .secret_env
            .iter()
            .map(|(k, v)| (k.as_str(), v.expose()))
            .collect();
        assert_eq!(secrets, vec![(OPENCODE_ROUTE_KEY_ENV, "a-meta-key")]);
        assert_eq!(
            route.args,
            vec![
                "--model".to_string(),
                format!("{OPENCODE_ROUTE_PROVIDER_ID}/muse-wonder-1.0")
            ]
        );
    }
}

#[test]
fn opencode_refuses_an_account_that_serves_neither_openai_protocol() {
    // Given
    let anthropic_only = RouteTarget {
        preset: "anthropic-compatible".to_string(),
        endpoints: RouteEndpoints {
            anthropic_messages: Some("https://llm.example.com".to_string()),
            ..RouteEndpoints::default()
        },
        api_key: RouteSecret::new("k"),
        model: "m".to_string(),
    };

    // Then
    assert!(resolve_route(Harness::OpenCodeCli, &anthropic_only).is_err());
}
