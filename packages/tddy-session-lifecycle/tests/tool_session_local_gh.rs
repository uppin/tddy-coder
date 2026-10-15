//! A `tddy-coder` tool session of a **Local gh** project acts as the host's own `gh` login: the
//! daemon asks `gh`, as the session's OS user, who it is logged in as for the project's GitHub host,
//! and starts the child with that login's commit identity and with `TDDY_GITHUB_SOURCE` naming the
//! host — never with a token.
//!
//! Which source a session uses is decided at its start: a start that names one (a peer forwarding
//! the start of a project it knows as Local gh) is followed, and kept on resume; otherwise this
//! host's own project row decides.
//!
//! The child is a script recording the environment it was started with, spawned by the real
//! `StartSession` / `ResumeSession`. `gh` is a stub that answers only for the host it is logged in
//! to (`tddy_testing_commons::gh_cli`).
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-14-local-gh-github-source.md

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::{write_session_github_source, SessionGithubSource};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_host_service::GithubCli;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    GithubSource, ResumeSessionRequest, SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_testing_commons::builders::a_session_metadata;
use tddy_testing_commons::fs::write_session_yaml;
use tddy_testing_commons::gh_cli::a_gh_cli;
use tddy_testing_commons::process_env::ProcessEnv;

const PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";
const OWNER: &str = "testuser";
const RECORDED_SESSION: &str = "recorded-session";

const GITHUB_COM_URL: &str = "https://github.com/acme/alpha.git";
const ENTERPRISE_URL: &str = "git@github.example.com:acme/alpha.git";

const ADA_IDENTITY: [&str; 4] = [
    "GIT_AUTHOR_EMAIL=101+ada@users.noreply.github.com",
    "GIT_AUTHOR_NAME=ada",
    "GIT_COMMITTER_EMAIL=101+ada@users.noreply.github.com",
    "GIT_COMMITTER_NAME=ada",
];

const GRACE_IDENTITY: [&str; 4] = [
    "GIT_AUTHOR_EMAIL=202+grace@users.noreply.github.com",
    "GIT_AUTHOR_NAME=grace",
    "GIT_COMMITTER_EMAIL=202+grace@users.noreply.github.com",
    "GIT_COMMITTER_NAME=grace",
];

/// The variables a commit identity inherited from whoever runs the suite would arrive in.
const GIT_IDENTITY_VARIABLES: [&str; 4] = [
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "GIT_COMMITTER_NAME",
    "GIT_COMMITTER_EMAIL",
];

const GITHUB_TOKEN_VARIABLES: [&str; 4] = [
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
];

fn current_username() -> String {
    std::env::var("USER").expect("USER must be set to resolve the target account")
}

