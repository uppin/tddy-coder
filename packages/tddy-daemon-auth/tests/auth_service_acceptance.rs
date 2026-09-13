//! All five `auth.AuthService` methods, answered from `tddy-daemon-auth`.
//!
//! Driven through [`tddy_rpc::RpcBridge`] over the `ServiceEntry` this crate hands `runtime.rs`,
//! so what is exercised is what a browser actually reaches — not a hand-built `AuthServiceImpl`
//! that no registration produces. The five together are the whole of a sign-in: get a URL,
//! exchange the callback's code, ask who the resulting token belongs to, extend the session, and
//! end it.

use tddy_daemon_auth::auth::build_auth_entries;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{MultiRpcService, RequestMetadata, RpcBridge, RpcMessage, ServiceEntry, Status};
use tddy_service::proto::auth::{
    ExchangeCodeRequest, ExchangeCodeResponse, GetAuthStatusRequest, GetAuthStatusResponse,
    GetAuthUrlRequest, GetAuthUrlResponse, LogoutRequest, LogoutResponse, RefreshSessionRequest,
    RefreshSessionResponse,
};

/// The secret that signs both session tokens and LiveKit room JWTs across a deployment.
const FLEET_SECRET: &str = "shared-secret";
/// The `github.stub_codes` mapping a demo sign-in completes through.
const THE_CALLBACK_CODE: &str = "the-code";
const THE_LOGIN: &str = "operator";

#[tokio::test]
async fn hands_a_browser_a_url_to_start_the_github_sign_in_at() {
    // Given a daemon serving auth
    let (config, _dir) = a_daemon_with_github_configured();

    // When the web client asks where to send the operator
    let started: GetAuthUrlResponse = call(&config, "GetAuthUrl", GetAuthUrlRequest {})
        .await
        .expect("a configured daemon hands out an authorize url");

    // Then it is handed a URL carrying the state the exchange will be checked against
    assert!(
        started.authorize_url.contains(&started.state),
        "the authorize url must carry the state the exchange is checked against, got {}",
        started.authorize_url
    );
}

#[tokio::test]
async fn exchanges_a_callback_code_for_a_session_token_naming_the_login() {
    // Given an operator who has been sent to GitHub and come back with a code
    let (config, _dir) = a_daemon_with_github_configured();
    let entry = the_auth_service(&config);
    let started: GetAuthUrlResponse = call_entry(&entry, "GetAuthUrl", GetAuthUrlRequest {})
        .await
        .expect("a configured daemon hands out an authorize url");

    // When the browser's callback is exchanged
    let signed_in: ExchangeCodeResponse = call_entry(
        &entry,
        "ExchangeCode",
        ExchangeCodeRequest {
            code: THE_CALLBACK_CODE.to_string(),
            state: started.state,
        },
    )
    .await
    .expect("a registered code completes the exchange");

    // Then the session belongs to the login the code mapped to
    assert_eq!(
        signed_in.user.map(|user| user.login),
        Some(THE_LOGIN.to_string())
    );
}

#[tokio::test]
async fn reports_an_access_token_signed_with_the_fleet_secret_as_an_authenticated_session() {
    // Given an access token minted with the secret this daemon verifies against
    let (config, _dir) = a_daemon_with_github_configured();
    let session_token = an_access_token_for(THE_LOGIN);

    // When the web client asks whether it is still signed in
    let status: GetAuthStatusResponse = call(
        &config,
        "GetAuthStatus",
        GetAuthStatusRequest { session_token },
    )
    .await
    .expect("a status check is answered rather than refused");

    // Then it is, as the login the token carries
    assert_eq!(
        (status.authenticated, status.user.map(|user| user.login)),
        (true, Some(THE_LOGIN.to_string()))
    );
}

#[tokio::test]
async fn refuses_a_session_token_signed_with_a_foreign_secret() {
    // Given a token minted by a daemon holding a different secret
    let (config, _dir) = a_daemon_with_github_configured();
    let session_token = tddy_github::SessionTokenSigner::new(b"some-other-fleets-secret")
        .mint_access(&a_github_user(THE_LOGIN));

    // When the web client asks whether it is signed in
    let status: GetAuthStatusResponse = call(
        &config,
        "GetAuthStatus",
        GetAuthStatusRequest { session_token },
    )
    .await
    .expect("an unverifiable token is answered, not errored");

    // Then it is not — the answer is a refusal, never a permissive default
    assert_eq!((status.authenticated, status.user), (false, None));
}

