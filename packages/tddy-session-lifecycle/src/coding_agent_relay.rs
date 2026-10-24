//! Starting a coding agent as the session's OS user through `tddy-supervisor`, on a supervised host.
//!
//! An unprivileged daemon cannot become another user, and `SpawnSession` gives its child no stdio.
//! `SpawnRelayed` does: the daemon listens on a socket path of its own, asks the supervisor to start
//! the allowlisted harness as the session's user, and the supervisor connects to that socket and
//! relays the child's stdin/stdout over it (see `tddy_supervisor::relay`). What comes back here is a
//! connected stream, which the ACP driver serves in place of a child process.
//!
//! The supervisor's refusal is opaque by design, so the typed error names the three things an
//! operator can change: the harness in `allowed_relay_tool_paths`, the user in
//! `allowed_session_users`, every environment variable in `allowed_env_keys`.

use std::collections::BTreeMap;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tddy_core::AcpAgentCommand;
use tddy_discovery::subagent::SubagentError;
use tddy_supervisor::{SpawnRelayedRequest, SpawnedProcess, SupervisorError};

/// How long the supervisor gets to connect to the daemon's socket once it accepted the request.
const ACCEPT_DEADLINE: Duration = Duration::from_secs(15);
/// The longest socket path both Linux (108) and macOS (104) accept, with room for the NUL.
const MAX_SOCKET_PATH: usize = 100;

/// Asks a supervisor to start a relayed child. The seam a test replaces; production reaches the
/// real one over its socket ([`SupervisorRelay`]).
#[async_trait::async_trait]
pub trait RelaySpawner: Send + Sync {
    async fn spawn_relayed(
        &self,
        request: SpawnRelayedRequest,
    ) -> Result<SpawnedProcess, SupervisorError>;
}

/// The real supervisor, reached over its unix socket, connected per request.
pub struct SupervisorRelay {
    pub socket_path: PathBuf,
}

#[async_trait::async_trait]
impl RelaySpawner for SupervisorRelay {
    async fn spawn_relayed(
        &self,
        request: SpawnRelayedRequest,
    ) -> Result<SpawnedProcess, SupervisorError> {
        let client = tddy_spawn::supervisor_client::connect_supervisor(&self.socket_path)
            .await
            .map_err(|e| SupervisorError::Unavailable {
                message: e.to_string(),
            })?;
        client.spawn_relayed(request).await
    }
}

/// Where the daemon listens for the supervisor, and who it asks.
#[derive(Clone)]
pub struct RelayEndpoint {
    pub spawner: Arc<dyn RelaySpawner>,
    /// A daemon-private directory the per-launch sockets are bound in (created owner-only).
    pub socket_dir: PathBuf,
}

/// Start `command` as `os_user` through the supervisor and return the stream its stdio is relayed on.
pub async fn start_relayed(
    endpoint: &RelayEndpoint,
    os_user: &str,
    command: &AcpAgentCommand,
) -> Result<std::os::unix::net::UnixStream, SubagentError> {
    let socket = bind_private_socket(&endpoint.socket_dir)?;
    let request = SpawnRelayedRequest {
        os_user: os_user.to_string(),
        tool_path: resolve_program(&command.program),
        args: command.args.clone(),
        env: command.env.iter().cloned().collect::<BTreeMap<_, _>>(),
        working_dir: command.cwd.clone(),
        relay_socket: socket.path.clone(),
    };
    endpoint
        .spawner
        .spawn_relayed(request)
        .await
        .map_err(|e| refusal(os_user, &command.program, e))?;
    let (stream, _) = tokio::time::timeout(ACCEPT_DEADLINE, socket.listener.accept())
        .await
        .map_err(|_| {
            SubagentError::from(
                "the supervisor accepted the coding-agent spawn but never connected to the relay \
                 socket",
            )
        })?
        .map_err(|e| SubagentError::from(format!("accepting the supervisor's relay: {e}")))?;
    stream
        .into_std()
        .map_err(|e| SubagentError::from(format!("taking the relay stream: {e}")))
}

fn refusal(os_user: &str, program: &Path, error: SupervisorError) -> SubagentError {
    SubagentError::from(match error {
        SupervisorError::Denied => format!(
            "tddy-supervisor refused to run {} as '{os_user}' (its refusals are opaque): its \
             spawn_policy must list the harness binary in allowed_relay_tool_paths, '{os_user}' in \
             allowed_session_users, and every environment variable the launch sets in \
             allowed_env_keys",
            program.display()
        ),
        other => format!("tddy-supervisor could not start the coding agent: {other}"),
    })
}

/// The harness as the supervisor's allowlist names it: absolute. A bare name is looked up on this
/// daemon's `PATH`; one that is not found is sent as given and refused by the supervisor.
fn resolve_program(program: &Path) -> PathBuf {
    if program.is_absolute() {
        return program.to_path_buf();
    }
    tddy_daemon_kernel::spawn_as_user::find_program_on_spawn_child_path(&program.to_string_lossy())
        .unwrap_or_else(|| program.to_path_buf())
}

