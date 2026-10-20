//! When an extension language's server counts as ready (`#reshape` extension-language-server, the
//! readiness rule of 2026-10-20): a server that sends no `experimental/serverStatus` — `tddy-lsp`'s
//! deterministic `fake_lsp` with no load flags — reads `READY` once `initialize` has completed,
//! while Rust's rule still reads such a server `LOADING`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tddy_index_daemon::proto::code_index::IndexState;
use tddy_index_daemon::readiness::{index_state_of, Readiness};
use tddy_lsp::{LanguageId, LaunchSpec, LspAllowList, LspKey, LspRegistry, LspService};
use tddy_task::TaskRegistry;

fn an_extension_language() -> LanguageId {
    LanguageId::new("demo-toml")
}

/// The fake server as an extension language's launch starts it: no load flags, so it never sends
/// `experimental/serverStatus`, and no request for one.
fn a_launch_without_server_status() -> LaunchSpec {
    LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp")).with_capabilities(json!({
        "general": { "positionEncodings": ["utf-8"] }
    }))
}

/// `spec`'s server, its `initialize` completed.
async fn an_initialized_server(spec: LaunchSpec) -> Arc<LspService> {
    let mut allow = LspAllowList::new();
    allow.allow(an_extension_language(), spec);
    let registry = LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60));
    registry
        .get_or_spawn(LspKey {
            root: PathBuf::from("/workspace"),
            language: an_extension_language(),
        })
        .await
        .expect("the fake server starts and initializes")
}

#[tokio::test]
async fn a_server_without_server_status_reads_ready_after_initialize() {
    // Given an initialized server that has sent no `experimental/serverStatus`
    let service = an_initialized_server(a_launch_without_server_status()).await;

    // When its index state is read by the rule for servers not asked for one
    let state = index_state_of(&service.client, Readiness::AfterInitialize);

    // Then it is ready
    assert_eq!(state, IndexState::Ready);
}

#[tokio::test]
async fn rusts_rule_still_reads_a_silent_server_loading() {
    // Given the same initialized, silent server
    let service = an_initialized_server(a_launch_without_server_status()).await;

    // When its index state is read by Rust's rule
    let state = index_state_of(&service.client, Readiness::ServerStatus);

    // Then it is loading, as before the rule existed: rust-analyzer has not said it is quiescent
    assert_eq!(state, IndexState::Loading);
}
