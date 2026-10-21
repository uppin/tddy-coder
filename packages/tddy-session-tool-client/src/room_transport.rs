use crate::dispatch_request_via_streaming_rpc;
use crate::execute_tool_request;
use crate::SessionToolEnvelope;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use tddy_room_client::{NoRoomTransport, RoomAuth, RoomConnector, RoomJoin, RoomSession};

/// Which remote daemon a cached LiveKit connection reaches.
///
/// Reuse is keyed by destination, `server_identity` included: handing back a client aimed at a
/// different daemon would execute the tool against the wrong host's filesystem. The token is part of
/// the key because it is what the room was joined with — a re-minted token is a different
/// connection, not the same one under a new name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LiveKitRoomKey {
    pub url: String,
    pub room: String,
    pub token: String,
    pub server_identity: String,
}

/// One LiveKit connection per destination, held for the life of the process.
///
/// `dispatch_via_livekit` used to connect a room, issue one call and drop it — per tool call, so an
/// agent doing fifty `Read`s paid fifty connects. `tddy-tools --mcp` runs for the whole session, so
/// it can hold the connection instead.
///
/// The per-key [`tokio::sync::OnceCell`] is what makes that safe under concurrency: an MCP server
/// issues tool calls in parallel, and a cache that checked the map, released it and only then
/// connected would open a room per racing call. Initialisation happens *inside* the cell, so the
/// first caller connects and the rest await that same connect. A failed connect leaves the cell
/// empty, so one unlucky first call cannot poison the session.
#[derive(Default)]
pub struct LiveKitRoomCache {
    cells: Mutex<HashMap<LiveKitRoomKey, RoomClientCell>>,
}

/// One destination's connection slot: empty until a caller finishes connecting it, and still empty
/// if that connect failed.
type RoomClientCell = Arc<tokio::sync::OnceCell<Arc<LiveKitSession>>>;

/// A held connection to one remote daemon, plus the means to ask whether that daemon is still in
/// the room.
///
/// The presence check exists because holding the connection moves the participant wait from *every
/// call* to *first connect*. If the codebase daemon restarts mid-session, a call issued against the
/// cached client would publish to an identity nobody is listening on — and neither
/// `tddy_livekit::RpcClient` nor `tddy_rpc`'s engine carries a request deadline, so it would hang
/// rather than error. Hanging is strictly worse than the clear timeout the connect-per-call
/// behaviour produced, so the caller checks first.
pub struct LiveKitSession {
    transport: Arc<dyn tddy_rpc::RpcClientTransport>,
    peer_present: Box<dyn Fn() -> bool + Send + Sync>,
}

impl LiveKitSession {
    pub fn new(
        transport: Arc<dyn tddy_rpc::RpcClientTransport>,
        peer_present: Box<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            transport,
            peer_present,
        }
    }

    /// The RPC transport addressed at the remote daemon.
    pub fn transport(&self) -> &Arc<dyn tddy_rpc::RpcClientTransport> {
        &self.transport
    }

    /// Whether the remote daemon's RPC-server participant is still in the room.
    pub fn peer_present(&self) -> bool {
        (self.peer_present)()
    }
}

/// The session for `key` from `cache`, joining through `connector` on first use: the room in the
/// key's token, as the token's identity, waiting for `key.server_identity`.
pub async fn room_session_via(
    cache: &LiveKitRoomCache,
    key: &LiveKitRoomKey,
    connector: &dyn RoomConnector,
) -> Result<Arc<LiveKitSession>, String> {
    cache
        .client_via(key, || connect_room_session(key, connector))
        .await
}

impl LiveKitRoomCache {
    /// Return the session for `key`, running `connect` only if this destination has no live one.
    pub async fn client_via<F, Fut>(
        &self,
        key: &LiveKitRoomKey,
        connect: F,
    ) -> Result<Arc<LiveKitSession>, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Arc<LiveKitSession>, String>>,
    {
        // The map lock only hands out the cell; every await happens on the cell, so the lock is
        // never held across one.
        let cell = {
            let mut cells = self.cells.lock().expect("livekit room cache");
            Arc::clone(cells.entry(key.clone()).or_default())
        };
        cell.get_or_try_init(connect).await.cloned()
    }
}

