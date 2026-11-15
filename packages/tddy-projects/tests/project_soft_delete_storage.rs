//! A project row's soft-delete tombstone in `projects.yaml`: how it is written, read back, restored
//! and reconciled between hosts — and that nothing but the row changes.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Deleting a project.
//! Changeset: docs/dev/1-WIP/2026-11-15-project-update-and-soft-delete.md

use pretty_assertions::assert_eq;
use tddy_projects::project_storage::{self, ProjectData, ProjectLifecycle};

const ALPHA: &str = "11111111-2222-4333-8444-555555555555";
const UNKNOWN: &str = "00000000-0000-4000-8000-000000000000";

const EARLY: &str = "2026-11-01T10:00:00.000Z";
const LATER: &str = "2026-11-02T10:00:00.000Z";
const LATEST: &str = "2026-11-03T10:00:00.000Z";

fn a_registry_with_alpha(extra: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {ALPHA}\n    name: alpha\n    git_url: https://github.com/acme/alpha.git\n    main_repo_path: /repos/alpha\n    home_session_id: main-1\n{extra}"
        ),
    )
    .unwrap();
    dir
}

fn stored_row(dir: &std::path::Path) -> ProjectData {
    project_storage::find_project(dir, ALPHA)
        .expect("read registry")
        .expect("project row is present")
}

fn deleted(at: &str, by: &str) -> ProjectLifecycle {
    ProjectLifecycle {
        deleted_at: Some(at.to_string()),
        deleted_by: Some(by.to_string()),
        restored_at: None,
    }
}

fn restored(at: &str) -> ProjectLifecycle {
    ProjectLifecycle {
        restored_at: Some(at.to_string()),
        ..ProjectLifecycle::default()
    }
}

#[test]
fn a_row_written_before_soft_delete_existed_reads_as_live() {
    // Given a row an older daemon wrote, with no tombstone keys
    let dir = a_registry_with_alpha("");

    // Then it reads as a live project with no lifecycle events
    let row = stored_row(dir.path());
    assert!(!row.is_deleted());
    assert_eq!(row.lifecycle, ProjectLifecycle::default());
}

#[test]
fn a_live_row_writes_no_tombstone_keys() {
    // Given a live row rewritten by this daemon
    let dir = a_registry_with_alpha("");
    project_storage::rename_project(dir.path(), ALPHA, "alpha-renamed").unwrap();

    // Then the file carries no lifecycle keys an older daemon would trip over
    let yaml = std::fs::read_to_string(dir.path().join("projects.yaml")).unwrap();
    assert!(!yaml.contains("deleted_at"), "{yaml}");
    assert!(!yaml.contains("deleted_by"), "{yaml}");
    assert!(!yaml.contains("restored_at"), "{yaml}");
}

#[test]
fn deleting_a_project_tombstones_its_row_at_the_row_level_and_keeps_every_other_field() {
    // Given a live project with a main session recorded
    let dir = a_registry_with_alpha("");
    let before = stored_row(dir.path());

    // When it is deleted
    let row = project_storage::mark_project_deleted(dir.path(), ALPHA, EARLY, "octocat")
        .unwrap()
        .expect("the row exists");

    // Then the row carries the tombstone and nothing else changed
    assert!(row.is_deleted());
    assert_eq!(row.lifecycle, deleted(EARLY, "octocat"));
    assert_eq!(
        ProjectData {
            lifecycle: ProjectLifecycle::default(),
            ..stored_row(dir.path())
        },
        before
    );
    // And the keys sit on the row itself, beside `name`
    let yaml = std::fs::read_to_string(dir.path().join("projects.yaml")).unwrap();
    assert!(yaml.contains(&format!("    deleted_at: '{EARLY}'")) || yaml.contains(&format!("    deleted_at: {EARLY}")), "{yaml}");
    assert!(yaml.contains("    deleted_by: octocat"), "{yaml}");
}

#[test]
fn deleting_an_already_deleted_project_keeps_the_first_tombstone() {
    // Given a project deleted earlier by someone else
    let dir = a_registry_with_alpha("");
    project_storage::mark_project_deleted(dir.path(), ALPHA, EARLY, "first").unwrap();

    // When it is deleted again
    project_storage::mark_project_deleted(dir.path(), ALPHA, LATER, "second").unwrap();

    // Then the original tombstone stands
    assert_eq!(stored_row(dir.path()).lifecycle, deleted(EARLY, "first"));
}

#[test]
fn deleting_an_unknown_project_reports_no_row_and_writes_nothing() {
    let dir = a_registry_with_alpha("");
    let before = std::fs::read_to_string(dir.path().join("projects.yaml")).unwrap();

    let outcome = project_storage::mark_project_deleted(dir.path(), UNKNOWN, EARLY, "octocat").unwrap();

    assert_eq!(outcome, None);
    assert_eq!(std::fs::read_to_string(dir.path().join("projects.yaml")).unwrap(), before);
}

#[test]
fn restoring_clears_the_tombstone_and_records_when() {
    // Given a deleted project
    let dir = a_registry_with_alpha("");
    project_storage::mark_project_deleted(dir.path(), ALPHA, EARLY, "octocat").unwrap();

    // When it is restored
    let row = project_storage::mark_project_restored(dir.path(), ALPHA, LATER)
        .unwrap()
        .expect("the row exists");

    // Then it is live again, and the restore is recorded
    assert!(!row.is_deleted());
    assert_eq!(stored_row(dir.path()).lifecycle, restored(LATER));
    assert_eq!(stored_row(dir.path()).home_session_id.as_deref(), Some("main-1"));
}

