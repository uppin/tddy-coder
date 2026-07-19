//! Coder-side `spawn_conversation` relay for a daemon-spawned tddy-coder session.
//!
//! A tddy-coder tool session (e.g. a `cursor` grill-me session) serves its own toolcall socket, so
//! the agent's `spawn_conversation` request lands on *this process's* listener — which cannot create
//! a new worktree/session (daemon-owned work). This handler relays the request back to the daemon as
//! a reverse RPC over the **stdio pipe** the daemon opened when it spawned us (`--stdio`). The daemon
//! hosts `HostSessionService` on the other end; because the pipe is bound 1:1 to this child, the
//! daemon already knows which session is calling — no auth token or caller id is sent.
//!
//! The stdio endpoint (and thus its `StdioRpcClient`) is created later than the toolcall listener
//! that owns this handler, so the client is delivered through a `watch` channel the handler
//! `await`s on first use.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_core::toolcall::{
    ConversationSpawnHandler, HOST_SESSION_SERVICE, SPAWN_CONVERSATION_METHOD,
};
use tddy_rpc::bridge::{RpcResult, RpcService};
use tddy_rpc::{RpcClientTransport, RpcMessage, Status};
use tddy_stdio::StdioRpcClient;
use tokio::sync::watch;

/// The run_daemon stdio setup publishes the reverse client here once its endpoint is up; the handler
/// awaits it. `watch` (not `OnceCell`) because the handler must *block until set* — tokio's
/// `OnceCell` has no async "wait until initialized" — and the value is read on every tool call.
pub type ReverseClientTx = watch::Sender<Option<Arc<StdioRpcClient>>>;
pub type ReverseClientRx = watch::Receiver<Option<Arc<StdioRpcClient>>>;

/// Create the channel bridging the (later-created) stdio endpoint's client to the (earlier-created)
/// relay handler bound on the toolcall listener.
pub fn reverse_client_channel() -> (ReverseClientTx, ReverseClientRx) {
    watch::channel(None)
}

/// Relays `spawn_conversation` to the daemon's `HostSessionService` over the stdio reverse channel.
pub struct DaemonRelayConversationSpawnHandler {
    client_rx: ReverseClientRx,
}

impl DaemonRelayConversationSpawnHandler {
    pub fn new(client_rx: ReverseClientRx) -> Self {
        Self { client_rx }
    }
}

