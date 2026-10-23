//! A server's `workspace/configuration` request is answered from its launch's settings
//! (`#reshape` extension-language-server, R4), against the deterministic `fake_lsp` started with
//! `--requests-configuration`, which asks for one section on `initialized` and replays the answer
//! through `tddy/configurationAnswers`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tddy_lsp::{LanguageId, LaunchSpec, LspAllowList, LspKey, LspRegistry, LspService};
use tddy_task::TaskRegistry;

/// A language an extension brings, as the allow-list keys it.
fn an_extension_language() -> LanguageId {
    LanguageId::new("demo-toml")
}

/// The fake server, asking for `section` once initialized, launched with `settings`.
fn a_launch_asking_for(section: &str, settings: Value) -> LaunchSpec {
    let mut spec =
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp")).with_workspace_configuration(settings);
    spec.args = vec!["--requests-configuration".to_string(), section.to_string()];
    spec
}

async fn a_server_started_from(spec: LaunchSpec) -> Arc<LspService> {
    let mut allow = LspAllowList::new();
    allow.allow(an_extension_language(), spec);
    let registry = LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60));
    registry
        .get_or_spawn(LspKey {
            root: PathBuf::from("/workspace"),
            language: an_extension_language(),
        })
        .await
        .expect("the fake server starts")
}

/// Every answer the client gave the server's `workspace/configuration` request.
async fn what_the_client_answered(service: &LspService) -> Value {
    service
        .client
        .request_raw("tddy/configurationAnswers", Value::Null)
        .await
        .expect("the fake server replays the configuration answers")
}

#[tokio::test]
async fn the_servers_configuration_request_is_answered_from_the_launch_settings() {
    // Given a server launched with settings, which asks for `fake.mode` once initialized
    let service = a_server_started_from(a_launch_asking_for(
        "fake.mode",
        json!({ "fake": { "mode": "strict" } }),
    ))
    .await;

    // When its configuration request has been answered
    let answers = what_the_client_answered(&service).await;

    // Then the one item was answered with the value at its section path
    assert_eq!(answers, json!([["strict"]]));
}

#[tokio::test]
async fn a_launch_without_settings_answers_null() {
    // Given a server launched with no settings — as Rust's is — which asks for `fake.mode`
    let service = a_server_started_from(a_launch_asking_for("fake.mode", Value::Null)).await;

    // When its configuration request has been answered
    let answers = what_the_client_answered(&service).await;

    // Then the one item was answered null, as every item was before settings existed
    assert_eq!(answers, json!([[null]]));
}
