//! Acceptance: asking a session's host for the GitHub token of the account its project acts as.
//!
//! The token is never in the asker's environment and never in a file: it is requested over the
//! session's own toolcall socket, per call, and a refusal arrives in the host's own words. With no
//! socket there is no host to ask, and that is an error — not a cue to look anywhere else.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tddy_toolcall::toolcall::{
    request_github_token, request_github_token_from_session, GithubCredentialHandler,
    ToolcallRpcService,
};

/// A session host that answers every `github-token` with a fixed outcome.
struct FixedCredential(Result<String, String>);

#[async_trait]
impl GithubCredentialHandler for FixedCredential {
    async fn github_token(&self) -> Result<String, String> {
        self.0.clone()
    }
}

/// Serve `outcome` on a fresh socket for the length of the test.
async fn a_session_host_answering(outcome: Result<String, String>) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let socket = dir.path().join("host.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    let handler: Arc<dyn GithubCredentialHandler> = Arc::new(FixedCredential(outcome));
    tokio::spawn(async move {
        let (tx, _rx) = std::sync::mpsc::sync_channel(1);
        while let Ok((stream, _)) = listener.accept().await {
            let service = ToolcallRpcService::new(
                tx.clone(),
                Arc::new(None),
                Arc::new(None),
                Arc::new(std::env::temp_dir()),
            )
            .with_github_credential_handler(Some(Arc::clone(&handler)));
            let (reader, writer) = stream.into_split();
            let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                reader,
                writer,
                service,
                tddy_rpc::RequestTransport::UnixSocket,
            );
            tokio::spawn(endpoint.run());
        }
    });
    (dir, socket)
}

#[tokio::test]
async fn the_asker_receives_the_token_the_session_host_resolved() {
    // Given a session host whose project resolves to a token
    let (_dir, socket) = a_session_host_answering(Ok("ghp_project_account".to_string())).await;

    // When the token is requested
    let token = request_github_token(Some(&socket)).await;

    // Then it is that token
    assert_eq!(token, Ok("ghp_project_account".to_string()));
}

#[tokio::test]
async fn a_refusal_reaches_the_asker_in_the_hosts_own_words() {
    // Given a session host whose project assigns no account
    let reason = "this project has no github account assigned; assign one to the project";
    let (_dir, socket) = a_session_host_answering(Err(reason.to_string())).await;

    // When the token is requested
    let token = request_github_token(Some(&socket)).await;

    // Then the asker is told why, not that authentication failed
    assert_eq!(token, Err(reason.to_string()));
}

#[tokio::test]
async fn without_a_session_socket_there_is_no_token_even_when_the_environment_holds_one() {
    // Given GITHUB_TOKEN exported where the asker runs
    std::env::set_var("GITHUB_TOKEN", "ghp_from_the_environment");
    std::env::set_var("GH_TOKEN", "ghp_from_the_environment");

    // When there is no session socket to ask
    let token = request_github_token(None).await;

    // Then the environment rescued nothing, and the error names TDDY_SOCKET
    let error = token.expect_err("no host, no token");
    assert!(error.contains("TDDY_SOCKET"), "got: {error}");
}

#[tokio::test]
async fn a_socket_nobody_listens_on_is_an_error_not_an_empty_token() {
    // Given a socket path with no host behind it
    let dir = tempfile::tempdir().expect("a temporary directory");
    let absent: &Path = &dir.path().join("absent.sock");

    // When the token is requested
    let token = request_github_token(Some(absent)).await;

    // Then it fails to connect
    assert!(token.is_err());
}

#[tokio::test]
async fn a_process_inside_a_session_asks_the_host_its_socket_names() {
    // Given a session host whose project resolves to a token, named by TDDY_SOCKET — the only test
    // in this file that touches the variable
    let (_dir, socket) = a_session_host_answering(Ok("ghp_project_account".to_string())).await;
    std::env::set_var("TDDY_SOCKET", &socket);

    // When a process of the session asks for the token without being handed a socket
    let token = request_github_token_from_session().await;

    // Then it is the host's token
    assert_eq!(token, Ok("ghp_project_account".to_string()));
}
