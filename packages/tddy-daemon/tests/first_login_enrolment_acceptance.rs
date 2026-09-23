//! A desktop deployment learns who its operator is from their first sign-in, and every service in
//! the running daemon knows them from that moment.
//!
//! `./install --desktop` renders a config with `users:` unset, and every token-gated RPC refuses a
//! caller it cannot map to an OS user. A server keeps that exactly: its installer writes `users:`.
//! A desktop has nobody to, so its first login is enrolled as it completes — persisted into the
//! file the daemon was started from, and applied to the one `users:` map every service authorizes
//! through, in the same process with no restart. A second, different account on an enrolled desktop
//! is refused as it would be anywhere, and adding one deliberately is `#keyring` 8/9's.

use std::path::{Path, PathBuf};

use tddy_daemon::config::{DaemonConfig, UserMapping};
use tddy_daemon::runtime::{self, RuntimeOptions};
use tddy_rpc::{Code, MultiRpcService, RpcBridge, RpcMessage, ServiceEntry, Status};
use tddy_service::proto::auth::{
    DeviceLoginState, ExchangeCodeRequest, ExchangeCodeResponse, GetAuthUrlRequest,
    GetAuthUrlResponse, PollDeviceLoginRequest, PollDeviceLoginResponse, StartDeviceLoginRequest,
    StartDeviceLoginResponse,
};
use tddy_service::proto::project::{ListProjectsRequest, ListProjectsResponse};
use tempfile::TempDir;
use tokio::net::TcpListener;

/// The stub completes a device login as the user of the first code it registers — the operator.
const THE_OPERATOR: &str = "operator";
const THE_OPERATORS_CODE: &str = "operator-code";
const SOMEBODY_ELSE: &str = "a-stranger";
const SOMEBODY_ELSES_CODE: &str = "stranger-code";

#[tokio::test]
async fn a_desktops_first_device_login_is_authorized_at_once() {
    // Given a freshly installed desktop, which maps nobody
    let desktop = a_deployment_mapping_nobody(RuntimeOptions::for_embedded()).await;

    // When its operator signs in with the device flow
    let token = desktop.a_device_login().await;

    // Then a token-gated RPC admits them in the same process, with no restart
    let listed = desktop.list_projects(&token).await;
    assert!(
        listed.is_ok(),
        "the enrolled operator must be authorized at once; got {:?}",
        listed.err()
    );
}

#[tokio::test]
async fn a_desktops_first_device_login_is_written_down_as_its_only_row() {
    // Given a freshly installed desktop, which maps nobody
    let desktop = a_deployment_mapping_nobody(RuntimeOptions::for_embedded()).await;

    // When its operator signs in with the device flow
    desktop.a_device_login().await;

    // Then the config file maps exactly them, to the account the desktop runs as
    assert_eq!(
        users_written_to(&desktop.config_path),
        vec![UserMapping {
            github_user: THE_OPERATOR.to_string(),
            os_user: the_os_user_this_process_runs_as(),
        }]
    );
}

#[tokio::test]
async fn a_second_account_on_an_enrolled_desktop_is_refused_and_not_enrolled() {
    // Given a desktop its operator has already signed in to
    let desktop = a_deployment_mapping_nobody(RuntimeOptions::for_embedded()).await;
    desktop.a_device_login().await;

    // When a different GitHub account signs in and calls a token-gated RPC
    let token = desktop.a_redirect_login(SOMEBODY_ELSES_CODE).await;
    let refusal = desktop.list_projects(&token).await.err().map(|s| s.code);

    // Then it is refused, and the file still maps only the operator
    assert_eq!(
        (
            refusal,
            users_written_to(&desktop.config_path)
                .into_iter()
                .map(|row| row.github_user)
                .collect::<Vec<_>>(),
        ),
        (Some(Code::PermissionDenied), vec![THE_OPERATOR.to_string()]),
        "signing in is not how a second GitHub account is added"
    );
}

#[tokio::test]
async fn a_server_mapping_nobody_enrols_no_one_and_refuses_as_it_always_has() {
    // Given a server deployment whose installer mapped nobody
    let server = a_deployment_mapping_nobody(RuntimeOptions::for_binary()).await;

    // When somebody signs in and calls a token-gated RPC
    let token = server.a_device_login().await;
    let refusal = server.list_projects(&token).await.err().map(|s| s.code);

    // Then they are refused, and the file still maps nobody
    assert_eq!(
        (refusal, users_written_to(&server.config_path)),
        (Some(Code::PermissionDenied), vec![]),
        "a server's users: is its installer's to write, never a login's"
    );
}

