//! Acceptance: a served daemon's orchestrator start options and start resolution run over the
//! daemon's wired host service (`#remote-shell` 11/11).
//!
//! The daemon is built by the real `runtime::build` from a stub-GitHub config. The stub user's
//! vault holds a Coder account whose deployment answers nothing. Through the wired service —
//! the one `runtime` attaches to the exec-tool handler — the account is listed as a derived
//! host the caller reaches, and a start naming it is refused with the listing's reason. Over
//! the bare service the handler would build on its own (no Coder accounts wired), the account
//! is never listed and the same start is refused as unknown instead: both assertions fail
//! without the attach in `runtime::mount_session_services`.

use std::sync::Arc;

use prost::Message;
use tddy_accounts::{CoderAccount, CoderDeploymentUrl, CoderUser};
use tddy_credentials::SecretString;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::runtime::{self, RuntimeOptions};
use tddy_daemon_auth::SessionVaults;
use tddy_projects::project_storage::{write_projects, ProjectData};
use tddy_rpc::{MultiRpcService, RequestMetadata, RequestTransport, RpcBridge, RpcMessage, Status};
use tddy_service::proto::auth::{
    ExchangeCodeRequest, ExchangeCodeResponse, GetAuthUrlRequest, GetAuthUrlResponse,
};
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::session::{StartSessionRequest, StartSessionResponse};
use tempfile::TempDir;
use tokio::net::TcpListener;

const THE_STUB_USER: &str = "testuser";
const THE_STUB_USERS_CODE: &str = "testuser-code";
const THE_VAULT_PASSPHRASE: &str = "correct horse battery staple";
const A_RECORD_WRITTEN_AT: u64 = 1_760_000_000;
const THIS_PROJECT: &str = "019d105b-ac0f-78d3-9a89-409731145a3b";
/// A Coder deployment nothing answers at: the account is listed, its workspaces cannot be.
const AN_UNREACHABLE_DEPLOYMENT: &str = "http://127.0.0.1:9";

#[tokio::test]
async fn a_served_daemons_start_options_list_the_callers_coder_account_as_a_derived_host() {
    // Given a served daemon whose stub user's vault holds a Coder account
    let daemon = a_stub_daemon().await;
    daemon.the_vault_of_the_stub_user_holding_a_coder_account();
    let token = daemon.a_login().await;
    let home = daemon.a_project_home(&token).await;

    // When the home session describes the start options
    let options = daemon.describe_start_options(&token, &home).await;

    // Then the caller's account is offered as a derived host of kind coder — listed through
    // the wired service, unusable only because its deployment answers nothing
    let derived = options["derived_hosts"]
        .as_array()
        .expect("derived_hosts is a list");
    let account_host = derived
        .iter()
        .find(|host| host["kind"] == "coder")
        .unwrap_or_else(|| panic!("the caller's Coder account is listed: {options}"));
    assert!(
        account_host["id"]
            .as_str()
            .is_some_and(|id| id.contains("/coder/coder-")),
        "the host is the account's: {account_host}"
    );
    assert_eq!(account_host["usable"], serde_json::json!(false));
}

#[tokio::test]
async fn a_served_daemons_start_naming_that_host_is_refused_with_its_reason() {
    // Given the same daemon, and the account host its options list
    let daemon = a_stub_daemon().await;
    daemon.the_vault_of_the_stub_user_holding_a_coder_account();
    let token = daemon.a_login().await;
    let home = daemon.a_project_home(&token).await;
    let options = daemon.describe_start_options(&token, &home).await;
    let account_host_id = options["derived_hosts"]
        .as_array()
        .and_then(|hosts| {
            hosts
                .iter()
                .find(|host| host["kind"] == "coder")
                .and_then(|host| host["id"].as_str().map(str::to_string))
        })
        .expect("the account host is listed");

    // When a start names it
    let answer = daemon
        .execute_tool(
            &token,
            &home,
            "StartSession",
            serde_json::json!({
                "session_type": "workspace",
                "target": account_host_id,
            }),
        )
        .await;

    // Then it is refused with the host's reason — resolved over the wired service, not
    // refused as a host nobody listed
    assert!(
        answer.is_error,
        "the start is refused: {}",
        answer.result_json
    );
    assert!(
        answer.error_message.contains(&account_host_id)
            && answer.error_message.contains("cannot be used now"),
        "the refusal names the host and its reason: {}",
        answer.error_message
    );
}

