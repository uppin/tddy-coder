//! Acceptance: an extension language's server starts from tddy's settings and gives the language
//! `rename_symbol` (`#reshape` extension-language-server, PRD acceptance criteria; changeset
//! R1–R6).
//!
//! The fixture is `demo-toml` (the vendored tree-sitter-toml grammar, `extension-outline`'s) with
//! `[language_servers.fake-ls]` declared for its language, and `<extensions dir>/settings.json`
//! pointing `lsp.fake-ls.binary.path` at `tddy-lsp`'s deterministic `fake_lsp` — never a real
//! language server. Supports come from `load_installed`, the function every front end registers
//! them through, so the settings reach a support the way they reach it in production.

mod outline_support;
mod server_support;

use std::fs;

use outline_support::{
    a_workspace_holding_the_app_config, an_extensions_dir_with, the_fingerprint_of_lines,
    AFixtureExtension, APP, APP_TOML,
};
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use server_support::{
    applied_cold, committed_as_the_baseline, resolved_and_applied, the_fake_server, the_server_of,
};
use tddy_code_restructuring::{LanguageSupport, RefactorOp, RestructureError};

/// `rename_symbol` of `config/app.toml::server::port` — line 3 of [`APP_TOML`] — to `new_name`,
/// anchored by item as `restructure anchors` writes it.
fn a_rename_of_the_port_key_to(new_name: &str) -> RefactorOp {
    serde_json::from_value(json!({
        "op": "rename_symbol",
        "anchor": {
            "kind": "item",
            "file": APP,
            "item": format!("{APP}::server::port"),
            "fingerprint": the_fingerprint_of_lines(3, 3),
        },
        "name": new_name,
    }))
    .expect("the rename parses")
}

/// Settings giving `fake-ls` the fake server as its binary, with `arguments` and `extra` fields of
/// the `lsp.fake-ls` entry.
fn settings_running_the_fake_server_with(arguments: &[&str], extra: Value) -> Value {
    let mut server = json!({
        "binary": { "path": the_fake_server(), "arguments": arguments },
    });
    if let (Some(entry), Some(extra)) = (server.as_object_mut(), extra.as_object()) {
        entry.extend(extra.clone());
    }
    json!({ "lsp": { "fake-ls": server } })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_configured_server_renames_an_extension_symbol() {
    // Given demo-toml with fake-ls declared for its language, configured to run the fake server,
    // which answers a rename with an edit of the identifier under the position
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);
    installed.declare_fake_ls_for_demo_toml();
    installed.write_settings(settings_running_the_fake_server_with(
        &["--answers-rename"],
        json!({}),
    ));
    let workspace = a_workspace_holding_the_app_config();
    let support = installed.the_demo_toml_support();
    let server = the_server_of(&support, workspace.root()).await;

    // When `port` under `[server]` is renamed to `listen_port` and the run applies the resolution
    let renamed = resolved_and_applied(
        support,
        Some(server),
        workspace.root(),
        a_rename_of_the_port_key_to("listen_port"),
    )
    .await;

    // Then the server's edit is on disk: the key renamed where the outline names it, nothing else
    // touched (R6)
    assert!(renamed.is_ok(), "the rename was refused: {renamed:?}");
    assert_eq!(
        fs::read_to_string(workspace.root().join(APP)).expect("read the app config"),
        APP_TOML.replace("port = 8080", "listen_port = 8080")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cold_apply_renames_an_extension_symbol_through_the_server_started_for_it() {
    // Given demo-toml's configured server answering renames, started for a committed workspace
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);
    installed.declare_fake_ls_for_demo_toml();
    installed.write_settings(settings_running_the_fake_server_with(
        &["--answers-rename"],
        json!({}),
    ));
    let workspace = a_workspace_holding_the_app_config();
    committed_as_the_baseline(workspace.root());
    let support = installed.the_demo_toml_support();
    let server = the_server_of(&support, workspace.root()).await;

    // When a plan renaming `port` is applied as `restructure apply` runs it, the server's client
    // handed to the run
    let run = applied_cold(
        support,
        server,
        workspace.root(),
        &a_rename_of_the_port_key_to("listen_port"),
    )
    .await;

    // Then the run applied the one operation, through the extension's server (R6)
    assert_eq!(
        run.map(|summary| summary.applied)
            .map_err(|e| e.to_string()),
        Ok(1)
    );
    assert_eq!(
        fs::read_to_string(workspace.root().join(APP)).expect("read the app config"),
        APP_TOML.replace("port = 8080", "listen_port = 8080")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_server_receives_the_initialization_options() {
    // Given fake-ls configured with initialization options
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);
    installed.declare_fake_ls_for_demo_toml();
    installed.write_settings(settings_running_the_fake_server_with(
        &[],
        json!({ "initialization_options": { "formatter": { "indent": 2 } } }),
    ));
    let workspace = a_workspace_holding_the_app_config();
    let support = installed.the_demo_toml_support();

    // When its server is started from the support's launch
    let server = the_server_of(&support, workspace.root()).await;
    let handshake = server
        .client
        .request_raw("tddy/initializeParams", Value::Null)
        .await
        .expect("the fake server replays its initialize params");

    // Then the handshake carried them as `initializationOptions` (R4)
    assert_eq!(
        handshake.get("initializationOptions"),
        Some(&json!({ "formatter": { "indent": 2 } }))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn workspace_configuration_is_answered_from_settings() {
    // Given fake-ls configured with settings, running a server that asks for `fake.mode` once
    // initialized
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);
    installed.declare_fake_ls_for_demo_toml();
    installed.write_settings(settings_running_the_fake_server_with(
        &["--requests-configuration", "fake.mode"],
        json!({ "settings": { "fake": { "mode": "strict" } } }),
    ));
    let workspace = a_workspace_holding_the_app_config();
    let support = installed.the_demo_toml_support();

    // When its server is started and its configuration request answered
    let server = the_server_of(&support, workspace.root()).await;
    let answers = server
        .client
        .request_raw("tddy/configurationAnswers", Value::Null)
        .await
        .expect("the fake server replays what the client answered");

    // Then the item was answered from the settings, by its section path (R4)
    assert_eq!(answers, json!([["strict"]]));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unconfigured_server_is_refused_before_anything_starts() {
    // Given fake-ls declared for demo-toml's language, and no settings file at all
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);
    installed.declare_fake_ls_for_demo_toml();
    let workspace = a_workspace_holding_the_app_config();
    let support = installed.the_demo_toml_support();
    let launch = support.launch();

    // When a rename is asked of a file of it, with no server — there is nothing to start
    let refused = resolved_and_applied(
        support,
        None,
        workspace.root(),
        a_rename_of_the_port_key_to("listen_port"),
    )
    .await;

    // Then the support offers no launch, and the rename is refused naming the server, its
    // extension and the setting to write — no command is guessed (R2, R3)
    match refused {
        Err(RestructureError::ServerNotConfigured { reason, .. }) => assert_eq!(
            (launch, reason),
            (
                None,
                format!(
                    "language server fake-ls (extension demo-toml) has no command: set \
                     lsp.fake-ls.binary.path in {}",
                    installed.settings_file()
                )
            )
        ),
        other => panic!("expected the unconfigured server to be refused, got {other:?}"),
    }
}
