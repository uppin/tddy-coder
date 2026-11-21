//! An orchestrator's own session files (`#orchestra` 3/9): briefs, plans and specs it writes into
//! its session directory, under `artifacts/files/<name>`, to attach to the sessions it starts.
//!
//! The directory sits inside the `SESSION_ARTIFACT` scope (`<session_dir>/artifacts`), so a file
//! here is attached by reference exactly as any other session artifact is: scope
//! `SESSION_ARTIFACT`, the orchestrator's session id, and `files/<name>`.
//!
//! Names are flat: one path segment, no `..`, no leading dot, at most [`MAX_SESSION_FILE_NAME`]
//! bytes; contents are at most [`MAX_SESSION_FILE_BYTES`].

use std::path::{Path, PathBuf};

/// Where session files live, relative to the session directory.
pub const SESSION_FILES_DIR: &str = "artifacts/files";

/// The same directory, relative to the `SESSION_ARTIFACT` scope's root (`<session_dir>/artifacts`).
pub const SESSION_FILES_ARTIFACT_PREFIX: &str = "files";

/// The longest name a session file may have, in bytes.
pub const MAX_SESSION_FILE_NAME: usize = 128;

/// The largest session file, in bytes.
pub const MAX_SESSION_FILE_BYTES: usize = 1024 * 1024;

/// One file in a session's files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFile {
    pub name: String,
    pub size_bytes: u64,
    pub modified_unix_ms: u64,
}

/// Why a session file was not written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionFileError {
    BadName(String),
    TooLarge {
        name: String,
        size: usize,
        max: usize,
    },
    Io {
        name: String,
        message: String,
    },
}

impl std::fmt::Display for SessionFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadName(name) => write!(
                f,
                "session file name {name:?} is not one flat file name \
                 (no '/', no '..', no leading '.', 1 to {MAX_SESSION_FILE_NAME} bytes)"
            ),
            Self::TooLarge { name, size, max } => {
                write!(
                    f,
                    "session file {name:?} is {size} bytes; at most {max} are kept"
                )
            }
            Self::Io { name, message } => {
                write!(f, "session file {name:?} could not be written: {message}")
            }
        }
    }
}

impl std::error::Error for SessionFileError {}

/// The session files of one session.
#[derive(Debug, Clone)]
pub struct SessionFileStore {
    session_dir: PathBuf,
}

impl SessionFileStore {
    /// The files of the session whose directory is `session_dir`. Opening never touches disk.
    pub fn open(session_dir: &Path) -> Self {
        Self {
            session_dir: session_dir.to_path_buf(),
        }
    }

    /// Write `content` as `name`, replacing a file of that name. Written atomically.
    pub fn write(&self, name: &str, content: &str) -> Result<SessionFile, SessionFileError> {
        // TODO(session-files): implement
        let _ = (&self.session_dir, name, content);
        unimplemented!("SessionFileStore::write")
    }

    /// Every session file, by name; none when the directory does not exist.
    pub fn list(&self) -> std::io::Result<Vec<SessionFile>> {
        // TODO(session-files): implement
        let _ = &self.session_dir;
        unimplemented!("SessionFileStore::list")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_file_is_listed_with_its_size() {
        let session = tempfile::tempdir().unwrap();
        let store = SessionFileStore::open(session.path());

        store.write("brief.md", "# Brief\n").unwrap();

        let listed: Vec<(String, u64)> = store
            .list()
            .unwrap()
            .into_iter()
            .map(|f| (f.name, f.size_bytes))
            .collect();
        assert_eq!(listed, vec![("brief.md".to_string(), 8)]);
        assert_eq!(
            std::fs::read_to_string(session.path().join(SESSION_FILES_DIR).join("brief.md"))
                .unwrap(),
            "# Brief\n"
        );
    }

    #[test]
    fn writing_a_name_again_replaces_the_file() {
        let session = tempfile::tempdir().unwrap();
        let store = SessionFileStore::open(session.path());
        store.write("brief.md", "first").unwrap();

        store.write("brief.md", "second").unwrap();

        assert_eq!(
            std::fs::read_to_string(session.path().join(SESSION_FILES_DIR).join("brief.md"))
                .unwrap(),
            "second"
        );
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn names_that_are_not_one_flat_file_name_are_refused() {
        let session = tempfile::tempdir().unwrap();
        let store = SessionFileStore::open(session.path());
        let long = "a".repeat(MAX_SESSION_FILE_NAME + 1);

        let refused: Vec<bool> = ["../escape.md", "dir/brief.md", ".hidden", "", long.as_str()]
            .iter()
            .map(|name| matches!(store.write(name, "x"), Err(SessionFileError::BadName(_))))
            .collect();

        assert_eq!(refused, vec![true; 5]);
    }

    #[test]
    fn content_over_the_limit_is_refused() {
        let session = tempfile::tempdir().unwrap();
        let store = SessionFileStore::open(session.path());

        let refused = store.write("big.md", &"x".repeat(MAX_SESSION_FILE_BYTES + 1));

        assert!(
            matches!(refused, Err(SessionFileError::TooLarge { .. })),
            "{refused:?}"
        );
    }

    #[test]
    fn a_session_without_files_lists_none() {
        let session = tempfile::tempdir().unwrap();

        assert_eq!(
            SessionFileStore::open(session.path()).list().unwrap(),
            Vec::new()
        );
    }
}
