//! `set_project_orchestrator_defaults` — the setter's own contract (`#orchestrators` 11/11): a save
//! replaces, never merges; an all-absent value is none; an unknown project is an error that leaves
//! the registry alone; the row's other fields are kept.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-10-project-settings-pane.md

use pretty_assertions::assert_eq;
use tddy_projects::project_storage::{self, OrchestratorDefaults, ProjectData};

const ALPHA: &str = "11111111-2222-4333-8444-555555555555";
const UNKNOWN: &str = "00000000-0000-4000-8000-000000000000";
const MAIN_SESSION: &str = "main-0000-0000-0000-000000000000";

fn a_registry_with_alpha(extra: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {ALPHA}\n    name: alpha\n    git_url: https://github.com/acme/alpha.git\n    main_repo_path: /repos/alpha\n{extra}"
        ),
    )
    .unwrap();
    dir
}

fn only_model(model: &str) -> OrchestratorDefaults {
    OrchestratorDefaults {
        model: Some(model.to_string()),
        ..OrchestratorDefaults::default()
    }
}

fn every_control(model: &str, effort: &str, account: &str) -> OrchestratorDefaults {
    OrchestratorDefaults {
        model: Some(model.to_string()),
        effort: Some(effort.to_string()),
        provider_account_id: Some(account.to_string()),
    }
}

fn stored_row(dir: &std::path::Path) -> ProjectData {
    project_storage::find_project(dir, ALPHA)
        .expect("read registry")
        .expect("project row is present")
}

#[test]
fn a_second_save_replaces_the_first_rather_than_merging() {
    // Given defaults saved for every control
    let dir = a_registry_with_alpha("");
    project_storage::set_project_orchestrator_defaults(
        dir.path(),
        ALPHA,
        Some(&every_control("opus", "high", "acct-meta")),
    )
    .expect("the first save succeeds");

    // When only a model is saved
    project_storage::set_project_orchestrator_defaults(
        dir.path(),
        ALPHA,
        Some(&only_model("sonnet")),
    )
    .expect("the second save succeeds");

    // Then the effort and account are gone: they fall back to the agent's own
    assert_eq!(
        stored_row(dir.path()).orchestrator_defaults,
        Some(only_model("sonnet"))
    );
}

#[test]
fn defaults_with_every_control_absent_are_stored_as_none() {
    // Given a row whose defaults were saved
    let dir = a_registry_with_alpha("    orchestrator_defaults:\n      model: opus\n");

    // When an all-absent value is saved
    project_storage::set_project_orchestrator_defaults(
        dir.path(),
        ALPHA,
        Some(&OrchestratorDefaults::default()),
    )
    .expect("saving succeeds");

    // Then
    assert_eq!(stored_row(dir.path()).orchestrator_defaults, None);
}

#[test]
fn saving_keeps_the_rows_other_fields() {
    // Given a project whose main is recorded
    let dir = a_registry_with_alpha(&format!("    home_session_id: {MAIN_SESSION}\n"));

    // When
    project_storage::set_project_orchestrator_defaults(
        dir.path(),
        ALPHA,
        Some(&only_model("opus")),
    )
    .expect("saving succeeds");

    // Then
    let row = stored_row(dir.path());
    assert_eq!(row.home_session_id.as_deref(), Some(MAIN_SESSION));
    assert_eq!(row.orchestrator_defaults, Some(only_model("opus")));
}

#[test]
fn saving_on_an_unknown_project_is_an_error_naming_it() {
    // Given
    let dir = a_registry_with_alpha("");
    let before = std::fs::read_to_string(dir.path().join("projects.yaml")).unwrap();

    // When
    let outcome = project_storage::set_project_orchestrator_defaults(
        dir.path(),
        UNKNOWN,
        Some(&only_model("opus")),
    );

    // Then the error names the project, and the registry is untouched
    let error = outcome
        .expect_err("an unknown project is refused")
        .to_string();
    assert!(error.contains(UNKNOWN), "names the project: {error}");
    assert!(
        error.contains("unknown project"),
        "says the project is unknown: {error}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("projects.yaml")).unwrap(),
        before
    );
}

/// Regression guard, passes on the contract: an older daemon's row has no key, and reads as none.
#[test]
fn an_older_row_without_the_key_reads_as_none() {
    // Given
    let dir = a_registry_with_alpha("");

    // When
    let row = stored_row(dir.path());

    // Then
    assert_eq!(row.orchestrator_defaults, None);
}
