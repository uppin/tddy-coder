//! Signing in to a daemon that holds a public client id and nothing else.
//!
//! The redirect flow this daemon serves today posts `client_secret` to exchange its code, so a
//! deployment without one registers no auth service at all — `auth.rs`'s
//! `(Some(id), Some(secret))` gate falls through to an empty entry list, and the dashboard's
//! `GetAuthUrl` answers `not_found`. Shipping a secret inside a desktop application is not the way
//! out: an OAuth client secret in a binary anyone can download is public the day it ships.
//!
//! The device flow is what the GitHub CLI uses for exactly this reason. It authenticates with the
//! public `client_id` alone: the daemon asks GitHub for a pair of codes, shows the operator the
//! short one, and polls until they approve it in their own browser. Nothing secret is ever held by
//! the application, and the session that comes out the far end is the same session the redirect
//! flow produces — same access token, same refresh token, same claims.

use tddy_daemon_auth::auth::{build_auth_entries, github_auth_flow, GitHubAuthFlow};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{
    Code, MultiRpcService, RequestMetadata, RpcBridge, RpcMessage, ServiceEntry, Status,
};
use tddy_service::proto::auth::{
    DeviceLoginState, GetAuthUrlRequest, GetAuthUrlResponse, PollDeviceLoginRequest,
    PollDeviceLoginResponse, StartDeviceLoginRequest, StartDeviceLoginResponse,
};

/// A GitHub OAuth App's client id is public by design — it appears in every authorize URL.
const THE_PUBLIC_CLIENT_ID: &str = "Iv1.0123456789abcdef";
const THE_CALLBACK_CODE: &str = "the-code";
const THE_LOGIN: &str = "operator";

#[tokio::test]
async fn a_daemon_holding_only_a_public_client_id_registers_its_auth_service() {
    // Given a desktop deployment configured with a client id and no secret
    let (config, _dir) = a_daemon_with_a_client_id_and_no_secret();

    // When its auth entries are built
    let registered: Vec<&str> = build_auth_entries(&config, "127.0.0.1", 8080)
        .expect("a daemon with github configured builds its auth entries")
        .entries
        .iter()
        .map(|entry| entry.name)
        .collect();

    // Then sign-in is served, because the flow it serves needs no secret
    assert!(
        registered.contains(&"auth.AuthService"),
        "a public client id is enough to sign in; it registered {registered:?}"
    );
}

#[tokio::test]
async fn a_daemon_holding_only_a_public_client_id_refuses_to_begin_the_redirect_flow() {
    // Given a desktop deployment configured with a client id and no secret
    let (config, _dir) = a_daemon_with_a_client_id_and_no_secret();
    let auth = the_auth_service(&config);

    // When a dashboard asks it for an authorize URL
    let refused: Result<GetAuthUrlResponse, Status> =
        call(&auth, "GetAuthUrl", GetAuthUrlRequest {}).await;

    // Then it is refused as a precondition, naming the flow that does work, rather than handing
    // out a URL whose code it could never exchange
    assert_eq!(
        refused
            .map(|_| ())
            .map_err(|status| (status.code, status.message.contains("device flow"))),
        Err((Code::FailedPrecondition, true))
    );
}

#[test]
fn a_daemon_holding_only_a_public_client_id_declares_the_device_flow() {
    // Given a desktop deployment configured with a client id and no secret
    let (config, _dir) = a_daemon_with_a_client_id_and_no_secret();

    // When the flow it serves is declared to its dashboard
    let declared = github_auth_flow(&config);

    // Then it is the device flow, the only one the provider it registers can complete
    assert_eq!(declared, Some(GitHubAuthFlow::Device));
}

#[test]
fn a_stub_daemon_declares_the_redirect_flow_its_dashboards_sign_in_with() {
    // Given a stub daemon, whose provider can complete either flow
    let (config, _dir) = a_stub_daemon();

    // When the flow it serves is declared to its dashboard
    let declared = github_auth_flow(&config);

    // Then it is the redirect flow every dashboard driving a stub daemon signs in with
    assert_eq!(declared.map(GitHubAuthFlow::as_str), Some("redirect"));
}

