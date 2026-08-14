//! Generic session tool dispatch — forwards MCP tool calls to `tddy-daemon` via sandbox IPC,
//! direct HTTP, or LiveKit RPC to a remote daemon, depending on environment.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub use tddy_sandbox::session_id_from_env;

/// How `tddy-tools --mcp` reaches the daemon's `ExecuteTool` handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionToolTransport {
    /// In-jail MCP → unix socket → sandbox-runner → SessionChannel → host daemon.
    SandboxIpc { socket_path: PathBuf },
    /// Direct HTTP Connect POST to `ConnectionService/ExecuteTool`.
    DaemonHttp {
        session_id: String,
        daemon_url: String,
        session_token: String,
        daemon_instance_id: String,
    },
    /// Direct LiveKit RPC to a *remote* daemon's `ConnectionService` — a split session, where the
    /// agent runs on one host and its worktree lives on another
    /// (docs/ft/daemon/remote-managed-worktree.md).
    LiveKit {
        url: String,
        room: String,
        token: String,
        server_identity: String,
        session_id: String,
        session_token: String,
        daemon_instance_id: String,
    },
    /// Not a transport but the absence of a usable one: some of the LiveKit variables are set and
    /// the rest are not.
    ///
    /// Carried as an outcome rather than reported as "nothing configured" because the two are not
    /// the same failure. The LiveKit variables are only ever exported for a split session, whose
    /// worktree is on another host — `split_remote_tool_env` leaves `daemon_url` empty for exactly
    /// that reason — so there is no relay on this side to fall through to, and a stray one would
    /// answer from the wrong host's filesystem. Naming the missing variable is the only answer that
    /// points at the fault.
    IncompleteLiveKit { missing: Vec<&'static str> },
}

/// The identity fields carried in every `ExecuteToolRequest`.
///
/// The sandbox socket implies identity by the connection itself and leaves these empty. A remote
/// daemon has no such context: it resolves the worktree from `session_id` against its *own*
/// sessions base and authenticates `session_token`, so an empty envelope would find no worktree
/// and no user.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionToolEnvelope {
    pub session_id: String,
    pub session_token: String,
    pub daemon_instance_id: String,
}

/// The longest a tool call bound for a remote daemon may block.
///
/// Nothing on this path bounds a call: `RpcClient::call_server_stream` carries no deadline and
/// [`dispatch_via_streaming_rpc`] awaits its frames unbounded, so a codebase daemon that dies after
/// the request is published hangs the tool forever rather than failing it (docs/dev/TODO.md, "No
/// LiveKit RPC call has a client-side deadline"). Short blocks are what keeps that bounded in
/// practice: the agent drives long work through the background-job protocol and polls in slices, so
/// a dead peer shows up as one poll that never returns rather than as a session that simply stopped.
///
/// The budget also stays clear of the 30s idle deadline a *forwarded* stream imposes
/// (`PEER_FORWARD_STREAM_IDLE_TIMEOUT`). Nothing forwards today — `tddy-tools` joins the room and
/// calls the codebase daemon's participant directly, which routes the call locally — but a request
/// ever routed through a third daemon would need that margin, and 10s of it is the round trip.
pub const MAX_REMOTE_BLOCK_MS: u64 = 20_000;

/// What the remote tool engine blocks for when a call names no block time.
///
/// Mirrored from `tddy_tool_engine`'s `tool_await` and `tool_shell` (`unwrap_or(30_000)`);
/// `tddy-tools` does not depend on that crate. It matters here because it sits *above* the ceiling:
/// the two request shapes an agent actually emits — `Await {job_id}` and `Shell {command}` — carry
/// no block time at all, so the ceiling has to be written in rather than left to the engine.
const REMOTE_ENGINE_DEFAULT_BLOCK_MS: u64 = 30_000;

