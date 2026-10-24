//! Acceptance: which credential a `RepointPlannedPr` of a started node reads.
//!
//! Re-targeting a node's PR needs a GitHub token. A **Local gh** session reads the host's own `gh`
//! login, as the session's OS user; a vault session reads the account its project is assigned from
//! the caller's vault. The source is the one recorded in the orchestrator's session directory, else
//! the project row's.
//!
//! Every case here is **refused** before any GitHub call: the `gh` stub is logged out, so the
//! refusal names which credential was asked, and nothing reaches the network or rewrites the plan.
//!
//! Feature: docs/ft/daemon/github-identity.md.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use tddy_core::changeset::{Changeset, Stack, StackNode};
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::{write_session_github_source, SessionGithubSource};
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_host_service::GithubCli;
use tddy_projects::project_storage::{self, ProjectData};
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::pr_stack::{PrStackService, RepointPlannedPrRequest};
use tddy_session_lifecycle::cli_session_manager::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_testing_commons::builders::a_session_metadata;
use tddy_testing_commons::gh_cli::a_gh_cli;

const ORCHESTRATOR: &str = "orchestrator-1";
const PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";
const TOKEN: &str = "valid-session-token";
const BASE_BRANCH: &str = "master";
const NODE: &str = "n1";
const NODE_BRANCH: &str = "feature/stack/n1";

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "`git {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn the_current_os_user() -> String {
    std::env::var("USER").expect("USER must be set to resolve the session's OS user")
}

/// A pr-stack orchestrator over a real clone with a bare `origin`, whose plan holds one started
/// node, in a project row that is Local gh or not.
struct World {
    tmp: tempfile::TempDir,
}

