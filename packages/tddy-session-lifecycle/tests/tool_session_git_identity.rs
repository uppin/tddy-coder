//! A `tddy-coder` tool session starts and resumes under the commit identity of the account its
//! project acts as — the same resolution every other session path makes.
//!
//! The daemon resolves the project's assignments once, over the signed-in owner's vault, and hands
//! the child the four `GIT_AUTHOR_*` / `GIT_COMMITTER_*` pairs through the spawn. A project that
//! resolves to nothing still starts its session, under the checkout's own identity, with no
//! variable added (the developer-consented refusal). No credential is among the pairs, and the
//! daemon's own `GITHUB_TOKEN` changes nothing.
//!
//! The child here is a script that records the environment it was started with, spawned by the real
//! `StartSession` / `ResumeSession` through the in-process spawn path — the same `ToolSpawnPlan`,
//! `SpawnOptions` and `spawn_as_user` the forked worker and the supervisor feed.
//!
//! Changeset: docs/dev/1-WIP/2026-09-19-keyring-github-identity.md

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_accounts::{META_SUBJECT, META_SUBJECT_ID, PROVIDER_GITHUB};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_credentials::{
    AccountId, CredentialRecord, CredentialStore, ProviderId, SecretString, SessionVaults,
    FIRST_VERSION,
};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    ResumeSessionRequest, SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_testing_commons::builders::a_session_metadata;
use tddy_testing_commons::fs::write_session_yaml;

const PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";
const OWNER: &str = "testuser";
const PASSPHRASE: &str = "correct horse battery staple";

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

fn ada_holding_account(account: &str) -> CredentialRecord {
    CredentialRecord {
        provider: ProviderId::new(PROVIDER_GITHUB),
        account: AccountId::new(account),
        label: "ada's account".to_string(),
        secret: SecretString::new("ghp_ada_token"),
        metadata: BTreeMap::from([
            (META_SUBJECT_ID.to_string(), "101".to_string()),
            (META_SUBJECT.to_string(), "ada".to_string()),
        ]),
        updated_at: 1_700_000_000,
        version: FIRST_VERSION,
    }
}

/// The vaults of a daemon on which the owner has unlocked a vault holding `acct-ada`.
fn vaults_holding_ada(dir: &Path) -> Arc<SessionVaults> {
    let passphrase = SecretString::new(PASSPHRASE);
    let store = CredentialStore::create(&CredentialStore::path_in(dir, OWNER), &passphrase, OWNER)
        .expect("the owner's vault is created");
    store
        .put(ada_holding_account("acct-ada"))
        .expect("the record is retained");
    let vaults = Arc::new(SessionVaults::new(dir));
    vaults
        .unlock(OWNER, &passphrase)
        .expect("the passphrase opens the vault");
    vaults
}

/// A daemon that spawns directly (no supervisor, no worker) and holds the owner's unlocked vault.
fn a_daemon(data_dir: &Path, vault_dir: &Path) -> DaemonSessionHost {
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
    .with_credential_vaults(vaults_holding_ada(vault_dir))
}

/// Register the project, assigning `accounts` (`(provider, account_id)` rows).
fn register_project(data_dir: &Path, repo: &Path, accounts: &[&str]) {
    let projects_dir = data_dir.join("projects");
    std::fs::create_dir_all(&projects_dir).unwrap();
    let rows: String = accounts
        .iter()
        .map(|account| {
            format!("      - provider: {PROVIDER_GITHUB}\n        account_id: {account}\n")
        })
        .collect();
    let accounts_yaml = if accounts.is_empty() {
        "    accounts: []\n".to_string()
    } else {
        format!("    accounts:\n{rows}")
    };
    let yaml = format!(
        "projects:\n  - project_id: {PROJECT_ID}\n    name: alpha\n    git_url: \"\"\n    main_repo_path: {}\n{accounts_yaml}",
        repo.display()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
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

    fn secret_bearing_lines(&self) -> Vec<&str> {
        self.0
            .lines()
            .filter(|line| line.contains("ghp_"))
            .collect()
    }
}

/// A world with one project assigning `accounts`, and a tool recording its start environment.
struct World {
    data_dir: tempfile::TempDir,
    _vault_dir: tempfile::TempDir,
    _repo: tempfile::TempDir,
    tools: tempfile::TempDir,
    tool: PathBuf,
    dump: PathBuf,
    daemon: DaemonSessionHost,
}

fn a_world_where_the_project_assigns(accounts: &[&str]) -> World {
    // The commit identity under test is the daemon's to add; one inherited from whoever runs the
    // suite would be indistinguishable from it.
    for key in [
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
    ] {
        std::env::remove_var(key);
    }
    let data_dir = tempfile::tempdir().unwrap();
    let vault_dir = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let dump = tools.path().join("env");
    let tool = a_tool_that_dumps_its_environment(tools.path(), &dump);
    register_project(data_dir.path(), repo.path(), accounts);
    let daemon = a_daemon(data_dir.path(), vault_dir.path());
    World {
        data_dir,
        _vault_dir: vault_dir,
        _repo: repo,
        tools,
        tool,
        dump,
        daemon,
    }
}

impl World {
    async fn start_a_tool_session(&self) {
        self.daemon
            .start_session(Request::direct(StartSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: PROJECT_ID.to_string(),
                tool_path: self.tool.to_str().unwrap().to_string(),
                ..Default::default()
            }))
            .await
            .expect("the tool session starts");
    }

    async fn resume_a_tool_session(&self) {
        let session_dir = unified_session_dir_path(self.data_dir.path(), "resumed-session");
        std::fs::create_dir_all(&session_dir).unwrap();
        write_session_yaml(
            &session_dir,
            &a_session_metadata()
                .with_session_id("resumed-session")
                .with_project_id(PROJECT_ID)
                .with_repo_path(self._repo.path().display().to_string())
                .with_tool(self.tool.display().to_string())
                .build(),
        );
        self.daemon
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: "resumed-session".to_string(),
            }))
            .await
            .expect("the tool session resumes");
    }

    /// The environment the child recorded. The spawn returns once the child has outlived its startup
    /// watch, not once it has written anything, so this waits for the file (the child is another
    /// process — there is no event to subscribe to) up to a deadline.
    async fn child_environment(&self) -> ChildEnvironment {
        let _keep_the_tool_dir_alive = &self.tools;
        // 10s: under a loaded CI host a freshly exec'd shell can take seconds to reach its first line.
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
}

