//! A work queue's items' files (`#workqueue` work item files): the durable store the bytes live
//! in, on the queue's owner host, under the owning session's directory —
//! `<sessions>/<session_id>/work-queue-files/<item_id>/<basename>` — so they live and are removed
//! with the session, exactly as `work-queue.yaml` is.
//!
//! The staged-attachment root is deliberately **not** the store: it is restart-cleared and not
//! durable (`docs/dev/todo/2026-08-01-session-attach-ui-follow-ups.md`), and a work item must
//! outlive restarts. The staged path is reused only as the *upload* mechanism; the attach call
//! promotes a completed staged file here.
//!
//! Every path segment is untrusted client input (`session_id`, `item_id`, `basename`), so each is
//! validated as a pure basename before it is joined — the same one-segment rule `work-queue.yaml`'s
//! path applies to its session id. The directories are created by this store, never by a caller,
//! and each file is written exclusively (`create_new`): an existing file is a refusal, never a
//! truncation. Reading the bytes back runs the full host-document containment gate
//! (`tddy-session-files::host_documents`), whose `WORK_QUEUE_FILE` scope resolves to this root;
//! the write path's trust boundary is the session directory's own — the same one the queue record
//! has always had.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_workflow::artifact_paths::WORK_QUEUE_FILES_SUBDIR;

/// The directory of a session's queue's items' files, inside the session directory.
pub const WORK_QUEUE_FILES_DIR: &str = WORK_QUEUE_FILES_SUBDIR;

#[derive(Debug)]
pub enum WorkQueueFileError {
    /// A segment (`session_id`, `item_id` or `basename`) that is not one pure path segment.
    InvalidSegment { name: String },
    /// The item already carries a file with this basename.
    AlreadyExists { path: PathBuf },
    /// No file (or no files directory) under the addressed name.
    NotFound { path: PathBuf },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for WorkQueueFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSegment { name } => {
                write!(f, "{name:?} is not a single path segment")
            }
            Self::AlreadyExists { path } => {
                write!(
                    f,
                    "a file already exists at {}: attach a different name",
                    path.display()
                )
            }
            Self::NotFound { path } => write!(f, "{}: not found", path.display()),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for WorkQueueFileError {}

/// The items' files of every session's queue on one host, addressed by session id.
pub struct SessionWorkQueueFiles {
    sessions_dir: PathBuf,
    fs: Arc<dyn tddy_fs::Fs>,
}

impl SessionWorkQueueFiles {
    /// `sessions_dir` is `<data>/sessions`.
    pub fn new(sessions_dir: impl Into<PathBuf>) -> Self {
        Self::with_fs(sessions_dir, Arc::new(tddy_fs::DiskFs))
    }

    /// [`SessionWorkQueueFiles::new`] with the filesystem port the files run through.
    pub fn with_fs(sessions_dir: impl Into<PathBuf>, fs: Arc<dyn tddy_fs::Fs>) -> Self {
        Self {
            sessions_dir: sessions_dir.into(),
            fs,
        }
    }

    /// Store `bytes` as `basename` of `item_id`, exclusively, and answer the size written.
    pub fn store(
        &self,
        session_id: &str,
        item_id: &str,
        basename: &str,
        bytes: &[u8],
    ) -> Result<u64, WorkQueueFileError> {
        let path = self.file_path(session_id, item_id, basename)?;
        self.fs
            .create_dir_all(path.parent().expect("the file always has a parent"))
            .map_err(|source| WorkQueueFileError::Io {
                path: path.parent().expect("checked").to_path_buf(),
                source,
            })?;
        self.fs.create_new(&path, bytes).map_err(|source| {
            if source.kind() == std::io::ErrorKind::AlreadyExists {
                WorkQueueFileError::AlreadyExists { path: path.clone() }
            } else {
                WorkQueueFileError::Io {
                    path: path.clone(),
                    source,
                }
            }
        })?;
        Ok(bytes.len() as u64)
    }

    /// The bytes of `item_id`'s file `basename`.
    pub fn read(
        &self,
        session_id: &str,
        item_id: &str,
        basename: &str,
    ) -> Result<Vec<u8>, WorkQueueFileError> {
        let path = self.file_path(session_id, item_id, basename)?;
        self.fs.read(&path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                WorkQueueFileError::NotFound { path }
            } else {
                WorkQueueFileError::Io { path, source }
            }
        })
    }

    /// Remove one stored file.
    pub fn remove(
        &self,
        session_id: &str,
        item_id: &str,
        basename: &str,
    ) -> Result<(), WorkQueueFileError> {
        let path = self.file_path(session_id, item_id, basename)?;
        self.fs.remove_file(&path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                WorkQueueFileError::NotFound { path }
            } else {
                WorkQueueFileError::Io { path, source }
            }
        })
    }

    /// Remove everything `item_id` stored — the cleanup a deleted item runs. An item that stored
    /// nothing has no directory, and removing nothing is success.
    pub fn remove_item(&self, session_id: &str, item_id: &str) -> Result<(), WorkQueueFileError> {
        let dir = self.item_dir(session_id, item_id)?;
        match self.fs.remove_dir_all(&dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(WorkQueueFileError::Io { path: dir, source }),
        }
    }

    /// Where `item_id`'s files live — the directory a deleted item's cleanup removes.
    pub fn item_dir(&self, session_id: &str, item_id: &str) -> Result<PathBuf, WorkQueueFileError> {
        Ok(self
            .session_dir(session_id)?
            .join(WORK_QUEUE_FILES_DIR)
            .join(item_id))
    }

    fn file_path(
        &self,
        session_id: &str,
        item_id: &str,
        basename: &str,
    ) -> Result<PathBuf, WorkQueueFileError> {
        for name in [session_id, item_id, basename] {
            validate_segment(name)?;
        }
        Ok(self
            .session_dir(session_id)?
            .join(WORK_QUEUE_FILES_DIR)
            .join(item_id)
            .join(basename))
    }

    fn session_dir(&self, session_id: &str) -> Result<PathBuf, WorkQueueFileError> {
        validate_segment(session_id)?;
        Ok(self.sessions_dir.join(session_id))
    }
}

/// One name joined into the store's path — a `session_id`, an `item_id` or a `basename` — must be
/// a single pure path segment: non-empty, no separators, no `.` or `..`.
pub fn validate_segment(name: &str) -> Result<(), WorkQueueFileError> {
    let one_segment =
        !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0']);
    if one_segment {
        Ok(())
    } else {
        Err(WorkQueueFileError::InvalidSegment {
            name: name.to_string(),
        })
    }
}
