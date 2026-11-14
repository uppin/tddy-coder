//! PR stacks as project records: `<projects dir>/<project_id>/stacks/<stack_id>.yaml`.
//!
//! A stack belongs to the project it was planned in, not to the session that planned it, so any
//! session of that project can address it by id. The session that planned it keeps only a
//! reference — [`crate::changeset::Changeset::stack_id`].
//!
//! Every write is a read-modify-write of one record under a per-stack lock (an in-process mutex
//! keyed by the record's path) finished by an atomic rename, so two writers to the same stack both
//! land and a reader never sees a half-written record.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::changeset::Stack;

/// One stack as the project holds it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StackRecord {
    /// A UUIDv7, assigned by [`ProjectStackStore::create`].
    pub stack_id: String,
    pub project_id: String,
    /// The session that planned the stack (or, for a migrated one, the session whose changeset
    /// carried it). The migration sweep keys on it so a rerun never creates a duplicate.
    pub created_by_session_id: String,
    pub created_at_unix_ms: u64,
    /// What the stack is for, as `pr_stack_create` named it (`#orchestrators` 7/11). Absent from a
    /// record written before titles existed, or created without one; such a record still reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub stack: Stack,
}

/// Why a store operation was refused or failed.
#[derive(Debug, thiserror::Error)]
pub enum StackStoreError {
    /// The project id cannot name a single directory under the projects dir (empty, `.`, `..`, or
    /// carrying a path separator), so it is refused rather than joined onto the path.
    #[error("project id {0:?} cannot name a directory")]
    InvalidProjectId(String),
    /// The stack id cannot name a single file under the project's `stacks/` directory.
    #[error("stack id {0:?} cannot name a file")]
    InvalidStackId(String),
    /// No record `stack_id` exists under `project_id`.
    #[error("project {project_id:?} holds no stack {stack_id:?}")]
    NotFound {
        project_id: String,
        stack_id: String,
    },
    /// A record exists but does not parse.
    #[error("stack record {path}: {message}")]
    Malformed { path: PathBuf, message: String },
    #[error("stack store I/O on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// The PR stacks of every project on this host, rooted at the host's projects directory
/// (`<sessions_base>/projects`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStackStore {
    projects_dir: PathBuf,
}

impl ProjectStackStore {
    /// A store over `projects_dir`. Nothing is created until a stack is.
    pub fn new(projects_dir: impl Into<PathBuf>) -> Self {
        Self {
            projects_dir: projects_dir.into(),
        }
    }

    /// The directory this store is rooted at.
    pub fn projects_dir(&self) -> &Path {
        &self.projects_dir
    }

    /// Store `stack` as a new record of `project_id`, planned by `created_by_session_id`, under a
    /// fresh stack id. Returns the record as written.
    pub fn create(
        &self,
        project_id: &str,
        created_by_session_id: &str,
        stack: Stack,
    ) -> Result<StackRecord, StackStoreError> {
        let stacks_dir = self.stacks_dir(project_id)?;
        let stack_id = uuid::Uuid::now_v7().to_string();
        let record = StackRecord {
            stack_id,
            project_id: project_id.to_string(),
            created_by_session_id: created_by_session_id.to_string(),
            created_at_unix_ms: now_unix_ms(),
            title: None,
            stack,
        };
        let path = record_path(&stacks_dir, &record.stack_id);
        with_record_lock(&path, || write_record(&path, &record))?;
        Ok(record)
    }

    /// [`Self::create`], with the record titled `title` (`#orchestrators` 7/11).
    pub fn create_titled(
        &self,
        project_id: &str,
        created_by_session_id: &str,
        title: Option<String>,
        stack: Stack,
    ) -> Result<StackRecord, StackStoreError> {
        // TODO(stack-plan-tools): implement — write `title` into the new record.
        let _ = title;
        self.create(project_id, created_by_session_id, stack)
    }

    /// The record `stack_id` of `project_id`.
    pub fn read(&self, project_id: &str, stack_id: &str) -> Result<StackRecord, StackStoreError> {
        let stacks_dir = self.stacks_dir(project_id)?;
        let path = self.existing_record_path(&stacks_dir, project_id, stack_id)?;
        read_record(&path, project_id, stack_id)
    }

    /// Apply `f` to the stack of record `stack_id` and write it back, serialized against every
    /// other writer of that stack in this process. Returns the record as written.
    pub fn update(
        &self,
        project_id: &str,
        stack_id: &str,
        f: impl FnOnce(&mut Stack),
    ) -> Result<StackRecord, StackStoreError> {
        let stacks_dir = self.stacks_dir(project_id)?;
        let path = self.existing_record_path(&stacks_dir, project_id, stack_id)?;
        with_record_lock(&path, || {
            let mut record = read_record(&path, project_id, stack_id)?;
            f(&mut record.stack);
            write_record(&path, &record)?;
            Ok(record)
        })
    }

