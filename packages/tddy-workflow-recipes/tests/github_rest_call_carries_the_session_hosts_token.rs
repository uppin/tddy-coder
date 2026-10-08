//! A GitHub REST call authenticates as the account the session's host answered with — end to end,
//! from the `github-token` request over the session's toolcall socket (`TDDY_SOCKET`) to the
//! `Authorization` header of the HTTP request that reaches the API.
//!
//! The API is a listener on loopback standing in for `api.github.com`: the client is given its base
//! URL, so no test reaches GitHub. A host that refuses means no request is made at all, and the
//! daemon's own `GITHUB_TOKEN` is never what authenticates a call.
//!
//! Changeset: docs/dev/1-WIP/2026-09-19-keyring-github-identity.md

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serial_test::serial;
use tddy_core::toolcall::{GithubCredentialHandler, ToolcallRpcService};
use tddy_github::pr_api::{GithubPrApi, RealGithubPrApi};

const PULLS_ANSWER: &str = r#"[{"number":7,"html_url":"https://github.example/acme/repo/pull/7","head":{"sha":"abc123"},"base":{"ref":"master"}}]"#;

/// Answers every `github-token` with a fixed outcome.
struct HostAnswering(Result<String, String>);

#[async_trait]
impl GithubCredentialHandler for HostAnswering {
    async fn github_token(&self) -> Result<String, String> {
        self.0.clone()
    }
}

/// A session host on its own socket and runtime, as in production a different process, with
/// `TDDY_SOCKET` pointing at it.
fn a_session_host_answering(outcome: Result<String, String>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let socket: PathBuf = dir.path().join("host.sock");
    let handler: Arc<dyn GithubCredentialHandler> = Arc::new(HostAnswering(outcome));
    let bound = std::os::unix::net::UnixListener::bind(&socket).expect("bind the host socket");
    bound.set_nonblocking(true).expect("non-blocking socket");
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the host's runtime");
        runtime.block_on(async move {
            let listener = tokio::net::UnixListener::from_std(bound).expect("adopt the socket");
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
    });
    std::env::set_var("TDDY_SOCKET", &socket);
    dir
}

/// A stand-in for the GitHub API on loopback: records the raw text of each request it receives and
/// answers every one with [`PULLS_ANSWER`].
struct FakeGithubApi {
    base_url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl FakeGithubApi {
    fn started() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind the fake API");
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let requests: Arc<Mutex<Vec<String>>> = Arc::default();
        let seen = Arc::clone(&requests);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut received = Vec::new();
                let mut chunk = [0u8; 4096];
                while !received.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => received.extend_from_slice(&chunk[..n]),
                    }
                }
                seen.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&received).into_owned());
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{PULLS_ANSWER}",
                    PULLS_ANSWER.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self { base_url, requests }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

/// The value of the request's `Authorization` header, if it carries one.
fn authorization_of(request: &str) -> Option<String> {
    request.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("authorization")
            .then(|| value.trim().to_string())
    })
}

#[test]
#[serial]
fn a_rest_call_carries_the_token_the_session_host_returned() {
    // Given a session host answering one account's token, and the API on loopback
    let _host = a_session_host_answering(Ok("ghp_the_hosts_token".to_string()));
    let api = FakeGithubApi::started();
    let client = RealGithubPrApi::asking_the_session_host("acme/repo").with_api_base(&api.base_url);

    // When the client looks a pull request up
    let found = client.get_open_pr("feature/auth");

    // Then the one request that reached the API was authenticated as that token
    assert_eq!(
        found.expect("the lookup succeeds").map(|pr| pr.number),
        Some(7)
    );
    let requests = api.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert!(
        requests[0].starts_with("GET /repos/acme/repo/pulls?"),
        "{}",
        requests[0]
    );
    assert_eq!(
        authorization_of(&requests[0]),
        Some("Bearer ghp_the_hosts_token".to_string())
    );
}

#[test]
#[serial]
fn a_refusing_host_means_no_request_reaches_the_api() {
    // Given a session host that refuses, and the API on loopback
    let _host = a_session_host_answering(Err(
        "this project has no github account assigned".to_string()
    ));
    let api = FakeGithubApi::started();
    let client = RealGithubPrApi::asking_the_session_host("acme/repo").with_api_base(&api.base_url);

    // When the client looks a pull request up
    let refusal = client
        .get_open_pr("feature/auth")
        .expect_err("a refused token is a failure")
        .to_string();

    // Then the host's words are the failure, and the API was never called
    assert!(
        refusal.contains("this project has no github account assigned"),
        "{refusal}"
    );
    assert!(api.requests().is_empty(), "{:?}", api.requests());
}

#[test]
#[serial]
fn a_github_token_in_the_environment_authenticates_no_call() {
    // Given GITHUB_TOKEN exported, a host answering another token, and the API on loopback
    std::env::set_var("GITHUB_TOKEN", "ghp_from_the_environment");
    std::env::set_var("GH_TOKEN", "ghp_from_the_environment");
    let _host = a_session_host_answering(Ok("ghp_the_hosts_token".to_string()));
    let api = FakeGithubApi::started();
    let client = RealGithubPrApi::asking_the_session_host("acme/repo").with_api_base(&api.base_url);

    // When the client looks a pull request up
    client
        .get_open_pr("feature/auth")
        .expect("the lookup succeeds");

    // Then the request carries the host's token, and the environment's appears nowhere in it
    let requests = api.requests();
    assert_eq!(
        authorization_of(&requests[0]),
        Some("Bearer ghp_the_hosts_token".to_string())
    );
    assert!(!requests[0].contains("ghp_from_the_environment"));
}

#[test]
#[serial]
fn the_token_travels_in_the_header_alone_not_in_the_url() {
    // Given a session host answering a token, and the API on loopback
    let _host = a_session_host_answering(Ok("ghp_the_hosts_token".to_string()));
    let api = FakeGithubApi::started();
    let client = RealGithubPrApi::asking_the_session_host("acme/repo").with_api_base(&api.base_url);

    // When a call is made
    client
        .get_open_pr("feature/auth")
        .expect("the lookup succeeds");

    // Then the request line (what proxies and access logs record) does not carry it
    let request_line = api.requests()[0].lines().next().unwrap().to_string();
    assert!(!request_line.contains("ghp_"), "{request_line}");
}
