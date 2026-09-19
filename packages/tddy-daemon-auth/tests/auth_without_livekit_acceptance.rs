//! A daemon with no `livekit:` block authenticates its users.
//!
//! Today it cannot, and it fails later than it looks. `build_auth_entries` registers
//! `auth.AuthService` and hands back an identity function whether or not LiveKit is configured — so
//! the daemon appears to serve sign-in. But the signing secret is `config.livekit.api_secret`, so
//! with no LiveKit block there is no signer, and the sign-in breaks at its last step:
//! **`ExchangeCode` answers `FailedPrecondition: "session token signing is not configured"`.** The
//! user gets all the way through GitHub and is refused on the way back.
//!
//! That shape is exactly Tddy Desktop, which serves no media, joins no fleet, and still has a user
//! who must sign in. Media configuration and user authentication are two concerns wearing one
//! secret. A keypair the daemon generates for itself severs them: signing depends on the daemon
//! having an identity, which every daemon has, and on nothing a deployment might leave out.

use tddy_daemon_auth::auth::build_auth_entries;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{MultiRpcService, RequestMetadata, RpcBridge, RpcMessage, ServiceEntry, Status};
use tddy_service::proto::auth::{
    ExchangeCodeRequest, ExchangeCodeResponse, GetAuthUrlRequest, GetAuthUrlResponse,
};

const THE_CALLBACK_CODE: &str = "the-code";
const THE_LOGIN: &str = "operator";

#[tokio::test]
async fn a_daemon_configured_without_livekit_completes_a_sign_in() {
    // Given an operator signing in on a daemon that has no LiveKit block
    let (config, _dir) = a_desktop_daemon_with_no_livekit();
    let auth = the_auth_service(&config);
    let started: GetAuthUrlResponse = call(
        &auth,
        "auth.AuthService",
        "GetAuthUrl",
        GetAuthUrlRequest {},
    )
    .await
    .expect("a daemon with github configured hands out an authorize url");

    // When GitHub sends them back with their callback code
    let signed_in: Result<ExchangeCodeResponse, Status> = call(
        &auth,
        "auth.AuthService",
        "ExchangeCode",
        ExchangeCodeRequest {
            code: THE_CALLBACK_CODE.to_string(),
            state: started.state,
        },
    )
    .await;

    // Then they are signed in, holding a session token
    assert_eq!(
        signed_in
            .map(|response| response.session_token.is_empty())
            .map_err(|refusal| refusal.message),
        Ok(false),
        "signing in must not require a media server to be configured"
    );
}

#[tokio::test]
async fn a_token_from_a_livekit_less_sign_in_resolves_to_the_user_who_signed_in() {
    // Given an operator who signed in on a daemon that has no LiveKit block
    let (config, _dir) = a_desktop_daemon_with_no_livekit();
    let session_token = sign_in(&config).await;

    // When that token is presented to the identity function every RPC is gated on
    let resolve = build_auth_entries(&config, "127.0.0.1", 8080)
        .expect("a daemon with github configured builds its auth entries")
        .user_resolver
        .expect("a daemon with github configured has an identity function");

    // Then it names the operator who signed in
    assert_eq!(resolve(&session_token), Some(THE_LOGIN.to_string()));
}

/// Complete a whole sign-in through the served `auth.AuthService` and keep its access token.
async fn sign_in(config: &DaemonConfig) -> String {
    let auth = the_auth_service(config);
    let started: GetAuthUrlResponse = call(
        &auth,
        "auth.AuthService",
        "GetAuthUrl",
        GetAuthUrlRequest {},
    )
    .await
    .expect("a configured daemon hands out an authorize url");
    let signed_in: ExchangeCodeResponse = call(
        &auth,
        "auth.AuthService",
        "ExchangeCode",
        ExchangeCodeRequest {
            code: THE_CALLBACK_CODE.to_string(),
            state: started.state,
        },
    )
    .await
    .expect("a registered code completes the exchange");
    signed_in.session_token
}

/// Tddy Desktop's shape: a user, GitHub, and not one line of LiveKit configuration.
fn a_desktop_daemon_with_no_livekit() -> (DaemonConfig, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let yaml = format!(
        "users:\n  - github_user: \"{THE_LOGIN}\"\n    os_user: \"{THE_LOGIN}-os\"\n\
         github:\n  stub: true\n  stub_codes: \"{THE_CALLBACK_CODE}:{THE_LOGIN}\"\n"
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
    service: &str,
    method: &str,
    request: Req,
) -> Result<Res, Status> {
    let bridge = RpcBridge::new(MultiRpcService::new(vec![ServiceEntry {
        name: entry.name,
        service: entry.service.clone(),
    }]));
    let message = RpcMessage {
        payload: request.encode_to_vec(),
        metadata: RequestMetadata::default(),
    };
    match bridge.handle_messages(service, method, &[message]).await? {
        tddy_rpc::ResponseBody::Complete(chunks) => {
            Ok(Res::decode(&chunks[0][..]).expect("a unary response decodes"))
        }
        _ => panic!("{method} is unary"),
    }
}
