//! The seam between Tauri's IPC layer and the `tddy-tauri-rpc` host.
//!
//! Two commands make up the whole UI↔daemon contract, and both carry `rpc_envelope` frames as raw
//! bytes — never JSON. A page registers one response channel with [`tddy_rpc_connect`], then
//! sends one encoded `RpcRequest` per call through [`tddy_rpc_send`]; every `RpcResponse` comes
//! back on the registered channel. That is a duplex frame pipe, which is all
//! [`WebviewRpcHost`] needs and all the browser-side transport (`tddy-tauri-web`) expects.

use std::sync::Arc;

use tauri::ipc::{Channel, InvokeBody, InvokeResponseBody, Request};
use tauri::State;
use tddy_rpc::MultiRpcService;
use tddy_tauri_rpc::{FrameError, FrameSink, SinkClosed, WebviewRpcHost};

/// The daemon's RPC roster, hosted for whichever page is currently connected.
pub struct RpcState {
    host: WebviewRpcHost<MultiRpcService>,
}

impl RpcState {
    pub fn new(host: WebviewRpcHost<MultiRpcService>) -> Self {
        Self { host }
    }
}

/// A [`FrameSink`] over one page's Tauri IPC channel.
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
        // this one is either reloading (it will register a new channel) or gone with the window.
        // Dropping the channel with this sink is the whole release.
    }
}

/// Register this page's response channel, identified by the client epoch it stamps its request
/// frames with. Whatever the previous page opened is abandoned.
#[tauri::command]
pub async fn tddy_rpc_connect(
    state: State<'_, RpcState>,
    channel: Channel<InvokeResponseBody>,
    client_epoch: u32,
) -> Result<(), String> {
    state
        .host
        .connect(Arc::new(ChannelSink { channel }), client_epoch)
        .await;
    Ok(())
}

/// Carry one encoded `RpcRequest` frame to the daemon.
///
/// The frame is the invoke body itself, so it crosses as bytes with no base64 and no JSON
/// envelope. Awaiting the dispatch is deliberate: the host's response queue is bounded, so a page
/// that has stopped reading its channel has its `invoke` promise held rather than being allowed to
/// pile up work the daemon cannot deliver.
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

/// A refused frame, as a message the page's transport can log.
fn describe_frame_error(error: FrameError) -> String {
    match error {
        FrameError::NotConnected => {
            "no response channel is registered: call tddy_rpc_connect first".to_string()
        }
        FrameError::Malformed(reason) => {
            format!("the request frame could not be decoded: {reason}")
        }
    }
}
