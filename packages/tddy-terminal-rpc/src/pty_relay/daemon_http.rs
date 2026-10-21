use crate::pty_relay::PtyRelayConfig;
use crate::pty_relay::LOBBY_ROOM;
use crate::pty_relay::ROOM_TOKEN_TTL;
use crate::pty_relay::SERVER_PARTICIPANT_WAIT;
use anyhow::Result;
use std::io::Read;
use std::sync::Arc;
use tddy_room_client::{RoomAuth, RoomConnectError, RoomConnector, RoomJoin, RoomSession};

/// Calls GetAuthUrl (which for stub auth embeds `?code=<code>&state=<uuid>` in the URL),
/// then ExchangeCode to get a session token without any manual browser interaction.
pub(super) async fn exchange_stub_session_token(daemon_url: &str) -> anyhow::Result<String> {
    use prost::Message as _;
    use tddy_service::proto::auth::{
        ExchangeCodeRequest, ExchangeCodeResponse, GetAuthUrlRequest, GetAuthUrlResponse,
    };

    let client = reqwest::Client::new();

    let url_req_bytes = GetAuthUrlRequest {}.encode_to_vec();
    let url_resp_bytes = connectrpc_post(
        &client,
        daemon_url,
        "auth.AuthService",
        "GetAuthUrl",
        url_req_bytes,
    )
    .await?;
    let url_resp = GetAuthUrlResponse::decode(url_resp_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("decode GetAuthUrlResponse: {}", e))?;

    // Stub URL: http://<host>/auth/callback?code=test-code&state=<uuid>
    let query = url_resp
        .authorize_url
        .split_once('?')
        .map(|(_, q)| q)
        .unwrap_or("");
    let mut code = String::new();
    let mut state = url_resp.state.clone();
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            match k {
                "code" => code = v.to_string(),
                "state" => state = v.to_string(),
                _ => {}
            }
        }
    }
    if code.is_empty() {
        anyhow::bail!(
            "stub authorize URL has no ?code= — is github.stub=true in daemon config? URL: {}",
            url_resp.authorize_url
        );
    }

    let exchange_req_bytes = ExchangeCodeRequest { code, state }.encode_to_vec();
    let exchange_resp_bytes = connectrpc_post(
        &client,
        daemon_url,
        "auth.AuthService",
        "ExchangeCode",
        exchange_req_bytes,
    )
    .await?;
    let exchange_resp = ExchangeCodeResponse::decode(exchange_resp_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("decode ExchangeCodeResponse: {}", e))?;

    log::info!(target: "tddy_terminal_rpc::pty_relay", "authenticated as: {}", exchange_resp.user.map(|u| u.login).unwrap_or_default());
    Ok(exchange_resp.session_token)
}

/// Join the room `config` names (default `tddy-lobby`) as `config.client_identity`, minting from its
/// API credentials, and wait for `server_identity` to be there — through `connector`, so this crate
/// names no transport.
pub async fn join_terminal_room(
    config: &PtyRelayConfig,
    livekit_url: &str,
    server_identity: &str,
    connector: &dyn RoomConnector,
) -> Result<Arc<dyn RoomSession>> {
    let join = RoomJoin {
        url: livekit_url.to_string(),
        room: config
            .livekit_room
            .as_deref()
            .unwrap_or(LOBBY_ROOM)
            .to_string(),
        identity: config.client_identity.clone(),
        auth: RoomAuth::Credentials {
            api_key: config.livekit_api_key.clone(),
            api_secret: config.livekit_api_secret.clone(),
            ttl: ROOM_TOKEN_TTL,
        },
    };
    connector
        .connect(&join, server_identity, SERVER_PARTICIPANT_WAIT)
        .await
        .map_err(|e| match e {
            RoomConnectError::Room(reason) => anyhow::anyhow!("{reason}"),
            RoomConnectError::PeerAbsent { identity, waited } => anyhow::anyhow!(
                "\"{identity}\" did not join room \"{}\" within {}s",
                join.room,
                waited.as_secs()
            ),
        })
}

/// Ask the daemon at `daemon_url` to open `session_id`'s room — with itself in it and the
/// session's terminal exposed there — before this relay joins the room to look for it.
///
/// `SessionRoomService.OpenSessionRoom` is the LiveKit integration's own call; `ConnectSession` is
/// local work and no longer opens a room for a caller that did not arrive over LiveKit. It fails,
/// naming the room, when the daemon's LiveKit cannot be reached.
pub async fn open_session_room_over_http(
    daemon_url: &str,
    session_token: &str,
    session_id: &str,
) -> Result<tddy_service::proto::session_room::OpenSessionRoomResponse> {
    use prost::Message as _;
    use tddy_service::proto::session_room::{OpenSessionRoomRequest, OpenSessionRoomResponse};

    let reply = connectrpc_post(
        &reqwest::Client::new(),
        daemon_url,
        "session_room.SessionRoomService",
        "OpenSessionRoom",
        OpenSessionRoomRequest {
            session_token: session_token.to_string(),
            session_id: session_id.to_string(),
        }
        .encode_to_vec(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("OpenSessionRoom for {session_id}: {e}"))?;
    OpenSessionRoomResponse::decode(reply.as_slice())
        .map_err(|e| anyhow::anyhow!("decode OpenSessionRoomResponse: {e}"))
}

pub(super) async fn connectrpc_post(
    client: &reqwest::Client,
    base: &str,
    service: &str,
    method: &str,
    body: Vec<u8>,
) -> anyhow::Result<Vec<u8>> {
    let url = format!("{}/rpc/{}/{}", base.trim_end_matches('/'), service, method);
    let resp = client
        .post(&url)
        .header("content-type", "application/proto")
        .body(body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("POST {}: {}", url, e))?;
    if !resp.status().is_success() {
        anyhow::bail!("POST {} → HTTP {}", url, resp.status());
    }
    Ok(resp
        .bytes()
        .await
        .map_err(|e| anyhow::anyhow!("read response: {}", e))?
        .to_vec())
}