/// A stand-in for `tddy-coder` that writes the environment it was started with to `dump`, then
/// stays alive past the startup watch.
fn a_tool_that_dumps_its_environment(dir: &Path, dump: &Path) -> PathBuf {
    let path = dir.join("fake-tddy-coder.sh");
    std::fs::write(
        &path,
        format!("#!/bin/sh\nenv > \"{}\"\nsleep 5\n", dump.display()),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// How this host's own copy of the project is registered.
#[derive(Clone, Copy)]
struct ProjectRow {
    git_url: &'static str,
    local_gh: bool,
}

fn a_local_gh_project_at(git_url: &'static str) -> ProjectRow {
    ProjectRow {
        git_url,
        local_gh: true,
    }
}

/// A project row this host was never told uses Local gh, and that assigns no account.
fn a_project_this_host_never_set() -> ProjectRow {
    ProjectRow {
        git_url: GITHUB_COM_URL,
        local_gh: false,
    }
}

fn register_project(data_dir: &Path, repo: &Path, row: ProjectRow) {
    let projects_dir = data_dir.join("projects");
    std::fs::create_dir_all(&projects_dir).unwrap();
    let local_gh = if row.local_gh {
        "    local_gh: true\n"
    } else {
        ""
    };
    std::fs::write(
        projects_dir.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {PROJECT_ID}\n    name: alpha\n    git_url: \"{git_url}\"\n    main_repo_path: {repo}\n{local_gh}",
            git_url = row.git_url,
            repo = repo.display(),
        ),
    )
    .unwrap();
}

/// A daemon that spawns directly (no supervisor, no worker) and asks `gh` at `gh`.
fn a_daemon(data_dir: &Path, gh: PathBuf) -> DaemonSessionHost {
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("daemon.yaml");
    std::fs::write(
        &config_path,
        format!(
            r#"
users:
  - github_user: "{OWNER}"
    os_user: "{os_user}"
livekit:
  url: ws://127.0.0.1:1
  api_key: dummy-key
  api_secret: dummy-secret
spawn_startup_grace_period_ms: 300
spawn_startup_poll_interval_ms: 10
"#,
            os_user = current_username()
        ),
    )
    .unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    let base = data_dir.to_path_buf();
    DaemonSessionHost::new(
        config,
        Arc::new(move |_| Some(base.clone())),
        data_dir.to_path_buf(),
        Arc::new(|token| (token == TEST_TOKEN).then(|| OWNER.to_string())),
        None,
        None,
        None,
        Arc::new(tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager::new()),
    )
    .with_github_cli(GithubCli::at(gh))
}

/// What a tool session's child recorded it was started with.
struct ChildEnvironment(String);

impl ChildEnvironment {
    /// Its commit identity lines, sorted.
    fn commit_identity(&self) -> Vec<&str> {
        let mut lines: Vec<&str> = self
            .0
            .lines()
            .filter(|line| line.starts_with("GIT_AUTHOR_") || line.starts_with("GIT_COMMITTER_"))
            .collect();
        lines.sort_unstable();
        lines
    }

    fn github_source(&self) -> Vec<&str> {
        self.0
            .lines()
            .filter(|line| line.starts_with("TDDY_GITHUB_SOURCE="))
            .collect()
    }

    fn lines_containing(&self, needle: &str) -> Vec<&str> {
        self.0
            .lines()
            .filter(|line| line.contains(needle))
            .collect()
    }
}

/// A host with one project, a `gh` stub, and a tool recording its start environment.
struct World {
    /// Held so no other test of this binary changes the environment while this world is alive.
    _environment: ProcessEnv,
    data_dir: tempfile::TempDir,
    repo: tempfile::TempDir,
    tools: tempfile::TempDir,
    tool: PathBuf,
    dump: PathBuf,
    gh_environment: PathBuf,
    daemon: DaemonSessionHost,
}

/// `gh_as` builds the host's `gh` stub in the directory it is given.
fn a_host_where(
    row: ProjectRow,
    gh_as: impl FnOnce(
        tddy_testing_commons::gh_cli::GhCliStub,
    ) -> tddy_testing_commons::gh_cli::GhCliStub,
) -> World {
    a_host_in(ProcessEnv::lock(), row, gh_as)
}

/// As [`a_host_where`], in a daemon environment the test already prepared. The world holds the
/// environment lock, so no other test of this binary changes it while the world is alive.
fn a_host_in(
    environment: ProcessEnv,
    row: ProjectRow,
    gh_as: impl FnOnce(
        tddy_testing_commons::gh_cli::GhCliStub,
    ) -> tddy_testing_commons::gh_cli::GhCliStub,
) -> World {
    // The commit identity under test is the daemon's to add; one inherited from whoever runs the
    // suite would be indistinguishable from it.
    let environment = GIT_IDENTITY_VARIABLES
        .iter()
        .fold(environment, |environment, key| environment.without(key));
    let data_dir = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let dump = tools.path().join("env");
    let gh_environment = tools.path().join("gh.env");
    let tool = a_tool_that_dumps_its_environment(tools.path(), &dump);
    let gh = gh_as(a_gh_cli(tools.path()))
        .recording_env_to(&gh_environment)
        .build();
    register_project(data_dir.path(), repo.path(), row);
    let daemon = a_daemon(data_dir.path(), gh);
    World {
        _environment: environment,
        data_dir,
        repo,
        tools,
        tool,
        dump,
        gh_environment,
        daemon,
    }
}

impl World {
    async fn start_a_tool_session(&self) {
        self.start_a_tool_session_naming(GithubSource::Unspecified)
            .await;
    }

    async fn start_a_tool_session_naming(&self, github_source: GithubSource) {
        self.daemon
            .start_session(Request::direct(StartSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: PROJECT_ID.to_string(),
                tool_path: self.tool.to_str().unwrap().to_string(),
                github_source: github_source as i32,
                ..Default::default()
            }))
            .await
            .expect("the tool session starts");
    }

    /// A tool session this host recorded earlier, without starting it.
    fn a_recorded_tool_session(&self) -> PathBuf {
        let session_dir = unified_session_dir_path(self.data_dir.path(), RECORDED_SESSION);
        std::fs::create_dir_all(&session_dir).unwrap();
        write_session_yaml(
            &session_dir,
            &a_session_metadata()
                .with_session_id(RECORDED_SESSION)
                .with_project_id(PROJECT_ID)
                .with_repo_path(self.repo.path().display().to_string())
                .with_tool(self.tool.display().to_string())
                .build(),
        );
        session_dir
    }

    async fn resume_the_recorded_tool_session(&self) {
        self.daemon
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: RECORDED_SESSION.to_string(),
            }))
            .await
            .expect("the tool session resumes");
    }

    /// The environment the child recorded. The spawn returns once the child has outlived its startup
    /// watch, not once it has written anything, so this waits for the file (the child is another
    /// process — there is no event to subscribe to) up to a deadline.
    async fn child_environment(&self) -> ChildEnvironment {
        let _keep_the_tool_dir_alive = &self.tools;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            match std::fs::read_to_string(&self.dump) {
                Ok(recorded) if !recorded.is_empty() => return ChildEnvironment(recorded),
                _ if std::time::Instant::now() < deadline => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await
                }
                other => panic!("the child never recorded its environment: {other:?}"),
            }
        }
    }

    /// The environment the daemon last started `gh` with.
    fn gh_environment(&self) -> ChildEnvironment {
        ChildEnvironment(
            std::fs::read_to_string(&self.gh_environment).expect("gh was asked and recorded"),
        )
    }
}