/// The engine's default is what an unclamped call would block for, so a ceiling at or above it would
/// be no ceiling at all in the case that actually happens.
const _: () = assert!(MAX_REMOTE_BLOCK_MS < REMOTE_ENGINE_DEFAULT_BLOCK_MS);

/// Cap a requested block time at [`MAX_REMOTE_BLOCK_MS`].
///
/// A ceiling only: `0` means "return the job's status immediately" and raising it would turn every
/// status poll into a blocking call.
pub fn clamp_remote_block_ms(requested: u64) -> u64 {
    requested.min(MAX_REMOTE_BLOCK_MS)
}

/// The argument each tool blocks on, in the order the remote engine reads them.
///
/// `tool_await` takes `timeout_ms` and falls back to `block_until_ms` only when the first key is
/// *absent*; `tool_shell` reads `block_until_ms` alone. Every other tool returns as soon as it has
/// its answer and carries no block time to cap.
fn remote_block_arg_keys(tool_name: &str) -> &'static [&'static str] {
    match tool_name {
        "Await" => &["timeout_ms", "block_until_ms"],
        "Shell" => &["block_until_ms"],
        _ => &[],
    }
}

/// Read a block time the way the remote engine would if it read every JSON number.
///
/// JSON has a single number type, so a serializer that emitted `30000.0` sent the same request as
/// one that emitted `30000` — the engine's `as_i64` rejects the first and falls back to its default,
/// which is precisely the block being capped. A negative value asks for an effectively unbounded
/// block (the engine casts it to `u64`), so it is treated as the largest request there is and lands
/// on the ceiling like any other. Anything that is not a number is not a request at all.
fn requested_block_ms(value: &serde_json::Value) -> Option<u64> {
    if let Some(requested) = value.as_u64() {
        return Some(requested);
    }
    if let Some(requested) = value.as_i64() {
        // Only negatives reach here; `as_u64` took the rest.
        debug_assert!(requested < 0);
        return Some(u64::MAX);
    }
    let requested = value.as_f64()?;
    if requested.is_nan() {
        return None;
    }
    if requested < 0.0 {
        return Some(u64::MAX);
    }
    // A float-to-integer cast saturates, so an infinity arrives as the same unbounded ask.
    Some(requested as u64)
}

/// Cap the block time of a tool call bound for a remote daemon.
///
/// The value is not merely clamped when present but *written in* when it is not: an absent key is
/// the common case, and it blocks for [`REMOTE_ENGINE_DEFAULT_BLOCK_MS`] rather than for nothing.
/// The same holds for a value the engine cannot read — it ignores it and applies that default too,
/// so a rewrite loses no request that was ever going to be honoured.
///
/// Only the one key the engine will actually read is touched. Filling in `timeout_ms` beside an
/// explicit `block_until_ms: 0` would override a status poll with a blocking call, since the engine
/// stops at the first key it finds.
pub fn clamp_remote_blocking_args(tool_name: &str, args: &serde_json::Value) -> serde_json::Value {
    let keys = remote_block_arg_keys(tool_name);
    let mut clamped = args.clone();
    let Some(fields) = clamped.as_object_mut() else {
        // Arguments that are not an object carry neither `command` nor `job_id`, so the call fails
        // in the engine before it blocks on anything.
        return clamped;
    };
    let Some(key) = keys
        .iter()
        .find(|key| fields.contains_key(**key))
        .or_else(|| keys.first())
    else {
        return clamped;
    };
    let requested = fields
        .get(*key)
        .and_then(requested_block_ms)
        .unwrap_or(REMOTE_ENGINE_DEFAULT_BLOCK_MS);
    fields.insert((*key).to_string(), clamp_remote_block_ms(requested).into());
    clamped
}