struct PrivateSocket {
    listener: tokio::net::UnixListener,
    path: PathBuf,
}

impl Drop for PrivateSocket {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn bind_private_socket(dir: &Path) -> Result<PrivateSocket, SubagentError> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| SubagentError::from(format!("creating {}: {e}", dir.display())))?;
    let path = dir.join(format!(
        "r{}-{}.sock",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    if path.as_os_str().len() > MAX_SOCKET_PATH {
        return Err(SubagentError::from(format!(
            "the relay socket path {} is longer than a unix socket path may be",
            path.display()
        )));
    }
    let listener = tokio::net::UnixListener::bind(&path)
        .map_err(|e| SubagentError::from(format!("binding {}: {e}", path.display())))?;
    Ok(PrivateSocket { listener, path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::Mutex;

    /// A supervisor that records the request and connects to its socket the way the real one does.
    struct FakeSupervisor {
        seen: Mutex<Vec<SpawnRelayedRequest>>,
        answer: Result<(), SupervisorError>,
    }

    #[async_trait::async_trait]
    impl RelaySpawner for FakeSupervisor {
        async fn spawn_relayed(
            &self,
            request: SpawnRelayedRequest,
        ) -> Result<SpawnedProcess, SupervisorError> {
            self.seen.lock().expect("seen").push(request.clone());
            self.answer.clone()?;
            let mut stream = std::os::unix::net::UnixStream::connect(&request.relay_socket)
                .expect("the supervisor connects");
            stream.write_all(b"from the agent").expect("write");
            Ok(SpawnedProcess { pid: 4242 })
        }
    }

    fn an_endpoint(
        answer: Result<(), SupervisorError>,
    ) -> (RelayEndpoint, Arc<FakeSupervisor>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let supervisor = Arc::new(FakeSupervisor {
            seen: Mutex::new(Vec::new()),
            answer,
        });
        let endpoint = RelayEndpoint {
            spawner: supervisor.clone(),
            socket_dir: dir.path().join("r"),
        };
        (endpoint, supervisor, dir)
    }

    fn a_command() -> AcpAgentCommand {
        AcpAgentCommand {
            program: PathBuf::from("/usr/bin/opencode"),
            args: vec!["acp".to_string()],
            env: vec![("K".to_string(), "V".to_string())],
            cwd: Some(PathBuf::from("/work")),
        }
    }

    #[tokio::test]
    async fn asks_the_supervisor_for_the_user_the_harness_and_a_socket_it_listens_on() {
        let (endpoint, supervisor, _dir) = an_endpoint(Ok(()));

        let mut stream = start_relayed(&endpoint, "dev", &a_command())
            .await
            .expect("relayed");

        let seen = supervisor.seen.lock().expect("seen")[0].clone();
        assert_eq!(seen.os_user, "dev");
        assert_eq!(seen.tool_path, PathBuf::from("/usr/bin/opencode"));
        assert_eq!(seen.args, ["acp"]);
        assert_eq!(seen.env.get("K").map(String::as_str), Some("V"));
        assert_eq!(seen.working_dir, Some(PathBuf::from("/work")));
        assert!(seen.relay_socket.starts_with(&endpoint.socket_dir));
        let mut received = [0u8; 14];
        stream.read_exact(&mut received).expect("the relayed bytes");
        assert_eq!(&received, b"from the agent");
    }

    #[tokio::test]
    async fn the_socket_is_removed_once_the_supervisor_has_connected() {
        let (endpoint, supervisor, _dir) = an_endpoint(Ok(()));

        let _stream = start_relayed(&endpoint, "dev", &a_command())
            .await
            .expect("relayed");

        let path = supervisor.seen.lock().expect("seen")[0]
            .relay_socket
            .clone();
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn the_supervisors_opaque_refusal_becomes_an_error_naming_what_to_configure() {
        let (endpoint, _supervisor, _dir) = an_endpoint(Err(SupervisorError::Denied));

        let refusal = start_relayed(&endpoint, "dev", &a_command())
            .await
            .expect_err("refused")
            .to_string();

        assert!(refusal.contains("allowed_relay_tool_paths"), "{refusal}");
        assert!(refusal.contains("allowed_session_users"), "{refusal}");
        assert!(refusal.contains("allowed_env_keys"), "{refusal}");
    }

    #[tokio::test]
    async fn a_socket_path_too_long_for_a_unix_socket_is_refused_before_asking() {
        let (mut endpoint, supervisor, dir) = an_endpoint(Ok(()));
        endpoint.socket_dir = dir.path().join("x".repeat(120));

        let refusal = start_relayed(&endpoint, "dev", &a_command()).await;

        assert!(refusal.is_err());
        assert!(supervisor.seen.lock().expect("seen").is_empty());
    }
}