fn a_repoint_of_a_started_node_in_a_project(local_gh: bool) -> World {
    let world = World {
        tmp: tempfile::tempdir().unwrap(),
    };
    let origin = world.tmp.path().join("origin.git");
    fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--quiet", "--bare", "-b", BASE_BRANCH]);
    let root = world.repo_root();
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "--quiet", "-b", BASE_BRANCH]);
    git(&root, &["config", "user.email", "test@test.com"]);
    git(&root, &["config", "user.name", "Test"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    fs::write(root.join("README.md"), "the project\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "--quiet", "-m", "initial"]);
    git(
        &root,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    git(&root, &["push", "--quiet", "-u", "origin", BASE_BRANCH]);
    git(&root, &["branch", NODE_BRANCH]);

    let dir = world.session_dir();
    fs::create_dir_all(&dir).unwrap();
    tddy_core::write_changeset(
        &dir,
        &Changeset {
            recipe: Some("pr-stack".to_string()),
            repo_path: Some(root.to_string_lossy().into_owned()),
            stack: Some(Stack {
                version: 1,
                nodes: vec![StackNode {
                    node_id: NODE.to_string(),
                    title: "n1 title".to_string(),
                    branch: Some(NODE_BRANCH.to_string()),
                    session_id: Some("session-for-n1".to_string()),
                    ..StackNode::default()
                }],
            }),
            ..Changeset::default()
        },
    )
    .unwrap();
    tddy_core::write_session_metadata(
        &dir,
        &a_session_metadata()
            .with_session_id(ORCHESTRATOR)
            .with_project_id(PROJECT_ID)
            .build(),
    )
    .unwrap();

    let projects_dir = world.data_dir().join("projects");
    fs::create_dir_all(&projects_dir).unwrap();
    project_storage::add_project(
        &projects_dir,
        ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "alpha".to_string(),
            git_url: "https://github.com/acme/alpha.git".to_string(),
            main_repo_path: root.to_string_lossy().into_owned(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: Default::default(),
            accounts: Vec::new(),
            local_gh,
            home_session_id: None,
        },
    )
    .unwrap();
    world
}

impl World {
    fn repo_root(&self) -> PathBuf {
        self.tmp.path().join("clone")
    }

    fn data_dir(&self) -> PathBuf {
        self.tmp.path().join("data")
    }

    fn session_dir(&self) -> PathBuf {
        self.data_dir().join(SESSIONS_SUBDIR).join(ORCHESTRATOR)
    }

    /// The session started with `source` recorded, as a start that named one does.
    fn started_with(&self, source: SessionGithubSource) -> &Self {
        write_session_github_source(&self.session_dir(), source).unwrap();
        self
    }

    /// A daemon whose `gh` is the one at `gh`, running as the current OS user, with no vaults.
    fn daemon_with_gh_at(&self, gh: PathBuf) -> TestDaemon {
        let config_dir = tempfile::tempdir().unwrap();
        let config_path = config_dir.path().join("config.yaml");
        fs::write(
            &config_path,
            format!(
                "users:\n  - github_user: \"u\"\n    os_user: \"{}\"\n",
                the_current_os_user()
            ),
        )
        .unwrap();
        let config = tddy_daemon_kernel::config::DaemonConfig::load(&config_path).unwrap();
        let data_dir = self.data_dir();
        let resolved = data_dir.clone();
        let sessions_base_resolver: SessionsBaseResolver =
            Arc::new(move |_| Some(resolved.clone()));
        let user_resolver: SessionUserResolver =
            Arc::new(|token| (token == TOKEN).then(|| "u".to_string()));
        TestDaemon::from_host(
            DaemonSessionHost::new(
                config.clone(),
                sessions_base_resolver,
                data_dir,
                user_resolver,
                None,
                tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
                tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
                Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
                Arc::new(CliSessionManager::new()),
            )
            .with_github_cli(GithubCli::at(gh)),
        )
    }
}

async fn repoint(daemon: &TestDaemon) -> Result<(), Status> {
    daemon
        .repoint_planned_pr(Request::direct(RepointPlannedPrRequest {
            session_token: TOKEN.to_string(),
            session_id: ORCHESTRATOR.to_string(),
            node_id: NODE.to_string(),
            target_base_branch: String::new(),
        }))
        .await
        .map(|_| ())
}

#[tokio::test]
async fn a_local_gh_sessions_repoint_asks_the_hosts_gh_not_the_vault() {
    // Given a Local gh project and a host whose gh is logged out
    let world = a_repoint_of_a_started_node_in_a_project(true);
    let gh_dir = tempfile::tempdir().unwrap();
    let env_record = gh_dir.path().join("gh-env");
    let gh = a_gh_cli(gh_dir.path())
        .recording_env_to(&env_record)
        .build();

    // When the started node is repointed
    let refusal = repoint(&world.daemon_with_gh_at(gh))
        .await
        .expect_err("a logged-out gh yields no token");

    // Then gh was asked, and the refusal is gh's own
    assert!(env_record.is_file(), "the host's gh was never asked");
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal
            .message()
            .contains("gh is not logged in to github.com"),
        "got: {}",
        refusal.message()
    );
}

#[tokio::test]
async fn a_vault_sessions_repoint_never_asks_the_hosts_gh() {
    // Given a vault project and a host with no vaults wired, whose gh would record being asked
    let world = a_repoint_of_a_started_node_in_a_project(false);
    let gh_dir = tempfile::tempdir().unwrap();
    let env_record = gh_dir.path().join("gh-env");
    let gh = a_gh_cli(gh_dir.path())
        .recording_env_to(&env_record)
        .build();

    // When the started node is repointed
    let refusal = repoint(&world.daemon_with_gh_at(gh))
        .await
        .expect_err("no vault means no token");

    // Then the vault is what refused, and gh was not asked
    assert!(!env_record.exists(), "a vault session asked the host's gh");
    assert!(
        refusal.message().contains("vault"),
        "got: {}",
        refusal.message()
    );
}

#[tokio::test]
async fn a_session_started_as_local_gh_asks_gh_even_when_its_project_row_says_vault() {
    // Given a vault project, and a session whose start recorded Local gh
    let world = a_repoint_of_a_started_node_in_a_project(false);
    world.started_with(SessionGithubSource::LocalGh);
    let gh_dir = tempfile::tempdir().unwrap();
    let env_record = gh_dir.path().join("gh-env");
    let gh = a_gh_cli(gh_dir.path())
        .recording_env_to(&env_record)
        .build();

    // When the started node is repointed
    let refusal = repoint(&world.daemon_with_gh_at(gh))
        .await
        .expect_err("a logged-out gh yields no token");

    // Then the session's own source wins over the row's
    assert!(env_record.is_file(), "the host's gh was never asked");
    assert!(
        refusal.message().contains("gh is not logged in"),
        "got: {}",
        refusal.message()
    );
}
