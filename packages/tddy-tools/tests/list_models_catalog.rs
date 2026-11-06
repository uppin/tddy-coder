//! The model catalogue `list-models` answers from the catalogue the binary injected: an
//! agent whose spec lists its models through its binary is enumerated by running it; an
//! agent with only a curated list answers from that list. No agent is named in the library.
//!
//! Feature: docs/ft/daemon/opencode-cli-session.md

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tddy_core::backend::BackendModels;
use tddy_tools::list_models::{list_agent_models, AgentModels, AgentModelsSource};

/// A stand-in binary that lists two models when asked for `models`, and fails otherwise.
fn a_stub_listing(dir: &Path, listing: &str) -> PathBuf {
    let path = dir.join("stub_agent.sh");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"models\" ]; then\ncat <<'EOF'\n{listing}\nEOF\nexit 0\nfi\nexit 64\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn a_listed_catalog(binary: PathBuf) -> Vec<AgentModels> {
    vec![AgentModels {
        names: vec!["acme".to_string(), "acme-cli".to_string()],
        source: AgentModelsSource::Listed {
            argv: vec![binary.to_string_lossy().into_owned(), "models".to_string()],
            adapter: "opencode-models".to_string(),
        },
    }]
}

#[tokio::test]
async fn a_listed_agents_catalogue_is_what_its_binary_lists() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let binary = a_stub_listing(dir.path(), "fireworks/kimi-k2\nanthropic/claude-sonnet-4\n");

    // When — asked by its session type, as the daemon's probe asks
    let catalogue: BackendModels = list_agent_models("acme-cli", None, &a_listed_catalog(binary))
        .await
        .expect("the listing is read");

    // Then
    let ids: Vec<&str> = catalogue.models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["fireworks/kimi-k2", "anthropic/claude-sonnet-4"]);
    assert_eq!(catalogue.default_model, "fireworks/kimi-k2");
}

#[tokio::test]
async fn a_binary_that_cannot_list_its_models_fails_the_probe() {
    // Given a binary that exits non-zero for `models`
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("broken.sh");
    std::fs::write(
        &binary,
        "#!/bin/sh\necho 'no providers configured' >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();

    // When
    let outcome = list_agent_models("acme", None, &a_listed_catalog(binary)).await;

    // Then a failed listing is an error, never an empty catalogue
    assert!(outcome.is_err(), "{outcome:?}");
}

#[tokio::test]
async fn a_curated_agents_catalogue_is_its_spec_list() {
    // Given
    let catalog = vec![AgentModels {
        names: vec!["acme".to_string()],
        source: AgentModelsSource::Curated {
            models: vec![
                ("acme-1".to_string(), "Acme 1".to_string()),
                ("acme-2".to_string(), "Acme 2".to_string()),
            ],
            default_model: "acme-1".to_string(),
        },
    }];

    // When
    let catalogue = list_agent_models("acme", None, &catalog)
        .await
        .expect("the curated list is read");

    // Then
    assert_eq!(catalogue.default_model, "acme-1");
    assert_eq!(catalogue.models[1].label, "Acme 2");
}