/// Read an environment variable, treating an empty value as unset — a blank join token or server
/// identity configures nothing.
fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// Every variable the LiveKit transport needs, reported missing in this order.
const LIVEKIT_ENV_KEYS: [&str; 5] = [
    "TDDY_REMOTE_LIVEKIT_URL",
    "TDDY_REMOTE_LIVEKIT_ROOM",
    "TDDY_REMOTE_LIVEKIT_TOKEN",
    "TDDY_REMOTE_SERVER_IDENTITY",
    "TDDY_REMOTE_SESSION_ID",
];

/// The variables that mean "this agent's worktree is on another host".
///
/// A narrower set than [`LIVEKIT_ENV_KEYS`]: `TDDY_REMOTE_SESSION_ID` is shared with the HTTP relay
/// and `TDDY_REMOTE_SERVER_IDENTITY` says nothing about LiveKit on its own, so an ordinary remote
/// session would look half-configured if either counted as intent.
const LIVEKIT_INTENT_ENV_KEYS: [&str; 3] = [
    "TDDY_REMOTE_LIVEKIT_URL",
    "TDDY_REMOTE_LIVEKIT_ROOM",
    "TDDY_REMOTE_LIVEKIT_TOKEN",
];

/// Detect which transport is configured for session tool dispatch.
pub fn detect_session_tool_transport() -> Option<SessionToolTransport> {
    if let Some(socket_path) = std::env::var_os("TDDY_SANDBOX_TOOL_IPC") {
        return Some(SessionToolTransport::SandboxIpc {
            socket_path: PathBuf::from(socket_path),
        });
    }
    // Every field is required. When some are set and some are not, this is a split session with a
    // broken environment, not a session with an HTTP relay: reporting what is missing is the only
    // answer that names the fault (see `SessionToolTransport::IncompleteLiveKit`).
    let configured = LIVEKIT_ENV_KEYS.map(non_empty_env);
    let missing: Vec<&'static str> = LIVEKIT_ENV_KEYS
        .iter()
        .zip(configured.iter())
        .filter(|(_, value)| value.is_none())
        .map(|(key, _)| *key)
        .collect();
    if let [Some(url), Some(room), Some(token), Some(server_identity), Some(session_id)] =
        configured
    {
        return Some(SessionToolTransport::LiveKit {
            url,
            room,
            token,
            server_identity,
            session_id,
            session_token: std::env::var("TDDY_REMOTE_SESSION_TOKEN").unwrap_or_default(),
            daemon_instance_id: std::env::var("TDDY_REMOTE_DAEMON_INSTANCE_ID").unwrap_or_default(),
        });
    }
    if LIVEKIT_INTENT_ENV_KEYS
        .iter()
        .any(|key| non_empty_env(key).is_some())
    {
        return Some(SessionToolTransport::IncompleteLiveKit { missing });
    }
    // Blank counts as unset here too: `RemoteToolEnv::env_pairs` always exports
    // `TDDY_REMOTE_DAEMON_URL`, and a split session exports it empty. Taken as a URL it produced
    // "relay connection error: relative URL without a base" on every tool call — an error naming
    // the relay for a LiveKit misconfiguration.
    if let (Some(session_id), Some(daemon_url)) = (
        non_empty_env("TDDY_REMOTE_SESSION_ID"),
        non_empty_env("TDDY_REMOTE_DAEMON_URL"),
    ) {
        return Some(SessionToolTransport::DaemonHttp {
            session_id,
            daemon_url,
            session_token: std::env::var("TDDY_REMOTE_SESSION_TOKEN").unwrap_or_default(),
            daemon_instance_id: std::env::var("TDDY_REMOTE_DAEMON_INSTANCE_ID").unwrap_or_default(),
        });
    }
    None
}

/// Format an MCP tool result string from a daemon `ExecuteToolResponse` body.
///
/// On success returns `result_json` verbatim. On error returns a JSON object with
/// `error` and `is_error: true`.
pub fn format_tool_dispatch_result(body: &serde_json::Value) -> String {
    if body
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        serde_json::json!({
            "error": body
                .get("error_message")
                .and_then(|v| v.as_str())
                .unwrap_or("relay error"),
            "is_error": true
        })
        .to_string()
    } else {
        body.get("result_json")
            .and_then(|v| v.as_str())
            .unwrap_or("{}")
            .to_string()
    }
}

