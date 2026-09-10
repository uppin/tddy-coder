//! A session token this crate signs admits its holder to a service implemented in another crate.
//!
//! `build_auth_entries` returns the daemon's **single** identity function, and every other service
//! — in `tddy-daemon`, in `tddy-host-service`, in `tddy-worktree-service` — authenticates with a
//! clone of it. Moving auth into its own crate is only safe if that rule still holds across the
//! new boundary, and the failure mode if it does not is silent: a second signer would make each
//! half accept its own tokens and refuse the other's, which looks exactly like an expired session.
//!
//! The service on the far side here is `tddy_service::TokenServiceImpl` — a real service, owned by
//! a different crate, gated by the authenticator this crate builds. It is deliberately *not* one
//! of the daemon's own services: this crate cannot reach those, and that is the property
//! `dependency_boundary_unit.rs` pins. What can be proved from here is the whole of what auth
//! contributes — that the rule travels.

use tddy_daemon_auth::auth::{build_auth_entries, build_token_service_entry};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{
    Code, MultiRpcService, RequestMetadata, RpcBridge, RpcMessage, ServiceEntry, Status,
};
use tddy_service::proto::auth::{
    ExchangeCodeRequest, ExchangeCodeResponse, GetAuthUrlRequest, GetAuthUrlResponse,
};
use tddy_service::proto::token::{GenerateTokenRequest, GenerateTokenResponse};

const FLEET_SECRET: &str = "shared-secret";
const API_KEY: &str = "devkey";
const COMMON_ROOM: &str = "tddy-lobby";
const THE_CALLBACK_CODE: &str = "the-code";
const THE_LOGIN: &str = "operator";

#[tokio::test]
async fn admits_a_caller_holding_a_token_this_crates_sign_in_minted() {
    // Given an operator who signed in through `auth.AuthService`
    let (config, _dir) = a_daemon_serving_a_common_room();
    let session_token = sign_in(&config).await;

    // When they call `token.TokenService`, whose implementation lives in tddy-service
    let minted = mint_through(the_web_mint(&config), &session_token).await;

    // Then they are admitted, and the mint answers
    assert_eq!(
        minted
            .map(|response| response.ttl_seconds > 0)
            .map_err(|refusal| refusal.code),
        Ok(true),
        "a token this crate signed must authenticate a service in another crate"
    );
}

#[tokio::test]
async fn refuses_a_caller_holding_a_token_another_fleet_signed() {
    // Given a token minted with a secret this deployment does not hold
    let (config, _dir) = a_daemon_serving_a_common_room();
    let foreign_token = tddy_github::SessionTokenSigner::new(b"another-fleets-secret").mint_access(
        &tddy_github::GitHubUser {
            id: 1,
            login: THE_LOGIN.to_string(),
            avatar_url: String::new(),
            name: THE_LOGIN.to_string(),
        },
    );

    // When it is presented to the same cross-crate service
    let refusal = mint_through(the_web_mint(&config), &foreign_token).await;

    // Then it is refused — the rule that travels is the *verifying* one, not a permissive stand-in
    assert_eq!(
        refusal.map(|_| ()).map_err(|status| status.code),
        Err(Code::Unauthenticated)
    );
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

async fn mint_through(
    entry: ServiceEntry,
    session_token: &str,
) -> Result<GenerateTokenResponse, Status> {
    call(
        &entry,
        "token.TokenService",
        "GenerateToken",
        GenerateTokenRequest {
            room: COMMON_ROOM.to_string(),
            identity: "web-operator".to_string(),
            session_token: session_token.to_string(),
        },
    )
    .await
}

fn a_daemon_serving_a_common_room() -> (DaemonConfig, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let yaml = format!(
        "users:\n  - github_user: \"{THE_LOGIN}\"\n    os_user: \"{THE_LOGIN}-os\"\n\
         github:\n  stub: true\n  stub_codes: \"{THE_CALLBACK_CODE}:{THE_LOGIN}\"\n\
         livekit:\n  enabled: true\n  url: \"ws://livekit.internal:7880\"\n  \
         api_key: \"{API_KEY}\"\n  api_secret: \"{FLEET_SECRET}\"\n  \
         common_room: \"{COMMON_ROOM}\"\n"
    );
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).expect("the config is written");
    (DaemonConfig::load(&path).expect("the config loads"), dir)
}

fn the_auth_service(config: &DaemonConfig) -> ServiceEntry {
    build_auth_entries(config, "127.0.0.1", 8080)
        .expect("a configured daemon builds its auth entries")
        .entries
        .into_iter()
        .find(|entry| entry.name == "auth.AuthService")
        .expect("auth.AuthService is one of the entries")
}

/// `token.TokenService`, gated by the identity function this crate produced.
fn the_web_mint(config: &DaemonConfig) -> ServiceEntry {
    let user_resolver = build_auth_entries(config, "127.0.0.1", 8080)
        .expect("a configured daemon builds its auth entries")
        .user_resolver
        .expect("a configured daemon has an identity function");
    build_token_service_entry(config, Some(&user_resolver))
        .expect("a daemon holding livekit credentials registers the mint")
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
