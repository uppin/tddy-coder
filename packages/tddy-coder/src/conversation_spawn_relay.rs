//! Coder-side client of the daemon's host-session socket, and the two relays built on it.
//!
//! A tddy-coder tool session serves its own toolcall socket, so the agent's `spawn_conversation` and
//! `github-token` requests land on *this process's* listener — which can neither create a worktree
//! nor read the owner's vault (daemon-owned work). The handlers here forward them to the daemon over
//! the per-OS-user unix socket it handed us as `--host-session-socket`.
//!
//! That socket serves every tool session of our OS user, so **each request names this session**
//! (`--session-id`, which the process already knows). The daemon refuses an id it does not know or
//! one that belongs to another OS user. The socket's filesystem permissions are the authentication
//! boundary; the id is a label, not a secret.
//!
//! The connection is made on first use and made again when it has gone — the daemon may have been
//! restarted and rebound the socket since — so a tool call never depends on a connection opened at
//! startup. With no socket, or no session id to name, there is **no handler at all**: the toolcall
//! listener then refuses a token request as having no credential handler. There is no fallback.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tddy_core::toolcall::{
    ConversationSpawnHandler, GithubCredentialHandler, GITHUB_TOKEN_METHOD, HOST_SESSION_SERVICE,
    SPAWN_CONVERSATION_METHOD,
};
use tddy_rpc::bridge::{RpcResult, RpcService};
use tddy_rpc::{RpcClientTransport, RpcMessage, Status};
use tddy_stdio::StdioRpcClient;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

/// A connection to the host: the client and the task running its read/dispatch loop, which ends
/// when the daemon's end closes.
struct HostConnection {
    client: Arc<StdioRpcClient>,
    task: JoinHandle<()>,
}

/// This session's lazily-made, remade-when-lost connection to the daemon's host-session socket.
pub struct HostSessionClient {
    socket: PathBuf,
    session_id: String,
    connection: Mutex<Option<HostConnection>>,
}

impl HostSessionClient {
    /// A client for `socket` that names `session_id` in every request. Nothing is connected yet.
    pub fn new(socket: impl Into<PathBuf>, session_id: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            socket: socket.into(),
            session_id: session_id.into(),
            connection: Mutex::new(None),
        })
    }

    async fn client(&self) -> Result<Arc<StdioRpcClient>, String> {
        let mut connection = self.connection.lock().await;
        if let Some(live) = connection.as_ref() {
            if !live.task.is_finished() {
                return Ok(Arc::clone(&live.client));
            }
        }
        let stream = tokio::net::UnixStream::connect(&self.socket)
            .await
            .map_err(|e| {
                format!(
                    "the daemon's host-session socket {} could not be reached: {e}",
                    self.socket.display()
                )
            })?;
        let (reader, writer) = tokio::io::split(stream);
        let (client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
            reader,
            writer,
            NoopRpcService,
            tddy_rpc::RequestTransport::UnixSocket,
        );
        let task = tokio::spawn(endpoint.run());
        *connection = Some(HostConnection {
            client: Arc::clone(&client),
            task,
        });
        Ok(client)
    }

    /// Whether the connection that just failed a call is gone (the daemon closed it), as opposed to
    /// the daemon having answered with a refusal.
    async fn connection_is_gone(&self) -> bool {
        let connection = self.connection.lock().await;
        let Some(live) = connection.as_ref() else {
            return true;
        };
        if live.task.is_finished() {
            return true;
        }
        drop(connection);
        // The loop ends a moment after the failure that reported it.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        self.connection
            .lock()
            .await
            .as_ref()
            .is_none_or(|live| live.task.is_finished())
    }

    /// Call `method` on the host service with this session's id added to `body`. `Err` carries the
    /// host's own words when it refused, or why it could not be reached.
    ///
    /// A call is repeated on a fresh connection, once, only when `repeatable` **and** the first
    /// connection turned out to be gone: a request that may have been acted on must not be sent twice.
    async fn call(
        &self,
        method: &str,
        mut body: serde_json::Value,
        repeatable: bool,
    ) -> Result<serde_json::Value, String> {
        body["session_id"] = serde_json::json!(self.session_id);
        let payload = serde_json::to_vec(&body).map_err(|e| format!("encode {method}: {e}"))?;
        let mut attempts_left = if repeatable { 2 } else { 1 };
        loop {
            attempts_left -= 1;
            let client = self.client().await?;
            match client
                .call_unary(HOST_SESSION_SERVICE, method, payload.clone())
                .await
            {
                Ok(bytes) => {
                    return serde_json::from_slice(&bytes)
                        .map_err(|e| format!("decode the host's {method} answer: {e}"))
                }
                Err(status) => {
                    if attempts_left > 0 && self.connection_is_gone().await {
                        continue;
                    }
                    return Err(status.message().to_string());
                }
            }
        }
    }
}