/// The answer to a tool call whose split session cannot reach its worktree.
///
/// Names the variables rather than the transport: the symptom an operator sees is a tool call
/// failing, and every other wording would send them looking at LiveKit, the relay or the daemon
/// instead of at the environment the agent was spawned with.
fn incomplete_livekit_error(missing: &[&str]) -> String {
    serde_json::json!({
        "error": format!(
            "remote worktree dispatch is misconfigured: a LiveKit environment is set but {} \
             {} empty or unset",
            missing.join(", "),
            if missing.len() == 1 { "is" } else { "are" }
        ),
        "is_error": true
    })
    .to_string()
}

fn not_configured_error() -> String {
    serde_json::json!({
        "error": "remote toolset not configured: TDDY_REMOTE_SESSION_ID and TDDY_REMOTE_DAEMON_URL must be set",
        "is_error": true
    })
    .to_string()
}

/// Dispatch a dynamic tool call to the session daemon (sandbox IPC, HTTP, or LiveKit RPC).
pub async fn dispatch_session_tool(tool_name: &str, args: serde_json::Value) -> String {
    let Some(transport) = detect_session_tool_transport() else {
        return not_configured_error();
    };
    match transport {
        SessionToolTransport::SandboxIpc { socket_path } => {
            dispatch_via_sandbox_ipc(&socket_path, tool_name, &args).await
        }
        SessionToolTransport::DaemonHttp {
            session_id,
            daemon_url,
            session_token,
            daemon_instance_id,
        } => {
            dispatch_via_daemon_http(
                &daemon_url,
                &session_id,
                &session_token,
                &daemon_instance_id,
                tool_name,
                &args,
            )
            .await
        }
        SessionToolTransport::LiveKit {
            url,
            room,
            token,
            server_identity,
            session_id,
            session_token,
            daemon_instance_id,
        } => {
            // Only this transport can hang: the sandbox socket and the HTTP relay both fail a dead
            // peer, so shortening their blocks would cost time and buy nothing.
            let args = clamp_remote_blocking_args(tool_name, &args);
            let key = LiveKitRoomKey {
                url,
                room,
                token,
                server_identity,
            };
            let envelope = SessionToolEnvelope {
                session_id,
                session_token,
                daemon_instance_id,
            };
            dispatch_via_livekit(&key, &envelope, tool_name, &args).await
        }
        SessionToolTransport::IncompleteLiveKit { missing } => incomplete_livekit_error(&missing),
    }
}

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
#[cfg(feature = "livekit")]
const SERVER_PARTICIPANT_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// The process-wide connection cache behind [`dispatch_via_livekit`].
#[cfg(feature = "livekit")]
fn livekit_room_cache() -> &'static LiveKitRoomCache {
    static CACHE: std::sync::OnceLock<LiveKitRoomCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(LiveKitRoomCache::default)
}

