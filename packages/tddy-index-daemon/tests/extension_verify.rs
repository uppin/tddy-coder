//! The index daemon's `Verify` judges the changed files of an installed extension language by what
//! its language server and grammar know of them (`#reshape` extension-code-actions, R5), and carries
//! what it finds in `VerifyResponse.source_findings`.
//!
//! The host is built as production builds it — the installed extensions registered, and the same
//! languages handed to the service to verify with — over a data dir holding `tddy-zed-extensions`'
//! `demo-toml` fixture whose server is `tddy-lsp`'s deterministic `fake_lsp`, run with
//! `--diagnoses BROKEN`: every `BROKEN` in a document is an error diagnostic. No real language
//! server runs.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use serde_json::json;
use tddy_code_restructuring::backends::rust::RustSupport;
use tddy_code_restructuring::SupportRegistry;
use tddy_index_daemon::proto::code_index::{CodeIndexService, VerifyRequest, VerifyResponse};
use tddy_index_daemon::{CodeIndexPorts, CodeIndexServiceImpl};
use tddy_lsp::LspRegistry;
use tddy_task::TaskRegistry;
use tddy_zed_extensions::{load_installed, testkit, ExtensionsDir};

const APP: &str = "config/app.toml";
const APP_TOML: &str = "[server]\nhost = \"localhost\"\nport = 8080\n";

/// A tddy data dir with `demo-toml` installed, `fake-ls` declared as its language's server and
/// `settings.json` running the fake server with `--diagnoses BROKEN`.
fn a_data_dir_with_a_diagnosing_extension_language() -> tempfile::TempDir {
    let data = tempfile::tempdir().expect("a temporary data dir");
    testkit::install_demo_toml(data.path());
    testkit::declare_in_demo_toml(
        data.path(),
        "[language_servers.fake-ls]\nlanguages = [\"Demo TOML\"]\n",
    );
    let settings = json!({ "lsp": { "fake-ls": { "binary": {
        "path": env!("CARGO_BIN_EXE_fake_lsp"),
        "arguments": ["--diagnoses", "BROKEN"],
    } } } });
    std::fs::write(
        data.path().join("extensions/settings.json"),
        settings.to_string(),
    )
    .expect("write the settings");
    data
}

/// A committed git worktree holding [`APP`] as [`APP_TOML`] and no Cargo package.
fn a_committed_worktree_holding_the_app_config() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    testkit::commit_baseline(workspace.path(), &[(APP, APP_TOML)]);
    workspace
}

/// The service a host builds over the extensions installed under `data`.
fn a_service_over(data: &Path) -> CodeIndexServiceImpl {
    let loaded = load_installed(&ExtensionsDir::under_data_dir(data)).expect("the extensions load");
    assert!(
        loaded.failures.is_empty(),
        "a grammar failed to load: {:?}",
        loaded.failures
    );
    let mut supports =
        SupportRegistry::with_builtin_rust(RustSupport::launching(env!("CARGO_BIN_EXE_fake_lsp")));
    for support in &loaded.supports {
        supports.register(Box::new(support.clone()));
    }
    let servers = LspRegistry::new(
        supports.allow_list(),
        TaskRegistry::new(),
        Duration::from_secs(60),
    );
    CodeIndexServiceImpl::with_supports(
        CodeIndexPorts {
            servers,
            wait_heartbeat: tddy_code_restructuring::backends::rust::WAIT_HEARTBEAT,
        },
        Arc::new(supports),
    )
    .with_verified_languages(loaded.supports)
}

/// `Verify` against `HEAD` of `workspace`, answered by `service`.
async fn verifying(service: &CodeIndexServiceImpl, workspace: &Path) -> VerifyResponse {
    service
        .verify(tddy_rpc::Request::direct(VerifyRequest {
            workspace_root: workspace.to_string_lossy().into_owned(),
            against: "HEAD".to_string(),
            ..Default::default()
        }))
        .await
        .expect("the tree is verified")
        .into_inner()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_error_diagnostic_in_an_extension_file_is_a_finding_and_the_verify_does_not_hold() {
    // Given a committed TOML file the extension's server is happy with, changed to a value it
    // reports as an error though it parses
    let data = a_data_dir_with_a_diagnosing_extension_language();
    let workspace = a_committed_worktree_holding_the_app_config();
    std::fs::write(
        workspace.path().join(APP),
        APP_TOML.replace("8080", "\"BROKEN\""),
    )
    .expect("change the app config");
    let service = a_service_over(data.path());

    // When the daemon verifies the tree against its baseline
    let answer = verifying(&service, workspace.path()).await;

    // Then the new error is the one finding, and the verify does not hold
    assert_eq!(
        (answer.holds, answer.source_findings),
        (
            false,
            vec![format!(
                "finding: {APP}:3:9: new error diagnostic: unexpected BROKEN"
            )]
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_clean_change_to_an_extension_file_has_no_findings_and_holds() {
    // Given a committed TOML file changed to another value the server has no complaint about
    let data = a_data_dir_with_a_diagnosing_extension_language();
    let workspace = a_committed_worktree_holding_the_app_config();
    std::fs::write(workspace.path().join(APP), APP_TOML.replace("8080", "9090"))
        .expect("change the app config");
    let service = a_service_over(data.path());

    // When the daemon verifies the tree against its baseline
    let answer = verifying(&service, workspace.path()).await;

    // Then nothing is found and the verify holds
    assert_eq!((answer.holds, answer.source_findings), (true, Vec::new()));
}
