//! The `World` both `EnsureProjectHomeSession` suites (`ensure_project_home_session_acceptance`,
//! `ensure_project_home_session_replace_edges`) drive: a daemon over one project, with the
//! set-up / ensure / stop steps and the reads both make of it. Each suite adds its own steps in an
//! `impl World` block of its own.
//!
//! Every item here is used by both suites, so neither test binary carries dead code.

use std::path::PathBuf;

use crate::project_home_support::{
    a_home_daemon, a_repo_with_origin_main, ensure_main, git, HomeDaemon, PROJECT_ID,
};
use tddy_core::session_metadata::{read_session_metadata, write_session_metadata, SessionMetadata};
use tddy_projects::project_storage;
use tddy_service::proto::session::{
    EnsureProjectHomeSessionResponse, SessionEntry, StartSessionRequest,
};

/// A daemon whose data dir holds one project (`origin/main` as its main branch ref), optionally
/// recording a main session id, and a stub `claude` that echoes its argv.
pub struct World {
    pub home: HomeDaemon,
    pub repo: tempfile::TempDir,
}

impl World {
    pub fn origin_main(&self) -> String {
        git(self.repo.path(), &["rev-parse", "origin/main"])
    }

    pub fn advance_origin_main(&self) -> String {
        git(
            self.repo.path(),
            &["commit", "-q", "--allow-empty", "-m", "moved on"],
        );
        git(self.repo.path(), &["push", "-q", "origin", "main"]);
        self.origin_main()
    }

    /// Ensure main without a start spec: reach the recorded session.
    pub async fn ensure(&self) -> Result<EnsureProjectHomeSessionResponse, tddy_rpc::Status> {
        ensure_main(&self.home.service, None).await
    }

    /// Set main up (or replace it) from `spec`.
    pub async fn set_up_main(
        &self,
        spec: StartSessionRequest,
    ) -> Result<EnsureProjectHomeSessionResponse, tddy_rpc::Status> {
        ensure_main(&self.home.service, Some(spec)).await
    }

    /// Set main up from the default Claude CLI spec, and return it running.
    pub async fn a_running_main(&self) -> SessionEntry {
        self.set_up_main(a_claude_cli_spec())
            .await
            .expect("setting up main succeeds")
            .session
            .expect("main is returned")
    }

    /// The home worktree a main session runs on, from its metadata.
    pub fn home_worktree_of(&self, session_id: &str) -> PathBuf {
        PathBuf::from(
            self.metadata_of(session_id)
                .repo_path
                .expect("main has a worktree"),
        )
    }

    /// The session id the project records as its main, read back from `projects.yaml`.
    pub fn recorded_main(&self) -> Option<String> {
        project_storage::find_project(&self.home.data.path().join("projects"), PROJECT_ID)
            .expect("projects.yaml reads")
            .expect("the project is registered")
            .home_session_id
    }

    pub fn metadata_of(&self, session_id: &str) -> SessionMetadata {
        read_session_metadata(&self.session_dir(session_id)).expect("the session has metadata")
    }

    /// Overwrite `session_id`'s metadata with `metadata`.
    pub fn rewrite_metadata_of(&self, session_id: &str, metadata: &SessionMetadata) {
        write_session_metadata(&self.session_dir(session_id), metadata)
            .expect("the session's metadata is written");
    }

    pub fn session_dirs(&self) -> usize {
        std::fs::read_dir(self.home.data.path().join("sessions"))
            .map(|d| d.count())
            .unwrap_or(0)
    }

    /// Whether the daemon still serves `session_id`'s agent terminal.
    pub async fn is_served(&self, session_id: &str) -> bool {
        self.home.manager.get(session_id).await.is_some()
    }

    /// Stop a started home session: end its agent's terminal and record it stopped, as the daemon
    /// leaves it after its process exits.
    pub async fn stop(&self, session_id: &str) {
        self.home.manager.stop_session(session_id).await;
        let mut meta = self.metadata_of(session_id);
        meta.status = "inactive".to_string();
        meta.pid = None;
        self.rewrite_metadata_of(session_id, &meta);
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.home.data.path().join("sessions").join(session_id)
    }
}

/// The New-session form's start request for a Claude CLI session on `opus`.
pub fn a_claude_cli_spec() -> StartSessionRequest {
    StartSessionRequest {
        session_type: "claude-cli".to_string(),
        model: "opus".to_string(),
        ..Default::default()
    }
}

pub fn a_daemon_with_a_project() -> World {
    a_daemon_with_a_project_recording(None)
}

pub fn a_daemon_with_a_project_recording(home_session_id: Option<&str>) -> World {
    let repo = a_repo_with_origin_main();
    let home_session_line = home_session_id
        .map(|id| format!("    home_session_id: {id}\n"))
        .unwrap_or_default();
    World {
        home: a_home_daemon(repo.path(), &home_session_line, ""),
        repo,
    }
}