#[tokio::test]
async fn a_local_gh_tool_session_commits_as_the_hosts_gh_login() {
    // Given a Local gh project on github.com, and gh logged in there as ada
    let world = a_host_where(a_local_gh_project_at(GITHUB_COM_URL), |gh| {
        gh.logged_in_as("ada", 101)
    });

    // When a tool session is started
    world.start_a_tool_session().await;

    // Then its child commits as ada's noreply identity
    assert_eq!(
        world.child_environment().await.commit_identity(),
        ADA_IDENTITY
    );
}

#[tokio::test]
async fn a_local_gh_tool_session_on_an_enterprise_host_asks_gh_about_that_host() {
    // Given a Local gh project on an Enterprise host, and gh logged in there as grace
    let world = a_host_where(a_local_gh_project_at(ENTERPRISE_URL), |gh| {
        gh.at_host("github.example.com").logged_in_as("grace", 202)
    });

    // When a tool session is started
    world.start_a_tool_session().await;

    // Then its child commits as grace, the login gh holds for that host
    assert_eq!(
        world.child_environment().await.commit_identity(),
        GRACE_IDENTITY
    );
}

#[tokio::test]
async fn a_local_gh_tool_session_is_told_its_github_source_and_host() {
    // Given a Local gh project on an Enterprise host
    let world = a_host_where(a_local_gh_project_at(ENTERPRISE_URL), |gh| {
        gh.at_host("github.example.com").logged_in_as("grace", 202)
    });

    // When a tool session is started
    world.start_a_tool_session().await;

    // Then its child is told to read its token from gh, for that host
    assert_eq!(
        world.child_environment().await.github_source(),
        vec!["TDDY_GITHUB_SOURCE=local-gh:github.example.com"]
    );
}

#[tokio::test]
async fn a_local_gh_tool_session_starts_with_no_token_in_any_variable() {
    // Given a Local gh project, and gh logged in as ada holding a token
    let world = a_host_where(a_local_gh_project_at(GITHUB_COM_URL), |gh| {
        gh.logged_in_as("ada", 101)
            .with_token("gho_ada_from_the_hosts_gh")
    });

    // When a tool session is started
    world.start_a_tool_session().await;

    // Then nothing the child started with carries that token
    assert_eq!(
        world
            .child_environment()
            .await
            .lines_containing("gho_ada_from_the_hosts_gh"),
        Vec::<&str>::new()
    );
}