/// How long to wait for the remote daemon's RPC-server participant to appear in the room. It is
/// normally already joined; this only covers the window right after a daemon restart.
const SERVER_PARTICIPANT_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// The room transport this process was given by its binary, set once at startup.
static ROOM_CONNECTOR: std::sync::OnceLock<Arc<dyn RoomConnector>> = std::sync::OnceLock::new();

/// A room transport was already installed in this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomConnectorAlreadyInstalled;

impl std::fmt::Display for RoomConnectorAlreadyInstalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a room transport is already installed in this process")
    }
}

impl std::error::Error for RoomConnectorAlreadyInstalled {}

/// Hand this process's room transport to the tool client: the one every split-session dispatch,
/// roster stream and conversation call joins its room through.
///
/// The binary calls this once at startup, because it is the only thing that knows which transport it
/// was built with — this library names none. Until it is called, a split session's dispatch fails
/// with [`NoRoomTransport`]'s message rather than reaching a room.
pub fn install_room_connector(
    connector: Arc<dyn RoomConnector>,
) -> Result<(), RoomConnectorAlreadyInstalled> {
    ROOM_CONNECTOR
        .set(connector)
        .map_err(|_| RoomConnectorAlreadyInstalled)
}

/// The installed room transport, or the one that refuses every join naming what is missing.
pub(crate) fn installed_room_connector() -> Arc<dyn RoomConnector> {
    ROOM_CONNECTOR
        .get()
        .cloned()
        .unwrap_or_else(|| Arc::new(NoRoomTransport))
}

/// The process-wide connection cache behind [`dispatch_via_livekit`].
pub(crate) fn livekit_room_cache() -> &'static LiveKitRoomCache {
    static CACHE: std::sync::OnceLock<LiveKitRoomCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(LiveKitRoomCache::default)
}

/// Join `key`'s room through `connector` and return a session addressed at the remote daemon's
/// RPC-server participant.
///
/// The returned session owns the room connection, so it lives exactly as long as the cache keeps it.
async fn connect_room_session(
    key: &LiveKitRoomKey,
    connector: &dyn RoomConnector,
) -> Result<Arc<LiveKitSession>, String> {
    let LiveKitRoomKey {
        url,
        room,
        token,
        server_identity,
    } = key;
    // The token was minted by the daemon for this room and carries the identity, so the join names
    // neither beyond the room it is for. Every client on the room shares one request-id space and one
    // response loop — the property this cache depends on, since it keeps the room alive across calls
    // rather than letting each call have a short-lived room of its own.
    let joined = connector
        .connect(
            &RoomJoin {
                url: url.clone(),
                room: room.clone(),
                identity: String::new(),
                auth: RoomAuth::Token(token.clone()),
            },
            server_identity,
            SERVER_PARTICIPANT_WAIT,
        )
        .await
        .map_err(|e| format!("{e} (room \"{room}\")"))?;

    spawn_worktree_activity_log(&joined);

    let presence = Arc::clone(&joined);
    Ok(Arc::new(LiveKitSession::new(
        joined.rpc(),
        Box::new(move || presence.peer_present()),
    )))
}

/// Log the worktree's activity for as long as this process holds the room.
///
/// The room the tools are dispatched in is the room of the checkout they operate on, and the daemon
/// that owns that checkout broadcasts what happens to it on
/// [`WORKTREE_ACTIVITY_TOPIC`](tddy_service::worktree_activity::WORKTREE_ACTIVITY_TOPIC).
/// Subscribing costs one task and yields one `DEBUG` line per event: nothing here derives state
/// from an event or acts on one, which
/// is deliberate — the log is what a `tddy-tools=debug` session leaves behind for someone asking
/// whether the agent's picture of the worktree was current, and the first consumer that needs more
/// than that will have this stream to read.
fn spawn_worktree_activity_log(room: &Arc<dyn RoomSession>) {
    let mut activity = room.subscribe(tddy_service::worktree_activity::WORKTREE_ACTIVITY_TOPIC);
    tokio::spawn(async move {
        while let Some(payload) = activity.recv().await {
            match worktree_activity_line(&payload) {
                Ok(line) => log::debug!(target: "tddy_session_tool_client", "{line}"),
                Err(e) => log::warn!(
                    target: "tddy_session_tool_client",
                    "worktree activity payload did not decode: {e}"
                ),
            }
        }
    });
}

