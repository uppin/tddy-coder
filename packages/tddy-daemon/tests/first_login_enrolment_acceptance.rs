//! A desktop deployment learns who its operator is from their first sign-in, and every service in
//! the running daemon knows them from that moment.
//!
//! `./install --desktop` renders a config with `users:` unset, and every token-gated RPC refuses a
//! caller it cannot map to an OS user. A server keeps that exactly: its installer writes `users:`.
//! A desktop has nobody to, so its first login is enrolled as it completes — persisted into the
//! file the daemon was started from, and applied to the one `users:` map every service authorizes
//! through, in the same process with no restart. A second, different account on an enrolled desktop
//! is refused as it would be anywhere, and adding one deliberately is `#keyring` 8/9's.
//!
//! Only a sign-in from the desktop's own window enrols. The same `auth.AuthService` is served on
//! the LiveKit common room and the agent tool socket, and a login completed over either is not the
//! person at this machine — so on an unenrolled desktop it is minted and left unmapped, exactly as
//! any unmapped login is, and writes nothing. The common-room and tool-socket suites drive the real
//! hosts (`LiveKitParticipant`, the runtime's own agent tool socket), whose stamps are what the
//! enrolment reads. The window's calls are handed to the roster stamped in-process, the stamp the
//! Tauri IPC host puts on every frame — pinned against that host in `tddy-tauri-rpc`'s
//! `stamps_the_in_process_transport` suite.
//!
//! The common-room tests need the LiveKit testkit container (Docker or `LIVEKIT_TESTKIT_WS_URL`);
//! `#[serial]` so they own it alone.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::{ParticipantIdentity, RoomEvent, RoomOptions};
use livekit::Room;
use serial_test::serial;
use tddy_daemon::config::{DaemonConfig, UserMapping};
use tddy_daemon::runtime::{self, RuntimeOptions, RuntimeTaskHandles};
use tddy_livekit::{LiveKitParticipant, RpcClient};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::{
    Code, MultiRpcService, RequestMetadata, RequestTransport, RpcBridge, RpcClientTransport,
    RpcMessage, ServiceEntry, Status,
};
use tddy_service::proto::auth::{
    DeviceLoginState, ExchangeCodeRequest, ExchangeCodeResponse, GetAuthUrlRequest,
    GetAuthUrlResponse, PollDeviceLoginRequest, PollDeviceLoginResponse, StartDeviceLoginRequest,
    StartDeviceLoginResponse,
};
use tddy_service::proto::project::{ListProjectsRequest, ListProjectsResponse};
use tddy_testing_commons::wait::eventually_awaiting;
use tempfile::TempDir;
use tokio::net::TcpListener;

/// The stub completes a device login as the user of the first code it registers — the operator.
const THE_OPERATOR: &str = "operator";
const THE_OPERATORS_CODE: &str = "operator-code";
const SOMEBODY_ELSE: &str = "a-stranger";
const SOMEBODY_ELSES_CODE: &str = "stranger-code";
/// The identity the desktop's roster is served under in the test room.
const THE_DESKTOPS_ROOM_IDENTITY: &str = "daemon-first-login-desktop";
const A_ROOM_PEERS_IDENTITY: &str = "web-a-room-peer";
const SERVING_TIMEOUT: Duration = Duration::from_secs(20);

#[tokio::test]
async fn a_desktops_first_device_login_is_authorized_at_once() {
    // Given a freshly installed desktop, which maps nobody
    let desktop = a_deployment_mapping_nobody(RuntimeOptions::for_embedded()).await;

    // When its operator signs in with the device flow, in the desktop's own window
    let token = desktop.a_device_login(&Via::TheDesktopWindow).await;

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

    // When its operator signs in with the device flow, in the desktop's own window
    desktop.a_device_login(&Via::TheDesktopWindow).await;

    // Then the config file maps exactly them, to the account the desktop runs as
    assert_eq!(
        users_written_to(&desktop.config_path),
        vec![the_operator_as_this_desktops_user()]
    );
}

