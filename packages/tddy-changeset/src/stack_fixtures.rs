//! Test fixtures for code that reads or writes a session's PR stack.
//!
//! A stack lives in the project's store, so a test needs the layout the daemon lays out — a
//! session directory at `<base>/sessions/<id>` with a `.session.yaml` naming its project, beside
//! `<base>/projects` — not a bare temp directory. Compiled only for this crate's own tests and for
//! crates that ask for the `test-support` feature in their `[dev-dependencies]`.

use std::path::{Path, PathBuf};

use crate::changeset::{
    read_changeset, read_session_stack, update_stack_atomic, write_changeset, Changeset, Stack,
};
use crate::project_stack_store::ProjectStackStore;
use crate::session_lifecycle::unified_session_dir_path;
use crate::session_metadata::{
    write_initial_tool_session_metadata, InitialToolSessionMetadataOpts,
};

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
    pub fn store(&self) -> ProjectStackStore {
        ProjectStackStore::new(self.projects_dir())
    }

    /// The directory of session `session_id`, with `.session.yaml` naming [`A_PROJECT`] and
    /// `changeset` as its manifest. Any `stack` on `changeset` is written as given — a legacy
    /// copy; use [`Self::orchestrator`] for a stack the store holds.
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

    /// A `pr-stack` session whose project holds `stack`, named by the session's `stack_id`.
    pub fn orchestrator(&self, session_id: &str, stack: Stack) -> PathBuf {
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

/// Make `stack` the stack of the session at `session_dir` (created in the project's store when the
/// session has none yet).
pub fn seed_session_stack(session_dir: &Path, stack: Stack) {
    update_stack_atomic(session_dir, |s| *s = stack).expect("seed the session's stack");
}

/// The stack of the session at `session_dir`, read from the project's store.
pub fn stack_on_disk(session_dir: &Path) -> Stack {
    read_session_stack(session_dir)
        .expect("read the session's stack")
        .expect("the session names a stack")
}

/// The id of the stack the session at `session_dir` names.
pub fn stack_id_of(session_dir: &Path) -> Option<String> {
    read_changeset(session_dir)
        .expect("read changeset")
        .stack_id
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
/// a `.session.yaml` naming [`A_PROJECT`] when it has none — with any `stack` it carries moved into
/// the project's store, so a test that spelled its orchestrator as one `Changeset { stack, .. }`
/// literal keeps that spelling.
pub fn write_changeset_with_stack(session_dir: &Path, mut changeset: Changeset) {
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
    let stack = changeset.stack.take();
    write_changeset(session_dir, &changeset).expect("write changeset.yaml");
    if let Some(stack) = stack {
        seed_session_stack(session_dir, stack);
    }
}