/// Decode one `worktree.activity` payload and render the `DEBUG` line it produces.
///
/// Split out of [`spawn_worktree_activity_log`] because it is the whole of what that task decides:
/// the loop around it only chooses which log level to emit at, and a payload that does not decode —
/// an event from a newer publisher whose schema this build cannot read — must surface as a warning
/// rather than as one more silent drop on a fire-and-forget topic.
pub(crate) fn worktree_activity_line(payload: &[u8]) -> Result<String, prost::DecodeError> {
    use prost::Message as _;
    use tddy_service::proto::worktree_activity::WorktreeActivityEvent;

    let event = WorktreeActivityEvent::decode(payload)?;
    Ok(tddy_service::worktree_activity::format_worktree_activity_for_log(&event))
}

/// Forward a tool call to a remote daemon over LiveKit, addressed at its RPC-server participant.
///
/// The join token is scoped to `key.room` by the daemon that minted it, so the room is not
/// re-derived here — it is carried for diagnostics, where "which room did we join" is the first
/// question.
///
/// Every failure below is also logged at `error`. The returned JSON is the *model's* answer and
/// goes nowhere an operator looks; a split session whose dispatch is failing would otherwise leave
/// no evidence on either host. `tddy-tools`' default filter is `warn`, so anything quieter than
/// this is invisible by construction.
pub async fn dispatch_via_livekit(
    key: &LiveKitRoomKey,
    envelope: &SessionToolEnvelope,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    dispatch_request_via_livekit(key, execute_tool_request(envelope, tool_name, args)).await
}

/// [`dispatch_via_livekit`] for a request already built.
pub(crate) async fn dispatch_request_via_livekit(
    key: &LiveKitRoomKey,
    request: tddy_service::proto::exec_tools::ExecuteToolRequest,
) -> String {
    let tool_name = request.tool_name.as_str();
    let LiveKitRoomKey {
        url,
        room,
        server_identity,
        ..
    } = key;
    log::info!(
        target: "tddy_session_tool_client",
        "dispatching {tool_name} to \"{server_identity}\" in room \"{room}\" at {url}"
    );
    let session = match room_session_via(
        livekit_room_cache(),
        key,
        installed_room_connector().as_ref(),
    )
    .await
    {
        Ok(session) => session,
        Err(e) => {
            log::error!(
                target: "tddy_session_tool_client",
                "{tool_name} failed: cannot reach codebase daemon \"{server_identity}\" \
                 in room \"{room}\" at {url}: {e}"
            );
            return serde_json::json!({"error": e, "is_error": true}).to_string();
        }
    };
    // Holding the connection moves the participant wait to first connect, so a daemon that
    // restarted since then would receive nothing — and with no request deadline anywhere below
    // this, the call would hang instead of failing. Checked per call for that reason.
    if !session.peer_present() {
        log::error!(
            target: "tddy_session_tool_client",
            "{tool_name} failed: codebase daemon \"{server_identity}\" left room \"{room}\" \
             at {url} — the held connection publishes to nobody until it rejoins"
        );
        return serde_json::json!({
            "error": format!(
                "remote daemon participant \"{server_identity}\" is no longer in room \"{room}\""
            ),
            "is_error": true
        })
        .to_string();
    }
    // Streaming, not unary: this is the one transport whose messages are chunk-framed past
    // MAX_CHUNK_FRAME_BYTES, where a lost chunk frame wedges the call with no error at all.
    dispatch_request_via_streaming_rpc(session.transport(), request).await
}
