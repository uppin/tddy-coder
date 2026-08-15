//! Acceptance: a daemon project used as a git remote, end to end.
//!
//! PRD: docs/ft/daemon/remote-git-repo.md § End to end (AC5, AC6, AC20–AC23).
//!
//! Real `git`, driving the real `tddy-remote-git-repo` binary as its `GIT_SSH_COMMAND`, over a real
//! LiveKit server, against the real `remote_git.RemoteGitService`, with the real
//! `auth.LiveKitTokenService` minting the room token over a real Connect-HTTP server. Nothing here
//! is stubbed: these are the tests that would have caught the pack corruption that rules out
//! serving git over the terminal RPC, because only a genuine `git clone` verifies a pack byte for
//! byte.
//!
//! The client holds a daemon token and a daemon URL and nothing else. It cannot mint a LiveKit
//! token, because `livekit.api_secret` is also the HMAC key session tokens are signed with — the
//! fixture uses exactly that secret for both, so a client that could reach it here could forge an
//! access token for any GitHub user.
//!
//! Requires a LiveKit server (`LIVEKIT_TESTKIT_WS_URL`, or Docker for the testcontainers path) and
//! a built `tddy-remote-git-repo`. Each test gets its own room and daemon identity; the suite still
//! runs serially because a LiveKit container is shared.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use serial_test::serial;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::project_storage::{write_projects, ProjectData};
use tddy_daemon::remote_git_service::{ProjectsDirResolver, RemoteGitServiceImpl, UserResolver};
use tddy_livekit::{LiveKitParticipant, RoomOptions};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_service::RemoteGitServiceServer;

const GITHUB_USER: &str = "testuser";
const PROJECT_NAME: &str = "my-app";
const PROJECT_ID: &str = "0198f1b0-0000-7000-8000-00000000e2e0";
/// The secret this deployment shares. LiveKit's API secret *and* the session-token signing key —
/// one value, which is the whole reason the client is never given it.
const FLEET_SECRET: &str = "secret";

/// The OS user the daemon serves the project as. Unlike the admission suite — which stops before
/// anything is spawned and can name any string — this suite runs a real `git` child, so the account
/// has to exist: `git_argv` resolves it through passwd to drop privilege to it.
fn serving_os_user() -> String {
    std::env::var("USER").expect("USER must be set")
}

/// A repository on the daemon side, plus the client-side environment git needs to reach it.
struct AServedProject {
    /// The daemon's checkout — the origin git talks to.
    repo_path: PathBuf,
    /// Where clones are made.
    workspace: PathBuf,
    /// The daemon this project is served by; the host half of every remote URL.
    daemon_instance_id: String,
    /// The daemon's Connect-HTTP root — the only address the client is given.
    daemon_url: String,
    ssh_command: String,
    _daemon: BackgroundTask,
    _daemon_http: BackgroundTask,
    _testkit: LiveKitTestkit,
    _home: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
}

impl AServedProject {
    fn remote_url(&self) -> String {
        format!("{}:{PROJECT_NAME}", self.daemon_instance_id)
    }

    /// A `GIT_SSH_COMMAND` for this daemon carrying `credential` instead of the fixture's default
    /// access token — e.g. `--refresh-token …`.
    fn ssh_command_with(&self, credential: &str) -> String {
        format!(
            "{} --daemon-url {} {credential}",
            remote_git_repo_binary().display(),
            self.daemon_url
        )
    }
}

/// A server that lives exactly as long as the fixture that started it.
struct BackgroundTask(tokio::task::JoinHandle<()>);

impl Drop for BackgroundTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// `git` with a deterministic identity and this feature's transport wired in.
///
/// `GIT_SSH_VARIANT` decides which argv shape git produces: `simple` passes `<host> <command>`,
/// `ssh` also passes options ahead of the host. Both must resolve — see AC3.
fn git_using(
    project: &AServedProject,
    variant: &str,
    cwd: &Path,
    args: &[&str],
) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_SSH_COMMAND", &project.ssh_command)
        .env("GIT_SSH_VARIANT", variant)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Acceptance")
        .env("GIT_AUTHOR_EMAIL", "acceptance@example.invalid")
        .env("GIT_COMMITTER_NAME", "Acceptance")
        .env("GIT_COMMITTER_EMAIL", "acceptance@example.invalid")
        .output()
        .expect("git must run")
}