#[tokio::test]
async fn a_second_account_on_an_enrolled_desktop_is_refused_and_not_enrolled() {
    // Given a desktop its operator has already signed in to
    let desktop = a_deployment_mapping_nobody(RuntimeOptions::for_embedded()).await;
    desktop.a_device_login(&Via::TheDesktopWindow).await;

    // When a different GitHub account signs in and calls a token-gated RPC
    let token = desktop
        .a_redirect_login(&Via::TheDesktopWindow, SOMEBODY_ELSES_CODE)
        .await;
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
#[serial]
async fn a_device_login_over_the_common_room_is_not_enrolled_on_an_unenrolled_desktop() {
    // Given a freshly installed desktop serving its roster on a LiveKit room, and a peer in it
    let desktop = a_deployment_mapping_nobody(RuntimeOptions::for_embedded()).await;
    let room_peer = a_room_peer_of(&desktop).await;

    // When the peer completes a device login over the room and calls a token-gated RPC
    let token = desktop.a_device_login(&room_peer.via).await;
    let refusal = desktop.list_projects(&token).await.err().map(|s| s.code);

    // Then that login is refused, and the file still maps nobody
    assert_eq!(
        (refusal, users_written_to(&desktop.config_path)),
        (Some(Code::PermissionDenied), vec![]),
        "a login from a room peer must never become this desktop's operator"
    );
}

#[tokio::test]
#[serial]
async fn a_desktop_still_enrols_its_window_login_after_a_common_room_login_was_not() {
    // Given a freshly installed desktop on which a room peer has already signed in, as somebody
    // else, over the LiveKit room
    let desktop = a_deployment_mapping_nobody(RuntimeOptions::for_embedded()).await;
    let room_peer = a_room_peer_of(&desktop).await;
    desktop
        .a_redirect_login(&room_peer.via, SOMEBODY_ELSES_CODE)
        .await;

    // When the operator signs in, in the desktop's own window
    desktop.a_device_login(&Via::TheDesktopWindow).await;

    // Then the operator, and only the operator, is enrolled
    assert_eq!(
        users_written_to(&desktop.config_path),
        vec![the_operator_as_this_desktops_user()]
    );
}

#[tokio::test]
async fn a_device_login_over_the_agent_tool_socket_is_not_enrolled_on_an_unenrolled_desktop() {
    // Given a freshly installed desktop serving its agent tool socket, and a co-located process
    // connected to it
    let mut desktop = a_deployment_mapping_nobody(RuntimeOptions::for_embedded()).await;
    let agent = an_agent_on_the_tool_socket_of(&mut desktop).await;

    // When that process completes a device login over the socket and calls a token-gated RPC
    let token = desktop.a_device_login(&agent).await;
    let refusal = desktop.list_projects(&token).await.err().map(|s| s.code);

    // Then that login is refused, and the file still maps nobody
    assert_eq!(
        (refusal, users_written_to(&desktop.config_path)),
        (Some(Code::PermissionDenied), vec![]),
        "a login from a co-located process must never become this desktop's operator"
    );
}

#[tokio::test]
async fn a_desktop_still_enrols_its_window_login_after_an_agent_tool_socket_login_was_not() {
    // Given a freshly installed desktop on which a co-located process has already signed in, as
    // somebody else, over the agent tool socket
    let mut desktop = a_deployment_mapping_nobody(RuntimeOptions::for_embedded()).await;
    let agent = an_agent_on_the_tool_socket_of(&mut desktop).await;
    desktop.a_redirect_login(&agent, SOMEBODY_ELSES_CODE).await;

    // When the operator signs in, in the desktop's own window
    desktop.a_device_login(&Via::TheDesktopWindow).await;

    // Then the operator, and only the operator, is enrolled
    assert_eq!(
        users_written_to(&desktop.config_path),
        vec![the_operator_as_this_desktops_user()]
    );
}

#[tokio::test]
async fn a_server_mapping_nobody_enrols_no_one_and_refuses_as_it_always_has() {
    // Given a server deployment whose installer mapped nobody
    let server = a_deployment_mapping_nobody(RuntimeOptions::for_binary()).await;

    // When somebody signs in and calls a token-gated RPC
    let token = server.a_device_login(&Via::TheDesktopWindow).await;
    let refusal = server.list_projects(&token).await.err().map(|s| s.code);

    // Then they are refused, and the file still maps nobody
    assert_eq!(
        (refusal, users_written_to(&server.config_path)),
        (Some(Code::PermissionDenied), vec![]),
        "a server's users: is its installer's to write, never a login's"
    );
}

#[tokio::test]
async fn a_desktop_given_no_config_file_to_enrol_into_refuses_to_assemble() {
    // Given a desktop configuration that maps nobody, handed over with no file it was loaded from
    let dir = tempfile::tempdir().expect("a temporary directory");
    let config = a_config_mapping_nobody(&dir).await;

    // When the embedded daemon is assembled
    let built = runtime::build(config, RuntimeOptions::for_embedded()).await;

    // Then it refuses, naming what is missing, rather than starting a desktop no login can enrol
    let refusal = built.err().map(|e| e.to_string());
    assert!(
        refusal
            .as_deref()
            .is_some_and(|message| message.contains("config file")),
        "an embedded desktop that could never enrol its operator must not start; got {refusal:?}"
    );
}

/// Where a call to a deployment comes from.
enum Via {
    /// The desktop application's own window, over its in-process bridge.
    TheDesktopWindow,
    /// A client of one of the transports the deployment serves besides the window.
    Transport(Arc<dyn RpcClientTransport>),
}

/// A daemon assembled from a config file that maps nobody, and the file it was started from.
struct Deployment {
    roster: Vec<ServiceEntry>,
    rpc: RpcBridge<MultiRpcService>,
    tasks: Option<runtime::RuntimeTasks>,
    running: Option<RuntimeTaskHandles>,
    config_path: PathBuf,
    data_dir: PathBuf,
    _dir: TempDir,
}

/// A deployment signing in through a stub GitHub that knows the operator and somebody else, with
/// its state in a temporary directory, built for the host `options` names.
async fn a_deployment_mapping_nobody(options: RuntimeOptions) -> Deployment {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let config = a_config_mapping_nobody(&dir).await;
    let config_path = the_config_file_in(&dir);

    let runtime = runtime::build(config, options.with_config_path(Some(config_path.clone())))
        .await
        .expect("the daemon runtime builds");
    Deployment {
        rpc: RpcBridge::new(MultiRpcService::new(cloned(&runtime.entries))),
        roster: runtime.entries,
        tasks: Some(runtime.tasks),
        running: None,
        config_path,
        data_dir: the_data_dir_in(&dir),
        _dir: dir,
    }
}

impl Deployment {
    /// Sign in with the device flow, as the operator, and return the session token.
    async fn a_device_login(&self, via: &Via) -> String {
        let started: StartDeviceLoginResponse = self
            .call(
                via,
                "auth.AuthService",
                "StartDeviceLogin",
                StartDeviceLoginRequest {},
            )
            .await
            .expect("a device login begins");
        loop {
            let polled: PollDeviceLoginResponse = self
                .call(
                    via,
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
    async fn a_redirect_login(&self, via: &Via, code: &str) -> String {
        let url: GetAuthUrlResponse = self
            .call(via, "auth.AuthService", "GetAuthUrl", GetAuthUrlRequest {})
            .await
            .expect("an authorize url is handed out");
        let exchanged: ExchangeCodeResponse = self
            .call(
                via,
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
    /// own data directory, called from the desktop's own window.
    async fn list_projects(&self, session_token: &str) -> Result<ListProjectsResponse, Status> {
        self.call(
            &Via::TheDesktopWindow,
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
        via: &Via,
        service: &str,
        method: &str,
        request: Req,
    ) -> Result<Res, Status> {
        let answer = match via {
            Via::TheDesktopWindow => self.call_in_process(service, method, request).await?,
            Via::Transport(client) => {
                client
                    .call_unary(service, method, request.encode_to_vec())
                    .await?
            }
        };
        Ok(Res::decode(&answer[..]).expect("a unary response decodes"))
    }

    /// Hand the roster one call as the Tauri IPC host does: stamped in-process.
    async fn call_in_process<Req: prost::Message>(
        &self,
        service: &str,
        method: &str,
        request: Req,
    ) -> Result<Vec<u8>, Status> {
        let message = RpcMessage::new(
            request.encode_to_vec(),
            RequestMetadata::over(RequestTransport::InProcess),
        );
        match self
            .rpc
            .handle_messages(service, method, &[message])
            .await?
        {
            tddy_rpc::ResponseBody::Complete(mut chunks) => Ok(chunks.remove(0)),
            _ => panic!("{service}/{method} is unary"),
        }
    }
}

impl Drop for Deployment {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            running.abort_all();
            let _ = std::fs::remove_file(the_agent_tool_socket_of(self));
        }
    }
}

/// A peer in a LiveKit room the deployment's roster is served on, the way its common room serves
/// it: through a `LiveKitParticipant`, which stamps everything it receives `LiveKit`.
struct RoomPeer {
    via: Via,
    _room: Arc<Room>,
    _serving: AbortOnDrop,
    _livekit: LiveKitTestkit,
}

async fn a_room_peer_of(deployment: &Deployment) -> RoomPeer {
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let url = livekit.get_ws_url();
    let room_name = format!("first-login-{}", uuid::Uuid::new_v4());
    let token_for = |identity: &str| {
        livekit
            .generate_token(&room_name, identity)
            .expect("a room token is minted")
    };

    let desktop = LiveKitParticipant::connect(
        &url,
        &token_for(THE_DESKTOPS_ROOM_IDENTITY),
        MultiRpcService::new(cloned(&deployment.roster)),
        RoomOptions::default(),
        None,
        None,
    )
    .await
    .expect("the desktop's roster joins the room");
    let serving = AbortOnDrop(tokio::spawn(desktop.run()));

    let (room, mut events) = Room::connect(
        &url,
        &token_for(A_ROOM_PEERS_IDENTITY),
        RoomOptions::default(),
    )
    .await
    .expect("the peer joins the room");
    sees_participant(&room, &mut events, THE_DESKTOPS_ROOM_IDENTITY).await;
    let room = Arc::new(room);
    let client = RpcClient::new_shared(
        Arc::clone(&room),
        THE_DESKTOPS_ROOM_IDENTITY.to_string(),
        room.subscribe(),
    );
    RoomPeer {
        via: Via::Transport(Arc::new(client)),
        _room: room,
        _serving: serving,
        _livekit: livekit,
    }
}

/// Wait until `identity` is in `room`.
async fn sees_participant(
    room: &Room,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    identity: &str,
) {
    let target: ParticipantIdentity = identity.to_string().into();
    if room.remote_participants().contains_key(&target) {
        return;
    }
    tokio::time::timeout(SERVING_TIMEOUT, async {
        while let Some(event) = events.recv().await {
            if let RoomEvent::ParticipantConnected(participant) = event {
                if participant.identity() == target {
                    return;
                }
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{identity} never joined the room"));
}

/// Start the deployment's own tasks — its agent tool socket among them — and connect to that
/// socket as a co-located process would.
async fn an_agent_on_the_tool_socket_of(deployment: &mut Deployment) -> Via {
    let tasks = deployment
        .tasks
        .take()
        .expect("the deployment's tasks are started once");
    deployment.running = Some(tasks.spawn());
    let socket = the_agent_tool_socket_of(deployment);
    let client = eventually_awaiting("the agent tool socket accepts", SERVING_TIMEOUT, || {
        let socket = socket.clone();
        async move {
            let stream = tokio::net::UnixStream::connect(&socket)
                .await
                .map_err(|e| format!("connect {}: {e}", socket.display()))?;
            let (reader, writer) = tokio::io::split(stream);
            // The agent hosts nothing for the daemon to call back.
            let (client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                reader,
                writer,
                MultiRpcService::new(Vec::new()),
                RequestTransport::UnixSocket,
            );
            tokio::spawn(endpoint.run());
            Ok(client)
        }
    })
    .await;
    Via::Transport(client)
}

fn the_agent_tool_socket_of(deployment: &Deployment) -> PathBuf {
    tddy_daemon::agent_tool_socket::agent_tool_socket_path(&deployment.data_dir)
}

/// Aborts the task it holds when dropped, so a test's servers end with the test.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Another handle on each of `roster`'s services, for one more host to serve.
fn cloned(roster: &[ServiceEntry]) -> Vec<ServiceEntry> {
    roster
        .iter()
        .map(|entry| ServiceEntry {
            name: entry.name,
            service: Arc::clone(&entry.service),
        })
        .collect()
}

/// A config that maps nobody and signs in through a stub GitHub that knows the operator and
/// somebody else, written to [`the_config_file_in`] `dir` and loaded back from it.
async fn a_config_mapping_nobody(dir: &TempDir) -> DaemonConfig {
    let config_path = the_config_file_in(dir);
    let yaml = format!(
        "listen:\n  web_port: {port}\n  web_host: 127.0.0.1\n\
         tddy_data_dir: \"{data_dir}\"\n\
         github:\n  stub: true\n  \
         stub_codes: \"{THE_OPERATORS_CODE}:{THE_OPERATOR},{SOMEBODY_ELSES_CODE}:{SOMEBODY_ELSE}\"\n",
        port = a_free_tcp_port().await,
        data_dir = the_data_dir_in(dir).display(),
    );
    std::fs::write(&config_path, yaml).expect("the config is written");
    DaemonConfig::load(&config_path).expect("the config loads")
}

fn the_config_file_in(dir: &TempDir) -> PathBuf {
    dir.path().join("desktop.yaml")
}

fn the_data_dir_in(dir: &TempDir) -> PathBuf {
    dir.path().join("tddy")
}

/// The `users:` rows the config file at `path` holds now.
fn users_written_to(path: &Path) -> Vec<UserMapping> {
    DaemonConfig::load(path)
        .expect("the config file loads")
        .users
        .snapshot()
}

/// The one row an enrolled desktop's file holds: the operator, as the account it runs as.
fn the_operator_as_this_desktops_user() -> UserMapping {
    UserMapping {
        github_user: THE_OPERATOR.to_string(),
        os_user: the_os_user_this_process_runs_as(),
    }
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
