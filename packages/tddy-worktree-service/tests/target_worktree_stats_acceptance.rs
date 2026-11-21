//! Acceptance: the Worktree tab of a session whose agent runs on a **target** shows that
//! session's worktree there — branch, diff summary and size measured on the target — without
//! holding up the local rows (`#target-parity` 7/7).
//!
//! On the Coder probe of 2026-11-21 the tab of the target session `01a1fcba-…` found no row for
//! its `/home/coder/…` worktree and showed it as missing.
//!
//! The target setup is the one `target_worktree_actions_acceptance.rs` uses (duplicated here;
//! recorded for the stack's sweep node to share).
//!
//! The target is `StandInSshTarget`: a stand-in `ssh` that runs each command on this host with
//! every `/home/dev` path mapped under a temp directory, reached through the service's
//! [`TargetShells`] port. Real git runs on both sides.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-21-target-worktree-stats.md.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use pretty_assertions::assert_eq;
use tddy_core::SessionTarget;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_remote_shell::{RemoteShell, SshShell};
use tddy_rpc::{Request, Status};
use tddy_service::proto::worktree::*;
use tddy_testing_commons::remote_target::StandInSshTarget;
use tddy_worktree_service::project_storage::{self, ProjectData};
use tddy_worktree_service::target_worktrees::TargetShells;
use tddy_worktree_service::test_util::TEST_TOKEN;
use tddy_worktree_service::WorktreeServiceImpl;

const SESSION_ID: &str = "01a1fcba-5ee1-7240-9d69-a7512de61640";
/// The session's branch, on the target's clone.
const BRANCH: &str = "probe/target";
/// The session's worktree, as the session records it: a path on the target.
const REMOTE_WORKTREE: &str = "/home/dev/repo/.worktrees/01a1fcba-5ee1-7240-9d69-a7512de61640";
/// The target's clone of the project, on the target.
const REMOTE_CLONE: &str = "/home/dev/repo";
/// A tracked file the agent changed in its worktree.
const CHANGED: &str = "README.md";

fn run_git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("git {args:?} in {cwd:?}: {e}"));
    assert!(status.success(), "git {args:?} failed in {cwd:?}");
}

/// A repository at `dir` with one commit on `master` and an `origin` that is itself.
fn a_repository_at(dir: &Path) {
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
enum Reach {
    /// Through the stand-in's `ssh`.
    Through(PathBuf),
    /// Not at all, for the reason given.
    Refused(String),
}

struct TheTargetShells(Reach);

#[async_trait]
impl TargetShells for TheTargetShells {
    async fn shell_for(
        &self,
        _target: &SessionTarget,
        _os_user: &str,
    ) -> Result<Arc<dyn RemoteShell>, Status> {
        match &self.0 {
            Reach::Through(ssh) => Ok(Arc::new(SshShell::through(ssh.clone(), "devbox"))),
            Reach::Refused(reason) => Err(Status::failed_precondition(reason.clone())),
        }
    }
}

/// A daemon serving one project, whose session runs on a target.
struct ADaemon {
    service: WorktreeServiceImpl,
    project_id: String,
    local_clone: PathBuf,
    target: StandInSshTarget,
    _data_dir: tempfile::TempDir,
    _local: tempfile::TempDir,
}

fn config_for(os_user: &str) -> DaemonConfig {
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
fn a_daemon_with_a_target_session(reach: impl FnOnce(&StandInSshTarget) -> Reach) -> ADaemon {
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
        target,
        _data_dir: data_dir,
        _local: local,
    }
}

fn reachable(target: &StandInSshTarget) -> Reach {
    Reach::Through(target.root.join("bin/ssh"))
}

/// The target answers each command only after `delay`, as a slow transport does.
fn slowly(delay: std::time::Duration) -> impl FnOnce(&StandInSshTarget) -> Reach {
    move |target| {
        let slow = target.root.join("bin/slow-ssh");
        tddy_testing_commons::remote_target::write_executable(
            &slow,
            &format!(
                "#!/bin/sh\n/bin/sleep {}\nexec '{}' \"$@\"\n",
                delay.as_secs_f64(),
                target.root.join("bin/ssh").display()
            ),
        );
        Reach::Through(slow)
    }
}

impl ADaemon {
    /// The session's worktree, where it lives on the target.
    fn remote_worktree(&self) -> PathBuf {
        self.target
            .root
            .join(REMOTE_WORKTREE.trim_start_matches('/'))
    }

    /// The agent changed a tracked file in its worktree on the target: two lines added.
    fn agent_changed_a_file(&self) {
        std::fs::write(
            self.remote_worktree().join(CHANGED),
            "# probe\nfirst line added\nsecond line added\n",
        )
        .unwrap();
    }

    /// The Worktree tab's stream of the project's worktrees.
    async fn streams(&self) -> tddy_worktree_service::MpscWorktreeStatsStream {
        self.service
            .stream_worktree_stats(Request::direct(StreamWorktreeStatsRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: self.project_id.clone(),
                recalculate_all: false,
            }))
            .await
            .expect("the stream opens")
            .into_inner()
    }
}

