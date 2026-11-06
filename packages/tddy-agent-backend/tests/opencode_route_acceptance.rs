//! The OpenCode route: a generated `opencode.json` that declares the account as a provider and
//! selects the model on it, with the key read from the environment rather than written to the file.
//!
//! Feature: docs/ft/daemon/opencode-cli-session.md

mod route_agents;

use route_agents::opencode_named;
use serde_json::Value;
use tddy_agent_backend::provider_route::{
    opencode_config_for, resolve_route, RouteEndpoints, RouteSecret, RouteTarget,
    OPENCODE_ROUTE_PROVIDER_ID,
};

/// The variable OpenCode's spec (`route.key_env`) hands the key in: the agent process sees no
/// `TDDY_*` route variable.
const OPENCODE_KEY_ENV: &str = "OPENCODE_PROVIDER_KEY";

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
    let file = opencode_config_for(target, OPENCODE_KEY_ENV)
        .expect("an account that serves an OpenAI protocol");
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
        format!("{{env:{OPENCODE_KEY_ENV}}}")
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
    let file = opencode_config_for(&on_meta("muse-spark-1.3"), OPENCODE_KEY_ENV)
        .expect("Meta serves Responses");

    // Then
    assert!(!file.contents.contains("a-meta-key"));
}

#[test]
fn an_opencode_route_carries_the_config_the_key_and_the_model_argument() {
    for harness in ["opencode-cli", "opencode"].map(opencode_named) {
        // When
        let route = resolve_route(&harness, &on_meta("muse-wonder-1.0"))
            .unwrap_or_else(|refusal| panic!("{}: {refusal}", harness.id));

        // Then
        assert_eq!(
            route.files,
            vec![
                opencode_config_for(&on_meta("muse-wonder-1.0"), OPENCODE_KEY_ENV)
                    .expect("Meta serves Responses")
            ]
        );
        let secrets: Vec<(&str, &str)> = route
            .secret_env
            .iter()
            .map(|(k, v)| (k.as_str(), v.expose()))
            .collect();
        assert_eq!(secrets, vec![(OPENCODE_KEY_ENV, "a-meta-key")]);
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
    assert!(resolve_route(&opencode_named("opencode-cli"), &anthropic_only).is_err());
}

#[test]
fn no_config_is_generated_for_an_account_that_serves_neither_openai_protocol() {
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
    assert_eq!(opencode_config_for(&anthropic_only, OPENCODE_KEY_ENV), None);
}
