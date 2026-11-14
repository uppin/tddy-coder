//! Acceptance: a project-home session commits and pushes a target session's work at any time, and
//! rebases, stashes, resets, checks out, cleans or force-pushes only while that session is not
//! active. A conflicting rebase is aborted with its paths; the home worktree is never a target.
//!
//! PRD: docs/ft/daemon/project-concept.md § Git tools (`#project-home` 7/7).

use std::path::{Path, PathBuf};

use tddy_daemon_rpc::test_util::{test_service, TestDaemon};
use tddy_projects::project_storage::{write_projects, ProjectData};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::session::{SessionService as SessionServiceTrait, StartSessionRequest};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const THIS_PROJECT: &str = "019d105b-ac0f-78d3-9a89-409731145a3d";
const OTHER_PROJECT: &str = "019d105b-ac0f-78d3-9a89-409731145a3e";

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} must succeed in {cwd:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn a_repo_with_origin() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q", "-b", "main"]);
    // The daemon commits and rebases in worktrees of this repository, which share its config; a
    // CI runner has no global identity for them to fall back on.
    git(repo.path(), &["config", "user.name", "Test"]);
    git(repo.path(), &["config", "user.email", "t@t.com"]);
    git(
        repo.path(),
        &["commit", "-q", "--allow-empty", "-m", "init"],
    );
    git(
        repo.path(),
        &["remote", "add", "origin", repo.path().to_str().unwrap()],
    );
    git(repo.path(), &["push", "-q", "-u", "origin", "main"]);
    repo
}

fn a_project(project_id: &str, repo: &Path) -> ProjectData {
    ProjectData {
        project_id: project_id.to_string(),
        name: project_id.to_string(),
        git_url: String::new(),
        main_repo_path: repo.display().to_string(),
        main_branch_ref: Some("origin/main".to_string()),
        remote_name: None,
        host_repo_paths: Default::default(),
        accounts: Vec::new(),
        local_gh: false,
        home_session_id: None,
    }
}

/// A daemon with two projects: a project-home session and two ordinary sessions in this project,
/// and one session in the other project.
struct World {
    service: TestDaemon,
    sessions: tempfile::TempDir,
    home: String,
    sibling: String,
    _second_sibling: String,
    _foreign: String,
    _repos: (tempfile::TempDir, tempfile::TempDir),
}

impl World {
    fn worktree_of(&self, session_id: &str) -> PathBuf {
        let dir = tddy_core::session_lifecycle::unified_session_dir_path(
            self.sessions.path(),
            session_id,
        );
        PathBuf::from(
            tddy_core::read_session_metadata(&dir)
                .expect("session metadata")
                .repo_path
                .expect("worktree"),
        )
    }

    async fn call_as(
        &self,
        session_id: &str,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<ExecuteToolResponse, tddy_rpc::Status> {
        self.service
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    async fn call_as_home(&self, tool: &str, args: serde_json::Value) -> ExecuteToolResponse {
        self.call_as(&self.home, tool, args)
            .await
            .unwrap_or_else(|s| panic!("{tool} is served to the home session: {s}"))
    }
}

async fn a_workspace_in(service: &TestDaemon, project_id: &str) -> String {
    service
        .start_session(Request::direct(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: project_id.to_string(),
            session_type: "workspace".to_string(),
            ..Default::default()
        }))
        .await
        .expect("workspace session starts")
        .into_inner()
        .session_id
}

async fn a_project_home_world() -> World {
    let this_repo = a_repo_with_origin();
    let other_repo = a_repo_with_origin();
    let sessions = tempfile::tempdir().unwrap();
    write_projects(
        &sessions.path().join("projects"),
        &[
            a_project(THIS_PROJECT, this_repo.path()),
            a_project(OTHER_PROJECT, other_repo.path()),
        ],
    )
    .unwrap();
    let service = test_service(sessions.path().to_path_buf());
    let home = a_workspace_in(&service, THIS_PROJECT).await;
    let sibling = a_workspace_in(&service, THIS_PROJECT).await;
    let second_sibling = a_workspace_in(&service, THIS_PROJECT).await;
    let foreign = a_workspace_in(&service, OTHER_PROJECT).await;
    let home_dir = tddy_core::session_lifecycle::unified_session_dir_path(sessions.path(), &home);
    let mut meta = tddy_core::read_session_metadata(&home_dir).unwrap();
    meta.session_role = Some("project-home".to_string());
    tddy_core::session_metadata::write_session_metadata(&home_dir, &meta).unwrap();
    World {
        service,
        sessions,
        home,
        sibling,
        _second_sibling: second_sibling,
        _foreign: foreign,
        _repos: (this_repo, other_repo),
    }
}

impl World {
    /// Make `session_id` active (its recorded process is alive) or stopped.
    fn set_active(&self, session_id: &str, active: bool) {
        let dir = tddy_core::session_lifecycle::unified_session_dir_path(
            self.sessions.path(),
            session_id,
        );
        let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
        meta.pid = active.then(std::process::id);
        meta.status = if active { "active" } else { "inactive" }.to_string();
        tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
    }

