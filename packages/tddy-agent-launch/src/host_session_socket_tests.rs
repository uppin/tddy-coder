//! Unit tests: the per-OS-user host-session socket — where it lives, who can reach it, how a
//! stale file is replaced, and that one socket answers many sessions without mixing them up.
//!
//! Every test binds a real unix socket under a temporary data dir and reads its real `stat`.
//!
//! Changeset: docs/dev/1-WIP/2026-09-19-keyring-github-identity.md (tool-session token delivery)

use super::host_session_socket::*;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tddy_core::toolcall::{GithubCredentialHandler, GITHUB_TOKEN_METHOD, HOST_SESSION_SERVICE};
use tddy_host_service::host_session_service::RegisteredSession;
use tddy_rpc::bridge::{RpcResult, RpcService};
use tddy_rpc::{RpcClientTransport, RpcMessage, Status};

/// The account this test process runs as — the one OS user a test can serve without privilege.
fn this_os_user() -> String {
    tddy_session_activity::user_sessions_path::username_for_uid(unsafe { libc::geteuid() })
        .expect("the test process's account has a passwd entry")
}

fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

struct FixedCredential(Result<String, String>);

#[async_trait::async_trait]
impl GithubCredentialHandler for FixedCredential {
    async fn github_token(&self) -> Result<String, String> {
        self.0.clone()
    }
}

fn a_session_of(os_user: &str, token: &str) -> RegisteredSession {
    RegisteredSession {
        os_user: os_user.to_string(),
        conversation_spawn_handler: None,
        github_credential_handler: Some(Arc::new(FixedCredential(Ok(token.to_string())))),
    }
}

struct NoService;

#[async_trait::async_trait]
impl RpcService for NoService {
    async fn handle_rpc(&self, _: &str, _: &str, _: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(Status::unimplemented("client hosts nothing")))
    }
}

/// Ask the socket at `path` for `session_id`'s token, as a coder would.
async fn ask_for_token(path: &Path, session_id: &str) -> Result<String, String> {
    let stream = tokio::net::UnixStream::connect(path)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    let (reader, writer) = tokio::io::split(stream);
    let (client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
        reader,
        writer,
        NoService,
        tddy_rpc::RequestTransport::UnixSocket,
    );
    tokio::spawn(endpoint.run());
    let payload = serde_json::to_vec(&serde_json::json!({ "session_id": session_id })).unwrap();
    let bytes = client
        .call_unary(HOST_SESSION_SERVICE, GITHUB_TOKEN_METHOD, payload)
        .await
        .map_err(|status| status.message().to_string())?;
    let answer: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    Ok(answer["token"].as_str().unwrap().to_string())
}

fn a_data_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("temp data dir")
}

fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

#[tokio::test]
async fn the_socket_is_owner_only_inside_an_owner_only_directory() {
    // Given a daemon serving this OS user
    let data = a_data_dir();
    let sockets = HostSessionSockets::default();
    let user = this_os_user();

    // When its host-session socket is bound
    let path = sockets.ensure_bound(&user, data.path()).await.unwrap();

    // Then the real socket is 0600 in a 0700 directory, both owned by that user
    let socket = std::fs::metadata(&path).unwrap();
    let dir = std::fs::metadata(path.parent().unwrap()).unwrap();
    assert!(socket.file_type().is_socket());
    assert_eq!(
        (
            mode_of(&path),
            mode_of(path.parent().unwrap()),
            socket.uid(),
            dir.uid()
        ),
        (0o600, 0o700, unsafe { libc::geteuid() }, unsafe {
            libc::geteuid()
        })
    );
}

#[tokio::test]
async fn the_path_is_deterministic_per_os_user_inside_the_data_dir() {
    // Given a data dir
    let data = a_data_dir();
    let sockets = HostSessionSockets::default();
    let user = this_os_user();

    // When the socket is bound, and the path is computed independently
    let bound = sockets.ensure_bound(&user, data.path()).await.unwrap();
    let computed = host_session_socket_path(data.path(), &user).unwrap();

    // Then they agree, and the path is inside the data dir under the user's own name
    assert_eq!(bound, computed);
    assert_eq!(bound, data.path().join("run").join(&user).join("host.sock"));
}

