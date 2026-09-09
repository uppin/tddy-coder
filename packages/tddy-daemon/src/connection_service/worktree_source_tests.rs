use super::{session_worktree_source, WorktreeSource};
use std::path::PathBuf;

#[test]
fn uses_the_client_repo_path_when_present() {
    // Given — a request carrying an explicit local repo path
    // When
    let source = session_worktree_source("/home/dev/proj", "proj-123");

    // Then
    assert_eq!(
        source,
        WorktreeSource::RepoPath(PathBuf::from("/home/dev/proj"))
    );
}

#[test]
fn falls_back_to_project_id_when_repo_path_is_empty() {
    // Given — no repo_path
    // When
    let source = session_worktree_source("", "proj-123");

    // Then
    assert_eq!(source, WorktreeSource::Project("proj-123".to_string()));
}
