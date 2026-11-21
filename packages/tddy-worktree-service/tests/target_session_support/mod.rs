//! The setup the target-session acceptance suites share: a daemon serving one project whose
//! session's agent works in its worktree on a **target** — `StandInSshTarget`, a stand-in `ssh`
//! that runs each command on this host with every `/home/dev` path mapped under a temp directory,
//! reached through the service's [`TargetShells`] port. Real git runs on both sides.
//!
//! Used by `target_worktree_actions_acceptance.rs` and `target_worktree_stats_acceptance.rs`.

#![allow(dead_code)] // each suite uses its own part of the fixture

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use async_trait::async_trait;
use tddy_core::SessionTarget;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_remote_shell::{RemoteShell, SshShell};
use tddy_rpc::Status;
use tddy_testing_commons::remote_target::StandInSshTarget;
use tddy_worktree_service::project_storage::{self, ProjectData};
use tddy_worktree_service::target_worktrees::TargetShells;
use tddy_worktree_service::test_util::TEST_TOKEN;
use tddy_worktree_service::WorktreeServiceImpl;

pub const SESSION_ID: &str = "01a1fcba-5ee1-7240-9d69-a7512de61640";
/// The session's branch, on the target's clone.
pub const BRANCH: &str = "probe/target";
/// The session's worktree, as the session records it: a path on the target.
pub const REMOTE_WORKTREE: &str = "/home/dev/repo/.worktrees/01a1fcba-5ee1-7240-9d69-a7512de61640";
/// The target's clone of the project, on the target.
pub const REMOTE_CLONE: &str = "/home/dev/repo";
pub fn run_git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("git {args:?} in {cwd:?}: {e}"));
    assert!(status.success(), "git {args:?} failed in {cwd:?}");
}

/// What `git -C <dir> <args>` prints; nothing when `dir` is not a repository (or not there).
pub fn git_output(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} in {dir:?}: {e}"));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A repository at `dir` with one commit on `master` and an `origin` that is itself.
pub fn a_repository_at(dir: &Path) {
    std::fs::create_dir_all(dir).expect("the repository's directory");
    run_git(dir, &["init", "-q"]);
    run_git(dir, &["config", "user.email", "t@e.st"]);
    run_git(dir, &["config", "user.name", "t"]);
    std::fs::write(dir.join("README.md"), "# probe\n").expect("a tracked file");
    run_git(dir, &["add", "README.md"]);
    run_git(dir, &["commit", "-q", "-m", "init"]);
    run_git(dir, &["branch", "-M", "master"]);
    run_git(dir, &["remote", "add", "origin", dir.to_str().unwrap()]);
    run_git(dir, &["fetch", "-q", "origin"]);
}

/// How the daemon reaches the session's target.
pub enum Reach {
    /// Through the stand-in's `ssh`.
    Through(PathBuf),
    /// Not at all, for the reason given.
    Refused(String),
}

pub struct TheTargetShells(pub Reach);

#[async_trait]
impl TargetShells for TheTargetShells {
    async fn shell_for(
        &self,
        _target: &SessionTarget,
        _subject: &str,
    ) -> Result<Arc<dyn RemoteShell>, Status> {
        match &self.0 {
            Reach::Through(ssh) => Ok(Arc::new(SshShell::through(ssh.clone(), "devbox"))),
            Reach::Refused(reason) => Err(Status::failed_precondition(reason.clone())),
        }
    }
}

/// A daemon serving one project, whose session runs on a target.
pub struct ADaemon {
    pub service: WorktreeServiceImpl,
    pub project_id: String,
    pub local_clone: PathBuf,
    pub session_dir: PathBuf,
    pub target: StandInSshTarget,
    _data_dir: tempfile::TempDir,
    _local: tempfile::TempDir,
}