/// Join `room` and return a client addressed at the remote daemon's RPC-server participant.
///
/// The returned client owns the `Arc<Room>`, so the connection lives exactly as long as the cache
/// keeps the client.
#[cfg(feature = "livekit")]
async fn connect_livekit_client(key: &LiveKitRoomKey) -> Result<Arc<LiveKitSession>, String> {
    use livekit::prelude::*;

    let LiveKitRoomKey {
        url,
        room,
        token,
        server_identity,
    } = key;
    let (connected_room, mut room_events) = Room::connect(url, token, RoomOptions::default())
        .await
        .map_err(|e| format!("livekit room connect: {e}"))?;
    let connected_room = Arc::new(connected_room);

    let target: ParticipantIdentity = server_identity.clone().into();
    if !connected_room.remote_participants().contains_key(&target) {
        let appeared = tokio::time::timeout(SERVER_PARTICIPANT_WAIT, async {
            while let Some(event) = room_events.recv().await {
                if let RoomEvent::ParticipantConnected(participant) = event {
                    if participant.identity() == target {
                        return;
                    }
                }
            }
        })
        .await;
        if appeared.is_err() {
            return Err(format!(
                "timed out waiting for remote daemon participant \"{server_identity}\" in room \"{room}\""
            ));
        }
    }

    // Through the factory rather than building an RpcClient directly: it owns one ClientEngine and
    // one response loop per room, so vended clients share a request-id space instead of each
    // starting at 1 and leaking its own `room.subscribe()` loop. Harmless while every call had its
    // own short-lived room; caching the room is what would make it bite.
    let presence_room = Arc::clone(&connected_room);
    let presence_target = target.clone();
    Ok(Arc::new(LiveKitSession::new(
        Arc::new(tddy_livekit::LiveKitRpcClientFactory::for_room(connected_room).client(target)),
        Box::new(move || {
            presence_room
                .remote_participants()
                .contains_key(&presence_target)
        }),
    )))
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
#[cfg(feature = "livekit")]
pub async fn dispatch_via_livekit(
    key: &LiveKitRoomKey,
    envelope: &SessionToolEnvelope,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    let LiveKitRoomKey {
        url,
        room,
        server_identity,
        ..
    } = key;
    log::info!(
        target: "tddy_tools::session_tool_client",
        "dispatching {tool_name} to \"{server_identity}\" in room \"{room}\" at {url}"
    );
    let session = match livekit_room_cache()
        .client_via(key, || connect_livekit_client(key))
        .await
    {
        Ok(session) => session,
        Err(e) => {
            log::error!(
                target: "tddy_tools::session_tool_client",
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
            target: "tddy_tools::session_tool_client",
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
    dispatch_via_streaming_rpc(session.transport(), envelope, tool_name, args).await
}

/// Without the `livekit` feature the SDK is not linked, so a split session cannot reach its
/// worktree at all — it says so rather than degrading to a transport that would target the wrong
/// host's filesystem.
#[cfg(not(feature = "livekit"))]
pub async fn dispatch_via_livekit(
    key: &LiveKitRoomKey,
    _envelope: &SessionToolEnvelope,
    tool_name: &str,
    _args: &serde_json::Value,
) -> String {
    // A build-time omission, not a runtime fault — but it presents as every tool call failing, so
    // it is logged where an operator will find it like any other dispatch failure.
    log::error!(
        target: "tddy_tools::session_tool_client",
        "{tool_name} failed: this tddy-tools was built without the 'livekit' feature, so the \
         codebase daemon \"{}\" in room \"{}\" cannot be reached at all",
        key.server_identity,
        key.room
    );
    serde_json::json!({
        "error": "remote worktree dispatch requires the 'livekit' cargo feature; \
                  rebuild with: cargo build -p tddy-tools --features livekit",
        "is_error": true
    })
    .to_string()
}

/// `dispatch_via_sandbox_ipc`'s stdio-RPC connection never receives inbound calls from the
/// sandbox-runner — any request here would be a bug, so it fails loudly rather than silently
/// no-op'ing.
struct NoCallbackToolService;

#[async_trait::async_trait]
impl tddy_rpc::RpcService for NoCallbackToolService {
    async fn handle_rpc(
        &self,
        service: &str,
        method: &str,
        _message: &tddy_rpc::RpcMessage,
    ) -> tddy_rpc::RpcResult {
        tddy_rpc::RpcResult::Unary(Err(tddy_rpc::Status::unimplemented(format!(
            "tddy-tools hosts no callback service, got {service}/{method}"
        ))))
    }
}

/// Forward a tool call over the sandbox unix IPC socket, using `tddy-rpc`'s length-prefixed
/// framing (`connection.ConnectionService/ExecuteTool`) rather than the socket path itself
/// carrying any particular wire format — the socket is just a duplex byte stream `tddy-stdio`'s
/// `StdioEndpoint` can wrap like any other (see `StdioEndpoint::from_duplex`).
pub async fn dispatch_via_sandbox_ipc(
    socket_path: &std::path::Path,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    let stream = match tokio::net::UnixStream::connect(socket_path).await {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({"error": format!("tool ipc connect: {e}"), "is_error": true})
                .to_string();
        }
    };
    let (read_half, write_half) = tokio::io::split(stream);
    let (client, endpoint) =
        tddy_stdio::StdioEndpoint::from_duplex(read_half, write_half, NoCallbackToolService);
    tokio::spawn(endpoint.run());
    let client: std::sync::Arc<dyn tddy_rpc::RpcClientTransport> = client;
    // The socket itself identifies the session to the sandbox-runner, so the envelope stays empty.
    dispatch_via_rpc_transport(&client, &SessionToolEnvelope::default(), tool_name, args).await
}

/// Forward a tool call via HTTP to `ConnectionService/ExecuteTool`.
pub async fn dispatch_via_daemon_http(
    daemon_url: &str,
    session_id: &str,
    session_token: &str,
    daemon_instance_id: &str,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    let req_body = serde_json::json!({
        "session_token": session_token,
        "session_id": session_id,
        "tool_name": tool_name,
        "args_json": args.to_string(),
        "daemon_instance_id": daemon_instance_id,
    });

    let url = format!(
        "{}/connection.ConnectionService/ExecuteTool",
        daemon_url.trim_end_matches('/')
    );

    let client = reqwest::Client::new();
    match client
        .post(&url)
        .header("content-type", "application/json")
        .json(&req_body)
        .send()
        .await
    {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(body) => format_tool_dispatch_result(&body),
            Err(e) => {
                serde_json::json!({"error": format!("relay parse error: {e}"), "is_error": true})
                    .to_string()
            }
        },
        Err(e) => {
            serde_json::json!({"error": format!("relay connection error: {e}"), "is_error": true})
                .to_string()
        }
    }
}

/// The request both the unary and the streaming call send. Shared so the two cannot drift: the
/// remote daemon resolves the worktree and authenticates from these fields alone, so a difference
/// between the transports would show up as a missing worktree rather than a wrong request.
fn execute_tool_request(
    envelope: &SessionToolEnvelope,
    tool_name: &str,
    args: &serde_json::Value,
) -> tddy_service::proto::connection::ExecuteToolRequest {
    tddy_service::proto::connection::ExecuteToolRequest {
        session_token: envelope.session_token.clone(),
        session_id: envelope.session_id.clone(),
        tool_name: tool_name.to_string(),
        args_json: args.to_string(),
        daemon_instance_id: envelope.daemon_instance_id.clone(),
    }
}

/// Forward a tool call over an already-connected RPC transport (`tddy-stdio`'s `StdioRpcClient`
/// over the sandbox socket, `tddy-livekit`'s `RpcClient` to a remote daemon), calling
/// `connection.ConnectionService/ExecuteTool`.
///
/// Transport-agnostic by construction: what differs between the two is the request envelope, not
/// the call.
pub async fn dispatch_via_rpc_transport(
    client: &std::sync::Arc<dyn tddy_rpc::RpcClientTransport>,
    envelope: &SessionToolEnvelope,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    use prost::Message;
    use tddy_service::proto::connection::ExecuteToolResponse;

    let request = execute_tool_request(envelope, tool_name, args);
    let response_bytes = match client
        .call_unary(
            "connection.ConnectionService",
            "ExecuteTool",
            request.encode_to_vec(),
        )
        .await
    {
        Ok(bytes) => bytes,
        Err(e) => {
            return serde_json::json!({"error": format!("tool rpc call: {e}"), "is_error": true})
                .to_string();
        }
    };
    let response = match ExecuteToolResponse::decode(response_bytes.as_slice()) {
        Ok(resp) => resp,
        Err(e) => {
            return serde_json::json!({
                "error": format!("tool rpc decode response: {e}"),
                "is_error": true
            })
            .to_string();
        }
    };
    if response.is_error {
        serde_json::json!({"error": response.error_message, "is_error": true}).to_string()
    } else {
        response.result_json
    }
}

/// Forward a tool call over an already-connected RPC transport, calling
/// `connection.ConnectionService/StreamExecuteTool` and reassembling its frames.
///
/// The unary sibling returns `result_json` as one string, which over LiveKit is chunk-framed past
/// `MAX_CHUNK_FRAME_BYTES` — and that reassembly is index-keyed and best-effort, so a lost frame
/// wedges the call permanently with no error. The streamed frames are bounded below that threshold
/// and the last one is marked, so a short result is *detectable* here rather than silently passed on
/// as a complete one.
///
/// Detectable once the stream *ends*, which is as far as this goes: nothing between here and the
/// remote daemon bounds how long that takes. `tddy-tools` calls the codebase daemon's participant
/// directly and that daemon routes the call locally, so no forwarding hop — and none of the idle
/// deadlines a forwarded stream has — is involved. A peer that dies after the request is published
/// leaves the loop below waiting forever.
///
/// Every failure is logged at `error` as well as returned. The returned JSON reaches the model and
/// nothing else, and `tddy-tools`' default filter is `warn` — so without this a split session whose
/// tool path is broken produces no evidence an operator can find. The session and daemon ids are
/// what identify *which* remote worktree the call was bound for.
pub async fn dispatch_via_streaming_rpc(
    client: &std::sync::Arc<dyn tddy_rpc::RpcClientTransport>,
    envelope: &SessionToolEnvelope,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    use prost::Message;
    use tddy_service::proto::connection::ExecuteToolChunk;

    let SessionToolEnvelope {
        session_id,
        daemon_instance_id,
        ..
    } = envelope;
    let request = execute_tool_request(envelope, tool_name, args);
    let mut frames = match client
        .call_server_stream(
            "connection.ConnectionService",
            "StreamExecuteTool",
            request.encode_to_vec(),
        )
        .await
    {
        Ok(frames) => frames,
        Err(e) => {
            log::error!(
                target: "tddy_tools::session_tool_client",
                "{tool_name} failed: StreamExecuteTool call to daemon \"{daemon_instance_id}\" \
                 for session {session_id} was not accepted: {e}"
            );
            return serde_json::json!({"error": format!("tool rpc call: {e}"), "is_error": true})
                .to_string();
        }
    };

    let mut result = Vec::new();
    // TODO(livekit-rpc-deadline): this await is unbounded, so a peer that stops sending hangs the
    // tool call rather than failing it. The fix belongs on the client — a deadline on
    // `tddy_livekit::RpcClient` / `tddy_rpc`'s `ClientEngine`, which long-lived streams must not
    // inherit — not a timeout wrapped around this loop. Recorded in docs/dev/TODO.md, "No LiveKit
    // RPC call has a client-side deadline". Until then, `MAX_REMOTE_BLOCK_MS` is what keeps a
    // hang to one poll rather than a whole session.
    while let Some(frame) = frames.recv().await {
        let frame = match frame {
            Ok(bytes) => bytes,
            Err(e) => {
                log::error!(
                    target: "tddy_tools::session_tool_client",
                    "{tool_name} failed: StreamExecuteTool from daemon \"{daemon_instance_id}\" \
                     for session {session_id} errored after {} bytes: {e}",
                    result.len()
                );
                return serde_json::json!({
                    "error": format!("tool rpc stream: {e}"),
                    "is_error": true
                })
                .to_string();
            }
        };
        let frame = match ExecuteToolChunk::decode(frame.as_slice()) {
            Ok(frame) => frame,
            Err(e) => {
                log::error!(
                    target: "tddy_tools::session_tool_client",
                    "{tool_name} failed: undecodable frame ({} bytes) from daemon \
                     \"{daemon_instance_id}\" for session {session_id} — the two hosts disagree \
                     about ExecuteToolChunk: {e}",
                    frame.len()
                );
                return serde_json::json!({
                    "error": format!("tool rpc decode frame: {e}"),
                    "is_error": true
                })
                .to_string();
            }
        };
        result.extend_from_slice(&frame.result_chunk);
        if !frame.last {
            continue;
        }
        // A frame boundary may split a multi-byte character, so only the reassembled result is
        // required to be UTF-8; if it is not, the frames did not reassemble into what was sent.
        let result_json = match String::from_utf8(result) {
            Ok(text) => text,
            Err(e) => {
                log::error!(
                    target: "tddy_tools::session_tool_client",
                    "{tool_name} failed: frames from daemon \"{daemon_instance_id}\" for session \
                     {session_id} did not reassemble as UTF-8: {e}"
                );
                return serde_json::json!({
                    "error": format!("tool result frames did not reassemble as UTF-8: {e}"),
                    "is_error": true
                })
                .to_string();
            }
        };
        // `job_id` and `job_running` are already inside `result_json`, exactly as with the unary
        // response — carrying them on the frame lets the daemon report them without parsing it.
        return format_tool_dispatch_result(&serde_json::json!({
            "result_json": result_json,
            "is_error": frame.is_error,
            "error_message": frame.error_message,
        }));
    }

    // The stream ended without its final frame. The bytes collected so far are a prefix of the
    // result, and returning them would hand the agent a half-read file that looks whole — the exact
    // failure this RPC exists to make visible.
    log::error!(
        target: "tddy_tools::session_tool_client",
        "{tool_name} failed: StreamExecuteTool from daemon \"{daemon_instance_id}\" for session \
         {session_id} ended after {} bytes without its final frame; the partial result was \
         discarded",
        result.len()
    );
    serde_json::json!({
        "error": format!(
            "tool result truncated: {tool_name} stream ended after {} bytes without its final frame",
            result.len()
        ),
        "is_error": true
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_tool_dispatch_result_returns_result_json_on_success() {
        // Given
        let body = serde_json::json!({
            "result_json": r#"{"path":"README.md"}"#,
            "is_error": false
        });

        // When
        let out = format_tool_dispatch_result(&body);

        // Then
        assert_eq!(out, r#"{"path":"README.md"}"#);
    }

    #[test]
    fn format_tool_dispatch_result_returns_error_object_on_failure() {
        // Given
        let body = serde_json::json!({
            "is_error": true,
            "error_message": "permission denied"
        });

        // When
        let out = format_tool_dispatch_result(&body);
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("json");

        // Then
        assert_eq!(parsed["is_error"], true);
        assert_eq!(parsed["error"], "permission denied");
    }

    #[test]
    #[serial_test::serial]
    fn detect_transport_prefers_sandbox_ipc_over_remote() {
        // Given
        std::env::set_var("TDDY_SANDBOX_TOOL_IPC", "/tmp/tddy-tool-ipc.sock");
        std::env::set_var("TDDY_REMOTE_SESSION_ID", "remote-session");
        std::env::set_var("TDDY_REMOTE_DAEMON_URL", "http://127.0.0.1:8080");

        // When
        let transport = detect_session_tool_transport().expect("transport");

        // Then
        assert_eq!(
            transport,
            SessionToolTransport::SandboxIpc {
                socket_path: PathBuf::from("/tmp/tddy-tool-ipc.sock")
            }
        );

        std::env::remove_var("TDDY_SANDBOX_TOOL_IPC");
        std::env::remove_var("TDDY_REMOTE_SESSION_ID");
        std::env::remove_var("TDDY_REMOTE_DAEMON_URL");
    }
}