/// Relays `spawn_conversation` to the daemon's host-session socket.
pub struct DaemonRelayConversationSpawnHandler {
    host: Arc<HostSessionClient>,
}

impl DaemonRelayConversationSpawnHandler {
    pub fn new(host: Arc<HostSessionClient>) -> Self {
        Self { host }
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
        let answer = self
            .host
            .call(
                SPAWN_CONVERSATION_METHOD,
                serde_json::json!({
                    "type": "spawn-conversation",
                    "prompt": prompt,
                    "branch": branch,
                    "base_ref": base_ref,
                }),
                // A spawn may have happened when the connection dropped; never send it twice.
                false,
            )
            .await
            .map_err(|e| format!("daemon spawn-conversation relay failed: {e}"))?;
        answer["session_id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "daemon spawn-conversation response missing session_id".to_string())
    }
}

/// Asks the daemon, over the host-session socket, for the GitHub token of the account this
/// session's project acts as — answering the agent's `github-token` request without this process
/// ever holding a credential of its own. The token is returned to the caller and kept nowhere.
pub struct DaemonRelayGithubCredential {
    host: Arc<HostSessionClient>,
}

impl DaemonRelayGithubCredential {
    pub fn new(host: Arc<HostSessionClient>) -> Self {
        Self { host }
    }
}

#[async_trait]
impl GithubCredentialHandler for DaemonRelayGithubCredential {
    async fn github_token(&self) -> Result<String, String> {
        // Asking is read-only, so a connection lost to a daemon restart is simply made again.
        let answer = self
            .host
            .call(GITHUB_TOKEN_METHOD, serde_json::json!({}), true)
            .await?;
        answer["token"]
            .as_str()
            .filter(|token| !token.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| "the daemon's host answered with no GitHub token".to_string())
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
    use std::path::Path;
    use std::sync::Mutex as StdMutex;

    /// Stands in for the daemon's host service on a real unix socket, mirroring its wire contract:
    /// every request names `session_id`; `GithubToken` answers `{"token"}` or refuses with a status;
    /// `SpawnConversation` answers `{"session_id"}`. Records every request it receives.
    struct FakeHost {
        token: Result<String, String>,
        requests: Arc<StdMutex<Vec<(String, serde_json::Value)>>>,
    }

    #[async_trait]
    impl RpcService for FakeHost {
        async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
            assert_eq!(service, HOST_SESSION_SERVICE, "addressed the wrong service");
            let body: serde_json::Value = serde_json::from_slice(&message.payload).unwrap();
            self.requests
                .lock()
                .unwrap()
                .push((method.to_string(), body));
            RpcResult::Unary(match method {
                GITHUB_TOKEN_METHOD => match &self.token {
                    Ok(token) => {
                        Ok(serde_json::to_vec(&serde_json::json!({ "token": token })).unwrap())
                    }
                    Err(refusal) => Err(Status::failed_precondition(refusal.clone())),
                },
                SPAWN_CONVERSATION_METHOD => Ok(serde_json::to_vec(
                    &serde_json::json!({ "session_id": "child-777" }),
                )
                .unwrap()),
                other => Err(Status::unimplemented(other.to_string())),
            })
        }
    }

    /// A host listening at `path`; dropping the returned handle stops accepting (connections
    /// already made stay open until aborted too, via `stop`).
    struct RunningHost {
        accept: JoinHandle<()>,
        connections: Arc<StdMutex<Vec<JoinHandle<()>>>>,
    }

    impl RunningHost {
        /// What a daemon restart does to the socket: every connection and the listener go away.
        fn stop(self) {
            self.accept.abort();
            for connection in self.connections.lock().unwrap().drain(..) {
                connection.abort();
            }
        }
    }

    type Requests = Arc<StdMutex<Vec<(String, serde_json::Value)>>>;

