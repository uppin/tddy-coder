//! Test fixtures for code that reads or writes a session's PR stack.
//!
//! A work queue lives in its owning session's directory, so a test needs the layout the daemon lays
//! out — a session directory at `<base>/sessions/<id>` with a `.session.yaml` naming its project —
//! not a bare temp directory. Compiled only for this crate's own tests and for
//! crates that ask for the `test-support` feature in their `[dev-dependencies]`.

use std::path::{Path, PathBuf};

use crate::changeset::{
    read_session_stack, update_stack_atomic, write_changeset, Changeset, WorkQueue,
};
use crate::session_lifecycle::unified_session_dir_path;
use crate::session_metadata::{
    write_initial_tool_session_metadata, InitialToolSessionMetadataOpts,
};
use crate::session_work_queue::SessionWorkQueues;
use crate::work_queue_store::WorkQueueStore;

/// The project every fixture session belongs to.
pub const A_PROJECT: &str = "proj-test";

/// One host's data directory: `sessions/` beside `projects/`, removed with the world.
pub struct AStackHost {
    root: tempfile::TempDir,
}

/// An empty host.
pub fn a_stack_host() -> AStackHost {
    AStackHost {
        root: tempfile::tempdir().expect("temp host data dir"),
    }
}

impl AStackHost {
    /// The host's data directory, the `sessions_base` every sessions-layout reader takes.
    pub fn base(&self) -> &Path {
        self.root.path()
    }

    /// `<base>/projects`.
    pub fn projects_dir(&self) -> PathBuf {
        self.root.path().join("projects")
    }

    /// The host's stack store.
    pub fn store(&self) -> WorkQueueStore {
        WorkQueueStore::new(self.base())
    }

    /// The directory of session `session_id`, with `.session.yaml` naming [`A_PROJECT`] and
    /// `changeset` as its manifest. The session owns no queue; use [`Self::orchestrator`] for one
    /// that does.
    pub fn session(&self, session_id: &str, changeset: Changeset) -> PathBuf {
        self.session_of_project(session_id, A_PROJECT, changeset)
    }

    /// [`Self::session`] in project `project_id`.
    pub fn session_of_project(
        &self,
        session_id: &str,
        project_id: &str,
        changeset: Changeset,
    ) -> PathBuf {
        let dir = unified_session_dir_path(self.base(), session_id);
        std::fs::create_dir_all(&dir).expect("session dir");
        write_initial_tool_session_metadata(
            &dir,
            InitialToolSessionMetadataOpts {
                project_id: project_id.to_string(),
                ..InitialToolSessionMetadataOpts::default()
            },
        )
        .expect("write .session.yaml");
        write_changeset(&dir, &changeset).expect("write changeset.yaml");
        dir
    }

    /// A session that owns `stack` as its work queue.
    pub fn orchestrator(&self, session_id: &str, stack: WorkQueue) -> PathBuf {
        let dir = self.session(
            session_id,
            Changeset {
                recipe: Some("pr-stack".to_string()),
                ..Changeset::default()
            },
        );
        seed_session_stack(&dir, stack);
        dir
    }
}

/// Make `stack` the work queue of the session at `session_dir`, giving the session a queue first
/// when it owns none (as its start would have).
pub fn seed_session_stack(session_dir: &Path, stack: WorkQueue) {
    let sessions_dir = session_dir.parent().expect("a session directory");
    let session_id = session_dir
        .file_name()
        .expect("a session directory")
        .to_string_lossy()
        .into_owned();
    let queues = SessionWorkQueues::new(sessions_dir);
    if queues.read(&session_id).expect("read the queue").is_none() {
        queues
            .create_for_session(&session_id, A_PROJECT, 0)
            .expect("create the session's queue");
    }
    update_stack_atomic(session_dir, |s| *s = stack).expect("seed the session's stack");
}

/// The work queue of the session at `session_dir`.
pub fn stack_on_disk(session_dir: &Path) -> WorkQueue {
    read_session_stack(session_dir)
        .expect("read the session's stack")
        .expect("the session names a stack")
}

/// The id of the queue the session at `session_dir` owns — its own session id — or `None` when it
/// owns none.
pub fn queue_id_of(session_dir: &Path) -> Option<String> {
    read_session_stack(session_dir)
        .expect("read the session's queue")
        .map(|_| {
            session_dir
                .file_name()
                .expect("a session directory")
                .to_string_lossy()
                .into_owned()
        })
}

/// A host with one session directory in it (`orch`, of [`A_PROJECT`], no changeset yet) — the
/// drop-in for the bare `tempfile::TempDir` a test used before stacks lived in the project:
/// `.path()` is the session directory, and [`Self::host`] reaches the rest of the layout.
pub struct ASessionDir {
    host: AStackHost,
    dir: PathBuf,
}

/// A host holding one empty session directory.
pub fn a_stack_session_dir() -> ASessionDir {
    let host = a_stack_host();
    let dir = unified_session_dir_path(host.base(), "orch");
    std::fs::create_dir_all(&dir).expect("session dir");
    write_initial_tool_session_metadata(
        &dir,
        InitialToolSessionMetadataOpts {
            project_id: A_PROJECT.to_string(),
            ..InitialToolSessionMetadataOpts::default()
        },
    )
    .expect("write .session.yaml");
    ASessionDir { host, dir }
}

impl ASessionDir {
    /// The session directory.
    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// The host the session directory sits in.
    pub fn host(&self) -> &AStackHost {
        &self.host
    }
}

/// Write `changeset` as the manifest of the session at `session_dir` — creating the directory and
/// a `.session.yaml` naming [`A_PROJECT`] when it has none — and, when `stack` is given, make the
/// session the owner of a work queue holding it.
pub fn write_changeset_with_stack(
    session_dir: &Path,
    changeset: Changeset,
    stack: Option<WorkQueue>,
) {
    std::fs::create_dir_all(session_dir).expect("session dir");
    if !session_dir
        .join(crate::session_metadata::SESSION_METADATA_FILENAME)
        .exists()
    {
        write_initial_tool_session_metadata(
            session_dir,
            InitialToolSessionMetadataOpts {
                project_id: A_PROJECT.to_string(),
                ..InitialToolSessionMetadataOpts::default()
            },
        )
        .expect("write .session.yaml");
    }
    write_changeset(session_dir, &changeset).expect("write changeset.yaml");
    if let Some(stack) = stack {
        seed_session_stack(session_dir, stack);
    }
}
