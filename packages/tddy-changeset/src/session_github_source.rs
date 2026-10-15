//! Where a session takes its GitHub identity from, when its start said so explicitly.
//!
//! Stored as `github-source.yaml` beside `.session.yaml`, not inside it:
//!
//! - `.session.yaml` is `deny_unknown_fields`, so a new key there would make every daemon that
//!   predates it unable to read the session at all;
//! - for a tool session `.session.yaml` is written by the child, while this is the daemon's to
//!   write, before the child exists.
//!
//! Only a start that **named** a source (one forwarded by a peer that knows the project as Local
//! gh) writes the file. A session without it re-reads its host's project setting when it resumes,
//! exactly as an account assignment is re-read. See docs/ft/daemon/github-identity.md.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The file this module reads and writes, inside a session directory.
pub const SESSION_GITHUB_SOURCE_FILENAME: &str = "github-source.yaml";

/// Where a session takes its GitHub identity from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionGithubSource {
    /// The account the project assigns, from the signed-in person's credential vault.
    Vault,
    /// The host's own GitHub CLI login, as the session's OS user.
    LocalGh,
}

/// Record `source` for the session in `session_dir`, replacing any earlier record.
pub fn write_session_github_source(
    session_dir: &Path,
    source: SessionGithubSource,
) -> Result<(), crate::WorkflowError> {
    let yaml = serde_yaml::to_string(&source).map_err(|e| {
        crate::WorkflowError::WriteFailed(format!("{SESSION_GITHUB_SOURCE_FILENAME}: {e}"))
    })?;
    std::fs::write(session_dir.join(SESSION_GITHUB_SOURCE_FILENAME), yaml).map_err(|e| {
        crate::WorkflowError::WriteFailed(format!("{SESSION_GITHUB_SOURCE_FILENAME}: {e}"))
    })
}

/// The source recorded for the session in `session_dir`; `None` when its start named none.
pub fn read_session_github_source(
    session_dir: &Path,
) -> Result<Option<SessionGithubSource>, crate::WorkflowError> {
    let path = session_dir.join(SESSION_GITHUB_SOURCE_FILENAME);
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(crate::WorkflowError::ChangesetInvalid(format!(
                "{}: {e}",
                path.display()
            )))
        }
    };
    serde_yaml::from_str(&contents)
        .map(Some)
        .map_err(|e| crate::WorkflowError::ChangesetInvalid(format!("{}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_whose_start_named_no_source_reads_as_none() {
        // Given a session directory holding no source record
        let session_dir = tempfile::tempdir().unwrap();

        // When its source is read
        let source =
            read_session_github_source(session_dir.path()).expect("an absent record reads");

        // Then there is none, and the host's project row decides
        assert_eq!(source, None);
    }

    #[test]
    fn a_recorded_source_survives_a_write_and_a_read() {
        // Given a session started naming Local gh
        let session_dir = tempfile::tempdir().unwrap();
        write_session_github_source(session_dir.path(), SessionGithubSource::LocalGh)
            .expect("the source is recorded");

        // When its source is read
        let source = read_session_github_source(session_dir.path()).expect("the record reads");

        // Then it is Local gh
        assert_eq!(source, Some(SessionGithubSource::LocalGh));
    }

    #[test]
    fn the_record_is_kept_beside_the_session_metadata_and_never_inside_it() {
        // Given a session started naming Local gh
        let session_dir = tempfile::tempdir().unwrap();
        write_session_github_source(session_dir.path(), SessionGithubSource::LocalGh)
            .expect("the source is recorded");

        // When the session directory is listed
        let mut names: Vec<String> = std::fs::read_dir(session_dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();

        // Then the record is its own file, and `.session.yaml` was not written
        assert_eq!(names, [SESSION_GITHUB_SOURCE_FILENAME]);
    }
}