// ---------------------------------------------------------------------------------------------
// The daemon under test

/// A daemon built from a stub-GitHub config with `auth_storage`, and its RPC roster.
struct AStubDaemon {
    rpc: RpcBridge<MultiRpcService>,
    vaults: Arc<SessionVaults>,
    _dir: TempDir,
    _repo: TempDir,
}

async fn a_stub_daemon() -> AStubDaemon {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let repo = a_repo_with_origin();
    write_projects(
        &dir.path().join("tddy").join("projects"),
        &[ProjectData {
            project_id: THIS_PROJECT.to_string(),
            name: THIS_PROJECT.to_string(),
            git_url: String::new(),
            main_repo_path: repo.path().display().to_string(),
            main_branch_ref: Some("origin/main".to_string()),
            remote_name: None,
            host_repo_paths: Default::default(),
            accounts: Vec::new(),
            local_gh: false,
            home_session_id: None,
            orchestrator_defaults: None,
            lifecycle: Default::default(),
        }],
    )
    .expect("the project is registered");
    let runtime = runtime::build(
        a_stub_config_in(&dir).await,
        RuntimeOptions::for_binary(tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker),
    )
    .await
    .expect("the daemon runtime builds");
    let vaults = Arc::clone(
        runtime
            .credential_vaults()
            .expect("a daemon with auth_storage keeps credential vaults"),
    );
    AStubDaemon {
        rpc: RpcBridge::new(MultiRpcService::new(runtime.entries)),
        vaults,
        _dir: dir,
        _repo: repo,
    }
}

/// A git repository with an `origin` it has pushed `main` to, so a worktree can be cut from it.
fn a_repo_with_origin() -> TempDir {
    let repo = tempfile::tempdir().expect("a temporary repository");
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} succeeds");
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.name", "Test"]);
    git(&["config", "user.email", "t@t.com"]);
    git(&["commit", "-q", "--allow-empty", "-m", "init"]);
    git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    git(&["push", "-q", "-u", "origin", "main"]);
    repo
}

impl AStubDaemon {
    /// Hand the daemon the stub user's Coder account as a login would, then create the vault,
    /// which seals it and leaves it open.
    fn the_vault_of_the_stub_user_holding_a_coder_account(&self) {
        let account = CoderAccount::for_user(
            CoderDeploymentUrl::parse(AN_UNREACHABLE_DEPLOYMENT)
                .expect("the deployment url parses"),
            &CoderUser {
                id: "ada-user-id".to_string(),
                username: "ada".to_string(),
            },
            "Work",
        );
        self.vaults
            .retain(
                THE_STUB_USER,
                account.into_record(
                    SecretString::new("adas-coder-token".to_string()),
                    A_RECORD_WRITTEN_AT,
                ),
            )
            .expect("the daemon holds the record for the vault");
        self.vaults
            .create(THE_STUB_USER, &SecretString::new(THE_VAULT_PASSPHRASE))
            .expect("the vault is created and opened");
    }

    /// Sign in through the stub's redirect flow; returns the session token.
    async fn a_login(&self) -> String {
        let url: GetAuthUrlResponse = self
            .call("auth.AuthService", "GetAuthUrl", GetAuthUrlRequest {})
            .await
            .expect("an authorize url is handed out");
        let exchanged: ExchangeCodeResponse = self
            .call(
                "auth.AuthService",
                "ExchangeCode",
                ExchangeCodeRequest {
                    code: THE_STUB_USERS_CODE.to_string(),
                    state: url.state,
                },
            )
            .await
            .expect("the stub completes the login");
        exchanged.session_token
    }

