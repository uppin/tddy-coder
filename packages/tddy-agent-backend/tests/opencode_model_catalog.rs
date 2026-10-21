//! Integration tests: the `opencode-cli` pseudo-agent's model catalogue is what the OpenCode binary
//! itself lists (`opencode models`), enumerated through the same `list-models` contract the other
//! agents use.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-opencode-cli.md

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tddy_agent_backend::backend::model_catalog::{resolve_agent_models, BackendCliPaths};

/// A stand-in `opencode` that lists two models when asked for `models`, and fails otherwise.
fn a_stub_opencode_listing(dir: &Path, listing: &str) -> PathBuf {
    let path = dir.join("stub_opencode.sh");
    std::fs::write(
        &path,
        format!("#!/bin/sh\nif [ \"$1\" = \"models\" ]; then\ncat <<'EOF'\n{listing}\nEOF\nexit 0\nfi\nexit 64\n"),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn paths_with_opencode(binary: PathBuf) -> BackendCliPaths {
    BackendCliPaths {
        opencode_cli_path: Some(binary),
        ..BackendCliPaths::default()
    }
}

#[tokio::test]
async fn the_opencode_cli_catalogue_is_what_opencode_lists() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let binary =
        a_stub_opencode_listing(dir.path(), "fireworks/kimi-k2\nanthropic/claude-sonnet-4\n");

    // When
    let catalogue = resolve_agent_models("opencode-cli", &paths_with_opencode(binary))
        .await
        .expect("the listing is read");

    // Then
    let ids: Vec<&str> = catalogue.models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["fireworks/kimi-k2", "anthropic/claude-sonnet-4"]);
    assert_eq!(catalogue.default_model, "fireworks/kimi-k2");
}

#[tokio::test]
async fn an_opencode_that_cannot_list_its_models_fails_the_probe() {
    // Given a binary that exits non-zero for `models`
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("broken_opencode.sh");
    std::fs::write(
        &binary,
        "#!/bin/sh\necho 'no providers configured' >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();

    // When
    let outcome = resolve_agent_models("opencode-cli", &paths_with_opencode(binary)).await;

    // Then a failed listing is an error, never an empty catalogue
    assert!(outcome.is_err(), "{outcome:?}");
}