#[tokio::test]
async fn a_local_gh_tool_session_where_gh_is_logged_out_still_starts_with_no_identity() {
    // Given a Local gh project on a host whose gh is logged in to nothing
    let world = a_host_where(a_local_gh_project_at(GITHUB_COM_URL), |gh| gh);

    // When a tool session is started
    world.start_a_tool_session().await;

    // Then it started, under the checkout's own identity: no commit identity was added
    assert_eq!(
        world.child_environment().await.commit_identity(),
        Vec::<&str>::new()
    );
}

#[tokio::test]
async fn a_local_gh_tool_session_where_gh_is_logged_out_is_still_told_its_github_source() {
    // Given a Local gh project on a host whose gh is logged in to nothing
    let world = a_host_where(a_local_gh_project_at(GITHUB_COM_URL), |gh| gh);

    // When a tool session is started
    world.start_a_tool_session().await;

    // Then its child is still told to ask gh, so a later GitHub step fails in gh's own words
    assert_eq!(
        world.child_environment().await.github_source(),
        vec!["TDDY_GITHUB_SOURCE=local-gh:github.com"]
    );
}

#[tokio::test]
async fn the_gh_a_local_gh_session_start_asks_never_sees_the_daemons_github_tokens() {
    // Given a daemon whose environment carries every GitHub token variable gh honours
    let environment = GITHUB_TOKEN_VARIABLES
        .iter()
        .fold(ProcessEnv::lock(), |environment, variable| {
            environment.setting(variable, "from_the_daemons_environment")
        });
    // And a Local gh project, with gh logged in as ada
    let world = a_host_in(environment, a_local_gh_project_at(GITHUB_COM_URL), |gh| {
        gh.logged_in_as("ada", 101)
    });

    // When a tool session is started
    world.start_a_tool_session().await;
    world.child_environment().await;

    // Then the gh the daemon asked was started with none of them
    assert_eq!(
        world
            .gh_environment()
            .lines_containing("from_the_daemons_environment"),
        Vec::<&str>::new()
    );
}

#[tokio::test]
async fn a_start_naming_local_gh_acts_as_gh_although_this_hosts_project_was_never_set() {
    // Given a host whose copy of the project was never set to Local gh, and gh logged in as ada
    let world = a_host_where(a_project_this_host_never_set(), |gh| {
        gh.logged_in_as("ada", 101)
    });

    // When a peer forwards a start naming Local gh
    world
        .start_a_tool_session_naming(GithubSource::LocalGh)
        .await;

    // Then the session acts as ada's gh login
    assert_eq!(
        world.child_environment().await.commit_identity(),
        ADA_IDENTITY
    );
}

#[tokio::test]
async fn a_resumed_session_that_was_started_naming_local_gh_still_acts_as_gh() {
    // Given a host whose copy of the project was never set to Local gh, and gh logged in as ada
    let world = a_host_where(a_project_this_host_never_set(), |gh| {
        gh.logged_in_as("ada", 101)
    });
    // And a session it recorded as started naming Local gh
    let session_dir = world.a_recorded_tool_session();
    write_session_github_source(&session_dir, SessionGithubSource::LocalGh)
        .expect("record the session's source");

    // When the session is resumed
    world.resume_the_recorded_tool_session().await;

    // Then it still acts as ada's gh login
    assert_eq!(
        world.child_environment().await.commit_identity(),
        ADA_IDENTITY
    );
}

#[tokio::test]
async fn a_resumed_session_of_a_local_gh_project_acts_as_gh() {
    // Given a Local gh project, gh logged in as ada, and a session recorded with no source of its own
    let world = a_host_where(a_local_gh_project_at(GITHUB_COM_URL), |gh| {
        gh.logged_in_as("ada", 101)
    });
    world.a_recorded_tool_session();

    // When the session is resumed
    world.resume_the_recorded_tool_session().await;

    // Then it acts as ada's gh login, as this host's project row says
    assert_eq!(
        world.child_environment().await.commit_identity(),
        ADA_IDENTITY
    );
}