pub fn config_for(os_user: &str) -> DaemonConfig {
    let dir = tempfile::tempdir().unwrap().keep();
    let path = dir.join("config.yaml");
    std::fs::write(&path, "\n").unwrap();
    std::fs::write(
        dir.join("users.yaml"),
        format!("- github_user: \"testuser\"\n  os_user: \"{os_user}\"\n"),
    )
    .unwrap();
    DaemonConfig::load(&path).unwrap()
}

/// A daemon whose project has a clone here and one on the target, and whose session's agent works
/// in its worktree on the target, reached as `reach` says.
pub fn a_daemon_with_a_target_session(reach: impl FnOnce(&StandInSshTarget) -> Reach) -> ADaemon {
    let os_user = std::env::var("USER").expect("USER must be set");
    let data_dir = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    let local_clone = std::fs::canonicalize(local.path()).unwrap().join("main");
    a_repository_at(&local_clone);

    let target = StandInSshTarget::behind_ssh();
    let remote_clone = target.root.join(REMOTE_CLONE.trim_start_matches('/'));
    a_repository_at(&remote_clone);
    run_git(
        &remote_clone,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            BRANCH,
            target
                .root
                .join(REMOTE_WORKTREE.trim_start_matches('/'))
                .to_str()
                .unwrap(),
            "master",
        ],
    );

    let projects_dir = projects_path_for_user(&os_user, Some(data_dir.path())).unwrap();
    let project_id = uuid::Uuid::new_v4().to_string();
    project_storage::add_project(
        &projects_dir,
        ProjectData {
            project_id: project_id.clone(),
            name: "probe".to_string(),
            git_url: "https://example.com/probe.git".to_string(),
            main_repo_path: local_clone.display().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: std::collections::HashMap::new(),
            accounts: Vec::new(),
            local_gh: false,
            home_session_id: None,
            orchestrator_defaults: None,
            lifecycle: Default::default(),
        },
    )
    .unwrap();

    let session_dir = data_dir.path().join("sessions").join(SESSION_ID);
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(
        session_dir.join(".session.yaml"),
        format!(
            "session_id: {SESSION_ID}\nproject_id: {project_id}\n\
             created_at: 2026-11-21T06:12:45+00:00\nupdated_at: 2026-11-21T06:12:45+00:00\n\
             status: active\nrepo_path: {REMOTE_WORKTREE}\npid: null\ntool: null\n\
             livekit_room: null\npending_elicitation: false\nsession_type: claude-cli\n\
             target:\n  kind: ssh\n  config_host: devbox\n"
        ),
    )
    .unwrap();
    tddy_core::write_changeset(
        &session_dir,
        &tddy_core::Changeset {
            name: Some("probe".to_string()),
            branch: Some(BRANCH.to_string()),
            branch_suggestion: Some(BRANCH.to_string()),
            worktree_suggestion: Some(SESSION_ID.to_string()),
            effective_worktree_integration_base_ref: Some("origin/master".to_string()),
            worktree: Some(REMOTE_WORKTREE.to_string()),
            repo_path: Some(REMOTE_WORKTREE.to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let user_resolver: tddy_daemon_kernel::SessionUserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string()));
    let shells = TheTargetShells(reach(&target));
    let service = WorktreeServiceImpl::new(
        config_for(&os_user),
        data_dir.path().to_path_buf(),
        user_resolver,
    )
    .with_target_shells(Arc::new(shells));
    ADaemon {
        service,
        project_id,
        local_clone,
        session_dir,
        target,
        _data_dir: data_dir,
        _local: local,
    }
}

pub fn reachable(target: &StandInSshTarget) -> Reach {
    Reach::Through(target.root.join("bin/ssh"))
}

impl ADaemon {
    /// The session's worktree, where it lives on the target.
    pub fn remote_worktree(&self) -> PathBuf {
        self.target
            .root
            .join(REMOTE_WORKTREE.trim_start_matches('/'))
    }

    /// The target's clone of the project, where it lives on the target.
    pub fn remote_clone(&self) -> PathBuf {
        self.target.root.join(REMOTE_CLONE.trim_start_matches('/'))
    }
}
