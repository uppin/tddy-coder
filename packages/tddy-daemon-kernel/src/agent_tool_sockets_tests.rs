//! Unit tests: the per-OS-user agent-tool socket (`#agent-tools` 26/28) — where it lives, who can
//! reach it, what it serves, which supervisor-handed descriptors are its, and that a user this
//! daemon cannot give one to is refused naming the setting.
//!
//! Every test binds a real unix socket under a temporary data dir as the account running it — the
//! one OS user a test can serve without privilege. Serving a *second* user (and that user being
//! unable to connect to the first one's socket) needs root and a real account: that is pinned by
//! the Linux-only privileged suites, not here.
//!
//! Changeset: docs/dev/1-WIP/2026-11-06-agent-tool-socket-per-os-user.md

use super::*;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::sync::Arc;
use std::time::Duration;

use tddy_rpc::bridge::{RpcResult, RpcService};
use tddy_rpc::{RpcClientTransport, RpcMessage, ServiceEntry, Status};

/// A session's connect gives up after this, rather than the test hanging on a socket nobody serves.
const A_CALL_ANSWERS_WITHIN: Duration = Duration::from_secs(10);

const A_ROSTER_SERVICE: &str = "test.Roster";
const ITS_ANSWER: &str = "answered by the daemon's roster";

fn this_os_user() -> String {
    // SAFETY: `getpwuid` points into static storage that is read at once.
    unsafe {
        let entry = libc::getpwuid(libc::geteuid());
        assert!(!entry.is_null(), "the test account has a passwd entry");
        std::ffi::CStr::from_ptr((*entry).pw_name)
            .to_string_lossy()
            .into_owned()
    }
}

fn is_root() -> bool {
    // SAFETY: reads this process's own credentials.
    unsafe { libc::geteuid() == 0 }
}

fn a_data_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temp data dir")
}

fn mode_of(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .expect("stat")
        .permissions()
        .mode()
        & 0o7777
}

/// A roster service that answers every call with [`ITS_ANSWER`].
struct ARosterService;

#[async_trait::async_trait]
impl RpcService for ARosterService {
    async fn handle_rpc(&self, _: &str, _: &str, _: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Ok(ITS_ANSWER.as_bytes().to_vec()))
    }
}

/// The client side of a connection hosts nothing.
struct NoService;

#[async_trait::async_trait]
impl RpcService for NoService {
    async fn handle_rpc(&self, _: &str, _: &str, _: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(Status::unimplemented("the client hosts nothing")))
    }
}

/// Per-OS-user sockets serving [`ARosterService`].
fn sockets_serving_a_roster() -> AgentToolSockets {
    let sockets = AgentToolSockets::new(AgentToolSocketPlacement::PerOsUser);
    sockets.install_roster(vec![ServiceEntry {
        name: A_ROSTER_SERVICE,
        service: Arc::new(ARosterService),
    }]);
    sockets
}

