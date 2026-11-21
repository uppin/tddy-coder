//! An orchestrator session's path grants: the paths an operator allowed it to read or write beyond
//! its default reach (`#orchestra` 1/9). Persisted in the session's own directory, so a grant lives
//! exactly as long as the session and survives a resume.
//!
//! This node reads grants (the reach consults them); `#orchestra` 6/9 adds them when an operator
//! answers an access request.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The file, in a session's directory, that holds its grants.
pub const PATH_GRANTS_FILE: &str = "path-grants.json";

/// What a grant allows on its path and everything under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathGrantMode {
    Read,
    /// Read and write.
    Write,
}

/// One operator-allowed path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathGrant {
    /// An absolute path; the grant covers it and everything under it.
    pub path: PathBuf,
    pub mode: PathGrantMode,
    pub granted_at_unix_ms: u64,
    /// The agent's stated reason, as the operator saw it.
    pub reason: String,
}

/// The grants of one session, kept in `<session_dir>/path-grants.json`.
#[derive(Debug, Clone)]
pub struct PathGrantStore {
    session_dir: PathBuf,
}

impl PathGrantStore {
    /// The store of the session whose directory is `session_dir`. Opening never touches disk.
    pub fn open(session_dir: &Path) -> Self {
        Self {
            session_dir: session_dir.to_path_buf(),
        }
    }

    /// Every grant recorded for the session, oldest first; none when the file does not exist.
    pub fn grants(&self) -> std::io::Result<Vec<PathGrant>> {
        let bytes = match std::fs::read(self.file()) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        serde_json::from_slice(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Record `grant`, keeping every earlier one. Written atomically.
    pub fn add(&self, grant: PathGrant) -> std::io::Result<()> {
        let mut grants = self.grants()?;
        grants.push(grant);
        let bytes = serde_json::to_vec_pretty(&grants)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::create_dir_all(&self.session_dir)?;
        let staging = self.session_dir.join(format!("{PATH_GRANTS_FILE}.tmp"));
        std::fs::write(&staging, bytes)?;
        std::fs::rename(&staging, self.file())
    }

    fn file(&self) -> PathBuf {
        self.session_dir.join(PATH_GRANTS_FILE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_grant(path: &str, mode: PathGrantMode) -> PathGrant {
        PathGrant {
            path: PathBuf::from(path),
            mode,
            granted_at_unix_ms: 1_795_000_000_000,
            reason: "review the other project's API".to_string(),
        }
    }

    #[test]
    fn a_session_with_no_grants_file_has_no_grants() {
        // Given
        let session = tempfile::tempdir().unwrap();

        // When
        let grants = PathGrantStore::open(session.path()).grants().unwrap();

        // Then
        assert_eq!(grants, Vec::<PathGrant>::new());
    }

    #[test]
    fn an_added_grant_is_listed_after_the_earlier_ones() {
        // Given
        let session = tempfile::tempdir().unwrap();
        let store = PathGrantStore::open(session.path());
        store.add(a_grant("/srv/a", PathGrantMode::Read)).unwrap();

        // When
        store.add(a_grant("/srv/b", PathGrantMode::Write)).unwrap();

        // Then
        assert_eq!(
            store.grants().unwrap(),
            vec![
                a_grant("/srv/a", PathGrantMode::Read),
                a_grant("/srv/b", PathGrantMode::Write)
            ]
        );
    }

    #[test]
    fn grants_survive_reopening_the_store() {
        // Given
        let session = tempfile::tempdir().unwrap();
        PathGrantStore::open(session.path())
            .add(a_grant("/srv/a", PathGrantMode::Read))
            .unwrap();

        // When
        let reopened = PathGrantStore::open(session.path()).grants().unwrap();

        // Then
        assert_eq!(reopened, vec![a_grant("/srv/a", PathGrantMode::Read)]);
        assert!(session.path().join(PATH_GRANTS_FILE).is_file());
    }
}