    /// Every stack record of `project_id`, sorted by `stack_id`. A project with no `stacks/`
    /// directory has none.
    pub fn list(&self, project_id: &str) -> Result<Vec<StackRecord>, StackStoreError> {
        let stacks_dir = self.stacks_dir(project_id)?;
        let entries = match std::fs::read_dir(&stacks_dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(StackStoreError::Io {
                    path: stacks_dir,
                    source,
                })
            }
        };
        let mut stack_ids = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| StackStoreError::Io {
                path: stacks_dir.clone(),
                source,
            })?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(stack_id) = name.strip_suffix(".yaml") {
                stack_ids.push(stack_id.to_string());
            }
        }
        stack_ids.sort();
        stack_ids
            .iter()
            .map(|stack_id| read_record(&record_path(&stacks_dir, stack_id), project_id, stack_id))
            .collect()
    }

    /// `<projects dir>/<project_id>/stacks`, after `project_id` is checked to name one directory.
    fn stacks_dir(&self, project_id: &str) -> Result<PathBuf, StackStoreError> {
        if !names_one_path_segment(project_id) {
            return Err(StackStoreError::InvalidProjectId(project_id.to_string()));
        }
        Ok(self.projects_dir.join(project_id).join("stacks"))
    }

    fn existing_record_path(
        &self,
        stacks_dir: &Path,
        project_id: &str,
        stack_id: &str,
    ) -> Result<PathBuf, StackStoreError> {
        if !names_one_path_segment(stack_id) {
            return Err(StackStoreError::InvalidStackId(stack_id.to_string()));
        }
        let path = record_path(stacks_dir, stack_id);
        if !path.is_file() {
            return Err(StackStoreError::NotFound {
                project_id: project_id.to_string(),
                stack_id: stack_id.to_string(),
            });
        }
        Ok(path)
    }
}

/// A non-empty id that is neither `.` nor `..` and carries no path separator.
fn names_one_path_segment(id: &str) -> bool {
    !id.is_empty() && id != "." && id != ".." && !id.contains(['/', '\\', '\0'])
}