    fn a_host_at(path: &Path, token: Result<&str, &str>) -> (RunningHost, Requests) {
        let _ = std::fs::remove_file(path);
        let listener = tokio::net::UnixListener::bind(path).unwrap();
        let requests: Requests = Arc::default();
        let connections: Arc<StdMutex<Vec<JoinHandle<()>>>> = Arc::default();
        let token = token.map(str::to_string).map_err(str::to_string);
        let (seen, conns) = (requests.clone(), connections.clone());
        let accept = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let (reader, writer) = tokio::io::split(stream);
                let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                    reader,
                    writer,
                    FakeHost {
                        token: token.clone(),
                        requests: seen.clone(),
                    },
                    tddy_rpc::RequestTransport::UnixSocket,
                );
                conns.lock().unwrap().push(tokio::spawn(endpoint.run()));
            }
        });
        (
            RunningHost {
                accept,
                connections,
            },
            requests,
        )
    }

    fn a_socket_path() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("host.sock");
        (dir, path)
    }

    #[tokio::test]
    async fn the_forwarded_token_request_names_the_session_and_returns_the_hosts_token() {
        // Given a host that answers ada's session with her token
        let (_dir, path) = a_socket_path();
        let (_host, requests) = a_host_at(&path, Ok("ghp_ada_token"));
        let credential =
            DaemonRelayGithubCredential::new(HostSessionClient::new(&path, "session-ada"));

        // When the agent's token request is forwarded
        let token = credential.github_token().await;

        // Then the host's token comes back, and the request named this session
        assert_eq!(token, Ok("ghp_ada_token".to_string()));
        let requests = requests.lock().unwrap();
        assert_eq!(
            *requests,
            vec![(
                GITHUB_TOKEN_METHOD.to_string(),
                serde_json::json!({ "session_id": "session-ada" })
            )]
        );
    }

    #[tokio::test]
    async fn the_hosts_refusal_is_returned_verbatim() {
        // Given a host that refuses with the resolver's words
        let (_dir, path) = a_socket_path();
        let (_host, _requests) =
            a_host_at(&path, Err("this project has no github account assigned"));
        let credential = DaemonRelayGithubCredential::new(HostSessionClient::new(&path, "s"));

        // When the request is forwarded
        let outcome = credential.github_token().await;

        // Then the agent reads exactly those words
        assert_eq!(
            outcome,
            Err("this project has no github account assigned".to_string())
        );
    }

    #[tokio::test]
    async fn an_unreachable_socket_is_an_error_that_names_it() {
        // Given a socket path nothing listens on
        let (_dir, path) = a_socket_path();
        let credential = DaemonRelayGithubCredential::new(HostSessionClient::new(&path, "s"));

        // When the request is forwarded
        let refusal = credential.github_token().await.unwrap_err();

        // Then it fails naming the socket — there is nowhere else to ask
        assert!(
            refusal.contains("could not be reached") && refusal.contains("host.sock"),
            "{refusal}"
        );
    }

    #[tokio::test]
    async fn the_connection_is_made_again_after_the_host_restarts_and_rebinds() {
        // Given a handler that has already fetched a token over a first connection
        let (_dir, path) = a_socket_path();
        let (host, _) = a_host_at(&path, Ok("ghp_before_restart"));
        let credential = DaemonRelayGithubCredential::new(HostSessionClient::new(&path, "s"));
        assert_eq!(
            credential.github_token().await,
            Ok("ghp_before_restart".to_string())
        );

        // When the daemon restarts: the old listener and connections go, a new host binds the path
        host.stop();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (_restarted, _) = a_host_at(&path, Ok("ghp_after_restart"));

        // Then the next request reconnects and is answered by the new host
        assert_eq!(
            credential.github_token().await,
            Ok("ghp_after_restart".to_string())
        );
    }

    #[tokio::test]
    async fn one_connection_serves_many_requests() {
        // Given a handler and a host that counts connections' requests
        let (_dir, path) = a_socket_path();
        let (_host, requests) = a_host_at(&path, Ok("ghp_ada_token"));
        let credential = DaemonRelayGithubCredential::new(HostSessionClient::new(&path, "s"));

        // When three requests are forwarded
        for _ in 0..3 {
            credential.github_token().await.unwrap();
        }

        // Then all three were answered
        assert_eq!(requests.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn spawn_conversation_is_relayed_naming_the_session_and_returns_the_child_id() {
        // Given a host that yields "child-777"
        let (_dir, path) = a_socket_path();
        let (_host, requests) = a_host_at(&path, Ok("unused"));
        let handler =
            DaemonRelayConversationSpawnHandler::new(HostSessionClient::new(&path, "orch-1"));

        // When the agent's spawn_conversation is relayed
        let child = handler
            .spawn_conversation("build the thing", Some("feat-y"), None)
            .await;

        // Then the child id round-trips and the host saw the session, prompt and branch
        assert_eq!(child, Ok("child-777".to_string()));
        let (method, body) = requests.lock().unwrap()[0].clone();
        assert_eq!(method, SPAWN_CONVERSATION_METHOD);
        assert_eq!(
            (
                body["session_id"].clone(),
                body["prompt"].clone(),
                body["branch"].clone()
            ),
            (
                serde_json::json!("orch-1"),
                serde_json::json!("build the thing"),
                serde_json::json!("feat-y")
            )
        );
    }

    #[tokio::test]
    async fn a_spawn_is_not_sent_twice_when_the_connection_was_lost() {
        // Given a handler whose host went away after its first answer
        let (_dir, path) = a_socket_path();
        let (host, requests) = a_host_at(&path, Ok("unused"));
        let handler = DaemonRelayConversationSpawnHandler::new(HostSessionClient::new(&path, "o"));
        handler.spawn_conversation("one", None, None).await.unwrap();
        host.stop();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // When the next spawn is attempted with no host to receive it
        let outcome = handler.spawn_conversation("two", None, None).await;

        // Then it fails rather than being repeated, and only the first request was ever received
        assert!(outcome.is_err());
        assert_eq!(requests.lock().unwrap().len(), 1);
    }
}
