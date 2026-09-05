//! The seam between Tauri's IPC layer and the `tddy-tauri-rpc` host.
//!
//! Three commands make up the whole UI↔daemon contract, and the frames they carry are
//! `rpc_envelope` bytes — never JSON. A page opens a connection with [`tddy_rpc_connect`], naming
//! what that connection reaches and registering a response channel for it; it sends one encoded
//! `RpcRequest` per call through [`tddy_rpc_send`]; and it gives a connection back with
//! [`tddy_rpc_disconnect`]. Every `RpcResponse` comes back on the channel the connection was
//! opened with.
//!
//! A page holds **as many connections as it has things to reach** — the daemon, and one per
//! attached session — so the host behind these commands is a [`MultiConnectionHost`]. Only
//! `tddy_rpc_connect` carries a target: a request frame is routed by the client epoch it is already
//! stamped with, which is the epoch its connection was opened under, so the send path needs no
//! target and must not grow one.

use std::sync::Arc;

use serde::Deserialize;
use tauri::ipc::{Channel, InvokeBody, InvokeResponseBody, Request};
use tauri::State;
use tddy_rpc::RpcService;
use tddy_tauri_rpc::{
    ConnectError, ConnectionTarget, FrameError, FrameSink, MultiConnectionHost, RosterResolver,
    SinkClosed,
};

/// The daemon's RPC, hosted for every connection the dashboard page holds.
pub struct RpcState {
    host: MultiConnectionHost<DaemonRosters>,
}

impl RpcState {
    pub fn new(host: MultiConnectionHost<DaemonRosters>) -> Self {
        Self { host }
    }

    /// Release every connection this host holds.
    ///
    /// A page owns its connections and nothing else does, so when a page goes away — a reload, or
    /// a navigation — everything it opened has to go with it. Nothing else can tell: the page that
    /// left cannot send a disconnect for connections it no longer remembers, and a host-side peer
    /// whose page is gone is only noticed lazily, when a response it can no longer deliver is
    /// published. See `lib.rs`, which calls this as the replacing page commits.
    pub async fn disconnect_all(&self) {
        self.host.disconnect_all().await;
    }
}

/// Resolves every connection target to the daemon this process is hosting.
///
/// A session target resolving to the daemon's roster is not a stand-in for something better — it
/// is what the embedded daemon *is*. It serves session-scoped RPCs itself, locally, and it routes
/// them by what the request names rather than by the connection they arrived on. So the addressing
/// is real everywhere it has to be: each connection gets its own epoch, its own engine, its own
/// peer and its own backpressure, and releasing one leaves the rest serving. What every target
/// shares is the roster behind it, because there is one daemon behind all of them.
///
/// Deliberately **not** gated on a live session. [`RosterResolver::roster_for`] is synchronous,
/// and the only lookup that could say a session is live — `CliSessionManager::get` — is async.
/// Refusing an unknown session at connect would mean widening that trait, which is a
/// `tddy-tauri-rpc` change this host has no business making. A call naming a session the daemon
/// does not have is answered by the daemon, with the error it answers a served page with.
pub struct DaemonRosters {
    daemon: Arc<dyn RpcService>,
}

impl DaemonRosters {
    /// Resolve every target to `daemon`.
    pub fn over(daemon: Arc<dyn RpcService>) -> Self {
        Self { daemon }
    }
}

impl RosterResolver for DaemonRosters {
    fn roster_for(&self, _target: &ConnectionTarget) -> Option<Arc<dyn RpcService>> {
        Some(self.daemon.clone())
    }
}

/// What a page named on [`tddy_rpc_connect`], as it crosses the IPC boundary.
///
/// A wire type of its own, rather than a `Deserialize` on [`ConnectionTarget`]: `tddy-tauri-rpc`
/// has no serde dependency and is not to grow one — it is a generic webview-RPC host, and how a
/// target happens to be spelt over *this* host's IPC is this host's business.
///
/// The spelling is the page's: `{ kind: "daemon" }` or `{ kind: "session", sessionId }`. Note that
/// the container's `rename_all` renames the **variant names** (`Daemon` → `"daemon"`) and nothing
/// inside them, so the field inside `Session` needs its own — hence `rename_all_fields`.
#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TargetArgument {
    Daemon,
    Session { session_id: String },
}

impl From<TargetArgument> for ConnectionTarget {
    fn from(target: TargetArgument) -> Self {
        match target {
            TargetArgument::Daemon => ConnectionTarget::Daemon,
            TargetArgument::Session { session_id } => ConnectionTarget::Session { session_id },
        }
    }
}

/// A [`FrameSink`] over one connection's Tauri IPC channel.
struct ChannelSink {
    channel: Channel<InvokeResponseBody>,
}

impl FrameSink for ChannelSink {
    fn send(&self, frame: Vec<u8>) -> Result<(), SinkClosed> {
        self.channel
            .send(InvokeResponseBody::Raw(frame))
            .map_err(|error| {
                log::debug!(
                    "[tddy-desktop] the webview's response channel refused a frame: {error}"
                );
                SinkClosed
            })
    }

