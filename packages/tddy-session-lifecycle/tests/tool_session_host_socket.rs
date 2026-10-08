//! Every `tddy-coder` tool session is started with `--host-session-socket`, whatever its recipe, and
//! the socket answers its agent's `github-token` request from the project's account.
//!
//! The socket is the OS user's: one path, shared by all that user's tool sessions, owner-only. Each
//! request names its session. The child here is a script that records its argv, spawned by the real
//! `StartSession` / `ResumeSession`; the requests below are made over the real bound socket the way
//! a coder makes them.
//!
//! Changeset: docs/dev/1-WIP/2026-09-19-keyring-github-identity.md

use std::collections::BTreeMap;
use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_accounts::{META_SUBJECT, META_SUBJECT_ID, PROVIDER_GITHUB};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::toolcall::{GITHUB_TOKEN_METHOD, HOST_SESSION_SERVICE};
use tddy_credentials::{
    AccountId, CredentialRecord, CredentialStore, ProviderId, SecretString, SessionVaults,
    FIRST_VERSION,
};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::bridge::{RpcResult, RpcService};
use tddy_rpc::{Request, RpcClientTransport, RpcMessage, Status};
use tddy_service::proto::session::{
    DeleteSessionRequest, ResumeSessionRequest, SessionService as SessionServiceTrait,
    StartSessionRequest,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_testing_commons::builders::a_session_metadata;
use tddy_testing_commons::fs::write_session_yaml;

const ADA_PROJECT: &str = "11111111-2222-4333-8444-555555555555";
const GRACE_PROJECT: &str = "66666666-7777-4888-8999-000000000000";
const OWNER: &str = "testuser";
const PASSPHRASE: &str = "correct horse battery staple";

fn current_username() -> String {
    std::env::var("USER").expect("USER must be set to resolve the target account")
}

fn a_github_account(account: &str, login: &str, subject_id: &str) -> CredentialRecord {
    CredentialRecord {
        provider: ProviderId::new(PROVIDER_GITHUB),
        account: AccountId::new(account),
        label: format!("{login}'s account"),
        secret: SecretString::new(format!("ghp_{login}_token")),
        metadata: BTreeMap::from([
            (META_SUBJECT_ID.to_string(), subject_id.to_string()),
            (META_SUBJECT.to_string(), login.to_string()),
        ]),
        updated_at: 1_700_000_000,
        version: FIRST_VERSION,
    }
}

/// The vaults of a daemon on which the owner has unlocked a vault holding ada's and grace's accounts.
fn vaults_holding_ada_and_grace(dir: &Path) -> Arc<SessionVaults> {
    let passphrase = SecretString::new(PASSPHRASE);
    let store = CredentialStore::create(&CredentialStore::path_in(dir, OWNER), &passphrase, OWNER)
        .expect("the owner's vault is created");
    store
        .put(a_github_account("acct-ada", "ada", "101"))
        .unwrap();
    store
        .put(a_github_account("acct-grace", "grace", "202"))
        .unwrap();
    let vaults = Arc::new(SessionVaults::new(dir));
    vaults.unlock(OWNER, &passphrase).unwrap();
    vaults
}

/// A stand-in for `tddy-coder` that records its argv to `args.<pid>` in `dir`, then outlives the
/// startup watch: it runs for the seconds named in `dir/lifetime` when that file exists (read when
/// the process starts, so a test can give a later start a different lifetime), else five.
fn a_tool_that_records_its_argv(dir: &Path) -> PathBuf {
    let path = dir.join("fake-tddy-coder.sh");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{dir}/args.$$\"\nsleep \"$(cat {dir}/lifetime 2>/dev/null || echo 5)\"\n",
            dir = dir.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn a_daemon(data_dir: &Path, vaults: Arc<SessionVaults>) -> DaemonSessionHost {
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
    .with_credential_vaults(vaults)
    .with_host_session_stop_watch_interval(std::time::Duration::from_millis(50))
}

/// Register projects: `(project_id, assigned account or none)`.
fn register_projects(data_dir: &Path, repo: &Path, projects: &[(&str, Option<&str>)]) {
    let projects_dir = data_dir.join("projects");
    std::fs::create_dir_all(&projects_dir).unwrap();
    let rows: String = projects
        .iter()
        .map(|(id, account)| {
            let accounts = match account {
                Some(account) => format!(
                    "    accounts:\n      - provider: {PROVIDER_GITHUB}\n        account_id: {account}\n"
                ),
                None => "    accounts: []\n".to_string(),
            };
            format!(
                "  - project_id: {id}\n    name: p-{id}\n    git_url: \"\"\n    main_repo_path: {}\n{accounts}",
                repo.display()
            )
        })
        .collect();
    std::fs::write(
        projects_dir.join("projects.yaml"),
        format!("projects:\n{rows}"),
    )
    .unwrap();
}

struct World {
    data_dir: tempfile::TempDir,
    _vault_dir: tempfile::TempDir,
    vaults: Arc<SessionVaults>,
    repo: tempfile::TempDir,
    tools: tempfile::TempDir,
    tool: PathBuf,
    daemon: DaemonSessionHost,
}

fn a_world_with(projects: &[(&str, Option<&str>)]) -> World {
    let data_dir = tempfile::tempdir().unwrap();
    let vault_dir = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let tool = a_tool_that_records_its_argv(tools.path());
    register_projects(data_dir.path(), repo.path(), projects);
    let vaults = vaults_holding_ada_and_grace(vault_dir.path());
    let daemon = a_daemon(data_dir.path(), Arc::clone(&vaults));
    World {
        data_dir,
        _vault_dir: vault_dir,
        vaults,
        repo,
        tools,
        tool,
        daemon,
    }
}

/// What one spawned child was given on its command line.
#[derive(Debug)]
struct ChildArgv(Vec<String>);

impl ChildArgv {
    fn value_of(&self, flag: &str) -> Option<&str> {
        let at = self.0.iter().position(|arg| arg == flag)?;
        self.0.get(at + 1).map(String::as_str)
    }
    fn host_session_socket(&self) -> PathBuf {
        PathBuf::from(
            self.value_of("--host-session-socket")
                .unwrap_or_else(|| panic!("no --host-session-socket in {:?}", self.0)),
        )
    }
    fn session_id(&self) -> String {
        self.value_of("--session-id")
            .or_else(|| self.value_of("--resume-from"))
            .unwrap_or_else(|| panic!("no session id in {:?}", self.0))
            .to_string()
    }
}

impl World {
    async fn start(&self, project_id: &str, recipe: &str) {
        self.daemon
            .start_session(Request::direct(StartSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: project_id.to_string(),
                tool_path: self.tool.to_str().unwrap().to_string(),
                recipe: recipe.to_string(),
                ..Default::default()
            }))
            .await
            .expect("the tool session starts");
    }

    async fn resume(&self, session_id: &str, project_id: &str) {
        let session_dir = unified_session_dir_path(self.data_dir.path(), session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        write_session_yaml(
            &session_dir,
            &a_session_metadata()
                .with_session_id(session_id)
                .with_project_id(project_id)
                .with_repo_path(self.repo.path().display().to_string())
                .with_tool(self.tool.display().to_string())
                .build(),
        );
        self.daemon
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .expect("the tool session resumes");
    }

    /// The argv of every child spawned so far, once `expected` of them have recorded one (the
    /// children are other processes: there is no event, so this waits up to a deadline).
    async fn children(&self, expected: usize) -> Vec<ChildArgv> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let mut recorded: Vec<ChildArgv> = std::fs::read_dir(self.tools.path())
                .unwrap()
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("args."))
                .filter_map(|e| std::fs::read_to_string(e.path()).ok())
                .filter(|content| !content.is_empty())
                .map(|content| ChildArgv(content.lines().map(str::to_string).collect()))
                .collect();
            if recorded.len() >= expected {
                recorded.sort_by_key(|c| c.session_id());
                return recorded;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "only {} of {expected} children recorded their argv",
                recorded.len()
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    async fn the_only_child(&self) -> ChildArgv {
        self.children(1).await.remove(0)
    }

    /// How long, in seconds, the stand-in tool runs for each start from now on.
    fn tools_run_for(&self, seconds: u32) {
        std::fs::write(self.tools.path().join("lifetime"), seconds.to_string()).unwrap();
    }

    /// Record `session_id` on disk the way a real coder does, so the daemon knows it exists.
    fn record_session(&self, session_id: &str, project_id: &str) {
        let session_dir = unified_session_dir_path(self.data_dir.path(), session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        write_session_yaml(
            &session_dir,
            &a_session_metadata()
                .with_session_id(session_id)
                .with_project_id(project_id)
                .with_repo_path(self.repo.path().display().to_string())
                .with_tool(self.tool.display().to_string())
                .build(),
        );
    }

    /// The daemon restarts: the process and everything it held in memory go, a new one starts over
    /// the same data and vaults. Its host-session sockets are bound again only when a session
    /// starts or resumes.
    async fn restart_daemon(&mut self) {
        // (the vaults stay open: what a restart loses here is the host-session registry, not them)
        self.daemon = a_daemon(self.data_dir.path(), Arc::clone(&self.vaults));
        // The old daemon's server tasks are cancelled by their drop; let the runtime run that.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    fn expected_socket(&self) -> PathBuf {
        self.data_dir
            .path()
            .join("run")
            .join(current_username())
            .join("host.sock")
    }
}

struct HostsNothing;

#[async_trait::async_trait]
impl RpcService for HostsNothing {
    async fn handle_rpc(&self, _: &str, _: &str, _: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(Status::unimplemented("a coder hosts nothing")))
    }
}

/// Ask the socket for `session_id`'s token, as a coder's forwarding handler does.
async fn token_over(socket: &Path, session_id: &str) -> Result<String, String> {
    let stream = tokio::net::UnixStream::connect(socket)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    let (reader, writer) = tokio::io::split(stream);
    let (client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
        reader,
        writer,
        HostsNothing,
        tddy_rpc::RequestTransport::UnixSocket,
    );
    tokio::spawn(endpoint.run());
    let payload = serde_json::to_vec(&serde_json::json!({ "session_id": session_id })).unwrap();
    let bytes = client
        .call_unary(HOST_SESSION_SERVICE, GITHUB_TOKEN_METHOD, payload)
        .await
        .map_err(|status| status.message().to_string())?;
    let answer: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    Ok(answer["token"].as_str().unwrap().to_string())
}

#[tokio::test]
async fn a_tool_session_is_given_the_host_session_socket_whatever_its_recipe() {
    // Given recipes that spawn conversations, orchestrate stacks, and plain ones
    for recipe in ["grill-me", "pr-stack", "tdd", "bugfix"] {
        let world = a_world_with(&[(ADA_PROJECT, Some("acct-ada"))]);

        // When a tool session of that recipe is started
        world.start(ADA_PROJECT, recipe).await;

        // Then its coder is told the OS user's socket, which exists
        let socket = world.the_only_child().await.host_session_socket();
        assert_eq!(socket, world.expected_socket(), "recipe {recipe}");
        assert!(
            std::fs::metadata(&socket).unwrap().file_type().is_socket(),
            "recipe {recipe}"
        );
    }
}

#[tokio::test]
async fn the_socket_a_session_is_given_is_owner_only_in_an_owner_only_directory() {
    // Given a started tool session
    let world = a_world_with(&[(ADA_PROJECT, Some("acct-ada"))]);
    world.start(ADA_PROJECT, "tdd").await;

    // When the real socket it was given is examined
    let socket = world.the_only_child().await.host_session_socket();
    let socket_meta = std::fs::metadata(&socket).unwrap();
    let dir_meta = std::fs::metadata(socket.parent().unwrap()).unwrap();

    // Then it is 0600 in a 0700 directory, both owned by the session's OS user
    let uid = unsafe { libc::geteuid() };
    assert_eq!(
        (
            socket_meta.permissions().mode() & 0o7777,
            dir_meta.permissions().mode() & 0o7777,
            socket_meta.uid(),
            dir_meta.uid()
        ),
        (0o600, 0o700, uid, uid)
    );
}

#[tokio::test]
async fn a_resumed_session_is_given_the_same_socket_it_had() {
    // Given a project, a session started on it, and another recorded for resume
    let world = a_world_with(&[(ADA_PROJECT, Some("acct-ada"))]);
    world.start(ADA_PROJECT, "tdd").await;

    // When a session of the same user is resumed
    world.resume("resumed-session", ADA_PROJECT).await;

    // Then both coders were given the one socket path
    let children = world.children(2).await;
    assert_eq!(
        children
            .iter()
            .map(ChildArgv::host_session_socket)
            .collect::<Vec<_>>(),
        vec![world.expected_socket(), world.expected_socket()]
    );
}

#[tokio::test]
async fn the_socket_answers_the_started_sessions_project_account() {
    // Given a project assigned to ada's account
    let world = a_world_with(&[(ADA_PROJECT, Some("acct-ada"))]);
    world.start(ADA_PROJECT, "tdd").await;
    let child = world.the_only_child().await;

    // When the coder asks over its socket, naming its session
    let token = token_over(&child.host_session_socket(), &child.session_id()).await;

    // Then it receives ada's token
    assert_eq!(token, Ok("ghp_ada_token".to_string()));
}

#[tokio::test]
async fn two_sessions_of_one_user_share_the_socket_and_are_answered_by_their_own_accounts() {
    // Given two projects of one owner, assigned to ada's and grace's accounts, one session each
    let world = a_world_with(&[
        (ADA_PROJECT, Some("acct-ada")),
        (GRACE_PROJECT, Some("acct-grace")),
    ]);
    world.start(ADA_PROJECT, "tdd").await;
    world.start(GRACE_PROJECT, "grill-me").await;
    let children = world.children(2).await;

    // When each asks over the socket it was given, naming its own session
    let mut tokens = Vec::new();
    for child in &children {
        let token = token_over(&child.host_session_socket(), &child.session_id()).await;
        tokens.push((child.host_session_socket(), token));
    }

    // Then they share one socket path, and the answers are two different accounts' tokens
    assert_eq!(tokens[0].0, tokens[1].0);
    let mut answers: Vec<_> = tokens
        .into_iter()
        .map(|(_, token)| token.unwrap())
        .collect();
    answers.sort();
    assert_eq!(answers, ["ghp_ada_token", "ghp_grace_token"]);
}

#[tokio::test]
async fn one_sessions_token_is_never_returned_for_another_sessions_id() {
    // Given ada's session, and grace's session on the same socket
    let world = a_world_with(&[
        (ADA_PROJECT, Some("acct-ada")),
        (GRACE_PROJECT, Some("acct-grace")),
    ]);
    world.start(ADA_PROJECT, "tdd").await;
    world.start(GRACE_PROJECT, "tdd").await;
    let children = world.children(2).await;
    let socket = children[0].host_session_socket();

    // When an id that is neither's asks for a token
    let outcome = token_over(&socket, "some-other-session").await;

    // Then it is refused, and neither account's token is in the refusal
    let refusal = outcome.expect_err("an unregistered id must be refused");
    assert!(
        !refusal.contains("ghp_") && refusal.contains("some-other-session"),
        "{refusal}"
    );
}

#[tokio::test]
async fn an_unassigned_and_an_unknown_account_are_refused_distinctly_and_verbatim() {
    // Given one project assigning no account, and one assigning an account this host does not hold
    let world = a_world_with(&[(ADA_PROJECT, None), (GRACE_PROJECT, Some("acct-unknown"))]);
    world.start(ADA_PROJECT, "tdd").await;
    world.start(GRACE_PROJECT, "tdd").await;
    let children = world.children(2).await;
    let socket = children[0].host_session_socket();

    // When both ask for a token
    let mut refusals = Vec::new();
    for child in &children {
        refusals.push(
            token_over(&socket, &child.session_id())
                .await
                .expect_err("neither resolves"),
        );
    }

    // Then each got the resolver's own distinct message
    assert_ne!(refusals[0], refusals[1]);
    let all = refusals.join("\n");
    assert!(
        all.contains("has no github account assigned")
            && all.contains("which this host does not hold"),
        "{all}"
    );
}

#[tokio::test]
async fn a_github_token_in_the_daemons_environment_rescues_an_unassigned_project_from_nothing() {
    // Given GITHUB_TOKEN exported where the daemon runs, and a project assigning no account
    std::env::set_var("GITHUB_TOKEN", "ghp_from_the_daemons_environment");
    std::env::set_var("GH_TOKEN", "ghp_from_the_daemons_environment");
    let world = a_world_with(&[(ADA_PROJECT, None)]);
    world.start(ADA_PROJECT, "tdd").await;
    let child = world.the_only_child().await;

    // When the session asks for its token
    let outcome = token_over(&child.host_session_socket(), &child.session_id()).await;

    // Then it is refused, and the environment's token is nowhere in the answer
    let refusal = outcome.expect_err("an unassigned project has no token");
    assert!(
        !refusal.contains("ghp_from_the_daemons_environment"),
        "{refusal}"
    );
}

#[tokio::test]
async fn a_deleted_session_is_no_longer_answered() {
    // Given a started session that gets a token
    let world = a_world_with(&[(ADA_PROJECT, Some("acct-ada"))]);
    let response = world
        .daemon
        .start_session(Request::direct(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: ADA_PROJECT.to_string(),
            tool_path: world.tool.to_str().unwrap().to_string(),
            ..Default::default()
        }))
        .await
        .expect("starts")
        .into_inner();
    let child = world.the_only_child().await;
    // (the stand-in tool writes no metadata of its own, which a delete needs to find the session)
    let session_dir = unified_session_dir_path(world.data_dir.path(), &response.session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_session_yaml(
        &session_dir,
        &a_session_metadata()
            .with_session_id(&response.session_id)
            .with_project_id(ADA_PROJECT)
            .build(),
    );
    assert_eq!(
        token_over(&child.host_session_socket(), &response.session_id).await,
        Ok("ghp_ada_token".to_string())
    );

    // When the session is deleted
    world
        .daemon
        .delete_session(Request::direct(DeleteSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: response.session_id.clone(),
        }))
        .await
        .expect("deletes");

    // Then its id is refused on the socket
    let outcome = token_over(&child.host_session_socket(), &response.session_id).await;
    assert!(outcome.unwrap_err().contains("no session"));
}

/// `start_toolcall_listener_with_handlers` names its socket by process id, so two listeners in one
/// test process would replace each other's path; the two tests that start one take turns.
static ONE_COMMUNICATING_LISTENER_AT_A_TIME: tokio::sync::Mutex<()> =
    tokio::sync::Mutex::const_new(());

/// A tool's `github-token` request to the listener a coder started with these flags.
async fn tools_token_request_to_a_coder_started_with(
    host_session_socket: Option<&str>,
    session_id: &str,
) -> Result<String, String> {
    let handlers = tddy_coder::tool_host_wiring::ToolHostHandlers::from_flags(
        host_session_socket,
        Some(session_id),
    );
    let data = tempfile::tempdir().unwrap();
    let (listener_socket, _requests) = tddy_core::toolcall::start_toolcall_listener_with_handlers(
        None,
        None,
        data.path().to_path_buf(),
        handlers.into_listener_handlers(),
    )
    .expect("the coder's listener starts");
    tddy_core::toolcall::request_github_token(Some(&listener_socket)).await
}

#[tokio::test]
async fn a_tools_token_request_reaches_the_assigned_account_through_the_coder_and_the_host_socket()
{
    // Given a started tool session whose project is assigned ada's account, and the coder's own
    // listener built from the flags that session's coder was given
    let _turn = ONE_COMMUNICATING_LISTENER_AT_A_TIME.lock().await;
    let world = a_world_with(&[(ADA_PROJECT, Some("acct-ada"))]);
    world.start(ADA_PROJECT, "tdd").await;
    let child = world.the_only_child().await;

    // When a tool asks that listener for a token (as tddy-tools does over its TDDY_SOCKET)
    let socket = child.host_session_socket();
    let token =
        tools_token_request_to_a_coder_started_with(socket.to_str(), &child.session_id()).await;

    // Then the chain coder listener -> forwarding handler -> user socket -> registry answers ada's token
    assert_eq!(token, Ok("ghp_ada_token".to_string()));
}

#[tokio::test]
async fn a_coder_given_no_host_socket_refuses_a_token_request_as_having_no_credential_handler() {
    // Given a coder started without --host-session-socket
    let _turn = ONE_COMMUNICATING_LISTENER_AT_A_TIME.lock().await;

    // When a tool asks its listener for a token
    let outcome = tools_token_request_to_a_coder_started_with(None, "any-session").await;

    // Then it is refused as having no credential handler: nothing else is consulted
    assert!(outcome.unwrap_err().contains("no credential handler"));
}

/// Ask until `session_id` is refused, or fail after a deadline (a process stopping is not an event
/// the test can wait on).
async fn refusal_once_stopped(socket: &Path, session_id: &str) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match token_over(socket, session_id).await {
            Err(refusal) => return refusal,
            Ok(_) => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "the stopped session is still answered"
                );
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }
    }
}

