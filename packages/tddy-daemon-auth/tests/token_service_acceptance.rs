//! Both mints sign with `config.livekit.api_secret`, and nothing else.
//!
//! That value signs **one** thing: LiveKit room JWTs. Session tokens used to be signed with it
//! too, which made a media credential the key to every identity on the fleet; they are now signed
//! with each daemon's own key, and this suite pins that the secret's remaining role survived the
//! separation untouched.
//!
//! So the assertion here is not "a token comes back". It is that the token that comes back
//! **verifies against the secret in the config**, checked with LiveKit's own verifier rather than
//! by reading the response fields the daemon filled in itself.

use livekit_api::access_token::TokenVerifier;
use prost::Message as _;
use tddy_daemon_auth::auth::{
    build_auth_entries, build_token_service_entry, LiveKitTokenServiceImpl,
};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{
    Code, MultiRpcService, Request, RequestMetadata, RpcBridge, RpcMessage, ServiceEntry,
};
use tddy_service::proto::auth::{LiveKitTokenService as _, MintLiveKitTokenRequest};
use tddy_service::proto::token::{GenerateTokenRequest, GenerateTokenResponse};

/// The secret the deployment's LiveKit server trusts room JWTs under.
const LIVEKIT_API_SECRET: &str = "shared-secret";
const API_KEY: &str = "devkey";
const COMMON_ROOM: &str = "tddy-lobby";
const THE_LOGIN: &str = "operator";

#[tokio::test]
async fn mints_a_room_jwt_that_verifies_against_the_configured_livekit_api_secret() {
    // Given a daemon serving a common room, and an operator's access token
    let (config, _dir) = a_daemon_serving_a_common_room();
    let mint = the_room_mint(&config);

    // When they ask for admission to the room
    let minted = mint
        .mint_live_kit_token(Request::direct(MintLiveKitTokenRequest {
            session_token: an_access_token_for(&config, THE_LOGIN),
        }))
        .await
        .expect("an authenticated operator is admitted")
        .into_inner();

    // Then LiveKit itself would accept the JWT, against the very secret the config carries
    assert_eq!(
        the_room_a_livekit_server_would_admit_it_to(&minted.token),
        Ok(COMMON_ROOM.to_string())
    );
}

#[tokio::test]
async fn mints_the_webs_room_jwt_against_the_same_api_secret() {
    // Given the mint the web UI joins presenter, session and lobby rooms through
    let (config, _dir) = a_daemon_serving_a_common_room();

    // When the web client mints for the room it wants to join
    let minted = generate_through(
        the_web_mint(&config),
        GenerateTokenRequest {
            room: COMMON_ROOM.to_string(),
            identity: "web-operator".to_string(),
            session_token: an_access_token_for(&config, THE_LOGIN),
        },
    )
    .await;

    // Then it is signed by the same secret — one signer, so a JWT minted through either mint is
    // admitted by the same LiveKit deployment
    assert_eq!(
        the_room_a_livekit_server_would_admit_it_to(&minted.token),
        Ok(COMMON_ROOM.to_string())
    );
}

#[tokio::test]
async fn mints_nothing_a_server_holding_a_different_secret_would_admit() {
    // Given a room JWT minted by a daemon holding this deployment's LiveKit secret
    let (config, _dir) = a_daemon_serving_a_common_room();
    let minted = generate_through(
        the_web_mint(&config),
        GenerateTokenRequest {
            room: COMMON_ROOM.to_string(),
            identity: "web-operator".to_string(),
            session_token: an_access_token_for(&config, THE_LOGIN),
        },
    )
    .await;

    // When a LiveKit server holding some other secret is asked to verify it
    let verified =
        TokenVerifier::with_api_key(API_KEY, "some-other-deployments-secret").verify(&minted.token);

    // Then it is refused — which is what makes the previous two tests say something
    assert!(
        verified.is_err(),
        "a JWT signed with this deployment's secret must not verify under another's"
    );
}

