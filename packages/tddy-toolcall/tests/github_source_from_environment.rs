//! What a process inside a session does when its environment says the project uses **Local gh**:
//! it reads the token from `gh`, never from its session host, and the `gh` it runs is given none
//! of the GitHub token variables the process happened to inherit.
//!
//! Every test here sets process-wide environment variables, so they hold [`ENVIRONMENT`] for their
//! whole length and live in a binary of their own.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-14-local-gh-github-source.md

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tddy_toolcall::toolcall::{
    request_github_token_from_session, GithubCredentialHandler, ToolcallRpcService,
    GITHUB_SOURCE_ENV,
};

static ENVIRONMENT: Mutex<()> = Mutex::new(());

const GITHUB_TOKEN_VARIABLES: [&str; 4] = [
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
];

/// A session host that would hand out a token, and counts how often it is asked.
struct CountingHost {
    asked: Arc<AtomicUsize>,
}

#[async_trait]
impl GithubCredentialHandler for CountingHost {
    async fn github_token(&self) -> Result<String, String> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Ok("ghp_the_session_hosts_vault_token".to_string())
    }
}

/// Serve a [`CountingHost`] on a fresh socket for the length of the test.
fn a_session_host_that_counts_its_askers(dir: &Path) -> (PathBuf, Arc<AtomicUsize>) {
    let socket = dir.join("host.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    let asked = Arc::new(AtomicUsize::new(0));
    let handler: Arc<dyn GithubCredentialHandler> = Arc::new(CountingHost {
        asked: Arc::clone(&asked),
    });
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
    (socket, asked)
}

/// A `gh` on `dir` logged in to github.com, answering with `token` and recording its environment
/// to `dir/gh.env`.
fn a_gh_on_path_logged_in_to_github_com(dir: &Path, token: &str) -> PathBuf {
    let record = dir.join("gh.env");
    let path = dir.join("gh");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nenv > \"{record}\"\n\
             if [ \"$*\" = \"auth token --hostname github.com\" ]; then printf '%s\\n' '{token}'; exit 0; fi\n\
             printf 'no oauth token found for %s\\n' \"$4\" >&2\nexit 1\n",
            record = record.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var(
        "PATH",
        format!("{}:{}", dir.display(), std::env::var("PATH").unwrap_or_default()),
    );
    record
}

#[tokio::test]
async fn a_session_told_local_gh_never_asks_its_session_host() {
    let _environment = ENVIRONMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // Given a session host that would answer, and gh logged in on this host
    let dir = tempfile::tempdir().unwrap();
    let (socket, asked) = a_session_host_that_counts_its_askers(dir.path());
    a_gh_on_path_logged_in_to_github_com(dir.path(), "gho_ada_from_the_hosts_gh");
    std::env::set_var("TDDY_SOCKET", &socket);
    // And a session told its project uses Local gh on github.com
    std::env::set_var(GITHUB_SOURCE_ENV, "local-gh:github.com");

    // When the session asks for its token
    let token = request_github_token_from_session().await;

    // Then it is gh's, and the session host was never asked
    assert_eq!(token, Ok("gho_ada_from_the_hosts_gh".to_string()));
    assert_eq!(asked.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_malformed_github_source_is_an_error_and_not_a_reason_to_ask_the_host() {
    let _environment = ENVIRONMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // Given a session host that would answer
    let dir = tempfile::tempdir().unwrap();
    let (socket, asked) = a_session_host_that_counts_its_askers(dir.path());
    std::env::set_var("TDDY_SOCKET", &socket);
    // And a GitHub source value nothing in tddy writes
    std::env::set_var(GITHUB_SOURCE_ENV, "github.com");

    // When the session asks for its token
    let token = request_github_token_from_session().await;

    // Then it is refused, naming the value, and the host was not asked instead
    let refusal = token.expect_err("a malformed source is not a source");
    assert!(
        refusal.contains(GITHUB_SOURCE_ENV) && refusal.contains("github.com"),
        "the refusal must name the variable and its value, got: {refusal}"
    );
    assert_eq!(asked.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn the_gh_a_session_runs_for_its_token_inherits_none_of_its_github_token_variables() {
    let _environment = ENVIRONMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // Given a session process that inherited every GitHub token variable gh honours
    let dir = tempfile::tempdir().unwrap();
    for variable in GITHUB_TOKEN_VARIABLES {
        std::env::set_var(variable, "from_whoever_exported_it");
    }
    let record = a_gh_on_path_logged_in_to_github_com(dir.path(), "gho_ada_from_the_hosts_gh");
    std::env::set_var(GITHUB_SOURCE_ENV, "local-gh:github.com");

    // When it asks gh for its token
    request_github_token_from_session()
        .await
        .expect("gh answers");

    // Then gh was started with none of them
    let gh_environment = std::fs::read_to_string(&record).expect("gh recorded its environment");
    let inherited: Vec<&str> = gh_environment
        .lines()
        .filter(|line| line.contains("from_whoever_exported_it"))
        .collect();
    assert_eq!(inherited, Vec::<&str>::new());
}
