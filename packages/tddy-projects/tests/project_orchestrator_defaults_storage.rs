//! Acceptance: a project row's orchestrator defaults (`orchestrator_defaults`) in `projects.yaml` —
//! what a new orchestrator for the project starts with on this host (`#orchestrators` 11/11). They
//! are saved, replaced and cleared through `set_project_orchestrator_defaults`, read back from the
//! file (so they survive a daemon restart), and a row that has none — an older daemon's, or one
//! whose defaults were cleared — reads as none and carries no key.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-10-project-settings-pane.md

use pretty_assertions::assert_eq;
use tddy_projects::project_storage::{self, OrchestratorDefaults, ProjectData};

const ALPHA: &str = "11111111-2222-4333-8444-555555555555";
const BETA: &str = "99999999-8888-4777-8666-555555555555";

/// A registry file holding `alpha` and `beta`, `alpha_extra` appended to alpha's keys.
fn a_registry_with_alpha_and_beta(alpha_extra: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {ALPHA}\n    name: alpha\n    git_url: https://github.com/acme/alpha.git\n    main_repo_path: /repos/alpha\n{alpha_extra}  - project_id: {BETA}\n    name: beta\n    git_url: https://github.com/acme/beta.git\n    main_repo_path: /repos/beta\n"
        ),
    )
    .unwrap();
    dir
}

/// A registry whose rows carry no defaults, as an older daemon wrote them.
fn a_registry_without_defaults() -> tempfile::TempDir {
    a_registry_with_alpha_and_beta("")
}

/// A registry whose `alpha` row has defaults saved for every control.
fn a_registry_with_alpha_defaults_saved() -> tempfile::TempDir {
    a_registry_with_alpha_and_beta(
        "    orchestrator_defaults:\n      model: opus\n      effort: high\n      provider_account_id: acct-meta\n",
    )
}

fn defaults(model: &str, effort: &str, provider_account_id: &str) -> OrchestratorDefaults {
    let some = |v: &str| (!v.is_empty()).then(|| v.to_string());
    OrchestratorDefaults {
        model: some(model),
        effort: some(effort),
        provider_account_id: some(provider_account_id),
    }
}

/// The row for `project_id`, read back from the file as a restarted daemon would.
fn stored_row(dir: &std::path::Path, project_id: &str) -> ProjectData {
    project_storage::find_project(dir, project_id)
        .expect("read registry")
        .expect("project row is present")
}

fn the_registry_file(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join("projects.yaml")).unwrap()
}

#[test]
fn orchestrator_defaults_are_saved_per_host_and_read_back() {
    // Given this host's registry, with no defaults saved
    let dir = a_registry_without_defaults();

    // When
    project_storage::set_project_orchestrator_defaults(
        dir.path(),
        ALPHA,
        Some(&defaults("sonnet", "high", "acct-meta")),
    )
    .expect("saving the defaults succeeds");

    // Then the row read back from the file carries them, and only that project's row does
    assert_eq!(
        stored_row(dir.path(), ALPHA).orchestrator_defaults,
        Some(defaults("sonnet", "high", "acct-meta"))
    );
    assert_eq!(stored_row(dir.path(), BETA).orchestrator_defaults, None);
}

#[test]
fn a_project_row_without_defaults_reads_as_none() {
    // Given a row whose defaults were saved
    let dir = a_registry_with_alpha_defaults_saved();

    // When they are cleared
    project_storage::set_project_orchestrator_defaults(dir.path(), ALPHA, None)
        .expect("clearing the defaults succeeds");

    // Then the row reads as having none, and the file carries no key for them
    assert_eq!(stored_row(dir.path(), ALPHA).orchestrator_defaults, None);
    let yaml = the_registry_file(dir.path());
    assert!(
        !yaml.contains("orchestrator_defaults"),
        "unexpected key in:\n{yaml}"
    );
}