#[test]
fn restoring_a_live_project_changes_nothing() {
    let dir = a_registry_with_alpha("");
    let before = std::fs::read_to_string(dir.path().join("projects.yaml")).unwrap();

    let row = project_storage::mark_project_restored(dir.path(), ALPHA, LATER)
        .unwrap()
        .expect("the row exists");

    assert!(!row.is_deleted());
    assert_eq!(std::fs::read_to_string(dir.path().join("projects.yaml")).unwrap(), before);
}

#[test]
fn renaming_changes_only_the_name() {
    // Given a project
    let dir = a_registry_with_alpha("");
    let before = stored_row(dir.path());

    // When it is renamed
    let row = project_storage::rename_project(dir.path(), ALPHA, "Alpha App")
        .unwrap()
        .expect("the row exists");

    // Then the name changed, and the repository identity did not
    assert_eq!(row.name, "Alpha App");
    assert_eq!(
        stored_row(dir.path()),
        ProjectData {
            name: "Alpha App".to_string(),
            ..before
        }
    );
}

#[test]
fn renaming_an_unknown_project_reports_no_row() {
    let dir = a_registry_with_alpha("");
    assert_eq!(project_storage::rename_project(dir.path(), UNKNOWN, "x").unwrap(), None);
}

#[test]
fn a_project_name_follows_the_create_rules() {
    assert_eq!(project_storage::validate_project_name("  Alpha App "), Ok("Alpha App"));
    assert!(project_storage::validate_project_name("   ").is_err());
    assert!(project_storage::validate_project_name("a/b").is_err());
    assert!(project_storage::validate_project_name("..").is_err());
}

// ── reconciling hosts ────────────────────────────────────────────────────────────────────────

#[test]
fn a_tombstone_on_any_host_wins_over_rows_that_never_saw_an_event() {
    // Given one host deleted the project while another was offline and never heard of it
    let rows = [ProjectLifecycle::default(), deleted(EARLY, "octocat")];

    // Then the logical project is deleted
    assert_eq!(ProjectLifecycle::newest(rows.iter()), deleted(EARLY, "octocat"));
}

#[test]
fn a_restore_newer_than_a_stale_tombstone_wins() {
    // Given a host that missed the restore still holds the old tombstone
    let rows = [deleted(EARLY, "octocat"), restored(LATER)];

    assert_eq!(ProjectLifecycle::newest(rows.iter()), restored(LATER));
}

#[test]
fn a_delete_newer_than_an_earlier_restore_wins() {
    let rows = [restored(LATER), deleted(LATEST, "octocat")];

    assert_eq!(ProjectLifecycle::newest(rows.iter()), deleted(LATEST, "octocat"));
}

#[test]
fn a_delete_and_a_restore_at_the_same_instant_resolve_to_deleted() {
    let rows = [restored(LATER), deleted(LATER, "octocat")];

    assert!(ProjectLifecycle::newest(rows.iter()).is_deleted());
}

#[test]
fn a_host_adopts_a_newer_tombstone_and_ignores_an_older_one() {
    // Given a host whose row was restored after an early delete
    let dir = a_registry_with_alpha("");
    project_storage::mark_project_restored(dir.path(), ALPHA, LATER).unwrap();

    // When an older tombstone from another host is offered, nothing changes
    assert!(!project_storage::adopt_project_lifecycle(dir.path(), ALPHA, &deleted(EARLY, "old")).unwrap());
    assert_eq!(stored_row(dir.path()).lifecycle, restored(LATER));

    // When a newer one is offered, it is adopted as is
    assert!(project_storage::adopt_project_lifecycle(dir.path(), ALPHA, &deleted(LATEST, "new")).unwrap());
    assert_eq!(stored_row(dir.path()).lifecycle, deleted(LATEST, "new"));
}

#[test]
fn a_deleted_project_is_found_by_its_git_url_or_its_checkout() {
    // Given a deleted project
    let dir = a_registry_with_alpha("");
    project_storage::mark_project_deleted(dir.path(), ALPHA, EARLY, "octocat").unwrap();

    // Then it is found by the same repository, however the URL is padded, or by its checkout
    let by_url = project_storage::find_deleted_project_matching(
        dir.path(),
        " https://github.com/acme/alpha.git ",
        "/elsewhere",
    )
    .unwrap();
    let by_path =
        project_storage::find_deleted_project_matching(dir.path(), "https://x/y.git", "/repos/alpha")
            .unwrap();
    assert_eq!(by_url.map(|p| p.project_id), Some(ALPHA.to_string()));
    assert_eq!(by_path.map(|p| p.project_id), Some(ALPHA.to_string()));
}

#[test]
fn a_live_project_is_never_matched_as_deleted() {
    let dir = a_registry_with_alpha("");
    let found = project_storage::find_deleted_project_matching(
        dir.path(),
        "https://github.com/acme/alpha.git",
        "/repos/alpha",
    )
    .unwrap();
    assert_eq!(found, None);
}

#[test]
fn a_deleted_project_is_refused_for_new_work_naming_it() {
    let dir = a_registry_with_alpha("");
    project_storage::mark_project_deleted(dir.path(), ALPHA, EARLY, "octocat").unwrap();

    let refusal = project_storage::refuse_if_deleted(&stored_row(dir.path())).unwrap_err();

    assert_eq!(refusal.code, tddy_rpc::Code::FailedPrecondition);
    assert!(refusal.message.contains("alpha"), "{}", refusal.message);
    assert!(refusal.message.contains("deleted"), "{}", refusal.message);
    assert!(refusal.message.contains("restore"), "{}", refusal.message);
}

#[test]
fn a_live_project_is_not_refused() {
    let dir = a_registry_with_alpha("");
    assert!(project_storage::refuse_if_deleted(&stored_row(dir.path())).is_ok());
}
