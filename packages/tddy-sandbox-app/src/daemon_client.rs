//! Linux daemon-assisted mode: the app sends its resolved sandbox params to a running tddy-daemon,
//! which spawns the cgroups-sandboxed session, then the app terminal-proxies the session over the
//! daemon's local Unix-socket gRPC endpoint. (macOS keeps the in-process Seatbelt spawn in
//! `spawn.rs` + `bridge::run_terminal_bridge`, both unchanged.)

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use bytes::Bytes;
use tddy_service::proto::connection::{
    MintLocalTokenRequest, SessionTerminalInput, StartSessionRequest,
};
use tddy_service::tonic_connection::connection_service_client::ConnectionServiceClient;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;
use tonic::transport::Channel;
use tonic::{Code, Status};

use crate::bridge::{
    bridge_stop_reason, classify_stdin_read, resize_frame_if_changed, terminal_size_or_default,
    RawMode, StdinRead,
};

/// Resolved inputs for the Linux daemon-assisted flow. Mirrors the subset of `SpawnParams` the
/// daemon needs — everything else (jail home seeding, context prep, binary resolution) is the
/// daemon's job once it owns the sandboxed session.
pub struct DaemonClientParams {
    /// Explicit `--daemon-socket` override; `None` resolves the default path.
    pub daemon_socket: Option<PathBuf>,
    pub repo: PathBuf,
    pub model: String,
    pub permission_mode: String,
    pub managed_codebase: bool,
    pub claude_args: Vec<String>,
    /// Names of specialized subagents to wire into the session. The daemon resolves each name
    /// against its own `<tddyhome>/agents` (+ builtins); an unknown name is a daemon-side request
    /// error, never a silent drop.
    pub specialized_agents: Vec<String>,
}

/// Drive the whole Linux flow: connect the daemon socket, mint a local peer-trust token, start the
/// sandboxed claude-cli session, then proxy the local terminal to it until the session ends.
pub async fn run(params: DaemonClientParams) -> Result<()> {
    let socket = resolve_daemon_socket_path(params.daemon_socket);
    eprintln!("connecting to tddy-daemon at {}", socket.display());
    let mut client = connect_connection_client(&socket).await?;

    let session_token = client
        .mint_local_token(MintLocalTokenRequest {})
        .await
        .map_err(|status| map_daemon_status("mint local token", &status))?
        .into_inner()
        .session_token;

    let codebase_mode = if params.managed_codebase {
        "managed"
    } else {
        "mounted"
    };
    if !params.specialized_agents.is_empty() {
        eprintln!("specialized_agents={}", params.specialized_agents.join(","));
    }
    let request = start_session_request_from(
        &session_token,
        &params.repo,
        &params.model,
        &params.permission_mode,
        codebase_mode,
        &params.claude_args,
        &params.specialized_agents,
    );

    let session_id = client
        .start_session(request)
        .await
        .map_err(|status| map_daemon_status("start session", &status))?
        .into_inner()
        .session_id;
    eprintln!("session_id={session_id} (running inside tddy-daemon sandbox)");

    run_daemon_terminal_bridge(client, &session_token, &session_id).await
}

