//! Acceptance: one session-display-label rule, shared by every surface that names a session.
//!
//! PRD: docs/ft/daemon/session-notifications.md (FR1, AC1, AC2).
//!
//! The rule is the one `tddy-web`'s session drawer already applies
//! (`packages/tddy-web/src/utils/sessionDrawerLabel.ts`): the basename of `repo_path`, falling
//! back to `workflow_goal`, falling back to the first eight characters of `session_id`. These
//! tests are the Rust half of that parity — each case mirrors a case in
//! `packages/tddy-web/src/utils/sessionDrawerLabel.test.ts`.

use tddy_core::session_label::session_display_label;

const A_SESSION_ID: &str = "018f1234-5678-7abc-8def-123456789abc";

#[test]
fn names_a_session_after_the_basename_of_its_repository_path() {
    // Given
    let repo_path = "/home/dev/my-feature-branch";

    // When
    let label = session_display_label(repo_path, "Build the session drawer", A_SESSION_ID);

    // Then
    assert_eq!(label, "my-feature-branch");
}

#[test]
fn ignores_a_trailing_slash_when_taking_the_repository_basename() {
    // Given
    let repo_path = "/home/dev/my-feature-branch/";

    // When
    let label = session_display_label(repo_path, "Build the session drawer", A_SESSION_ID);

    // Then
    assert_eq!(label, "my-feature-branch");
}

#[test]
fn ignores_surrounding_whitespace_around_the_repository_path() {
    // Given
    let repo_path = "  /home/dev/my-feature-branch  ";

    // When
    let label = session_display_label(repo_path, "Build the session drawer", A_SESSION_ID);

    // Then
    assert_eq!(label, "my-feature-branch");
}

#[test]
fn falls_back_to_the_workflow_goal_when_the_session_has_no_repository_path() {
    // Given
    let repo_path = "";

    // When
    let label = session_display_label(repo_path, "Build the session drawer", A_SESSION_ID);

    // Then
    assert_eq!(label, "Build the session drawer");
}

#[test]
fn falls_back_to_the_workflow_goal_when_the_repository_path_is_the_filesystem_root() {
    // Given — a session rooted at `/` has no directory name to be called after.
    let repo_path = "/";

    // When
    let label = session_display_label(repo_path, "Build the session drawer", A_SESSION_ID);

    // Then
    assert_eq!(label, "Build the session drawer");
}

#[test]
fn falls_back_to_the_first_eight_characters_of_the_session_id_when_nothing_else_is_set() {
    // Given
    let session_id = "deadbeef-0000-0000-0000-000000000000";

    // When
    let label = session_display_label("", "", session_id);

    // Then
    assert_eq!(label, "deadbeef");
}

#[test]
fn falls_back_to_the_whole_session_id_when_it_is_shorter_than_eight_characters() {
    // Given — `String.prototype.slice(0, 8)` in the web returns the whole string here, and the
    // Rust rule must not panic on a byte boundary or pad the result.
    let session_id = "abc123";

    // When
    let label = session_display_label("", "", session_id);

    // Then
    assert_eq!(label, "abc123");
}

/// `session_list_enrichment` reports a missing `workflow_goal` as the display placeholder `—`
/// (`SessionListStatusDisplay::all_placeholders`), and `ListSessions` hands that string to the
/// browser as-is. A label rule that took it literally would call a session "—" on every surface —
/// consistently, but uselessly. The placeholder therefore counts as absent.
#[test]
fn treats_the_display_placeholder_as_an_absent_workflow_goal() {
    // Given
    let session_id = "deadbeef-0000-0000-0000-000000000000";

    // When
    let label = session_display_label("", "\u{2014}", session_id);

    // Then
    assert_eq!(label, "deadbeef");
}

#[test]
fn still_prefers_the_repository_basename_over_the_display_placeholder() {
    // Given
    let repo_path = "/home/dev/my-feature-branch";

    // When
    let label = session_display_label(repo_path, "\u{2014}", A_SESSION_ID);

    // Then
    assert_eq!(label, "my-feature-branch");
}