#[tokio::test]
async fn refreshes_a_session_into_a_fresh_access_token_for_the_same_login() {
    // Given the long-lived refresh token a sign-in also returned
    let (config, _dir) = a_daemon_with_github_configured();
    let refresh_token = tddy_github::SessionTokenSigner::new(FLEET_SECRET.as_bytes())
        .mint_refresh(&a_github_user(THE_LOGIN));

    // When the web client extends its session with it
    let extended: RefreshSessionResponse = call(
        &config,
        "RefreshSession",
        RefreshSessionRequest { refresh_token },
    )
    .await
    .expect("a valid refresh token extends the session");

    // Then it holds a fresh access token for the same operator
    assert_eq!(
        extended.user.map(|user| user.login),
        Some(THE_LOGIN.to_string())
    );
    assert_eq!(
        the_login_the_resolver_reads(&config, &extended.session_token),
        Some(THE_LOGIN.to_string()),
        "the freshly minted access token must authenticate the very resolver every other daemon \
         service gates on"
    );
}

#[tokio::test]
async fn logs_out_without_needing_any_server_side_session_state() {
    // Given a signed-in operator's access token
    let (config, _dir) = a_daemon_with_github_configured();
    let session_token = an_access_token_for(THE_LOGIN);

    // When they sign out
    let farewell: Result<LogoutResponse, Status> =
        call(&config, "Logout", LogoutRequest { session_token }).await;

    // Then the daemon answers — session tokens are stateless, so there is nothing to invalidate
    assert_eq!(farewell.map(|_| ()).map_err(|refusal| refusal.code), Ok(()));
}

/// A daemon with a stub GitHub app, one mapped operator, and the fleet's signing secret.
fn a_daemon_with_github_configured() -> (DaemonConfig, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let yaml = format!(
        "users:\n  - github_user: \"{THE_LOGIN}\"\n    os_user: \"{THE_LOGIN}-os\"\n\
         github:\n  stub: true\n  stub_codes: \"{THE_CALLBACK_CODE}:{THE_LOGIN}\"\n\
         livekit:\n  api_secret: \"{FLEET_SECRET}\"\n"
    );
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).expect("the config is written");
    (DaemonConfig::load(&path).expect("the config loads"), dir)
}

/// The `auth.AuthService` entry this crate registers, as `runtime.rs` receives it.
fn the_auth_service(config: &DaemonConfig) -> ServiceEntry {
    build_auth_entries(config, "127.0.0.1", 8080)
        .expect("a configured daemon builds its auth entries")
        .entries
        .into_iter()
        .find(|entry| entry.name == "auth.AuthService")
        .expect("auth.AuthService is one of the entries")
}

fn a_github_user(login: &str) -> tddy_github::GitHubUser {
    tddy_github::GitHubUser {
        id: 1,
        login: login.to_string(),
        avatar_url: String::new(),
        name: login.to_string(),
    }
}

fn an_access_token_for(login: &str) -> String {
    tddy_github::SessionTokenSigner::new(FLEET_SECRET.as_bytes()).mint_access(&a_github_user(login))
}

/// The login the daemon's single identity function reads out of a token — the rule every other
/// service in every other crate admits a caller by.
fn the_login_the_resolver_reads(config: &DaemonConfig, token: &str) -> Option<String> {
    let resolver = build_auth_entries(config, "127.0.0.1", 8080)
        .expect("a configured daemon builds its auth entries")
        .user_resolver
        .expect("a configured daemon has an identity function");
    resolver(token)
}

async fn call<Req: prost::Message, Res: prost::Message + Default>(
    config: &DaemonConfig,
    method: &str,
    request: Req,
) -> Result<Res, Status> {
    call_entry(&the_auth_service(config), method, request).await
}

/// Drive one unary method of a registered entry the way Connect-HTTP does.
async fn call_entry<Req: prost::Message, Res: prost::Message + Default>(
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
        metadata: RequestMetadata::default(),
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
