//! Muse Code as a CLI harness: registered, launched with its model, resumed, and routed only on a
//! Meta account with the key in the variable Muse Code reads.
//!
//! Feature: docs/ft/daemon/muse-cli-session.md

use tddy_agent_backend::cli_harness::{cli_harness, CliLaunch};
use tddy_agent_backend::cli_harness_muse::{muse_cli_models, MUSE_ROUTE_KEY_ENV};
use tddy_agent_backend::provider_route::{
    resolve_route, Harness, RouteEndpoints, RouteRefusal, RouteSecret, RouteTarget,
};

fn an_account(preset: &str) -> RouteTarget {
    RouteTarget {
        preset: preset.to_string(),
        endpoints: RouteEndpoints {
            anthropic_messages: Some("https://example".to_string()),
            openai_responses: Some("https://example/v1".to_string()),
            openai_chat: Some("https://example/v1".to_string()),
        },
        api_key: RouteSecret::new("a-key"),
        model: "muse-wonder-1.0".to_string(),
    }
}

#[test]
fn muse_code_is_registered_as_a_cli_harness() {
    // When
    let muse = cli_harness("muse-cli").expect("muse-cli is registered");

    // Then
    assert_eq!(muse.label, "Muse Code");
    assert_eq!(muse.route_harness, Harness::MuseCli);
    assert_eq!(muse.default_binary, "muse");
}

#[test]
fn a_muse_session_starts_with_its_model_and_first_prompt() {
    // When
    let argv = (cli_harness("muse-cli").expect("registered").argv)(&CliLaunch {
        binary: "muse",
        model: "muse-wonder-1.0",
        session_id: "s",
        initial_prompt: Some("plan the refactor"),
        ..CliLaunch::default()
    });

    // Then
    assert_eq!(
        argv,
        vec!["muse", "--model", "muse-wonder-1.0", "plan the refactor"]
    );
}

#[test]
fn a_resumed_muse_session_continues_its_session() {
    // When
    let argv = (cli_harness("muse-cli").expect("registered").argv)(&CliLaunch {
        binary: "muse",
        resume: true,
        ..CliLaunch::default()
    });

    // Then
    assert_eq!(argv, vec!["muse", "--continue"]);
}

#[test]
fn muse_on_meta_is_handed_the_key_and_nothing_else() {
    // When
    let route = resolve_route(Harness::MuseCli, &an_account("meta")).expect("routes on Meta");

    // Then
    let secrets: Vec<(&str, &str)> = route
        .secret_env
        .iter()
        .map(|(k, v)| (k.as_str(), v.expose()))
        .collect();
    assert_eq!(secrets, vec![(MUSE_ROUTE_KEY_ENV, "a-key")]);
    assert!(route.env.is_empty());
    assert!(route.files.is_empty());
}

#[test]
fn muse_accepts_only_meta_accounts() {
    // Then
    assert_eq!(
        resolve_route(Harness::MuseCli, &an_account("fireworks")).expect_err("refused"),
        RouteRefusal::PresetNotAccepted {
            harness: Harness::MuseCli,
            preset: "fireworks".to_string()
        }
    );
}

#[test]
fn the_curated_muse_models_default_to_spark() {
    // When
    let catalog = muse_cli_models();

    // Then
    let ids: Vec<&str> = catalog.models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["muse-spark-1.3", "muse-wonder-1.0"]);
    assert_eq!(catalog.default_model, "muse-spark-1.3");
}