#[tokio::test]
async fn a_tool_session_whose_process_has_stopped_is_no_longer_answered() {
    // Given a started session whose coder runs for one second, and which has a record on disk
    let world = a_world_with(&[(ADA_PROJECT, Some("acct-ada"))]);
    world.tools_run_for(1);
    world.start(ADA_PROJECT, "tdd").await;
    let child = world.the_only_child().await;
    world.record_session(&child.session_id(), ADA_PROJECT);
    assert_eq!(
        token_over(&child.host_session_socket(), &child.session_id()).await,
        Ok("ghp_ada_token".to_string())
    );

    // When the coder's process exits
    let refusal = refusal_once_stopped(&child.host_session_socket(), &child.session_id()).await;

    // Then the daemon has dropped the session, and no token is in the refusal
    assert!(
        refusal.contains("resume it") && !refusal.contains("ghp_"),
        "{refusal}"
    );
}

#[tokio::test]
async fn a_resumed_session_stays_answered_when_its_earlier_process_stops() {
    // Given a session started with a one-second coder, then resumed with a long-lived one
    let world = a_world_with(&[(ADA_PROJECT, Some("acct-ada"))]);
    world.tools_run_for(1);
    world.start(ADA_PROJECT, "tdd").await;
    let first = world.the_only_child().await;
    let session_id = first.session_id();
    world.tools_run_for(30);
    world.resume(&session_id, ADA_PROJECT).await;
    world.children(2).await;

    // When the first process has exited
    tokio::time::sleep(std::time::Duration::from_millis(1600)).await;

    // Then the session is still answered: the resume's process is the registered one
    assert_eq!(
        token_over(&first.host_session_socket(), &session_id).await,
        Ok("ghp_ada_token".to_string())
    );
}