#[tokio::test]
async fn a_device_login_hands_the_operator_a_code_to_approve_elsewhere() {
    // Given a daemon serving sign-in
    let (config, _dir) = a_stub_daemon();
    let auth = the_auth_service(&config);

    // When a device login begins
    let started: StartDeviceLoginResponse =
        call(&auth, "StartDeviceLogin", StartDeviceLoginRequest {})
            .await
            .expect("a daemon serving sign-in can begin a device login");

    // Then the operator is given a code and somewhere to type it, and the daemon keeps its own half
    assert_eq!(
        (
            started.user_code.is_empty(),
            started.verification_uri.is_empty(),
            started.device_code.is_empty(),
            started.interval_seconds > 0,
        ),
        (false, false, false, true),
        "a started device login carries both halves of the exchange and a poll interval"
    );
}

#[tokio::test]
async fn an_approved_device_login_yields_the_session_a_callback_would_have() {
    // Given a device login the operator has not approved yet
    let (config, _dir) = a_stub_daemon();
    let auth = the_auth_service(&config);
    let started: StartDeviceLoginResponse =
        call(&auth, "StartDeviceLogin", StartDeviceLoginRequest {})
            .await
            .expect("a daemon serving sign-in can begin a device login");
    let waiting: PollDeviceLoginResponse = poll(&auth, &started.device_code).await;

    // When they approve it and the daemon polls again
    let approved: PollDeviceLoginResponse = poll(&auth, &started.device_code).await;

    // Then the wait was reported as pending, and the approval carries a whole session
    assert_eq!(
        (
            waiting.state(),
            approved.state(),
            approved.session_token.is_empty(),
            approved.refresh_token.is_empty(),
            approved.user.map(|user| user.login),
        ),
        (
            DeviceLoginState::Pending,
            DeviceLoginState::Complete,
            false,
            false,
            Some(THE_LOGIN.to_string()),
        ),
        "an approved device login produces the same session an OAuth callback does"
    );
}

async fn poll(auth: &ServiceEntry, device_code: &str) -> PollDeviceLoginResponse {
    call(
        auth,
        "PollDeviceLogin",
        PollDeviceLoginRequest {
            device_code: device_code.to_string(),
        },
    )
    .await
    .expect("polling a started device login answers")
}

/// The shape `./install --desktop` renders: a public client id, no secret, no LiveKit.
fn a_daemon_with_a_client_id_and_no_secret() -> (DaemonConfig, tempfile::TempDir) {
    a_daemon_configured_with(&format!(
        "github:\n  client_id: \"{THE_PUBLIC_CLIENT_ID}\"\n"
    ))
}

/// The same daemon with GitHub itself replaced, so the flow runs without leaving the machine.
fn a_stub_daemon() -> (DaemonConfig, tempfile::TempDir) {
    a_daemon_configured_with(&format!(
        "github:\n  stub: true\n  client_id: \"{THE_PUBLIC_CLIENT_ID}\"\n  \
         stub_codes: \"{THE_CALLBACK_CODE}:{THE_LOGIN}\"\n"
    ))
}

/// `auth_storage` is a directory of this test's own, so the signing key a sign-in generates lives
/// and dies with the test rather than in the checkout.
fn a_daemon_configured_with(github: &str) -> (DaemonConfig, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let yaml = format!(
        "users:\n  - github_user: \"{THE_LOGIN}\"\n    os_user: \"{THE_LOGIN}-os\"\n{github}\
         auth_storage: \"{}\"\n",
        dir.path().join("auth").display()
    );
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).expect("the config is written");
    (DaemonConfig::load(&path).expect("the config loads"), dir)
}

fn the_auth_service(config: &DaemonConfig) -> ServiceEntry {
    build_auth_entries(config, "127.0.0.1", 8080)
        .expect("a daemon with github configured builds its auth entries")
        .entries
        .into_iter()
        .find(|entry| entry.name == "auth.AuthService")
        .expect("auth.AuthService is one of the entries")
}

async fn call<Req: prost::Message, Res: prost::Message + Default>(
    entry: &ServiceEntry,
    method: &str,
    request: Req,
) -> Result<Res, Status> {
    let bridge = RpcBridge::new(MultiRpcService::new(vec![ServiceEntry {
        name: entry.name,
        service: entry.service.clone(),
    }]));
    let message = RpcMessage {
        payload: request.encode_to_vec(),
        metadata: RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    };
    match bridge
        .handle_messages("auth.AuthService", method, &[message])
        .await?
    {
        tddy_rpc::ResponseBody::Complete(chunks) => {
            Ok(Res::decode(&chunks[0][..]).expect("a unary response decodes"))
        }
        _ => panic!("{method} is unary"),
    }
}