const ADA_IDENTITY: [&str; 4] = [
    "GIT_AUTHOR_EMAIL=101+ada@users.noreply.github.com",
    "GIT_AUTHOR_NAME=ada",
    "GIT_COMMITTER_EMAIL=101+ada@users.noreply.github.com",
    "GIT_COMMITTER_NAME=ada",
];

#[tokio::test]
async fn a_started_tool_session_commits_as_the_account_its_project_acts_as() {
    // Given a project assigned to ada's account, which the owner's vault holds
    let world = a_world_where_the_project_assigns(&["acct-ada"]);

    // When a tool session is started
    world.start_a_tool_session().await;

    // Then its child started with her four pairs, exactly
    assert_eq!(world.child_environment().await.commit_identity(), ADA_IDENTITY);
}

#[tokio::test]
async fn a_resumed_tool_session_commits_as_the_account_its_project_acts_as() {
    // Given a project assigned to ada's account, and a tool session recorded for it
    let world = a_world_where_the_project_assigns(&["acct-ada"]);

    // When the session is resumed
    world.resume_a_tool_session().await;

    // Then its child started with her four pairs, exactly
    assert_eq!(world.child_environment().await.commit_identity(), ADA_IDENTITY);
}

#[tokio::test]
async fn a_tool_session_whose_project_assigns_no_account_still_starts_with_no_identity_added() {
    // Given a project that assigns no account
    let world = a_world_where_the_project_assigns(&[]);

    // When a tool session is started
    world.start_a_tool_session().await;

    // Then it started, under the checkout's own identity: no variable was added
    assert_eq!(
        world.child_environment().await.commit_identity(),
        Vec::<&str>::new()
    );
}

#[tokio::test]
async fn a_tool_session_whose_account_this_host_does_not_hold_still_starts_with_no_identity_added() {
    // Given a project assigned to an account the owner's vault does not hold
    let world = a_world_where_the_project_assigns(&["acct-unknown"]);

    // When a tool session is started
    world.start_a_tool_session().await;

    // Then it started, under the checkout's own identity
    assert_eq!(
        world.child_environment().await.commit_identity(),
        Vec::<&str>::new()
    );
}

#[tokio::test]
async fn the_accounts_token_is_in_no_variable_the_tool_session_starts_with() {
    // Given a project assigned to ada's account, whose token is `ghp_ada_token`
    let world = a_world_where_the_project_assigns(&["acct-ada"]);

    // When a tool session is started
    world.start_a_tool_session().await;

    // Then nothing the child started with carries it
    assert_eq!(
        world.child_environment().await.secret_bearing_lines(),
        Vec::<&str>::new()
    );
}

#[tokio::test]
async fn a_github_token_in_the_daemons_environment_gives_an_unassigned_project_no_identity() {
    // Given GITHUB_TOKEN exported where the daemon runs, and a project that assigns no account
    std::env::set_var("GITHUB_TOKEN", "from_the_daemons_environment");
    std::env::set_var("GH_TOKEN", "from_the_daemons_environment");
    let world = a_world_where_the_project_assigns(&[]);

    // When a tool session is started
    world.start_a_tool_session().await;

    // Then the environment rescued nothing: no identity was resolved from it
    assert_eq!(
        world.child_environment().await.commit_identity(),
        Vec::<&str>::new()
    );
}
