//! Unit tests: host-session sockets handed to the daemon by `tddy-supervisor` — which descriptors
//! are adopted, which are refused, and that an adopted one is served as the user's socket.
//!
//! The supervisor's side (creating the socket as root, owned by the user) cannot be played here;
//! these tests stand in for it by creating the same shape of socket as the account running them, and
//! pass its descriptor through `dup`, which is all a handover is from this process's point of view.
//!
//! Changeset: docs/dev/1-WIP/2026-09-19-keyring-github-identity.md (host socket under the supervisor)

use super::host_session_socket::*;
use super::inherited_host_sockets::{adopt_descriptors, adopt_from_variables, InheritedHostSocket};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::Arc;
use tddy_core::toolcall::{GithubCredentialHandler, GITHUB_TOKEN_METHOD, HOST_SESSION_SERVICE};
use tddy_host_service::host_session_service::RegisteredSession;
use tddy_rpc::bridge::{RpcResult, RpcService};
use tddy_rpc::{RpcClientTransport, RpcMessage, Status};

fn this_os_user() -> String {
    crate::user_sessions_path::username_for_uid(unsafe { libc::geteuid() })
        .expect("the test process's account has a passwd entry")
}

struct FixedCredential(String);

#[async_trait::async_trait]
impl GithubCredentialHandler for FixedCredential {
    async fn github_token(&self) -> Result<String, String> {
        Ok(self.0.clone())
    }
}

struct NoService;

#[async_trait::async_trait]
impl RpcService for NoService {
    async fn handle_rpc(&self, _: &str, _: &str, _: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(Status::unimplemented("client hosts nothing")))
    }
}

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