#[tokio::test]
async fn two_sessions_of_one_user_share_one_socket_and_each_gets_its_own_token() {
    // Given two sessions of one OS user registered with different accounts' tokens
    let data = a_data_dir();
    let sockets = HostSessionSockets::default();
    let user = this_os_user();
    sockets
        .registry()
        .register("s-ada", a_session_of(&user, "ghp_ada_token"));
    sockets
        .registry()
        .register("s-grace", a_session_of(&user, "ghp_grace_token"));

    // When both are bound to the user's socket
    let first = sockets.ensure_bound(&user, data.path()).await.unwrap();
    let second = sockets.ensure_bound(&user, data.path()).await.unwrap();

    // Then it is the same path, and each session is answered with its own token over it
    assert_eq!(first, second);
    assert_eq!(
        ask_for_token(&first, "s-ada").await,
        Ok("ghp_ada_token".into())
    );
    assert_eq!(
        ask_for_token(&first, "s-grace").await,
        Ok("ghp_grace_token".into())
    );
}

#[tokio::test]
async fn a_session_of_another_os_user_is_refused_over_this_users_socket() {
    // Given this user's socket and a session registered as another user's
    let data = a_data_dir();
    let sockets = HostSessionSockets::default();
    let user = this_os_user();
    sockets
        .registry()
        .register("bobs", a_session_of("somebody-else", "ghp_bob_token"));
    let path = sockets.ensure_bound(&user, data.path()).await.unwrap();

    // When its id is asked about over this user's socket
    let outcome = ask_for_token(&path, "bobs").await;

    // Then it is refused, and the refusal does not carry the token
    let refusal = outcome.expect_err("another user's session must be refused");
    assert!(
        refusal.contains("different OS user") && !refusal.contains("ghp_bob_token"),
        "{refusal}"
    );
}

#[tokio::test]
async fn an_unregistered_session_is_refused() {
    // Given a bound socket and a session that was registered and then deleted
    let data = a_data_dir();
    let sockets = HostSessionSockets::default();
    let user = this_os_user();
    sockets
        .registry()
        .register("gone", a_session_of(&user, "ghp_ada_token"));
    let path = sockets.ensure_bound(&user, data.path()).await.unwrap();
    sockets.registry().unregister("gone");

    // When it asks
    let outcome = ask_for_token(&path, "gone").await;

    // Then it is refused
    assert!(outcome.unwrap_err().contains("no session `gone`"));
}

/// A hard link removed when dropped.
struct KeptLink(std::path::PathBuf);

impl KeptLink {
    fn to(target: &std::path::Path, link: &std::path::Path) -> Self {
        std::fs::hard_link(target, link).expect("a second name for the stale socket file");
        Self(link.to_path_buf())
    }
}