#[tokio::test]
async fn mints_no_room_jwt_for_a_caller_whose_session_token_cannot_be_resolved() {
    // Given a daemon serving a common room, and a session token it can resolve to nobody
    let (config, _dir) = a_daemon_serving_a_common_room();
    let mint = the_room_mint(&config);

    // When admission to the room is asked for with it
    let refusal = mint
        .mint_live_kit_token(Request::direct(MintLiveKitTokenRequest {
            session_token: "not-a-token-any-signer-produced".to_string(),
        }))
        .await;

    // Then nothing is minted — this is the mint that hands out LiveKit admission, so an
    // unresolvable caller must be turned away rather than admitted under some default identity
    assert_eq!(
        refusal.map(|_| ()).map_err(|status| status.code),
        Err(Code::Unauthenticated),
        "an unresolvable session token must never be admitted to the common room"
    );
}

/// A daemon with GitHub auth, one mapped operator, and a fully configured common room.
fn a_daemon_serving_a_common_room() -> (DaemonConfig, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let yaml = format!(
        "users:\n  - github_user: \"{THE_LOGIN}\"\n    os_user: \"{THE_LOGIN}-os\"\n\
         github:\n  stub: true\n\
         auth_storage: \"{}\"\n\
         livekit:\n  enabled: true\n  url: \"ws://livekit.internal:7880\"\n  \
         api_key: \"{API_KEY}\"\n  api_secret: \"{LIVEKIT_API_SECRET}\"\n  \
         common_room: \"{COMMON_ROOM}\"\n",
        dir.path().join("auth").display()
    );
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).expect("the config is written");
    (DaemonConfig::load(&path).expect("the config loads"), dir)
}

/// `auth.LiveKitTokenService` — the mint that chooses the room and the identity itself.
fn the_room_mint(config: &DaemonConfig) -> LiveKitTokenServiceImpl {
    LiveKitTokenServiceImpl::new(
        the_identity_function(config),
        std::sync::Arc::new(config.clone()),
    )
}

/// `token.TokenService` — the mint the web UI names its own room and identity on.
fn the_web_mint(config: &DaemonConfig) -> ServiceEntry {
    build_token_service_entry(config, Some(&the_identity_function(config)))
        .expect("a daemon holding livekit credentials registers the mint")
}

fn the_identity_function(config: &DaemonConfig) -> tddy_daemon_kernel::SessionUserResolver {
    build_auth_entries(config, "127.0.0.1", 8080)
        .expect("a configured daemon builds its auth entries")
        .user_resolver
        .expect("a configured daemon has an identity function")
}

/// An access token for `login`, issued by the daemon `config` describes.
fn an_access_token_for(config: &DaemonConfig, login: &str) -> String {
    tddy_daemon_auth::load_signing_key(config)
        .expect("the daemon's signing key loads")
        .signer()
        .mint_access(&tddy_github::GitHubUser {
            id: 1,
            login: login.to_string(),
            avatar_url: String::new(),
            name: login.to_string(),
        })
}

/// The room a LiveKit server holding this deployment's key and secret would let the JWT into.
///
/// Read through LiveKit's own verifier rather than by decoding the payload: what matters is that
/// the *signature* holds under `config.livekit.api_secret`, not what the claims say.
fn the_room_a_livekit_server_would_admit_it_to(token: &str) -> Result<String, String> {
    Ok(TokenVerifier::with_api_key(API_KEY, LIVEKIT_API_SECRET)
        .verify(token)
        .map_err(|e| e.to_string())?
        .video
        .room)
}

async fn generate_through(
    entry: ServiceEntry,
    request: GenerateTokenRequest,
) -> GenerateTokenResponse {
    let bridge = RpcBridge::new(MultiRpcService::new(vec![entry]));
    let message = RpcMessage {
        payload: request.encode_to_vec(),
        metadata: RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    };
    let body = bridge
        .handle_messages("token.TokenService", "GenerateToken", &[message])
        .await
        .expect("an authenticated caller mints");
    match body {
        tddy_rpc::ResponseBody::Complete(chunks) => {
            GenerateTokenResponse::decode(&chunks[0][..]).expect("a unary response decodes")
        }
        _ => panic!("GenerateToken is unary"),
    }
}