#[async_trait]
impl ConversationSpawnHandler for DaemonRelayConversationSpawnHandler {
    async fn spawn_conversation(
        &self,
        prompt: &str,
        branch: Option<&str>,
        base_ref: Option<&str>,
    ) -> Result<String, String> {
        // Block until the daemon's stdio endpoint has been wired (normally already set by the time
        // the agent can invoke a tool).
        let mut rx = self.client_rx.clone();
        let client = loop {
            if let Some(c) = rx.borrow().clone() {
                break c;
            }
            rx.changed().await.map_err(|_| {
                "daemon stdio endpoint closed before spawn-conversation was ready".to_string()
            })?;
        };
        let payload = serde_json::to_vec(&serde_json::json!({
            "type": "spawn-conversation",
            "prompt": prompt,
            "branch": branch,
            "base_ref": base_ref,
        }))
        .map_err(|e| format!("encode spawn-conversation request: {e}"))?;
        let resp = client
            .call_unary(HOST_SESSION_SERVICE, SPAWN_CONVERSATION_METHOD, payload)
            .await
            .map_err(|s| format!("daemon spawn-conversation relay failed: {s}"))?;
        let v: serde_json::Value = serde_json::from_slice(&resp)
            .map_err(|e| format!("decode spawn-conversation response: {e}"))?;
        v["session_id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "daemon spawn-conversation response missing session_id".to_string())
    }
}

/// A service that hosts nothing — used on the coder's end of the stdio pipe. The coder only *calls*
/// the daemon (reverse spawn); the daemon reaches the coder over gRPC/LiveKit, not stdio, so any
/// inbound stdio request is unexpected.
pub struct NoopRpcService;

#[async_trait]
impl RpcService for NoopRpcService {
    async fn handle_rpc(&self, service: &str, method: &str, _message: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(Status::unimplemented(format!(
            "tddy-coder stdio endpoint hosts no services (got {service}/{method})"
        ))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tddy_stdio::StdioEndpoint;
    use tokio::io::split;

    type Seen = Arc<Mutex<Option<(String, Option<String>, Option<String>)>>>;

    /// Stands in for the daemon's `HostSessionService` on the far end of the pipe, mirroring its
    /// exact wire contract: decode the JSON `spawn-conversation` request, record it, and answer
    /// `{"session_id": ...}`. Keeps this test free of a `tddy-daemon` dependency while still pinning
    /// the coder↔daemon wire format (service/method names + JSON shape) end to end.
    struct FakeHostService {
        session_id: String,
        seen: Seen,
    }

    #[async_trait]
    impl RpcService for FakeHostService {
        async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
            assert_eq!(
                service, HOST_SESSION_SERVICE,
                "relay addressed wrong service"
            );
            assert_eq!(method, SPAWN_CONVERSATION_METHOD, "relay used wrong method");
            let v: serde_json::Value = serde_json::from_slice(&message.payload).unwrap();
            *self.seen.lock().unwrap() = Some((
                v["prompt"].as_str().unwrap_or_default().to_string(),
                v["branch"].as_str().map(str::to_string),
                v["base_ref"].as_str().map(str::to_string),
            ));
            let resp =
                serde_json::to_vec(&serde_json::json!({ "session_id": self.session_id })).unwrap();
            RpcResult::Unary(Ok(resp))
        }
    }

    /// Wire a coder relay handler to a fake host service over a real in-process stdio duplex.
    fn wired_relay(
        session_id: &str,
    ) -> (DaemonRelayConversationSpawnHandler, ReverseClientTx, Seen) {
        let (daemon_side, coder_side) = tokio::io::duplex(8192);
        let (d_read, d_write) = split(daemon_side);
        let (c_read, c_write) = split(coder_side);
        let seen: Seen = Arc::new(Mutex::new(None));
        let (_daemon_client, daemon_endpoint) = StdioEndpoint::from_duplex(
            d_read,
            d_write,
            FakeHostService {
                session_id: session_id.to_string(),
                seen: seen.clone(),
            },
        );
        let (coder_client, coder_endpoint) =
            StdioEndpoint::from_duplex(c_read, c_write, NoopRpcService);
        tokio::spawn(daemon_endpoint.run());
        tokio::spawn(coder_endpoint.run());
        let (tx, rx) = reverse_client_channel();
        // Publish the client (as run_daemon does once its endpoint is up).
        tx.send(Some(coder_client)).unwrap();
        (DaemonRelayConversationSpawnHandler::new(rx), tx, seen)
    }

    #[tokio::test]
    async fn relays_spawn_conversation_over_stdio_and_returns_the_child_session_id() {
        // Given a relay wired to a host service that yields "child-777"
        let (handler, _tx, seen) = wired_relay("child-777");

        // When the agent's spawn_conversation is relayed over the pipe
        let session_id = handler
            .spawn_conversation("build the thing", Some("feat-y"), None)
            .await
            .expect("relay should succeed");

        // Then the child id round-trips back and the host saw the exact prompt/branch (base_ref None)
        assert_eq!(session_id, "child-777");
        let seen = seen
            .lock()
            .unwrap()
            .clone()
            .expect("host service was called");
        assert_eq!(seen.0, "build the thing");
        assert_eq!(seen.1.as_deref(), Some("feat-y"));
        assert_eq!(seen.2, None, "base_ref must be null when omitted");
    }

    #[tokio::test]
    async fn relay_blocks_until_the_client_is_published_then_resolves() {
        // Given a handler whose client is published only AFTER the call has started
        let (daemon_side, coder_side) = tokio::io::duplex(8192);
        let (d_read, d_write) = split(daemon_side);
        let (c_read, c_write) = split(coder_side);
        let seen: Seen = Arc::new(Mutex::new(None));
        let (_dc, de) = StdioEndpoint::from_duplex(
            d_read,
            d_write,
            FakeHostService {
                session_id: "c2".to_string(),
                seen: seen.clone(),
            },
        );
        let (cc, ce) = StdioEndpoint::from_duplex(c_read, c_write, NoopRpcService);
        tokio::spawn(de.run());
        tokio::spawn(ce.run());
        let (tx, rx) = reverse_client_channel();
        let handler = DaemonRelayConversationSpawnHandler::new(rx);

        // When the call starts before the client exists, then the client is published
        let call = tokio::spawn(async move { handler.spawn_conversation("x", None, None).await });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        tx.send(Some(cc)).unwrap();

        // Then it resolves rather than erroring on the missing client
        let out = call
            .await
            .unwrap()
            .expect("resolves once the client is published");
        assert_eq!(out, "c2");
    }
}
