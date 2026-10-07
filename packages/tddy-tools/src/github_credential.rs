//! The GitHub credential the agent's PR tools authenticate with — asked of the session's host at
//! call time.
//!
//! The token is **not** in this process's environment and not in a file. `tddy-tools` asks over the
//! session's own toolcall socket (`TDDY_SOCKET`); the host resolves the session's project account
//! (`tddy_accounts::acting_identity`) and answers per call. A refusal — no account assigned, an
//! account this host does not hold, a locked vault — arrives as the host's own words and is what the
//! PR tool returns to the agent. Nothing here reads `GITHUB_TOKEN` or `GH_TOKEN`, and with no socket
//! there is no token: the absence of a host to ask is an error, not a cue to look elsewhere.

use std::path::Path;

use tddy_workflow_recipes::orchestrate_pr_stack::RealGithubPrApi;

/// The token of the account the session's project acts as, from the host listening on `socket`.
///
/// `Err` carries the reason there is none, verbatim from the host when it refused.
pub(crate) fn github_token_from(socket: Option<&Path>) -> Result<String, String> {
    let socket = socket.ok_or_else(|| {
        "TDDY_SOCKET is not set, so there is no session host to ask for this project's GitHub \
         account; the PR tools only work in a managed session"
            .to_string()
    })?;
    let response = ask_host(socket)?;
    match response["status"].as_str() {
        Some("ok") => response["token"]
            .as_str()
            .filter(|token| !token.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| "the session host answered with no GitHub token".to_string()),
        Some("error") => Err(response["message"]
            .as_str()
            .filter(|message| !message.is_empty())
            .unwrap_or("the session host refused to resolve a GitHub account")
            .to_string()),
        _ => Err("the session host sent an unrecognised answer to github-token".to_string()),
    }
}

/// A GitHub client for `repo` that authenticates as the session's project account.
pub(crate) fn github_api_for(
    repo: String,
    socket: Option<&Path>,
) -> Result<RealGithubPrApi, String> {
    Ok(RealGithubPrApi::with_token(
        repo,
        github_token_from(socket)?,
    ))
}

/// One `github-token` round trip. The PR tools are synchronous and run on the MCP server's runtime,
/// so the call runs on a thread of its own with a runtime of its own rather than blocking inside
/// the caller's.
fn ask_host(socket: &Path) -> Result<serde_json::Value, String> {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| format!("could not start a runtime to ask the session host: {e}"))?
                    .block_on(tddy_core::toolcall::dispatch_toolcall(
                        socket,
                        serde_json::json!({ "type": "github-token" }),
                    ))
            })
            .join()
            .map_err(|_| "asking the session host for a GitHub token panicked".to_string())?
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    use async_trait::async_trait;
    use serial_test::serial;
    use tddy_core::toolcall::{GithubCredentialHandler, ToolcallRpcService};

    /// A session host that answers every `github-token` with a fixed outcome.
    struct FixedCredential(Result<String, String>);

    #[async_trait]
    impl GithubCredentialHandler for FixedCredential {
        async fn github_token(&self) -> Result<String, String> {
            self.0.clone()
        }
    }

    /// Serve `outcome` on a fresh socket from a thread of its own, so a blocking caller on the
    /// test's thread cannot starve it.
    fn a_session_host_answering(outcome: Result<String, String>) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let socket = dir.path().join("host.sock");
        let bound = socket.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("a runtime");
            runtime.block_on(async move {
                let listener = tokio::net::UnixListener::bind(&bound).expect("bind");
                ready_tx.send(()).ok();
                let handler: Arc<dyn GithubCredentialHandler> = Arc::new(FixedCredential(outcome));
                let (tx, _rx) = std::sync::mpsc::sync_channel(1);
                loop {
                    let Ok((stream, _)) = listener.accept().await else {
                        break;
                    };
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
        });
        ready_rx.recv().expect("the host is listening");
        (dir, socket)
    }

    #[test]
    fn the_agent_receives_the_token_the_session_host_resolved() {
        // Given a session host whose project resolves to a token
        let (_dir, socket) = a_session_host_answering(Ok("ghp_project_account".to_string()));

        // When the PR tools ask for the credential
        let token = github_token_from(Some(&socket));

        // Then it is that token
        assert_eq!(token, Ok("ghp_project_account".to_string()));
    }

    #[test]
    fn a_refusal_reaches_the_agent_in_the_hosts_own_words() {
        // Given a session host whose project assigns no account
        let reason = "this project has no github account assigned; assign one to the project";
        let (_dir, socket) = a_session_host_answering(Err(reason.to_string()));

        // When the PR tools ask for the credential
        let token = github_token_from(Some(&socket));

        // Then the agent is told why, not that authentication failed
        assert_eq!(token, Err(reason.to_string()));
    }

    #[test]
    fn a_refusal_stops_the_github_client_from_being_built() {
        // Given a session host that refuses
        let (_dir, socket) = a_session_host_answering(Err("no account assigned".to_string()));

        // When a client for the repository is requested
        let api = github_api_for("acme/repo".to_string(), Some(&socket));

        // Then there is none, and the reason is the host's
        assert_eq!(api.err(), Some("no account assigned".to_string()));
    }

    #[test]
    fn a_host_that_resolves_a_token_yields_a_client() {
        // Given a session host whose project resolves to a token
        let (_dir, socket) = a_session_host_answering(Ok("ghp_project_account".to_string()));

        // When a client for the repository is requested
        let api = github_api_for("acme/repo".to_string(), Some(&socket));

        // Then a client exists
        assert!(api.is_ok());
    }

    #[test]
    #[serial]
    fn without_a_session_socket_there_is_no_token_even_when_the_environment_holds_one() {
        // Given GITHUB_TOKEN exported where the tools run
        std::env::set_var("GITHUB_TOKEN", "ghp_from_the_environment");
        std::env::set_var("GH_TOKEN", "ghp_from_the_environment");

        // When there is no session socket to ask
        let token = github_token_from(None);

        // Then the environment rescued nothing, and the error names TDDY_SOCKET
        let error = token.expect_err("no host, no token");
        assert!(error.contains("TDDY_SOCKET"), "got: {error}");
    }

    #[test]
    fn a_socket_nobody_listens_on_is_an_error_not_an_empty_token() {
        // Given a socket path with no host behind it
        let dir = tempfile::tempdir().expect("a temporary directory");

        // When the PR tools ask for the credential
        let token = github_token_from(Some(&dir.path().join("absent.sock")));

        // Then it fails to connect
        assert!(token.is_err());
    }
}