/// Every row the stream carries for `path`, in order — the snapshot's and each update's — until
/// one satisfies `done` or [`OBSERVE`] passes. The last row seen, if any.
async fn the_row_for(
    stream: &mut tddy_worktree_service::MpscWorktreeStatsStream,
    path: &str,
    done: impl Fn(&WorktreeRow) -> bool,
) -> Option<WorktreeRow> {
    let mut last = None;
    let _ = tokio::time::timeout(OBSERVE, async {
        while let Some(Ok(event)) = stream.next().await {
            for row in event.snapshot.into_iter().chain(event.updated) {
                if row.path == path {
                    let finished = done(&row);
                    last = Some(row);
                    if finished {
                        return;
                    }
                }
            }
        }
    })
    .await;
    last
}

/// A safety net for the stand-in target to answer, not a prediction of how long it takes.
const OBSERVE: std::time::Duration = std::time::Duration::from_secs(20);

fn sized(row: &WorktreeRow) -> bool {
    row.size_status == WorktreeSizeStatus::Cached as i32
}

#[tokio::test]
async fn the_stream_holds_a_target_sessions_worktree_with_its_branch_diff_and_size() {
    // Given a target session whose agent changed a file in its worktree there
    let daemon = a_daemon_with_a_target_session(reachable);
    daemon.agent_changed_a_file();

    // When the Worktree tab streams the project's worktrees
    let mut stream = daemon.streams().await;
    let row = the_row_for(&mut stream, REMOTE_WORKTREE, sized).await;

    // Then the session's worktree is there, measured on the target
    assert_eq!(
        row.map(|r| (
            r.branch_label,
            r.changed_files,
            r.lines_added,
            r.stale,
            r.disk_bytes > 0
        )),
        Some((BRANCH.to_string(), 1, 2, false, true))
    );
}

#[tokio::test]
async fn the_local_snapshot_is_sent_before_the_target_answers() {
    // Given a target session on a target that takes seconds to answer each command
    let daemon = a_daemon_with_a_target_session(slowly(std::time::Duration::from_secs(5)));

    // When the stream opens
    let started = std::time::Instant::now();
    let mut stream = daemon.streams().await;
    let first = stream.next().await.and_then(Result::ok);

    // Then its first event — the snapshot holding the local clone — comes at once
    assert_eq!(
        (
            first.map(|event| event
                .snapshot
                .iter()
                .any(|r| Path::new(&r.path) == daemon.local_clone)),
            started.elapsed() < std::time::Duration::from_secs(3),
        ),
        (Some(true), true)
    );
}

#[tokio::test]
async fn an_unreachable_targets_row_is_stale_without_numbers() {
    // Given a target session whose target cannot be reached now
    let daemon = a_daemon_with_a_target_session(|_| {
        Reach::Refused("the Coder vault of ada is locked".to_string())
    });

    // When
    let mut stream = daemon.streams().await;
    let row = the_row_for(&mut stream, REMOTE_WORKTREE, |r| r.stale).await;

    // Then its row is there — not missing — marked stale, with no numbers
    assert_eq!(
        row.map(|r| (r.stale, r.disk_bytes, r.changed_files)),
        Some((true, 0, 0))
    );
}

#[tokio::test]
async fn calculating_a_target_worktrees_size_measures_it_on_the_target() {
    // Given an open stream for a project with a target session
    let daemon = a_daemon_with_a_target_session(reachable);
    let mut stream = daemon.streams().await;

    // When the tab asks for the target worktree's size
    let asked = daemon
        .service
        .calculate_worktree_size(Request::direct(CalculateWorktreeSizeRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: daemon.project_id.clone(),
            worktree_path: REMOTE_WORKTREE.to_string(),
        }))
        .await
        .map(|r| r.into_inner().ok)
        .map_err(|s| s.message().to_string());
    let row = the_row_for(&mut stream, REMOTE_WORKTREE, sized).await;

    // Then it is accepted, and the stream carries the size measured there
    assert_eq!(
        (asked, row.map(|r| r.disk_bytes > 0)),
        (Ok(true), Some(true))
    );
}
