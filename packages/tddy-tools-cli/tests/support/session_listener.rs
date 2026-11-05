//! A session's toolcall listener standing in for the daemon's, for the tests that drive
//! `tddy-tools submit`, `ask` and `transition` and need a session to report to
//! (`#agent-tools 15/19`: a `tddy-tools` with no session is an error, never a silent local run).
//!
//! Every request is acknowledged with `{"status":"ok","goal":<the request's goal>,"answers":"A"}`;
//! point a command at it with `--socket`.

#![cfg(unix)]
#![allow(dead_code)]

use std::path::PathBuf;

use serde_json::Value;

/// Acknowledges every toolcall.
#[derive(Clone, Default)]
struct AnAcknowledgingSession;

#[async_trait::async_trait]
impl tddy_rpc::RpcService for AnAcknowledgingSession {
    async fn handle_rpc(
        &self,
        _service: &str,
        _method: &str,
        message: &tddy_rpc::RpcMessage,
    ) -> tddy_rpc::RpcResult {
        let body: Value = serde_json::from_slice(&message.payload).unwrap_or(Value::Null);
        let answer = serde_json::json!({
            "status": "ok",
            "goal": body["goal"],
            "answers": "A",
        });
        tddy_rpc::RpcResult::Unary(Ok(answer.to_string().into_bytes()))
    }
}

/// A listening session. Its socket is removed with it.
pub struct ASessionListener {
    pub socket: PathBuf,
    _dir: tempfile::TempDir,
}

impl ASessionListener {
    /// Starts listening on a socket of its own, on a thread of its own.
    pub fn start() -> Self {
        let dir = tempfile::tempdir().expect("session socket directory");
        let socket = dir.path().join("toolcall.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind the session");
        listener.set_nonblocking(true).expect("non-blocking");
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");
            runtime.block_on(async move {
                let listener =
                    tokio::net::UnixListener::from_std(listener).expect("tokio listener");
                loop {
                    let Ok((stream, _)) = listener.accept().await else {
                        return;
                    };
                    tokio::spawn(async move {
                        let (reader, writer) = stream.into_split();
                        let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                            reader,
                            writer,
                            AnAcknowledgingSession,
                            tddy_rpc::RequestTransport::UnixSocket,
                        );
                        endpoint.run().await;
                    });
                }
            });
        });
        Self { socket, _dir: dir }
    }
}