fn git(project: &AServedProject, cwd: &Path, args: &[&str]) -> std::process::Output {
    git_using(project, "simple", cwd, args)
}

fn git_ok(project: &AServedProject, cwd: &Path, args: &[&str]) -> String {
    assert_git_succeeded(&git(project, cwd, args), args)
}

fn assert_git_succeeded(output: &std::process::Output, args: &[&str]) -> String {
    assert!(
        output.status.success(),
        "git {args:?} failed ({}):\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Plain `git` with no transport wiring — for setting up and inspecting the origin.
fn local_git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Acceptance")
        .env("GIT_AUTHOR_EMAIL", "acceptance@example.invalid")
        .env("GIT_COMMITTER_NAME", "Acceptance")
        .env("GIT_COMMITTER_EMAIL", "acceptance@example.invalid")
        .output()
        .expect("git must run");
    assert!(
        output.status.success(),
        "git {args:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The client binary, built into the same target directory as this test.
fn remote_git_repo_binary() -> PathBuf {
    let mut dir = std::env::current_exe().expect("test executable path");
    dir.pop(); // deps/
    if dir.ends_with("deps") {
        dir.pop();
    }
    let binary = dir.join("tddy-remote-git-repo");
    assert!(
        binary.exists(),
        "tddy-remote-git-repo is not built at {}; build it before running this suite",
        binary.display()
    );
    binary
}

/// A repository containing a text file and a binary file — the binary one is what proves the pack
/// survived, since any newline translation or dropped byte changes its hash.
fn seed_repository(repo_path: &Path) {
    std::fs::create_dir_all(repo_path).expect("create repo dir");
    local_git(repo_path, &["init", "--initial-branch=main"]);
    std::fs::write(repo_path.join("README.md"), "line one\nline two\n").expect("write README");
    let binary_content: Vec<u8> = (0..=255u8).cycle().take(64 * 1024).collect();
    std::fs::write(repo_path.join("payload.bin"), &binary_content).expect("write payload");
    local_git(repo_path, &["add", "."]);
    local_git(repo_path, &["commit", "-m", "seed"]);
}

fn a_github_user(login: &str) -> tddy_github::GitHubUser {
    tddy_github::GitHubUser {
        id: 1,
        login: login.to_string(),
        avatar_url: String::new(),
        name: login.to_string(),
    }
}

/// An access token of the kind the web UI hands out, signed with the deployment's shared secret.
fn an_access_token_for(login: &str) -> String {
    tddy_github::SessionTokenSigner::new(FLEET_SECRET.as_bytes()).mint_access(&a_github_user(login))
}

/// The 7-day credential a developer configures once.
fn a_refresh_token_for(login: &str) -> String {
    tddy_github::SessionTokenSigner::new(FLEET_SECRET.as_bytes())
        .mint_refresh(&a_github_user(login))
}

/// Stand up everything a `git clone` needs: a seeded repository, the daemon's LiveKit participant
/// serving `RemoteGitService`, and the daemon's Connect-HTTP surface serving `auth.AuthService` and
/// `auth.LiveKitTokenService`.
///
/// `suffix` names this test's own room and daemon instance, so one test's participants cannot be
/// mistaken for another's.
async fn a_served_project(suffix: &str) -> AServedProject {
    let testkit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit must start");
    let ws_url = testkit.get_ws_url();
    let room = format!("tddy-remote-git-{suffix}");
    let daemon_instance_id = format!("acceptance-daemon-{suffix}");

    let home = tempfile::tempdir().expect("tempdir");
    let repo_path = home.path().join("repos").join(PROJECT_NAME);
    seed_repository(&repo_path);

    let projects_dir = home.path().join(".tddy").join("projects");
    write_projects(
        &projects_dir,
        &[ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: PROJECT_NAME.to_string(),
            git_url: "https://github.com/example/my-app.git".to_string(),
            main_repo_path: repo_path.to_string_lossy().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: Default::default(),
        }],
    )
    .expect("write projects.yaml");

    // The same config the daemon would load: one mapped user, stub GitHub, and a LiveKit block
    // whose `api_secret` is also the session-token signing key.
    let config_dir = tempfile::tempdir().expect("tempdir");
    let config_path = config_dir.path().join("daemon.yaml");
    std::fs::write(
        &config_path,
        format!(
            "users:\n  - github_user: \"{GITHUB_USER}\"\n    os_user: \"{}\"\n\
             github:\n  stub: true\n\
             livekit:\n  url: \"{ws_url}\"\n  api_key: \"devkey\"\n  \
             api_secret: \"{FLEET_SECRET}\"\n  common_room: \"{room}\"\n",
            serving_os_user()
        ),
    )
    .expect("write daemon.yaml");
    let config = DaemonConfig::load(&config_path).expect("config must parse");

    // The daemon's own auth wiring — the OAuth service, the room-token mint, and the resolver both
    // they and `RemoteGitService` verify tokens with. One resolver, as in production.
    let auth =
        tddy_daemon::auth::build_auth_entries(&config, "127.0.0.1", 0).expect("auth must build");
    let user_resolver: UserResolver = auth
        .user_resolver
        .clone()
        .expect("auth must produce a resolver");

    let daemon_http_router = tddy_connectrpc::connect_router(tddy_rpc::RpcBridge::new(
        tddy_rpc::MultiRpcService::new(auth.entries),
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the daemon's HTTP surface");
    let daemon_url = format!("http://{}", listener.local_addr().expect("local addr"));
    let daemon_http = BackgroundTask(tokio::spawn(async move {
        let _ = axum::serve(listener, daemon_http_router).await;
    }));

    let resolver_dir = projects_dir.clone();
    let projects_dir_resolver: ProjectsDirResolver =
        Arc::new(move |os_user| (os_user == serving_os_user()).then(|| resolver_dir.clone()));

    let service = RemoteGitServiceServer::new(RemoteGitServiceImpl::new(
        user_resolver,
        projects_dir_resolver,
        Arc::new(config),
    ));
    let daemon_token = testkit
        .generate_token(&room, &format!("daemon-{daemon_instance_id}"))
        .expect("daemon token");
    let participant = LiveKitParticipant::connect(
        &ws_url,
        &daemon_token,
        service,
        RoomOptions::default(),
        None,
        None,
    )
    .await
    .expect("the daemon participant must join");
    let daemon = BackgroundTask(tokio::spawn(async move {
        let _ = participant.run().await;
    }));

    let workspace = home.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("create workspace");

    let ssh_command = format!(
        "{} --daemon-url {daemon_url} --session-token {}",
        remote_git_repo_binary().display(),
        an_access_token_for(GITHUB_USER)
    );

    AServedProject {
        repo_path,
        workspace,
        daemon_instance_id,
        daemon_url,
        ssh_command,
        _daemon: daemon,
        _daemon_http: daemon_http,
        _testkit: testkit,
        _home: home,
        _config_dir: config_dir,
    }
}

// --- AC20: clone ------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn clones_a_daemon_project_onto_the_head_the_origin_is_on() {
    // Given a daemon serving "my-app"
    let project = a_served_project("clone").await;
    let origin_head = local_git(&project.repo_path, &["rev-parse", "HEAD"]);

    // When
    git_ok(
        &project,
        &project.workspace,
        &["clone", &project.remote_url(), "clone-a"],
    );

    // Then
    let clone = project.workspace.join("clone-a");
    assert_eq!(local_git(&clone, &["rev-parse", "HEAD"]), origin_head);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn clones_when_git_passes_ssh_options_ahead_of_the_host() {
    // Given git's `ssh` variant, which prefixes the argv with its protocol-v2 probe
    // (`-o SendEnv=GIT_PROTOCOL`) rather than passing a bare `<host> <command>`
    let project = a_served_project("clone-ssh-variant").await;
    let origin_head = local_git(&project.repo_path, &["rev-parse", "HEAD"]);

    // When
    let args = ["clone", &project.remote_url(), "clone-ssh-variant"];
    let output = git_using(&project, "ssh", &project.workspace, &args);

    // Then the options are dropped rather than mistaken for the host, and the clone lands
    assert_git_succeeded(&output, &args);
    let clone = project.workspace.join("clone-ssh-variant");
    assert_eq!(local_git(&clone, &["rev-parse", "HEAD"]), origin_head);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn clones_binary_content_without_corrupting_a_single_byte() {
    // Given a repository whose contents include every byte value
    let project = a_served_project("binary").await;
    let expected = std::fs::read(project.repo_path.join("payload.bin")).expect("read origin blob");

    // When
    git_ok(
        &project,
        &project.workspace,
        &["clone", &project.remote_url(), "clone-binary"],
    );

    // Then the working copy is byte-identical — the assertion a PTY transport cannot pass
    let cloned = std::fs::read(project.workspace.join("clone-binary").join("payload.bin"))
        .expect("read cloned blob");
    assert_eq!(cloned.len(), expected.len());
    assert_eq!(cloned, expected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn passes_gits_own_integrity_check_on_the_cloned_object_database() {
    // Given a fresh clone
    let project = a_served_project("fsck").await;
    git_ok(
        &project,
        &project.workspace,
        &["clone", &project.remote_url(), "clone-fsck"],
    );

    // When git verifies every object it received
    let clone = project.workspace.join("clone-fsck");
    let output = git(&project, &clone, &["fsck", "--strict"]);

    // Then
    assert!(
        output.status.success(),
        "git fsck reported a damaged object database:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn resolves_a_project_by_id_in_the_remote_url_as_well_as_by_name() {
    // Given a remote written with the project's uuid
    let project = a_served_project("by-id").await;
    let origin_head = local_git(&project.repo_path, &["rev-parse", "HEAD"]);

    // When
    git_ok(
        &project,
        &project.workspace,
        &[
            "clone",
            &format!("{}:{PROJECT_ID}", project.daemon_instance_id),
            "clone-by-id",
        ],
    );

    // Then
    let clone = project.workspace.join("clone-by-id");
    assert_eq!(local_git(&clone, &["rev-parse", "HEAD"]), origin_head);
}

// --- AC5: credentials -------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn clones_with_a_refresh_token_alone_because_an_access_token_expires_in_five_minutes() {
    // Given a client configured with only the 7-day refresh token — the credential a developer
    // puts in a GIT_SSH_COMMAND once and forgets
    let mut project = a_served_project("refresh-token").await;
    project.ssh_command = project.ssh_command_with(&format!(
        "--refresh-token {}",
        a_refresh_token_for(GITHUB_USER)
    ));
    let origin_head = local_git(&project.repo_path, &["rev-parse", "HEAD"]);

    // When
    git_ok(
        &project,
        &project.workspace,
        &["clone", &project.remote_url(), "clone-refresh"],
    );

    // Then the exchange, the mint and the clone all happened on the one configured credential
    let clone = project.workspace.join("clone-refresh");
    assert_eq!(local_git(&clone, &["rev-parse", "HEAD"]), origin_head);
}

// --- AC21: fetch ------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn fetches_a_commit_that_landed_upstream_after_the_clone() {
    // Given a clone taken before an upstream commit
    let project = a_served_project("fetch").await;
    git_ok(
        &project,
        &project.workspace,
        &["clone", &project.remote_url(), "clone-fetch"],
    );
    let clone = project.workspace.join("clone-fetch");
    std::fs::write(project.repo_path.join("NEW.md"), "landed later\n").expect("write NEW.md");
    local_git(&project.repo_path, &["add", "."]);
    local_git(&project.repo_path, &["commit", "-m", "later"]);
    let upstream_head = local_git(&project.repo_path, &["rev-parse", "HEAD"]);

    // When
    git_ok(&project, &clone, &["fetch", "origin"]);

    // Then
    assert_eq!(
        local_git(&clone, &["rev-parse", "origin/main"]),
        upstream_head
    );
}

// --- AC22 / AC23: push ------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn pushes_a_new_branch_into_the_daemon_side_repository() {
    // Given a clone with a commit on a new branch
    let project = a_served_project("push").await;
    git_ok(
        &project,
        &project.workspace,
        &["clone", &project.remote_url(), "clone-push"],
    );
    let clone = project.workspace.join("clone-push");
    local_git(&clone, &["checkout", "-b", "feat/from-the-clone"]);
    std::fs::write(clone.join("FEATURE.md"), "pushed\n").expect("write FEATURE.md");
    local_git(&clone, &["add", "."]);
    local_git(&clone, &["commit", "-m", "feature"]);
    let pushed_head = local_git(&clone, &["rev-parse", "HEAD"]);

    // When
    git_ok(&project, &clone, &["push", "origin", "feat/from-the-clone"]);

    // Then the branch exists on the daemon side at the pushed commit
    assert_eq!(
        local_git(
            &project.repo_path,
            &["rev-parse", "refs/heads/feat/from-the-clone"]
        ),
        pushed_head
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn refuses_a_push_to_the_branch_the_daemon_side_repository_has_checked_out() {
    // Given a clone with a commit on the branch the origin itself is sitting on
    let project = a_served_project("deny-current-branch").await;
    git_ok(
        &project,
        &project.workspace,
        &["clone", &project.remote_url(), "clone-deny"],
    );
    let clone = project.workspace.join("clone-deny");
    std::fs::write(clone.join("CONFLICT.md"), "would clobber\n").expect("write CONFLICT.md");
    local_git(&clone, &["add", "."]);
    local_git(
        &clone,
        &["commit", "-m", "would clobber the checked-out tree"],
    );
    let origin_head_before = local_git(&project.repo_path, &["rev-parse", "HEAD"]);

    // When
    let output = git(&project, &clone, &["push", "origin", "main"]);

    // Then git's own protection refuses it, and the working tree is untouched
    assert!(
        !output.status.success(),
        "a push to the checked-out branch must not succeed"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("checked out"),
        "the daemon must surface git's denyCurrentBranch message verbatim, got:\n{stderr}"
    );
    assert_eq!(
        local_git(&project.repo_path, &["rev-parse", "HEAD"]),
        origin_head_before
    );
}

// --- AC6: exit status -------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn surfaces_a_remote_failure_as_a_failing_git_command_rather_than_a_silent_success() {
    // Given a remote naming a project the daemon does not serve
    let project = a_served_project("unknown-project").await;

    // When
    let output = git(
        &project,
        &project.workspace,
        &[
            "clone",
            &format!("{}:no-such-project", project.daemon_instance_id),
            "clone-missing",
        ],
    );

    // Then git fails, and the reason reaches the user rather than being swallowed
    assert!(
        !output.status.success(),
        "cloning an unknown project must fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no-such-project"),
        "the failure must name the project that could not be resolved, got:\n{stderr}"
    );
    assert!(
        !project
            .workspace
            .join("clone-missing")
            .join(".git")
            .exists(),
        "a failed clone must leave no repository behind"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn refuses_an_unauthenticated_clone_before_any_room_token_is_minted() {
    // Given a client presenting a token this deployment did not sign
    let mut project = a_served_project("unauthenticated").await;
    project.ssh_command = project.ssh_command_with("--session-token not-a-valid-token");

    // When
    let output = git(
        &project,
        &project.workspace,
        &["clone", &project.remote_url(), "clone-unauth"],
    );

    // Then it fails *naming the authentication refusal*. A bare non-zero exit would pass here
    // whether the token was rejected or the whole feature was broken.
    assert!(
        !output.status.success(),
        "an unauthenticated clone must fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unauthenticated") && stderr.contains("MintLiveKitToken"),
        "the failure must say the daemon refused to authenticate the token, got:\n{stderr}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn reports_an_unreachable_daemon_within_the_connect_timeout() {
    // Given a remote naming a daemon that is not in the room
    let mut project = a_served_project("absent-daemon").await;
    project.ssh_command = format!("{} --connect-timeout-secs 2", project.ssh_command);

    // When
    let started = std::time::Instant::now();
    let output = git(
        &project,
        &project.workspace,
        &["clone", "absent-daemon:my-app", "clone-absent"],
    );

    // Then it fails promptly, naming the participant it waited for
    assert!(!output.status.success(), "an absent daemon must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("daemon-absent-daemon") && stderr.contains("not in the room"),
        "the failure must name the daemon it waited for, got:\n{stderr}"
    );
    // 10s ceiling for a 2s wait: the mint round trip, the LiveKit room connect and git's own
    // process spawn all sit either side of it, and none of them is the thing under test.
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the connect timeout must bound the wait, took {:?}",
        started.elapsed()
    );
}

// --- Throughput probe -------------------------------------------------------------------------

/// Payload the throughput probe pushes through the data channel. Overridable so the same probe can
/// be pointed at a repository large enough to expose a ceiling a smaller one would hide.
fn throughput_payload_bytes() -> usize {
    std::env::var("TDDY_REMOTE_GIT_THROUGHPUT_BYTES")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(64 * 1024 * 1024)
}

/// Incompressible content, so the measurement reflects bytes actually carried rather than how well
/// zlib folded a repeating pattern away.
fn incompressible_bytes(len: usize) -> Vec<u8> {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

/// How fast a clone actually moves over the LiveKit data channel.
///
/// `#[ignore]` on purpose, for two reasons that would otherwise make it a bad default-suite test:
/// it seeds and transfers tens of megabytes, far outside the suite's time budget, and the rate is
/// hardware- and network-dependent, so any threshold would be a flake generator rather than a
/// guarantee. What it asserts is correctness at size — every byte arrives intact — and what it
/// *reports* is the rate, so the ceiling can be re-measured on demand instead of guessed at.
///
/// Run: `cargo test -p tddy-daemon --test remote_git_livekit_acceptance -- --ignored --nocapture`
/// Size: `TDDY_REMOTE_GIT_THROUGHPUT_BYTES=157286400` for 150 MiB.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
#[ignore = "throughput probe: seeds and transfers tens of MB on demand"]
async fn clones_a_large_repository_with_every_byte_intact() {
    // Given a served project carrying a payload big enough to need many thousands of frames
    let project = a_served_project("throughput").await;
    let payload = incompressible_bytes(throughput_payload_bytes());
    std::fs::write(project.repo_path.join("large.bin"), &payload).expect("write large payload");
    local_git(&project.repo_path, &["add", "."]);
    local_git(&project.repo_path, &["commit", "-m", "large payload"]);

    // When it is cloned through the real binary over the real data channel
    let started = std::time::Instant::now();
    git_ok(
        &project,
        &project.workspace,
        &["clone", &project.remote_url(), "clone-large"],
    );
    let elapsed = started.elapsed();

    // Then every byte survived the round trip
    let cloned = std::fs::read(project.workspace.join("clone-large").join("large.bin"))
        .expect("read cloned payload");
    assert_eq!(cloned.len(), payload.len());
    assert_eq!(cloned, payload);

    let megabytes = payload.len() as f64 / (1024.0 * 1024.0);
    println!(
        "throughput: {:.1} MiB cloned in {:.1}s = {:.2} MiB/s",
        megabytes,
        elapsed.as_secs_f64(),
        megabytes / elapsed.as_secs_f64()
    );
}