fn record_path(stacks_dir: &Path, stack_id: &str) -> PathBuf {
    stacks_dir.join(format!("{stack_id}.yaml"))
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The in-process lock of one record, shared by every store handle in this process.
fn with_record_lock<T>(path: &Path, f: impl FnOnce() -> T) -> T {
    static LOCKS: std::sync::OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> =
        std::sync::OnceLock::new();
    let lock = {
        let mut locks = LOCKS
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Arc::clone(locks.entry(path.to_path_buf()).or_default())
    };
    let _held = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    f()
}

fn read_record(
    path: &Path,
    project_id: &str,
    stack_id: &str,
) -> Result<StackRecord, StackStoreError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(StackStoreError::NotFound {
                project_id: project_id.to_string(),
                stack_id: stack_id.to_string(),
            })
        }
        Err(source) => {
            return Err(StackStoreError::Io {
                path: path.to_path_buf(),
                source,
            })
        }
    };
    serde_yaml::from_str(&text).map_err(|e| StackStoreError::Malformed {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

fn write_record(path: &Path, record: &StackRecord) -> Result<(), StackStoreError> {
    let text = serde_yaml::to_string(record).map_err(|e| StackStoreError::Malformed {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    tddy_session_store::atomic_file::write_atomic(path, text).map_err(|source| {
        StackStoreError::Io {
            path: path.to_path_buf(),
            source,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changeset::StackNode;
    use rstest::rstest;

    fn a_one_node_stack() -> Stack {
        Stack {
            version: 1,
            nodes: vec![StackNode {
                node_id: "n1".to_string(),
                title: "PR n1".to_string(),
                ..StackNode::default()
            }],
        }
    }

    fn a_store() -> (tempfile::TempDir, ProjectStackStore) {
        let root = tempfile::tempdir().expect("temp projects dir");
        let store = ProjectStackStore::new(root.path().join("projects"));
        (root, store)
    }

    #[rstest]
    #[case::empty("")]
    #[case::current_dir(".")]
    #[case::parent_dir("..")]
    #[case::nested("proj/inner")]
    #[case::backslash("proj\\inner")]
    fn a_project_id_that_names_no_single_directory_is_refused(#[case] project_id: &str) {
        // Given
        let (_root, store) = a_store();

        // When
        let refused = store.list(project_id);

        // Then
        assert!(
            matches!(&refused, Err(StackStoreError::InvalidProjectId(id)) if id == project_id),
            "expected InvalidProjectId({project_id:?}), got {refused:?}"
        );
    }

    #[rstest]
    #[case::parent_dir("..")]
    #[case::nested("../other")]
    #[case::empty("")]
    fn a_stack_id_that_names_no_single_file_is_refused(#[case] stack_id: &str) {
        // Given
        let (_root, store) = a_store();

        // When
        let refused = store.read("proj-auth", stack_id);

        // Then
        assert!(
            matches!(&refused, Err(StackStoreError::InvalidStackId(id)) if id == stack_id),
            "expected InvalidStackId({stack_id:?}), got {refused:?}"
        );
    }

    #[test]
    fn reading_a_stack_the_project_does_not_hold_is_not_found() {
        // Given
        let (_root, store) = a_store();

        // When
        let missing = store.read("proj-auth", "0190a000-0000-7000-8000-000000000000");

        // Then
        assert!(
            matches!(
                &missing,
                Err(StackStoreError::NotFound { project_id, stack_id })
                    if project_id == "proj-auth"
                        && stack_id == "0190a000-0000-7000-8000-000000000000"
            ),
            "expected NotFound, got {missing:?}"
        );
    }

    #[test]
    fn updating_a_stack_the_project_does_not_hold_creates_nothing() {
        // Given
        let (_root, store) = a_store();

        // When
        let missing = store.update(
            "proj-auth",
            "0190a000-0000-7000-8000-000000000000",
            |stack| stack.version = 9,
        );

        // Then
        assert!(
            matches!(&missing, Err(StackStoreError::NotFound { .. })),
            "expected NotFound, got {missing:?}"
        );
        assert!(
            !store.projects_dir().join("proj-auth").exists(),
            "a refused update must not create the project's directory"
        );
    }

    #[test]
    fn a_project_without_stacks_lists_none() {
        // Given
        let (_root, store) = a_store();

        // When
        let listed = store.list("proj-auth").expect("list");

        // Then
        assert_eq!(listed, Vec::<StackRecord>::new());
    }

    #[test]
    fn two_creates_in_one_project_get_distinct_ids() {
        // Given
        let (_root, store) = a_store();

        // When
        let first = store
            .create("proj-auth", "sess-a", a_one_node_stack())
            .expect("create first");
        let second = store
            .create("proj-auth", "sess-a", a_one_node_stack())
            .expect("create second");

        // Then
        assert_ne!(first.stack_id, second.stack_id);
    }

    #[test]
    fn an_update_returns_and_persists_the_changed_stack() {
        // Given
        let (_root, store) = a_store();
        let created = store
            .create("proj-auth", "sess-a", a_one_node_stack())
            .expect("create");

        // When
        let updated = store
            .update("proj-auth", &created.stack_id, |stack| {
                stack.nodes[0].title = "Renamed".to_string();
            })
            .expect("update");

        // Then — the returned record is the stored one, and only the stack changed
        let read = store.read("proj-auth", &created.stack_id).expect("read");
        assert_eq!(read, updated);
        assert_eq!(
            StackRecord {
                stack: a_one_node_stack(),
                ..read
            },
            created
        );
        assert_eq!(updated.stack.nodes[0].title, "Renamed");
    }

    #[test]
    fn a_record_that_does_not_parse_is_reported_malformed() {
        // Given
        let (_root, store) = a_store();
        let created = store
            .create("proj-auth", "sess-a", a_one_node_stack())
            .expect("create");
        let path = store
            .projects_dir()
            .join("proj-auth")
            .join("stacks")
            .join(format!("{}.yaml", created.stack_id));
        std::fs::write(&path, "stack: [this is not a record").expect("corrupt the record");

        // When
        let read = store.read("proj-auth", &created.stack_id);

        // Then
        assert!(
            matches!(&read, Err(StackStoreError::Malformed { path: p, .. }) if *p == path),
            "expected Malformed at {}, got {read:?}",
            path.display()
        );
    }

    #[test]
    fn an_update_leaves_no_temporary_file_beside_the_record() {
        // Given
        let (_root, store) = a_store();
        let created = store
            .create("proj-auth", "sess-a", a_one_node_stack())
            .expect("create");

        // When
        store
            .update("proj-auth", &created.stack_id, |stack| stack.version = 2)
            .expect("update");

        // Then — the atomic rename consumed its temporary
        let stacks_dir = store.projects_dir().join("proj-auth").join("stacks");
        let mut names: Vec<String> = std::fs::read_dir(&stacks_dir)
            .expect("read stacks dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec![format!("{}.yaml", created.stack_id)]);
    }
}
