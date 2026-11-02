//! Where a stopped session's tabs run: the worktree its session directory records.
//!
//! A running session's tabs open in its main terminal's worktree. A stopped one has no main
//! terminal, so the path comes from the record the session left behind — the same record a resume
//! reads. There is no substitute directory: a shell opened somewhere other than the session's
//! checkout would look like the session and act on something else.

use std::path::{Path, PathBuf};

use tddy_rpc::Status;

/// The worktree `session_id`'s directory under `tddy_data_dir` records, as
/// [`tddy_core::repo_root_for_session`] reads it.
///
/// Refuses with `invalid_argument` when the id is not a single path segment, and with
/// `failed_precondition` naming the session when nothing on this host records its worktree — an
/// unknown session, or a split session's agent half whose checkout lives on another daemon.
pub(super) fn recorded_worktree(tddy_data_dir: &Path, session_id: &str) -> Result<PathBuf, Status> {
    // TODO(dormant-tabs): implement
    let _ = tddy_data_dir;
    Err(Status::unimplemented(format!(
        "starting a tab on stopped session {session_id} is not implemented"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    use tddy_rpc::Code;

    const SESSION_ID: &str = "stopped-session";

    /// A daemon data dir holding one session directory whose `.session.yaml` names `repo_path`.
    fn a_data_dir_recording(repo_path: Option<&str>) -> tempfile::TempDir {
        let data_dir = tempfile::tempdir().unwrap();
        let session_dir =
            tddy_core::session_lifecycle::unified_session_dir_path(data_dir.path(), SESSION_ID);
        std::fs::create_dir_all(&session_dir).unwrap();
        let repo_line = repo_path
            .map(|path| format!("repo_path: \"{path}\"\n"))
            .unwrap_or_default();
        std::fs::write(
            session_dir.join(".session.yaml"),
            format!(
                "session_id: {SESSION_ID}\nproject_id: p1\ncreated_at: \"2026-11-02T00:00:00Z\"\n\
                 updated_at: \"2026-11-02T00:00:00Z\"\nstatus: exited\n{repo_line}"
            ),
        )
        .unwrap();
        data_dir
    }

    #[test]
    fn a_stopped_session_resolves_to_the_worktree_its_metadata_records() {
        // Given
        let data_dir = a_data_dir_recording(Some("/home/dev/finished-work"));

        // When
        let worktree = recorded_worktree(data_dir.path(), SESSION_ID);

        // Then
        assert_eq!(
            worktree.expect("the recorded worktree"),
            PathBuf::from("/home/dev/finished-work")
        );
    }

    #[test]
    fn a_session_whose_metadata_records_no_worktree_is_refused_naming_it() {
        // Given
        let data_dir = a_data_dir_recording(None);

        // When
        let refusal = recorded_worktree(data_dir.path(), SESSION_ID).unwrap_err();

        // Then
        assert_eq!(refusal.code(), Code::FailedPrecondition);
        assert!(
            refusal.message().contains(SESSION_ID),
            "{}",
            refusal.message()
        );
    }

    #[test]
    fn a_session_with_no_directory_on_this_host_is_refused_naming_it() {
        // Given
        let data_dir = tempfile::tempdir().unwrap();

        // When
        let refusal = recorded_worktree(data_dir.path(), SESSION_ID).unwrap_err();

        // Then
        assert_eq!(refusal.code(), Code::FailedPrecondition);
        assert!(
            refusal.message().contains(SESSION_ID),
            "{}",
            refusal.message()
        );
    }

    #[test]
    fn a_session_id_that_is_not_one_path_segment_is_refused() {
        // Given
        let data_dir = a_data_dir_recording(Some("/home/dev/finished-work"));

        // When
        let refusal = recorded_worktree(data_dir.path(), "../stopped-session").unwrap_err();

        // Then
        assert_eq!(refusal.code(), Code::InvalidArgument);
    }
}