/// Call the roster over the socket at `path`, as a co-located agent does.
async fn a_call_over(path: &Path) -> Result<String, String> {
    let call = async {
        let stream = tokio::net::UnixStream::connect(path)
            .await
            .map_err(|e| format!("connect {}: {e}", path.display()))?;
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

/// A listener shaped as the supervisor makes one — `<root>/<user>/agent.sock`, directory `0700`,
/// socket `0600`, both this account's — outside any data dir.
fn a_supervisor_made_socket(root: &Path, user: &str) -> InheritedAgentToolSocket {
    use std::os::unix::fs::DirBuilderExt;
    let directory = root.join(user);
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .expect("the user's directory is created");
    let path = directory.join(AGENT_TOOL_SOCKET_FILE);
    let listener = UnixListener::bind(&path).expect("the socket binds");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .expect("the socket is restricted");
    InheritedAgentToolSocket {
        os_user: user.to_string(),
        listener,
        path,
    }
}

#[tokio::test]
async fn a_users_socket_is_owner_only_inside_an_owner_only_directory_of_its_own() {
    // Given a server daemon's sockets
    let data = a_data_dir();
    let sockets = sockets_serving_a_roster();
    let user = this_os_user();

    // When a session of this user asks for its socket
    let path = sockets
        .ensure_bound(&user, data.path())
        .await
        .map_err(|e| format!("{e:#}"));

    // Then it is `<data>/run-agent-tools/<user>/agent.sock`: a 0600 socket in a 0700 directory,
    // both the user's
    let expected = data
        .path()
        .join(AGENT_TOOL_SOCKETS_RUN_DIR)
        .join(&user)
        .join(AGENT_TOOL_SOCKET_FILE);
    let shape = path.as_ref().ok().map(|path| {
        let socket = std::fs::symlink_metadata(path).expect("stat the socket");
        let directory = path.parent().expect("a directory");
        (
            socket.file_type().is_socket(),
            mode_of(path),
            mode_of(directory),
            socket.uid(),
            std::fs::metadata(directory).expect("stat").uid(),
        )
    });
    // SAFETY: reads this process's own credentials.
    let me = unsafe { libc::geteuid() };
    assert_eq!(
        (path.clone(), shape),
        (Ok(expected), Some((true, 0o600, 0o700, me, me)))
    );
}

#[tokio::test]
async fn a_call_over_a_users_socket_reaches_the_daemons_roster() {
    // Given a user's socket, bound
    let data = a_data_dir();
    let sockets = sockets_serving_a_roster();
    let path = sockets
        .ensure_bound(&this_os_user(), data.path())
        .await
        .map_err(|e| format!("{e:#}"));

    // When a co-located agent calls over it
    let answered = match &path {
        Ok(path) => a_call_over(path).await,
        Err(refused) => Err(refused.clone()),
    };

    // Then the roster answered
    assert_eq!(answered, Ok(ITS_ANSWER.to_string()));
}

#[tokio::test]
async fn two_sessions_of_one_user_share_one_socket() {
    // Given one user's socket, bound by a first session
    let data = a_data_dir();
    let sockets = sockets_serving_a_roster();
    let user = this_os_user();
    let first = sockets
        .ensure_bound(&user, data.path())
        .await
        .map_err(|e| format!("{e:#}"));

    // When a second session of the same user asks
    let second = sockets
        .ensure_bound(&user, data.path())
        .await
        .map_err(|e| format!("{e:#}"));

    // Then both are told the same, served path
    assert!(first.is_ok(), "{first:?}");
    assert_eq!(second, first);
}

#[tokio::test]
async fn an_unlisted_user_under_the_supervisor_is_refused_naming_the_setting() {
    // Given a daemon without the privilege to chown — as under tddy-supervisor — and another
    // account's name
    if is_root() {
        return; // root can chown anywhere; the refusal this pins is the unprivileged one
    }
    let data = a_data_dir();
    let sockets = sockets_serving_a_roster();
    let another_account = "root";

    // When a session of that account asks for its socket
    let refusal = sockets
        .ensure_bound(another_account, data.path())
        .await
        .map(|path| path.display().to_string())
        .map_err(|e| format!("{e:#}"));

    // Then it is refused, naming what to add to supervisor.yaml, and nothing is left behind
    let names_the_setting = refusal
        .as_ref()
        .err()
        .map(|why| why.contains("agent_tool_sockets") && why.contains("supervisor.yaml"));
    assert_eq!(
        (
            names_the_setting,
            data.path()
                .join(AGENT_TOOL_SOCKETS_RUN_DIR)
                .join(another_account)
                .exists()
        ),
        (Some(true), false),
        "{refusal:?}"
    );
}

#[tokio::test]
async fn an_inherited_socket_is_served_at_its_own_path_instead_of_binding_one() {
    // Given a daemon handed this user's socket by the supervisor
    let root = tempfile::tempdir().expect("a temp root");
    let data = a_data_dir();
    let user = this_os_user();
    let sockets = sockets_serving_a_roster();
    let inherited = a_supervisor_made_socket(root.path(), &user);
    let its_path = inherited.path.clone();
    sockets.adopt_inherited(vec![inherited]);

    // When a session of that user asks for its socket, and calls over it
    let path = sockets
        .ensure_bound(&user, data.path())
        .await
        .map_err(|e| format!("{e:#}"));
    let answered = match &path {
        Ok(path) => a_call_over(path).await,
        Err(refused) => Err(refused.clone()),
    };

    // Then it is the supervisor's socket, serving the roster — nothing was bound under the data dir
    assert_eq!(
        (
            path,
            answered,
            data.path().join(AGENT_TOOL_SOCKETS_RUN_DIR).exists()
        ),
        (Ok(its_path), Ok(ITS_ANSWER.to_string()), false)
    );
}

#[test]
fn reads_the_agent_tool_descriptors_among_the_handed_over_ones() {
    // Given the service's own socket at 3, alice's host socket at 4, alice's agent-tool socket at 5
    let names = "connection:host-session.alice:agent-tools.alice";

    // When
    let descriptors = agent_tool_fds(7, Some("7"), Some("3"), Some(names));

    // Then only the agent-tool one is this registry's
    assert_eq!(descriptors, vec![("alice".to_string(), 5)]);
}

#[test]
fn finds_no_agent_tool_descriptors_when_the_variables_describe_another_process() {
    // Given a LISTEN_PID that is the parent's, not ours
    let names = "connection:agent-tools.alice";

    // Then — guard: a descriptor adopted for the wrong process would serve somebody else's socket
    assert_eq!(
        agent_tool_fds(7, Some("1"), Some("2"), Some(names)),
        Vec::new()
    );
}

#[test]
fn a_user_name_that_could_escape_the_directory_names_no_socket() {
    // Given
    let data = a_data_dir();

    // When
    let path = agent_tool_socket_path_for_user(data.path(), "../elsewhere");

    // Then
    assert!(path.is_err(), "{path:?}");
}

#[test]
fn a_data_dir_too_deep_for_a_unix_socket_names_no_socket() {
    // Given a data dir deeper than AF_UNIX allows
    let deep = a_data_dir().path().join("a".repeat(120));

    // When
    let path = agent_tool_socket_path_for_user(&deep, &this_os_user());

    // Then
    assert!(path.is_err(), "{path:?}");
}

#[tokio::test]
async fn an_embedded_host_tells_every_session_the_runtimes_one_socket() {
    // Given an embedded host whose runtime is serving its one socket
    let data = a_data_dir();
    let runtime_socket = crate::agent_tool_socket_path(data.path());
    let _served = UnixListener::bind(&runtime_socket).expect("the runtime's socket binds");
    let sockets = AgentToolSockets::new(AgentToolSocketPlacement::Embedded);

    // When a session asks for its socket
    let path = sockets
        .ensure_bound(&this_os_user(), data.path())
        .await
        .map_err(|e| format!("{e:#}"));
    let _ = std::fs::remove_file(&runtime_socket);

    // Then — guard: the desktop keeps its single socket
    assert_eq!(path, Ok(runtime_socket));
}

#[tokio::test]
async fn an_embedded_host_whose_socket_nobody_serves_is_refused_not_pointed_at_it() {
    // Given the socket file an exited embedded daemon left behind: binding creates the file, and
    // dropping the listener does not unlink it
    let data = a_data_dir();
    let runtime_socket = crate::agent_tool_socket_path(data.path());
    drop(UnixListener::bind(&runtime_socket).expect("the runtime's socket binds"));
    let sockets = AgentToolSockets::new(AgentToolSocketPlacement::Embedded);

    // When a session asks for its socket
    let refusal = sockets
        .ensure_bound(&this_os_user(), data.path())
        .await
        .map_err(|e| format!("{e:#}"));
    let _ = std::fs::remove_file(&runtime_socket);

    // Then the dead socket is not chosen: every report sent to it would be lost
    assert!(refusal.is_err(), "{refusal:?}");
}