#[tokio::test]
async fn after_a_daemon_restart_a_running_session_is_told_to_resume_and_then_is_answered() {
    // Given a session that started before a restart and is still running
    let mut world = a_world_with(&[(ADA_PROJECT, Some("acct-ada"))]);
    world.start(ADA_PROJECT, "tdd").await;
    let child = world.the_only_child().await;
    let session_id = child.session_id();
    world.record_session(&session_id, ADA_PROJECT);

    // When the daemon restarts and another session of the user starts, rebinding the socket
    world.restart_daemon().await;
    world.start(ADA_PROJECT, "tdd").await;
    let still_running = token_over(&child.host_session_socket(), &session_id).await;

    // Then the running session is told, in so many words, to resume — not left hanging
    assert_eq!(
        still_running,
        Err("this session was started before the daemon restarted; resume it to re-enable GitHub tools"
            .to_string())
    );

    // And once it is resumed it is answered again, from the refreshed resolution
    world.resume(&session_id, ADA_PROJECT).await;
    assert_eq!(
        token_over(&child.host_session_socket(), &session_id).await,
        Ok("ghp_ada_token".to_string())
    );
}

#[tokio::test]
async fn after_a_daemon_restart_a_session_that_never_existed_is_not_told_to_resume() {
    // Given a restarted daemon with a session of its own running
    let mut world = a_world_with(&[(ADA_PROJECT, Some("acct-ada"))]);
    world.restart_daemon().await;
    world.start(ADA_PROJECT, "tdd").await;
    let child = world.the_only_child().await;

    // When an id that exists nowhere asks
    let refusal = token_over(&child.host_session_socket(), "no-such-session")
        .await
        .expect_err("refused");

    // Then it is the unknown-session refusal
    assert!(
        refusal.contains("no session `no-such-session`") && !refusal.contains("resume"),
        "{refusal}"
    );
}