    fn origin(&self) -> &Path {
        self._repos.0.path()
    }

    async fn git_tool(&self, tool: &str, args: serde_json::Value) -> ExecuteToolResponse {
        self.call_as_home(tool, args).await
    }
}

/// A committed change on the sibling's branch touching `file`.
fn a_commit_on(worktree: &Path, file: &str, contents: &str) {
    std::fs::write(worktree.join(file), contents).unwrap();
    git(worktree, &["add", file]);
    git(worktree, &["commit", "-q", "-m", &format!("edit {file}")]);
}

fn assert_refused_as_active(response: &ExecuteToolResponse, session_id: &str) {
    assert!(response.is_error, "expected a refusal");
    assert!(
        response.error_message.contains(session_id) && response.error_message.contains("is active"),
        "refusal names the active session: {}",
        response.error_message
    );
}

#[tokio::test]
async fn commit_and_push_work_on_an_active_target() {
    // Given an active sibling with an uncommitted file
    let world = a_project_home_world().await;
    world.set_active(&world.sibling, true);
    let worktree = world.worktree_of(&world.sibling);
    std::fs::write(worktree.join("done.txt"), "done\n").unwrap();

    // When
    let commit = world
        .git_tool(
            "CommitSessionWorktree",
            serde_json::json!({ "session_id": world.sibling, "message": "land the work" }),
        )
        .await;
    let push = world
        .git_tool(
            "PushSessionBranch",
            serde_json::json!({ "session_id": world.sibling }),
        )
        .await;

    // Then
    assert!(!commit.is_error, "{}", commit.error_message);
    assert_eq!(
        git(&worktree, &["log", "-1", "--format=%s"]),
        "land the work"
    );
    assert!(!push.is_error, "{}", push.error_message);
    let branch = git(&worktree, &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert_eq!(
        git(
            world.origin(),
            &["rev-parse", &format!("refs/heads/{branch}")]
        ),
        git(&worktree, &["rev-parse", "HEAD"])
    );
}

#[tokio::test]
async fn rebase_on_an_active_target_is_refused_naming_the_session() {
    // Given
    let world = a_project_home_world().await;
    world.set_active(&world.sibling, true);

    // When
    let rebase = world
        .git_tool(
            "RebaseSessionBranch",
            serde_json::json!({ "session_id": world.sibling, "onto": "origin/main" }),
        )
        .await;

    // Then
    assert_refused_as_active(&rebase, &world.sibling);
}

#[tokio::test]
async fn rebase_on_a_stopped_target_moves_its_branch_onto_the_base() {
    // Given a stopped sibling with one commit, and a main branch that moved on
    let world = a_project_home_world().await;
    world.set_active(&world.sibling, false);
    let worktree = world.worktree_of(&world.sibling);
    a_commit_on(&worktree, "feature.txt", "feature\n");
    a_commit_on(world.origin(), "upstream.txt", "upstream\n");
    git(world.origin(), &["push", "-q", "origin", "main"]);

    // When
    let rebase = world
        .git_tool(
            "RebaseSessionBranch",
            serde_json::json!({ "session_id": world.sibling, "onto": "origin/main" }),
        )
        .await;

    // Then
    assert!(!rebase.is_error, "{}", rebase.error_message);
    git(&worktree, &["fetch", "-q", "origin"]);
    assert_eq!(
        git(&worktree, &["merge-base", "HEAD", "origin/main"]),
        git(&worktree, &["rev-parse", "origin/main"])
    );
    assert!(worktree.join("feature.txt").exists() && worktree.join("upstream.txt").exists());
}

#[tokio::test]
async fn a_conflicting_rebase_is_aborted_and_reports_the_paths() {
    // Given a stopped sibling and main both changing the same file differently
    let world = a_project_home_world().await;
    world.set_active(&world.sibling, false);
    let worktree = world.worktree_of(&world.sibling);
    a_commit_on(&worktree, "shared.txt", "ours\n");
    let before = git(&worktree, &["rev-parse", "HEAD"]);
    a_commit_on(world.origin(), "shared.txt", "theirs\n");
    git(world.origin(), &["push", "-q", "origin", "main"]);

    // When
    let rebase = world
        .git_tool(
            "RebaseSessionBranch",
            serde_json::json!({ "session_id": world.sibling, "onto": "origin/main" }),
        )
        .await;

    // Then — conflict reported, rebase aborted, worktree as it was
    let result: serde_json::Value = serde_json::from_str(&rebase.result_json).expect("JSON result");
    assert_eq!(result["conflict_paths"], serde_json::json!(["shared.txt"]));
    assert_eq!(git(&worktree, &["rev-parse", "HEAD"]), before);
    assert_eq!(git(&worktree, &["status", "--porcelain"]), "");
}

#[tokio::test]
async fn reset_stash_checkout_and_clean_are_refused_on_an_active_target() {
    // Given
    let world = a_project_home_world().await;
    world.set_active(&world.sibling, true);
    let sid = world.sibling.clone();

    // When / Then
    for (tool, args) in [
        (
            "ResetSessionWorktree",
            serde_json::json!({ "session_id": sid, "mode": "hard", "to": "HEAD" }),
        ),
        (
            "StashSessionWorktree",
            serde_json::json!({ "session_id": sid }),
        ),
        (
            "CheckoutSessionWorktree",
            serde_json::json!({ "session_id": sid, "to": "main" }),
        ),
        (
            "CleanSessionWorktree",
            serde_json::json!({ "session_id": sid }),
        ),
    ] {
        let response = world.git_tool(tool, args).await;
        assert_refused_as_active(&response, &sid);
    }
}

#[tokio::test]
async fn reset_on_a_stopped_target_moves_its_head() {
    // Given a stopped sibling two commits ahead
    let world = a_project_home_world().await;
    world.set_active(&world.sibling, false);
    let worktree = world.worktree_of(&world.sibling);
    a_commit_on(&worktree, "one.txt", "1\n");
    let first = git(&worktree, &["rev-parse", "HEAD"]);
    a_commit_on(&worktree, "two.txt", "2\n");

    // When
    let reset = world
        .git_tool(
            "ResetSessionWorktree",
            serde_json::json!({ "session_id": world.sibling, "mode": "hard", "to": first }),
        )
        .await;

    // Then
    assert!(!reset.is_error, "{}", reset.error_message);
    assert_eq!(git(&worktree, &["rev-parse", "HEAD"]), first);
    assert!(!worktree.join("two.txt").exists());
}

#[tokio::test]
async fn force_with_lease_push_is_refused_on_an_active_target() {
    // Given
    let world = a_project_home_world().await;
    world.set_active(&world.sibling, true);

    // When
    let push = world
        .git_tool(
            "PushSessionBranch",
            serde_json::json!({ "session_id": world.sibling, "force_with_lease": true }),
        )
        .await;

    // Then
    assert_refused_as_active(&push, &world.sibling);
}

#[tokio::test]
async fn the_home_worktree_is_never_a_git_target() {
    // Given
    let world = a_project_home_world().await;

    // When
    let commit = world
        .git_tool(
            "CommitSessionWorktree",
            serde_json::json!({ "session_id": world.home, "message": "nope" }),
        )
        .await;

    // Then
    assert!(commit.is_error);
    assert!(
        commit.error_message.contains("project-home"),
        "{}",
        commit.error_message
    );
}

// ── the project's remote and account (`#orchestrators` 10/11) ────────────────────────────────

mod support;

/// The token a Local gh project's account resolves to: what the host's `gh` stub answers.
const PROJECTS_ACCOUNT_TOKEN: &str = "gho_the_projects_account";

/// A host whose project is on GitHub as the host's `gh` login (Local gh), its repository's only
/// remote a bare `upstream` the project's row names, and a `pre-push` hook recording into
/// `credential_record` the credential git can obtain for github.com from inside that push. The hook
/// reads no user or system git config, so only what the daemon hands its git can answer.
struct AnUpstreamProject {
    host: support::Host,
    upstream: tempfile::TempDir,
    credential_record: PathBuf,
    _scratch: tempfile::TempDir,
}

fn an_upstream_project_on_github() -> AnUpstreamProject {
    use std::os::unix::fs::PermissionsExt;
    let scratch = tempfile::tempdir().unwrap();
    let gh = tddy_testing_commons::gh_cli::a_gh_cli(scratch.path())
        .logged_in_as("acme-bot", 7)
        .with_token(PROJECTS_ACCOUNT_TOKEN)
        .build();
    let peers = std::sync::Arc::new(support::InProcessPeers::default());
    let host = support::a_host_as_current_user_with_gh_at(support::HOST_A, &peers, &gh);
    let projects_dir = host.sessions.path().join("projects");
    let mut rows = tddy_projects::project_storage::read_projects(&projects_dir).unwrap();
    let row = rows
        .iter_mut()
        .find(|row| row.project_id == support::THIS_PROJECT)
        .expect("the project is registered");
    let repo = PathBuf::from(&row.main_repo_path);
    let upstream = tempfile::tempdir().unwrap();
    git(upstream.path(), &["init", "-q", "--bare", "-b", "main"]);
    git(&repo, &["remote", "remove", "origin"]);
    git(
        &repo,
        &[
            "remote",
            "add",
            "upstream",
            upstream.path().to_str().unwrap(),
        ],
    );
    git(&repo, &["push", "-q", "-u", "upstream", "main"]);
    git(&repo, &["fetch", "-q", "upstream"]);
    row.remote_name = Some("upstream".to_string());
    row.main_branch_ref = Some("upstream/main".to_string());
    row.git_url = "https://github.com/acme/alpha.git".to_string();
    row.local_gh = true;
    write_projects(&projects_dir, &rows).unwrap();

    let hooks = repo.join(".git").join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    git(
        &repo,
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    );
    let credential_record = scratch.path().join("credential.txt");
    let isolated_home = scratch.path().join("home");
    std::fs::create_dir_all(&isolated_home).unwrap();
    let hook = hooks.join("pre-push");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\nprintf 'protocol=https\\nhost=github.com\\n\\n' | HOME='{home}' XDG_CONFIG_HOME='{home}' GIT_CONFIG_NOSYSTEM=1 git credential fill > '{record}' 2>/dev/null\nexit 0\n",
            home = isolated_home.display(),
            record = credential_record.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    AnUpstreamProject {
        host,
        upstream,
        credential_record,
        _scratch: scratch,
    }
}

impl AnUpstreamProject {
    /// Whether the push's git could obtain the project account's token for github.com.
    fn the_push_could_authenticate_as_the_projects_account(&self) -> bool {
        std::fs::read_to_string(&self.credential_record)
            .unwrap_or_default()
            .lines()
            .any(|line| line == format!("password={PROJECTS_ACCOUNT_TOKEN}"))
    }
}

#[tokio::test]
async fn push_session_branch_pushes_to_the_projects_remote_with_the_account_token() {
    // Given a project whose only remote is `upstream`, acting as the host's gh login, and a session
    // of it with a commit on its branch; main is the project's home
    let project = an_upstream_project_on_github();
    let host = &project.host;
    let main = support::a_workspace_in(&host.daemon, support::THIS_PROJECT).await;
    host.make_project_home(&main);
    let sibling = support::a_workspace_in(&host.daemon, support::THIS_PROJECT).await;
    let worktree = host.worktree_of(&sibling);
    std::fs::write(worktree.join("done.txt"), "done\n").unwrap();
    git(&worktree, &["add", "done.txt"]);
    git(&worktree, &["commit", "-q", "-m", "land the work"]);

    // When main pushes the sibling's branch
    let pushed = host
        .daemon
        .execute_tool(Request::direct(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: main.clone(),
            tool_name: "PushSessionBranch".to_string(),
            args_json: serde_json::json!({ "session_id": sibling }).to_string(),
            daemon_instance_id: String::new(),
            conversation_id: String::new(),
        }))
        .await
        .expect("PushSessionBranch is served to main")
        .into_inner();

    // Then the branch landed on the project's remote, `upstream` ...
    assert!(!pushed.is_error, "{}", pushed.error_message);
    let branch = git(&worktree, &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert_eq!(
        git(
            project.upstream.path(),
            &["rev-parse", &format!("refs/heads/{branch}")]
        ),
        git(&worktree, &["rev-parse", "HEAD"])
    );
    // ... and the push's git could authenticate as the project's account
    assert!(
        project.the_push_could_authenticate_as_the_projects_account(),
        "git, pushing for the daemon, could not obtain the project account's token for github.com"
    );
}