    fn close(&self) {
        // Nothing to signal: a Tauri channel has no end-of-stream marker, and the page that owned
        // this one either released it on purpose or is gone with its window. Dropping the channel
        // with this sink is the whole release.
    }
}

/// Release the connection registered under `client_epoch`.
///
/// Sessions come and go far more often than pages do: a detach must release its host-side peer, or
/// every attach leaks one. The single-connection host never needed this, because `connect` reaped
/// the one slot on its own — with addressed connections that is no longer implicit.
///
/// Idempotent, so a detach racing an unmount is harmless.
#[tauri::command]
pub async fn tddy_rpc_disconnect(
    state: State<'_, RpcState>,
    client_epoch: u32,
) -> Result<(), String> {
    state.host.disconnect(client_epoch).await;
    Ok(())
}

/// Open a connection to `target`, registering the response channel its answers come back on.
///
/// `client_epoch` is the connection's identity: the page mints one per transport and stamps every
/// frame with it. Opening a connection disturbs none of the page's others.
#[tauri::command]
pub async fn tddy_rpc_connect(
    state: State<'_, RpcState>,
    channel: Channel<InvokeResponseBody>,
    client_epoch: u32,
    target: TargetArgument,
) -> Result<(), String> {
    state
        .host
        .connect(
            target.into(),
            Arc::new(ChannelSink { channel }),
            client_epoch,
        )
        .await
        .map_err(describe_connect_error)
}

/// Carry one encoded `RpcRequest` frame to the daemon.
///
/// The frame is the invoke body itself, so it crosses as bytes with no base64 and no JSON
/// envelope. Awaiting the dispatch is deliberate: each connection's response queue is bounded, so a
/// page that has stopped reading one channel has that connection's `invoke` promise held rather
/// than being allowed to pile up work the daemon cannot deliver — and only that connection's.
#[tauri::command]
pub async fn tddy_rpc_send(state: State<'_, RpcState>, request: Request<'_>) -> Result<(), String> {
    let InvokeBody::Raw(frame) = request.body() else {
        return Err(
            "tddy_rpc_send expects the encoded request frame as the raw invoke body".to_string(),
        );
    };
    state
        .host
        .handle_request_frame(frame)
        .await
        .map_err(describe_frame_error)
}

/// A refused connection, as a message the page's transport can log.
///
/// Both refusals are the page's to act on rather than the host's to paper over: a connection that
/// was not opened has no peer behind it, and a page told nothing would issue calls onto it and wait
/// forever for answers nothing is producing.
fn describe_connect_error(error: ConnectError) -> String {
    match error {
        ConnectError::NoSuchTarget { target } => format!(
            "nothing here serves {}: it is unknown, or it has ended",
            describe_target(&target)
        ),
        // Epochs are minted per transport on the page side, so a collision means the page reused
        // one. The connection already registered under it keeps serving.
        ConnectError::EpochInUse { client_epoch } => format!(
            "a connection is already open under client epoch {client_epoch}; \
             mint one epoch per connection"
        ),
    }
}

/// A target, as prose for a log line.
fn describe_target(target: &ConnectionTarget) -> String {
    match target {
        ConnectionTarget::Daemon => "the daemon".to_string(),
        ConnectionTarget::Session { session_id } => format!("session {session_id}"),
    }
}

/// A refused frame, as a message the page's transport can log.
fn describe_frame_error(error: FrameError) -> String {
    match error {
        FrameError::NotConnected => {
            "no connection is open for that epoch: call tddy_rpc_connect first".to_string()
        }
        FrameError::Malformed(reason) => {
            format!("the request frame could not be decoded: {reason}")
        }
        // The connection this frame belongs to is gone — most often because the page that opened it
        // was replaced by a reload, whose connections took its place. Saying so is the point: the
        // frame's answer has no sink to go out on, so a caller that is not told waits forever.
        FrameError::StaleConnection { connected, frame } => format!(
            "this frame belongs to a connection that is gone (frame epoch {frame}; \
             the one open connection is epoch {connected}); reconnect before sending"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The target argument as `tddy-tauri-web`'s bridge spells it on the wire, parsed the way a
    /// Tauri command argument is: out of the invoke payload's JSON.
    fn target_named_by_a_page(json: &str) -> ConnectionTarget {
        serde_json::from_str::<TargetArgument>(json)
            .expect("a page's target should deserialize")
            .into()
    }

    #[test]
    fn reads_the_daemon_target_a_page_names() {
        // Given the JSON `DAEMON_TARGET` crosses as
        let named = r#"{"kind":"daemon"}"#;

        // When the connect command deserializes its target argument
        let target = target_named_by_a_page(named);

        // Then
        assert_eq!(target, ConnectionTarget::Daemon);
    }

    #[test]
    fn reads_the_session_target_a_page_names_with_its_camel_cased_session_id() {
        // Given the JSON `sessionTarget("sess-7")` crosses as
        let named = r#"{"kind":"session","sessionId":"sess-7"}"#;

        // When the connect command deserializes its target argument
        let target = target_named_by_a_page(named);

        // Then
        assert_eq!(
            target,
            ConnectionTarget::Session {
                session_id: "sess-7".to_string()
            }
        );
    }
}