/// A credential handler that answers one fixed token, standing for a session's resolved account.
struct AnswersWith(&'static str);

#[async_trait::async_trait]
impl tddy_core::toolcall::GithubCredentialHandler for AnswersWith {
    async fn github_token(&self) -> Result<String, String> {
        Ok(self.0.to_string())
    }
}

/// Serve `owner`'s host-session socket at `path` over `registry`, as the daemon does per OS user.
fn serve_the_socket_of(
    owner: &str,
    path: &Path,
    registry: &Arc<tddy_host_service::host_session_service::HostSessionRegistry>,
) {
    let listener = tokio::net::UnixListener::bind(path).unwrap();
    let owner = owner.to_string();
    let registry = Arc::clone(registry);
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let service = tddy_host_service::host_session_service::HostSessionService::new(
                &owner,
                Arc::clone(&registry),
            );
            let (reader, writer) = tokio::io::split(stream);
            let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                reader,
                writer,
                service,
                tddy_rpc::RequestTransport::UnixSocket,
            );
            tokio::spawn(endpoint.run());
        }
    });
}

#[tokio::test]
async fn one_os_users_socket_cannot_fetch_another_os_users_session_token() {
    // Given two OS users' sockets over one registry, and a session registered as alice's
    // (a daemon cannot bind a socket for a second real OS user without privilege, so the two
    // services here are the daemon's own, over real sockets, with the owners named directly)
    use tddy_host_service::host_session_service::{HostSessionRegistry, RegisteredSession};
    let registry = Arc::new(HostSessionRegistry::default());
    registry.register(
        "alices-session",
        RegisteredSession {
            os_user: "alice".to_string(),
            conversation_spawn_handler: None,
            github_credential_handler: Some(Arc::new(AnswersWith("ghp_alice_token"))),
        },
    );
    let sockets = tempfile::tempdir().unwrap();
    let alices = sockets.path().join("alice.sock");
    let bobs = sockets.path().join("bob.sock");
    serve_the_socket_of("alice", &alices, &registry);
    serve_the_socket_of("bob", &bobs, &registry);

    // When bob's socket is asked for alice's session's token, and alice's socket is
    let over_bobs = token_over(&bobs, "alices-session").await;
    let over_alices = token_over(&alices, "alices-session").await;

    // Then only her own socket answers, and bob's refusal carries no token
    assert_eq!(over_alices, Ok("ghp_alice_token".to_string()));
    let refusal = over_bobs.expect_err("bob's socket must refuse alice's session");
    assert!(
        refusal.contains("different OS user") && !refusal.contains("ghp_alice_token"),
        "{refusal}"
    );
}