impl Drop for KeptLink {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[tokio::test]
async fn a_stale_socket_file_is_replaced_and_keeps_its_permissions() {
    // Given the file a daemon that died left behind: a real socket nothing listens on
    let data = a_data_dir();
    let user = this_os_user();
    let path = host_session_socket_path(data.path(), &user).unwrap();
    drop(a_dead_daemons_socket_at(&path));
    let stale_inode = std::fs::metadata(&path).unwrap().ino();
    // a second name keeps the stale inode allocated, so a replacement cannot be handed the same
    // number (Linux reuses a freed inode at once)
    let _keeps_the_inode = KeptLink::to(&path, &data.path().join("stale-socket-link"));

    // When a daemon binds the user's socket
    let sockets = HostSessionSockets::default();
    sockets
        .registry()
        .register("s1", a_session_of(&user, "ghp_ada_token"));
    let bound = sockets.ensure_bound(&user, data.path()).await.unwrap();

    // Then it replaced the file, still 0600 in a 0700 directory, and answers
    assert_ne!(std::fs::metadata(&bound).unwrap().ino(), stale_inode);
    assert_eq!(
        (mode_of(&bound), mode_of(bound.parent().unwrap())),
        (0o600, 0o700)
    );
    assert_eq!(
        ask_for_token(&bound, "s1").await,
        Ok("ghp_ada_token".into())
    );
}

#[tokio::test]
async fn a_directory_left_with_wider_permissions_is_narrowed_before_the_socket_exists() {
    // Given a stale per-user directory somebody left world-readable
    let data = a_data_dir();
    let user = this_os_user();
    let path = host_session_socket_path(data.path(), &user).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::set_permissions(
        path.parent().unwrap(),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();

    // When the socket is bound
    let bound = HostSessionSockets::default()
        .ensure_bound(&user, data.path())
        .await
        .unwrap();

    // Then the directory is 0700
    assert_eq!(mode_of(bound.parent().unwrap()), 0o700);
}

#[tokio::test]
async fn a_live_socket_is_not_stolen() {
    // Given one daemon serving the user's socket
    let data = a_data_dir();
    let user = this_os_user();
    let first = HostSessionSockets::default();
    first
        .registry()
        .register("s1", a_session_of(&user, "ghp_first_token"));
    let path = first.ensure_bound(&user, data.path()).await.unwrap();
    let inode = std::fs::metadata(&path).unwrap().ino();

    // When a second daemon process tries to bind the same user's socket
    let second = HostSessionSockets::default();
    let outcome = second.ensure_bound(&user, data.path()).await;

    // Then it is refused, and the first is still the one answering at the same inode
    let refusal = outcome.expect_err("a live socket must not be replaced");
    assert!(refusal.to_string().contains("listening"), "{refusal}");
    assert_eq!(std::fs::metadata(&path).unwrap().ino(), inode);
    assert_eq!(
        ask_for_token(&path, "s1").await,
        Ok("ghp_first_token".into())
    );
}

#[tokio::test]
async fn a_file_that_is_not_a_socket_is_never_removed() {
    // Given a regular file where the socket belongs
    let data = a_data_dir();
    let user = this_os_user();
    let path = host_session_socket_path(data.path(), &user).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "precious").unwrap();

    // When the socket is bound
    let outcome = HostSessionSockets::default()
        .ensure_bound(&user, data.path())
        .await;

    // Then it is refused and the file is untouched
    assert!(outcome.unwrap_err().to_string().contains("not a socket"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "precious");
}

#[tokio::test]
async fn a_socket_whose_file_was_removed_is_bound_again() {
    // Given a bound socket whose file something deleted (a tmp cleaner, an operator)
    let data = a_data_dir();
    let user = this_os_user();
    let sockets = HostSessionSockets::default();
    sockets
        .registry()
        .register("s1", a_session_of(&user, "ghp_ada_token"));
    let path = sockets.ensure_bound(&user, data.path()).await.unwrap();
    std::fs::remove_file(&path).unwrap();

    // When the next session needs it
    let again = sockets.ensure_bound(&user, data.path()).await.unwrap();

    // Then the same path answers again
    assert_eq!(again, path);
    assert_eq!(
        ask_for_token(&again, "s1").await,
        Ok("ghp_ada_token".into())
    );
}

#[tokio::test]
async fn an_account_this_daemon_cannot_give_the_socket_to_is_refused_and_leaves_nothing() {
    // Given a daemon without the privilege to chown, and another account's name
    if is_root() {
        return; // root can chown anywhere; the refusal this pins is the unprivileged one
    }
    let data = a_data_dir();
    let other = "root";

    // When it is asked to serve that account
    let outcome = HostSessionSockets::default()
        .ensure_bound(other, data.path())
        .await;

    // Then it refuses, saying why and what topology it is — rather than leaving a socket the
    // account cannot use or others can
    let refusal = format!(
        "{:#}",
        outcome.expect_err("an unprivileged daemon cannot serve another account")
    );
    assert!(
        refusal.contains("not privileged") && refusal.contains("tddy-supervisor"),
        "{refusal}"
    );
    let path = host_session_socket_path(data.path(), other).unwrap();
    assert!(!path.exists(), "no socket may be left behind");
    assert!(
        !path.parent().unwrap().exists(),
        "no directory may be left behind"
    );
}

#[tokio::test]
async fn a_path_too_long_for_a_unix_socket_is_refused() {
    // Given a data dir deeper than AF_UNIX allows
    let data = a_data_dir();
    let deep: PathBuf = data.path().join("a".repeat(120));
    std::fs::create_dir_all(&deep).unwrap();

    // When the socket is bound under it
    let outcome = HostSessionSockets::default()
        .ensure_bound(&this_os_user(), &deep)
        .await;

    // Then it says why, instead of a bare "invalid argument"
    assert!(outcome.unwrap_err().to_string().contains("too long"));
}

#[tokio::test]
async fn a_user_name_that_could_escape_the_directory_is_refused() {
    // Given a data dir
    let data = a_data_dir();

    // When an OS user name holds a path separator
    let outcome = HostSessionSockets::default()
        .ensure_bound("../evil", data.path())
        .await;

    // Then nothing is bound
    assert!(outcome.is_err());
}

/// The socket file a daemon leaves when it dies: bound, then nothing listening.
fn a_dead_daemons_socket_at(path: &Path) -> std::os::unix::net::UnixListener {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::set_permissions(
        path.parent().unwrap(),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    // The caller drops the listener: that closes the descriptor but, as with a crashed daemon,
    // leaves the file.
    std::os::unix::net::UnixListener::bind(path).unwrap()
}