/// A daemon assembled from a config file that maps nobody, and the file it was started from.
struct Deployment {
    rpc: RpcBridge<MultiRpcService>,
    config_path: PathBuf,
    _dir: TempDir,
}

/// A deployment signing in through a stub GitHub that knows the operator and somebody else, with
/// its state in a temporary directory, built for the host `options` names.
async fn a_deployment_mapping_nobody(options: RuntimeOptions) -> Deployment {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let config_path = dir.path().join("desktop.yaml");
    let yaml = format!(
        "listen:\n  web_port: {port}\n  web_host: 127.0.0.1\n\
         tddy_data_dir: \"{data_dir}\"\n\
         github:\n  stub: true\n  \
         stub_codes: \"{THE_OPERATORS_CODE}:{THE_OPERATOR},{SOMEBODY_ELSES_CODE}:{SOMEBODY_ELSE}\"\n",
        port = a_free_tcp_port().await,
        data_dir = dir.path().join("tddy").display(),
    );
    std::fs::write(&config_path, yaml).expect("the config is written");
    let config = DaemonConfig::load(&config_path).expect("the config loads");

    let runtime = runtime::build(config, options.with_config_path(Some(config_path.clone())))
        .await
        .expect("the daemon runtime builds");
    let entries = runtime
        .entries
        .iter()
        .map(|entry| ServiceEntry {
            name: entry.name,
            service: entry.service.clone(),
        })
        .collect();
    Deployment {
        rpc: RpcBridge::new(MultiRpcService::new(entries)),
        config_path,
        _dir: dir,
    }
}

impl Deployment {
    /// Sign in with the device flow, as the operator, and return the session token.
    async fn a_device_login(&self) -> String {
        let started: StartDeviceLoginResponse = self
            .call(
                "auth.AuthService",
                "StartDeviceLogin",
                StartDeviceLoginRequest {},
            )
            .await
            .expect("a device login begins");
        loop {
            let polled: PollDeviceLoginResponse = self
                .call(
                    "auth.AuthService",
                    "PollDeviceLogin",
                    PollDeviceLoginRequest {
                        device_code: started.device_code.clone(),
                    },
                )
                .await
                .expect("a started device login can be polled");
            match polled.state() {
                DeviceLoginState::Pending => continue,
                DeviceLoginState::Complete => return polled.session_token,
                other => panic!("the stub's device login ended {other:?}"),
            }
        }
    }

    /// Sign in with the redirect flow, as whoever `code` names, and return the session token.
    async fn a_redirect_login(&self, code: &str) -> String {
        let url: GetAuthUrlResponse = self
            .call("auth.AuthService", "GetAuthUrl", GetAuthUrlRequest {})
            .await
            .expect("an authorize url is handed out");
        let exchanged: ExchangeCodeResponse = self
            .call(
                "auth.AuthService",
                "ExchangeCode",
                ExchangeCodeRequest {
                    code: code.to_string(),
                    state: url.state,
                },
            )
            .await
            .expect("a login GitHub vouches for completes");
        exchanged.session_token
    }

    /// A token-gated RPC that resolves its caller's OS user and reads only under this deployment's
    /// own data directory.
    async fn list_projects(&self, session_token: &str) -> Result<ListProjectsResponse, Status> {
        self.call(
            "project.ProjectService",
            "ListProjects",
            ListProjectsRequest {
                session_token: session_token.to_string(),
                local_only: true,
            },
        )
        .await
    }

    async fn call<Req: prost::Message, Res: prost::Message + Default>(
        &self,
        service: &str,
        method: &str,
        request: Req,
    ) -> Result<Res, Status> {
        let message = RpcMessage {
            payload: request.encode_to_vec(),
            metadata: Default::default(),
        };
        match self
            .rpc
            .handle_messages(service, method, &[message])
            .await?
        {
            tddy_rpc::ResponseBody::Complete(chunks) => {
                Ok(Res::decode(&chunks[0][..]).expect("a unary response decodes"))
            }
            _ => panic!("{service}/{method} is unary"),
        }
    }
}

/// The `users:` rows the config file at `path` holds now.
fn users_written_to(path: &Path) -> Vec<UserMapping> {
    DaemonConfig::load(path)
        .expect("the config file loads")
        .users
        .snapshot()
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
