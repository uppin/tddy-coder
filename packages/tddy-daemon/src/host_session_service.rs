//! Daemon-hosted RPC service reached over a spawned tddy-coder session's **stdio pipe**.
//!
//! A tddy-coder tool session (e.g. a `cursor` grill-me session) serves its own toolcall socket, so
//! its agent's `spawn_conversation` request lands on the *coder's* listener — which has no way to
//! create a new worktree/session (daemon-owned work). The coder therefore relays the request back
//! to the daemon as a **reverse RPC** over the same stdio pipe the daemon opened when it spawned the
//! coder. Because a stdio pipe pair is bound 1:1 to exactly the child the daemon spawned, the
//! orchestrator context (`GrillMeConversationSpawnHandler`, built for that specific session) is
//! captured when this service is constructed — no auth token or caller-identity check is needed.
//!
//! This is the daemon-side peer of the coder-side `ConversationSpawnHandler`; the two talk over
//! [`tddy_stdio`]. See [`crate::connection_service`] for where the endpoint is wired to a child's
//! piped fds.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_core::toolcall::{ConversationSpawnHandler, SpawnConversationRequestWire};
use tddy_rpc::bridge::{RpcResult, RpcService};
use tddy_rpc::{RpcMessage, Status};

// Service/method names are shared with the coder-side relay client via tddy-core so both agree on
// the address without either crate depending on the other.
pub use tddy_core::toolcall::{HOST_SESSION_SERVICE, SPAWN_CONVERSATION_METHOD};

/// Hosts [`SPAWN_CONVERSATION_METHOD`] over a session's stdio pipe, delegating to the
/// [`ConversationSpawnHandler`] built for that session (its orchestrator context is baked in).
///
/// Wire format is the same JSON [`SpawnConversationRequestWire`] the toolcall relay already uses, so
/// no new schema/codegen is introduced; the response is `{"session_id": "<id>"}`.
pub struct HostSessionService {
    conversation_spawn_handler: Arc<dyn ConversationSpawnHandler>,
}

impl HostSessionService {
    pub fn new(conversation_spawn_handler: Arc<dyn ConversationSpawnHandler>) -> Self {
        Self {
            conversation_spawn_handler,
        }
    }

    async fn handle_spawn_conversation(&self, payload: &[u8]) -> Result<Vec<u8>, Status> {
        let wire: SpawnConversationRequestWire = serde_json::from_slice(payload).map_err(|e| {
            Status::invalid_argument(format!("invalid spawn-conversation request: {e}"))
        })?;
        let session_id = self
            .conversation_spawn_handler
            .spawn_conversation(
                &wire.prompt,
                wire.branch.as_deref(),
                wire.base_ref.as_deref(),
            )
            .await
            .map_err(Status::internal)?;
        serde_json::to_vec(&serde_json::json!({ "session_id": session_id }))
            .map_err(|e| Status::internal(format!("serialize spawn-conversation response: {e}")))
    }
}

#[async_trait]
impl RpcService for HostSessionService {
    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        if service == HOST_SESSION_SERVICE && method == SPAWN_CONVERSATION_METHOD {
            RpcResult::Unary(self.handle_spawn_conversation(&message.payload).await)
        } else {
            RpcResult::Unary(Err(Status::unimplemented(format!(
                "HostSessionService has no method {service}/{method}"
            ))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The `(prompt, branch, base_ref)` the fake handler last saw, shared with the test body.
    type SeenSpawnArgs = Arc<Mutex<Option<(String, Option<String>, Option<String>)>>>;

    /// Records the arguments it was called with and returns a fixed child session id.
    struct FakeConversationSpawn {
        session_id: String,
        seen: SeenSpawnArgs,
    }

    #[async_trait]
    impl ConversationSpawnHandler for FakeConversationSpawn {
        async fn spawn_conversation(
            &self,
            prompt: &str,
            branch: Option<&str>,
            base_ref: Option<&str>,
        ) -> Result<String, String> {
            *self.seen.lock().unwrap() = Some((
                prompt.to_string(),
                branch.map(str::to_string),
                base_ref.map(str::to_string),
            ));
            Ok(self.session_id.clone())
        }
    }

    fn request_bytes(json: serde_json::Value) -> RpcMessage {
        RpcMessage::new(serde_json::to_vec(&json).unwrap(), Default::default())
    }

    #[tokio::test]
    async fn spawn_conversation_routes_to_the_handler_and_returns_the_child_session_id() {
        // Given a host service backed by a handler that yields "child-123"
        let seen = Arc::new(Mutex::new(None));
        let service = HostSessionService::new(Arc::new(FakeConversationSpawn {
            session_id: "child-123".to_string(),
            seen: seen.clone(),
        }));

        // When a coder relays a spawn-conversation request over the pipe
        let msg = request_bytes(serde_json::json!({
            "type": "spawn-conversation",
            "prompt": "implement the brief",
            "branch": "feat-x",
        }));
        let result = service
            .handle_rpc(HOST_SESSION_SERVICE, SPAWN_CONVERSATION_METHOD, &msg)
            .await;

        // Then the handler received the prompt/branch (base_ref absent) and the response carries the id
        let bytes = match result {
            RpcResult::Unary(Ok(bytes)) => bytes,
            RpcResult::Unary(Err(status)) => panic!("expected ok, got error: {status:?}"),
            _ => panic!("expected a unary response"),
        };
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["session_id"], "child-123");

        let seen = seen.lock().unwrap().clone().expect("handler was called");
        assert_eq!(seen.0, "implement the brief");
        assert_eq!(seen.1.as_deref(), Some("feat-x"));
        assert_eq!(seen.2, None, "base_ref must be None when omitted");
    }

    #[tokio::test]
    async fn a_spawn_conversation_handler_error_surfaces_as_an_internal_status() {
        struct FailingSpawn;
        #[async_trait]
        impl ConversationSpawnHandler for FailingSpawn {
            async fn spawn_conversation(
                &self,
                _prompt: &str,
                _branch: Option<&str>,
                _base_ref: Option<&str>,
            ) -> Result<String, String> {
                Err("worktree already exists".to_string())
            }
        }
        let service = HostSessionService::new(Arc::new(FailingSpawn));
        let msg = request_bytes(serde_json::json!({
            "type": "spawn-conversation",
            "prompt": "x",
        }));
        let result = service
            .handle_rpc(HOST_SESSION_SERVICE, SPAWN_CONVERSATION_METHOD, &msg)
            .await;
        match result {
            RpcResult::Unary(Err(status)) => {
                assert!(
                    format!("{status:?}").contains("worktree already exists"),
                    "error message must be surfaced verbatim; got {status:?}"
                );
            }
            _ => panic!("expected an error status"),
        }
    }

    #[tokio::test]
    async fn an_unknown_method_is_unimplemented() {
        let service = HostSessionService::new(Arc::new(FakeConversationSpawn {
            session_id: "unused".to_string(),
            seen: Arc::new(Mutex::new(None)),
        }));
        let msg = request_bytes(serde_json::json!({}));
        let result = service.handle_rpc(HOST_SESSION_SERVICE, "Nope", &msg).await;
        matches!(result, RpcResult::Unary(Err(_)))
            .then_some(())
            .expect("unknown method must be rejected");
    }
}
