use super::ACTIVITY_SERVICE;

use super::SessionHookArgs;

/// How long one report may take, over either transport: a hook that hangs blocks Claude Code.
pub(crate) const REPORT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Send an encoded protobuf request to a daemon [`ACTIVITY_SERVICE`] method — over the daemon's
/// agent-facing socket when the hook was given one, over HTTP otherwise.
pub(crate) async fn deliver(
    args: &SessionHookArgs,
    method: &str,
    body: Vec<u8>,
) -> anyhow::Result<()> {
    exchange(args, method, body, REPORT_TIMEOUT).await.map(drop)
}

/// Where a hook's reports go (`#agent-tools 7/12`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookTransport {
    /// The in-jail tool-IPC socket (`TDDY_SANDBOX_TOOL_IPC`); the runner relays each report to the
    /// facilitating daemon (`tddy_service::session_activity::IN_JAIL_RELAYABLE_HOOK_REPORTS`).
    SandboxIpc(std::path::PathBuf),
    /// An embedded daemon's agent-tool socket (`--daemon-socket`).
    DaemonSocket(std::path::PathBuf),
    /// The daemon's HTTP RPC (`--daemon`).
    Http(String),
}

/// The transport for `args`: the sandbox tool-IPC socket when the hook runs in a jail
/// (`sandbox_tool_ipc`, read from `TDDY_SANDBOX_TOOL_IPC` by the caller) — whatever the baked-in
/// command names, since a jail reaches neither a host socket path nor the daemon's port — then
/// `--daemon-socket`, then `--daemon`.
pub fn hook_transport(
    args: &SessionHookArgs,
    sandbox_tool_ipc: Option<&std::path::Path>,
) -> HookTransport {
    if let Some(socket) = sandbox_tool_ipc {
        return HookTransport::SandboxIpc(socket.to_path_buf());
    }
    match &args.daemon_socket {
        Some(socket) => HookTransport::DaemonSocket(socket.clone()),
        None => HookTransport::Http(args.daemon.clone()),
    }
}

/// [`deliver`], returning the response body and bounding the call by `timeout`.
///
/// The transport is [`hook_transport`]'s: the tool-IPC socket when the hook process was given
/// `TDDY_SANDBOX_TOOL_IPC`, the socket it was given, or the daemon's HTTP RPC.
pub(crate) async fn exchange(
    args: &SessionHookArgs,
    method: &str,
    body: Vec<u8>,
    timeout: std::time::Duration,
) -> anyhow::Result<Vec<u8>> {
    let sandbox_tool_ipc = std::env::var_os("TDDY_SANDBOX_TOOL_IPC")
        .filter(|socket| !socket.is_empty())
        .map(std::path::PathBuf::from);
    match hook_transport(args, sandbox_tool_ipc.as_deref()) {
        HookTransport::SandboxIpc(socket) | HookTransport::DaemonSocket(socket) => {
            call_over_socket(&socket, method, body, timeout).await
        }
        HookTransport::Http(daemon) => post_proto(&daemon, method, body, timeout).await,
    }
}

/// Call a daemon [`ACTIVITY_SERVICE`] method over an embedded daemon's agent-tool socket: the
/// `tddy-rpc` length-prefixed framing that socket serves (`tddy_daemon_kernel::agent_tool_sockets`), one
/// connection per call. No envelope is needed — both reports authenticate by the `hook_token` they
/// carry, exactly as they do over HTTP.
async fn call_over_socket(
    socket: &std::path::Path,
    method: &str,
    body: Vec<u8>,
    timeout: std::time::Duration,
) -> anyhow::Result<Vec<u8>> {
    let context = format!("{ACTIVITY_SERVICE}/{method} over {}", socket.display());
    let call = async {
        let client = tddy_session_tool_client::connect_sandbox_ipc(socket)
            .await
            .map_err(|e| anyhow::anyhow!("{context}: {e}"))?;
        client
            .call_unary(ACTIVITY_SERVICE, method, body)
            .await
            .map_err(|e| anyhow::anyhow!("{context}: {e}"))
    };
    tokio::time::timeout(timeout, call)
        .await
        .map_err(|_| anyhow::anyhow!("{context}: timed out"))?
}

/// POST an encoded protobuf request to a daemon [`ACTIVITY_SERVICE`] method via the Connect
/// protocol (async reqwest with a 2-second timeout).
async fn post_proto(
    daemon: &str,
    method: &str,
    body: Vec<u8>,
    timeout: std::time::Duration,
) -> anyhow::Result<Vec<u8>> {
    let url = format!(
        "{}/rpc/{ACTIVITY_SERVICE}/{method}",
        daemon.trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| anyhow::anyhow!("build http client: {e}"))?;

    let resp = client
        .post(&url)
        .header("content-type", "application/proto")
        .body(body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("POST {url}: {e}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("POST {url} → HTTP {}", resp.status());
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| anyhow::anyhow!("POST {url}: read response: {e}"))?;
    Ok(bytes.to_vec())
}