/// Resolve the daemon Unix-socket path: the explicit override wins; otherwise mirror
/// `DaemonConfig::local_socket_path` — `${XDG_RUNTIME_DIR}/tddy-daemon.sock`, falling back to
/// `/run/tddy-daemon.sock` when `XDG_RUNTIME_DIR` is unset.
pub fn resolve_daemon_socket_path(override_path: Option<PathBuf>) -> PathBuf {
    override_path.unwrap_or_else(|| {
        default_socket_path_from(
            std::env::var_os("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .as_deref(),
        )
    })
}

/// Pure core of [`resolve_daemon_socket_path`]: given the value of `XDG_RUNTIME_DIR` (if any),
/// return the default socket path. `/run` is the fallback runtime dir.
fn default_socket_path_from(xdg_runtime_dir: Option<&Path>) -> PathBuf {
    xdg_runtime_dir
        .unwrap_or_else(|| Path::new("/run"))
        .join("tddy-daemon.sock")
}

/// Connect a tonic `ConnectionService` client over the daemon's AF_UNIX socket, reusing the shared
/// UDS connector from `tddy-sandbox-runner`.
async fn connect_connection_client(socket: &Path) -> Result<ConnectionServiceClient<Channel>> {
    let channel = tddy_sandbox_runner::connect_uds_channel(socket)
        .await
        .with_context(|| {
            format!(
                "could not connect to tddy-daemon at {}. Is the daemon running? \
                 Pass --daemon-socket to point at a different socket.",
                socket.display()
            )
        })?;
    Ok(ConnectionServiceClient::new(channel))
}

/// Turn a daemon RPC `Status` into an actionable error, calling out the common local-setup failure
/// (an unmapped OS user) explicitly rather than surfacing a bare gRPC code.
fn map_daemon_status(action: &str, status: &Status) -> anyhow::Error {
    match status.code() {
        Code::PermissionDenied => anyhow::anyhow!(
            "{action}: permission denied by tddy-daemon ({}). Your OS user is likely not mapped to \
             a daemon-authorized user — check the daemon's user mapping configuration.",
            status.message()
        ),
        code => anyhow::anyhow!("{action}: {} ({code:?})", status.message()),
    }
}

/// Assemble the daemon `StartSession` request from the app's resolved sandbox params. Pure so it is
/// unit-testable without a daemon. Always a sandboxed claude-cli session; `managed_codebase` is
/// derived from `codebase_mode` ("managed" => true, anything else e.g. "mounted" => false). Extra
/// `claude_args` are forwarded to the in-jail `claude`; `specialized_agents` names are resolved by
/// the daemon against its own agents pool.
pub fn start_session_request_from(
    session_token: &str,
    repo: &Path,
    model: &str,
    permission_mode: &str,
    codebase_mode: &str,
    claude_args: &[String],
    specialized_agents: &[String],
) -> StartSessionRequest {
    StartSessionRequest {
        session_token: session_token.to_string(),
        session_type: "claude-cli".into(),
        sandbox: true,
        repo_path: repo.to_string_lossy().into_owned(),
        model: model.to_string(),
        permission_mode: permission_mode.to_string(),
        managed_codebase: codebase_mode == "managed",
        claude_args: claude_args.to_vec(),
        specialized_agents: specialized_agents.to_vec(),
        ..Default::default()
    }
}

/// The first frame of a `StreamSessionTerminalIO` stream: it carries the auth pair
/// (`session_token` + `session_id`) the daemon reads before wiring up the terminal, plus an initial
/// in-band OSC resize (`\x1b]resize;{cols};{rows}\x07`) so the sandboxed PTY opens at the host
/// terminal's real size from the first byte. Pure so it is unit-testable without a daemon.
pub(crate) fn first_terminal_input(
    session_token: &str,
    session_id: &str,
    cols: u16,
    rows: u16,
) -> SessionTerminalInput {
    SessionTerminalInput {
        session_token: session_token.to_string(),
        session_id: session_id.to_string(),
        data: format!("\x1b]resize;{cols};{rows}\x07").into_bytes(),
        ..Default::default()
    }
}

/// A subsequent stream frame carrying only terminal data (stdin bytes or an in-band OSC resize).
/// The daemon has already authenticated the stream on the first frame, so these need no token.
fn terminal_input_data(data: Bytes) -> SessionTerminalInput {
    SessionTerminalInput {
        data: data.to_vec(),
        ..Default::default()
    }
}

/// Proxy the local terminal to a daemon-hosted sandbox session over the bidi
/// `StreamSessionTerminalIO`. Reuses the same front-end helpers as the macOS in-process bridge:
/// [`RawMode`], [`classify_stdin_read`] (Ctrl-C is forwarded, only EOF disconnects),
/// [`resize_frame_if_changed`] (100ms live-resize poll → in-band OSC), and [`bridge_stop_reason`].
async fn run_daemon_terminal_bridge(
    mut client: ConnectionServiceClient<Channel>,
    session_token: &str,
    session_id: &str,
) -> Result<()> {
    let (rows, cols) = terminal_size_or_default();

    // Outbound frames: first the auth + initial-resize frame, then stdin bytes / OSC resizes.
    let (out_tx, out_rx) = mpsc::unbounded_channel::<SessionTerminalInput>();
    out_tx
        .send(first_terminal_input(session_token, session_id, cols, rows))
        .map_err(|_| anyhow::anyhow!("terminal input channel closed before first frame"))?;

    let response = client
        .stream_session_terminal_io(UnboundedReceiverStream::new(out_rx))
        .await
        .map_err(|status| map_daemon_status("open terminal stream", &status))?;
    let mut inbound = response.into_inner();

    // Inbound: session terminal output → local stdout. Task ends when the daemon closes the stream
    // (the session ended), which the loop below treats as a stop condition.
    let stdout_task = tokio::spawn(async move {
        let mut stdout = std::io::stdout();
        while let Some(item) = inbound.next().await {
            match item {
                Ok(out) => {
                    let _ = stdout.write_all(&out.data);
                    let _ = stdout.flush();
                }
                Err(_) => break,
            }
        }
    });

    log::info!(
        target: "tddy_sandbox_app::daemon_client",
        "terminal stream open session_id={session_id} terminal={cols}x{rows}"
    );
    eprintln!("terminal bridge active (Ctrl-D to disconnect)");

    let _raw = RawMode::enable();
    let shutdown = Arc::new(AtomicBool::new(false));

    // Real stdin → the session PTY, via a blocking read thread. The resize poll below shares the
    // same outbound channel through a cloned sender.
    let resize_tx = out_tx.clone();
    let shutdown_stdin = Arc::clone(&shutdown);
    std::thread::spawn(move || {
        let mut buf = [0u8; 256];
        let mut stdin = std::io::stdin();
        loop {
            if shutdown_stdin.load(Ordering::Relaxed) {
                break;
            }
            match stdin.read(&mut buf) {
                Err(_) => {
                    shutdown_stdin.store(true, Ordering::Relaxed);
                    break;
                }
                Ok(n) => match classify_stdin_read(n, &buf) {
                    StdinRead::Disconnected => {
                        shutdown_stdin.store(true, Ordering::Relaxed);
                        break;
                    }
                    StdinRead::Forward(bytes) => {
                        if out_tx.send(terminal_input_data(bytes)).is_err() {
                            shutdown_stdin.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                },
            }
        }
    });

    let mut last_sent_size = (rows, cols);
    loop {
        // No local child process here (the sandbox lives in the daemon); the stream closing is the
        // "session ended" signal, mapped onto `relay_finished`.
        if let Some(reason) = bridge_stop_reason(
            shutdown.load(Ordering::Relaxed),
            stdout_task.is_finished(),
            false,
        ) {
            log::info!(target: "tddy_sandbox_app::daemon_client", "bridge loop stopping: {reason:?}");
            break;
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                let current = terminal_size_or_default();
                if let Some(frame) = resize_frame_if_changed(current, last_sent_size) {
                    if resize_tx.send(terminal_input_data(frame)).is_err() {
                        log::warn!(target: "tddy_sandbox_app::daemon_client", "resize frame: stream channel closed");
                    } else {
                        last_sent_size = current;
                    }
                }
            }
            res = tokio::signal::ctrl_c() => {
                match res {
                    Ok(()) => {
                        log::info!(target: "tddy_sandbox_app::daemon_client", "Ctrl-C — shutting down");
                        shutdown.store(true, Ordering::Relaxed);
                    }
                    Err(e) => {
                        log::warn!(target: "tddy_sandbox_app::daemon_client", "ctrl_c listener: {e}");
                    }
                }
                break;
            }
        }
    }
    shutdown.store(true, Ordering::Relaxed);
    stdout_task.abort();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_sandboxed_claude_cli_request_from_app_params() {
        // Given — resolved params for a managed-codebase session with extra claude args
        let claude_args = vec!["--add-dir".to_string(), "/extra".to_string()];

        // When
        let req = start_session_request_from(
            "tok",
            Path::new("/home/dev/proj"),
            "claude-opus-4-8",
            "plan",
            "managed",
            &claude_args,
            &[],
        );

        // Then
        assert_eq!(req.session_token, "tok");
        assert_eq!(req.session_type, "claude-cli");
        assert!(req.sandbox);
        assert_eq!(req.repo_path, "/home/dev/proj");
        assert_eq!(req.model, "claude-opus-4-8");
        assert_eq!(req.permission_mode, "plan");
        assert!(req.managed_codebase);
        assert_eq!(
            req.claude_args,
            vec!["--add-dir".to_string(), "/extra".to_string()]
        );
    }

    #[test]
    fn maps_mounted_codebase_mode_to_a_non_managed_request() {
        // Given / When — codebase_mode "mounted"
        let req =
            start_session_request_from("tok", Path::new("/r"), "m", "auto", "mounted", &[], &[]);

        // Then
        assert!(!req.managed_codebase);
    }

    /// Requested specialized-agent names are forwarded verbatim for the daemon to resolve against
    /// its own agents pool — never dropped or reordered.
    #[test]
    fn forwards_specialized_agent_names_for_the_daemon_to_resolve() {
        // Given
        let agents = vec!["fastcontext".to_string(), "my-linter".to_string()];

        // When
        let req = start_session_request_from(
            "tok",
            Path::new("/r"),
            "m",
            "auto",
            "mounted",
            &[],
            &agents,
        );

        // Then
        assert_eq!(
            req.specialized_agents,
            vec!["fastcontext".to_string(), "my-linter".to_string()]
        );
    }

    /// An explicit `--daemon-socket` override is honored verbatim, regardless of the environment.
    #[test]
    fn resolve_daemon_socket_path_honors_explicit_override() {
        // Given
        let override_path = PathBuf::from("/custom/tddy.sock");

        // When
        let resolved = resolve_daemon_socket_path(Some(override_path.clone()));

        // Then
        assert_eq!(resolved, override_path);
    }

    /// With `XDG_RUNTIME_DIR` set, the default socket lives under it — matching
    /// `DaemonConfig::local_socket_path`.
    #[test]
    fn default_socket_path_uses_xdg_runtime_dir_when_present() {
        // Given / When
        let path = default_socket_path_from(Some(Path::new("/run/user/1000")));

        // Then
        assert_eq!(path, PathBuf::from("/run/user/1000/tddy-daemon.sock"));
    }

    /// With no `XDG_RUNTIME_DIR`, the default falls back to `/run/tddy-daemon.sock`.
    #[test]
    fn default_socket_path_falls_back_to_run_when_xdg_unset() {
        // Given / When
        let path = default_socket_path_from(None);

        // Then
        assert_eq!(path, PathBuf::from("/run/tddy-daemon.sock"));
    }

    /// The first stream frame carries the auth pair plus an initial in-band OSC resize sized to the
    /// host terminal, so the sandboxed PTY opens at the right size from the first byte.
    #[test]
    fn first_terminal_input_carries_auth_and_initial_resize_osc() {
        // Given / When
        let frame = first_terminal_input("tok", "sess-1", 100, 30);

        // Then
        assert_eq!(frame.session_token, "tok");
        assert_eq!(frame.session_id, "sess-1");
        assert_eq!(frame.data, b"\x1b]resize;100;30\x07".to_vec());
    }

    /// Subsequent data frames carry only the bytes — the stream is already authenticated on the
    /// first frame, so no token is repeated.
    #[test]
    fn terminal_input_data_carries_only_the_bytes() {
        // Given / When
        let frame = terminal_input_data(Bytes::from_static(b"ls\r"));

        // Then
        assert_eq!(frame.data, b"ls\r".to_vec());
        assert!(frame.session_token.is_empty());
        assert!(frame.session_id.is_empty());
    }
}