    /// The project's project-home session: a workspace session of the project, marked with
    /// the project-home role in its record — the caller every project-home tool requires.
    async fn a_project_home(&self, token: &str) -> String {
        let started: StartSessionResponse = self
            .call(
                "session.SessionService",
                "StartSession",
                StartSessionRequest {
                    session_token: token.to_string(),
                    project_id: THIS_PROJECT.to_string(),
                    session_type: "workspace".to_string(),
                    ..Default::default()
                },
            )
            .await
            .expect("a workspace session starts in the project");
        let dir = tddy_core::session_lifecycle::unified_session_dir_path(
            &self._dir.path().join("tddy"),
            &started.session_id,
        );
        let mut meta = tddy_core::read_session_metadata(&dir).expect("session metadata");
        meta.session_role = Some("project-home".to_string());
        tddy_core::session_metadata::write_session_metadata(&dir, &meta)
            .expect("the home role is recorded");
        started.session_id
    }

    /// `DescribeSessionStartOptions` from the home session, as the result's JSON.
    async fn describe_start_options(&self, token: &str, home: &str) -> serde_json::Value {
        let answer = self
            .execute_tool(
                token,
                home,
                "DescribeSessionStartOptions",
                serde_json::json!({}),
            )
            .await;
        assert!(
            !answer.is_error,
            "the options are described: {}",
            answer.error_message
        );
        serde_json::from_str(&answer.result_json).expect("a JSON result")
    }

    async fn execute_tool(
        &self,
        token: &str,
        home: &str,
        tool_name: &str,
        args: serde_json::Value,
    ) -> ExecuteToolResponse {
        self.call(
            "exec_tools.ExecToolService",
            "ExecuteTool",
            ExecuteToolRequest {
                session_token: token.to_string(),
                session_id: home.to_string(),
                tool_name: tool_name.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            },
        )
        .await
        .unwrap_or_else(|status| panic!("{tool_name} is served to the home session: {status}"))
    }

    async fn call<Req: Message, Res: Message + Default>(
        &self,
        service: &str,
        method: &str,
        request: Req,
    ) -> Result<Res, Status> {
        let message = RpcMessage::new(
            request.encode_to_vec(),
            RequestMetadata::over(RequestTransport::InProcess),
        );
        let wire_bytes = match self
            .rpc
            .handle_messages(service, method, &[message])
            .await?
        {
            tddy_rpc::ResponseBody::Complete(mut chunks) => chunks.remove(0),
            _ => panic!("{service}/{method} is unary"),
        };
        Ok(Res::decode(&wire_bytes[..]).expect("a unary response decodes"))
    }
}

/// Stub GitHub knowing [`THE_STUB_USER`] by [`THE_STUB_USERS_CODE`], a `users:` row for them,
/// and `auth_storage` and the data directory under `dir`.
async fn a_stub_config_in(dir: &TempDir) -> DaemonConfig {
    let yaml = format!(
        "listen:\n  web_port: {port}\n  web_host: 127.0.0.1\n\
         tddy_data_dir: \"{data_dir}\"\n\
         auth_storage: \"{auth_storage}\"\n\
         github:\n  stub: true\n  stub_codes: \"{THE_STUB_USERS_CODE}:{THE_STUB_USER}\"\n\
         users:\n  - github_user: {THE_STUB_USER}\n    os_user: {os_user}\n",
        port = a_free_tcp_port().await,
        data_dir = dir.path().join("tddy").display(),
        auth_storage = dir.path().join("auth").display(),
        os_user = the_os_user_this_process_runs_as(),
    );
    serde_yaml::from_str(&yaml).expect("the config fixture parses")
}

fn the_os_user_this_process_runs_as() -> String {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    tddy_session_lifecycle::user_sessions_path::username_for_uid(unsafe { libc::getuid() })
        .expect("the test process runs as a named user")
}

/// A port that is free at this moment, so nothing in the fixture competes for a fixed one.
async fn a_free_tcp_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("no loopback port was available")
        .local_addr()
        .expect("the bound listener has no address")
        .port()
}