/// A listener shaped as the supervisor makes one — `<dir>/<user>/host.sock`, directory `0700`, socket
/// `0600`, both the current account's — outside any data dir.
fn a_supervisor_made_socket(root: &Path, user: &str) -> UnixListener {
    let directory = root.join(user);
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let path = directory.join("host.sock");
    let listener = UnixListener::bind(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    listener
}

/// The descriptor number a child would be handed: a copy, because adoption takes ownership of it.
fn handed_over(listener: &UnixListener) -> i32 {
    // SAFETY: duplicating a descriptor this test holds open.
    let fd = unsafe { libc::dup(listener.as_raw_fd()) };
    assert!(fd >= 0);
    fd
}

/// Adopt `fd` as `user`'s host socket.
fn adopt_one(user: &str, fd: i32) -> Vec<InheritedHostSocket> {
    adopt_descriptors(vec![(user.to_string(), fd)])
}

#[test]
fn adopts_a_listener_owned_by_its_user_and_closed_to_everybody_else() {
    // Given a socket as the supervisor makes them
    let root = tempfile::tempdir().unwrap();
    let user = this_os_user();
    let listener = a_supervisor_made_socket(root.path(), &user);

    // When it is adopted as that user's
    let adopted = adopt_one(&user, handed_over(&listener));

    // Then
    assert_eq!(adopted.len(), 1);
    assert_eq!(adopted[0].os_user, user);
    assert_eq!(adopted[0].path, root.path().join(&user).join("host.sock"));
}

#[test]
fn refuses_a_descriptor_that_is_not_a_listening_socket() {
    // Given a connected stream and a regular file's descriptor
    let (stream, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    let file = std::fs::File::open("/dev/null").unwrap();
    // SAFETY: duplicating descriptors this test holds open.
    let (stream_fd, file_fd) =
        unsafe { (libc::dup(stream.as_raw_fd()), libc::dup(file.as_raw_fd())) };

    // When they are adopted
    let adopted = adopt_descriptors(vec![(this_os_user(), stream_fd), (this_os_user(), file_fd)]);

    // Then neither is served
    assert!(adopted.is_empty());
}

#[test]
fn refuses_a_socket_owned_by_somebody_other_than_the_user_it_is_named_for() {
    // Given a socket the current account owns, announced as root's
    let root = tempfile::tempdir().unwrap();
    let listener = a_supervisor_made_socket(root.path(), "elsewhere");

    // When / Then — one user's sessions are never served on another user's socket
    if unsafe { libc::geteuid() } != 0 {
        assert!(adopt_one("root", handed_over(&listener)).is_empty());
    }
}

#[test]
fn refuses_a_socket_that_group_or_others_could_reach() {
    // Given the user's own socket, loosened to 0660
    let root = tempfile::tempdir().unwrap();
    let user = this_os_user();
    let listener = a_supervisor_made_socket(root.path(), &user);
    std::fs::set_permissions(
        root.path().join(&user).join("host.sock"),
        std::fs::Permissions::from_mode(0o660),
    )
    .unwrap();

    // When / Then
    assert!(adopt_one(&user, handed_over(&listener)).is_empty());
}

#[test]
fn an_adopted_descriptor_does_not_survive_an_exec() {
    // Given an adopted socket
    let root = tempfile::tempdir().unwrap();
    let user = this_os_user();
    let listener = a_supervisor_made_socket(root.path(), &user);
    let fd = handed_over(&listener);

    // When
    let adopted = adopt_one(&user, fd);

    // Then the daemon's own children do not inherit a listener that is not theirs
    assert_eq!(adopted.len(), 1);
    let flags = unsafe { libc::fcntl(adopted[0].listener.as_raw_fd(), libc::F_GETFD) };
    assert_ne!(flags & libc::FD_CLOEXEC, 0);
}

#[test]
fn ignores_variables_that_were_set_for_another_process() {
    // Given a LISTEN_PID that is not ours
    let adopted = adopt_from_variables(
        std::process::id(),
        Some("1"),
        Some("2"),
        Some("connection:host-session.alice"),
    );

    // Then nothing is looked up, let alone closed
    assert!(adopted.is_empty());
}

#[tokio::test]
async fn serves_an_inherited_socket_at_its_own_path_instead_of_binding_one() {
    // Given a daemon handed this user's socket by the supervisor, and a session registered for them
    let root = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let user = this_os_user();
    let listener = a_supervisor_made_socket(root.path(), &user);
    let sockets = HostSessionSockets::default();
    sockets.registry().register(
        "s1",
        RegisteredSession {
            os_user: user.clone(),
            conversation_spawn_handler: None,
            github_credential_handler: Some(Arc::new(FixedCredential("ghp_inherited".into()))),
        },
    );
    sockets.adopt_inherited(adopt_one(&user, handed_over(&listener)));

    // When the session's socket is asked for
    let path = sockets.ensure_bound(&user, data.path()).await.unwrap();

    // Then it is the supervisor's socket, answering — and nothing was bound under the data dir
    assert_eq!(path, root.path().join(&user).join("host.sock"));
    assert_eq!(ask_for_token(&path, "s1").await, Ok("ghp_inherited".into()));
    assert!(!data.path().join("run").exists());
}

#[tokio::test]
async fn refuses_a_user_the_supervisor_gave_no_socket_and_says_where_to_declare_one() {
    // Given a daemon handed a socket for this user only
    if unsafe { libc::geteuid() } == 0 {
        return; // root can chown anywhere, so it would bind rather than refuse
    }
    let root = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let user = this_os_user();
    let listener = a_supervisor_made_socket(root.path(), &user);
    let sockets = HostSessionSockets::default();
    sockets.adopt_inherited(adopt_one(&user, handed_over(&listener)));

    // When another account's session needs a socket
    let refusal = sockets
        .ensure_bound("root", data.path())
        .await
        .expect_err("no socket was declared for root");

    // Then it is told what to add to supervisor.yaml
    let refusal = format!("{refusal:#}");
    assert!(refusal.contains("supervisor.yaml"), "{refusal}");
    assert!(refusal.contains("host_sockets"), "{refusal}");
}

#[tokio::test]
async fn does_not_rebind_an_inherited_socket_whose_file_was_removed() {
    // Given a served inherited socket whose file something deleted
    let root = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let user = this_os_user();
    let listener = a_supervisor_made_socket(root.path(), &user);
    let sockets = HostSessionSockets::default();
    sockets.adopt_inherited(adopt_one(&user, handed_over(&listener)));
    let path = sockets.ensure_bound(&user, data.path()).await.unwrap();
    std::fs::remove_file(&path).unwrap();

    // When the next session needs it
    let refusal = sockets
        .ensure_bound(&user, data.path())
        .await
        .expect_err("the daemon cannot recreate a socket only the supervisor may make");

    // Then it says so rather than quietly serving a path of its own under the same user
    let refusal = format!("{refusal:#}");
    assert!(refusal.contains("supervisor"), "{refusal}");
    assert!(!data.path().join("run").exists());
}

// ---------------------------------------------------------------------------------------------
// One daemon socket per mapped OS user (`#agent-tools` 31/32)
// ---------------------------------------------------------------------------------------------

/// How long a call over a served socket may take to be answered.
const A_CALL_ANSWERS_WITHIN: std::time::Duration = std::time::Duration::from_secs(10);

const A_ROSTER_SERVICE: &str = "test.Roster";
const THE_ROSTERS_ANSWER: &str = "answered by the roster";

struct ARoster;

#[async_trait::async_trait]
impl RpcService for ARoster {
    async fn handle_rpc(&self, _: &str, _: &str, _: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Ok(THE_ROSTERS_ANSWER.as_bytes().to_vec()))
    }
}

/// Call the agent-tool roster over the socket at `path`, as a co-located agent does.
async fn a_roster_call_over(path: &Path) -> Result<String, String> {
    let call = async {
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
        client
            .call_unary(A_ROSTER_SERVICE, "Anything", Vec::new())
            .await
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .map_err(|status| status.message().to_string())
    };
    tokio::time::timeout(A_CALL_ANSWERS_WITHIN, call)
        .await
        .unwrap_or_else(|_| Err("the call was not answered in time".to_string()))
}

/// A listener shaped as the supervisor makes a user's one socket — `<root>/<user>/daemon.sock`,
/// directory `0700`, socket `0600`, both the current account's.
fn a_supervisor_made_user_socket(root: &Path, user: &str) -> UnixListener {
    let directory = root.join(user);
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let path = directory.join(tddy_daemon_kernel::agent_tool_sockets::USER_SOCKET_FILE);
    let listener = UnixListener::bind(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    listener
}

#[tokio::test]
async fn a_supervised_daemon_serves_host_session_and_agent_tool_calls_on_one_user_socket() {
    // Given a supervised daemon handed this user's one socket, serving a roster, with the
    // host-session service served on the same sockets and a session registered for the user
    let root = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let user = this_os_user();
    let listener = a_supervisor_made_user_socket(root.path(), &user);
    let user_sockets = Arc::new(
        tddy_daemon_kernel::agent_tool_sockets::AgentToolSockets::new(
            tddy_daemon_kernel::agent_tool_sockets::AgentToolSocketPlacement::Supervised,
        ),
    );
    user_sockets.install_roster(vec![tddy_rpc::ServiceEntry {
        name: A_ROSTER_SERVICE,
        service: Arc::new(ARoster),
    }]);
    user_sockets.adopt_inherited(
        tddy_daemon_kernel::agent_tool_sockets::adopt_user_descriptors(vec![(
            user.clone(),
            handed_over(&listener),
        )]),
    );
    let host_sessions = HostSessionSockets::default();
    host_sessions.registry().register(
        "s1",
        RegisteredSession {
            os_user: user.clone(),
            conversation_spawn_handler: None,
            github_credential_handler: Some(Arc::new(FixedCredential("ghp_one_socket".into()))),
        },
    );
    host_sessions.serve_on(Arc::clone(&user_sockets));

    // When a session of that user asks for its host-session socket and its agent-tool socket
    let host_path = host_sessions.ensure_bound(&user, data.path()).await;
    let tool_path = user_sockets.ensure_bound(&user, data.path()).await;

    // Then both are the supervisor's one socket, and it answers both kinds of call
    let the_socket = root
        .path()
        .join(&user)
        .join(tddy_daemon_kernel::agent_tool_sockets::USER_SOCKET_FILE);
    assert_eq!(
        (
            host_path.as_ref().ok().cloned(),
            tool_path.as_ref().ok().cloned()
        ),
        (Some(the_socket.clone()), Some(the_socket.clone())),
        "host {host_path:?}, tools {tool_path:?}"
    );
    assert_eq!(
        (
            tokio::time::timeout(A_CALL_ANSWERS_WITHIN, ask_for_token(&the_socket, "s1"))
                .await
                .unwrap_or_else(|_| Err("the call was not answered in time".to_string())),
            a_roster_call_over(&the_socket).await,
        ),
        (
            Ok("ghp_one_socket".to_string()),
            Ok(THE_ROSTERS_ANSWER.to_string())
        )
    );
    assert!(
        !data.path().join("run").exists(),
        "a supervised daemon binds nothing of its own"
    );
}

#[tokio::test]
async fn a_supervised_host_session_refusal_names_users_yaml() {
    // Given a supervised daemon handed no socket for another account
    let data = tempfile::tempdir().unwrap();
    let user_sockets = Arc::new(
        tddy_daemon_kernel::agent_tool_sockets::AgentToolSockets::new(
            tddy_daemon_kernel::agent_tool_sockets::AgentToolSocketPlacement::Supervised,
        ),
    );
    let host_sessions = HostSessionSockets::default();
    host_sessions.serve_on(user_sockets);

    // When a session of this account needs its host-session socket
    let refusal = host_sessions
        .ensure_bound(&this_os_user(), data.path())
        .await
        .map_err(|e| format!("{e:#}"));

    // Then the operator is told to map the user in users.yaml — no supervisor.yaml list remains
    assert!(
        matches!(&refusal, Err(why) if why.contains("users.yaml") && !why.contains("host_sockets")),
        "{refusal:?}"
    );
}

#[test]
fn the_supervisor_and_the_daemon_share_one_user_socket_prefix() {
    // Given the supervisor's own writer of the names: its socket, then alice's and bob's
    let names = format!(
        "{}:{}:{}",
        tddy_supervisor::SERVICE_SOCKET_FD_NAME,
        tddy_supervisor::user_socket_fd_name("alice"),
        tddy_supervisor::user_socket_fd_name("bob")
    );

    // Then the daemon's one reader finds them at 4 and 5 — which it can only do by sharing the
    // supervisor's prefix — and the variable naming users.yaml is spelled alike on both sides
    assert_eq!(
        (
            tddy_supervisor::USER_SOCKET_FD_NAME_PREFIX,
            tddy_daemon_kernel::agent_tool_sockets::user_socket_fds(
                7,
                Some("7"),
                Some("3"),
                Some(&names)
            ),
            tddy_supervisor::spawn_broker::USERS_FILE_ENV,
        ),
        (
            tddy_daemon_kernel::agent_tool_sockets::USER_SOCKET_FD_NAME_PREFIX,
            vec![("alice".to_string(), 4), ("bob".to_string(), 5)],
            tddy_daemon_kernel::users_file::USERS_FILE_ENV,
        )
    );
}
