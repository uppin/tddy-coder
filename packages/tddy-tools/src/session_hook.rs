//! `session-hook` subcommand: report granular session activity status to the daemon.
//!
//! Claude Code invokes this binary as a hook (stdin = hook event JSON). The subcommand:
//! 1. Reads stdin and parses the hook event.
//! 2. Maps the event to a [`SessionActivityStatus`] via [`activity_status_from_hook`].
//! 3. If the mapping is `None` (no-op event), exits 0 immediately — no daemon call.
//! 4. Otherwise, POSTs a `ReportSessionStatus` RPC to the daemon.
//! 5. **Fail-quiet contract**: any error (parse, network, daemon rejection) is printed to stderr
//!    and the process exits 0. Claude Code must never be blocked by a failing hook.

use clap::Args;
use prost::Message as _;
use std::io::Read;
use tddy_core::{activity_status_from_hook, parse_hook_event};
use tddy_service::proto::connection::ReportSessionStatusRequest;

#[derive(Args)]
pub struct SessionHookArgs {
    /// Daemon session id (baked in at worktree-prep time).
    #[arg(long)]
    pub session: String,

    /// Daemon HTTP base URL (e.g. http://127.0.0.1:8899).
    #[arg(long, default_value = "http://127.0.0.1:8899")]
    pub daemon: String,

    /// OS user owning the session directory.
    #[arg(long)]
    pub os_user: String,

    /// Per-session hook authentication token.
    #[arg(long)]
    pub hook_token: String,

    /// Claude Code hook event name (e.g. SessionStart, Stop).
    #[arg(long)]
    pub event: String,
}

pub async fn run_session_hook(args: SessionHookArgs) {
    if let Err(e) = try_run_session_hook(args).await {
        eprintln!("[session-hook] error (ignored, fail-quiet): {e}");
    }
    // Always exit 0 — never block Claude Code.
}

async fn try_run_session_hook(args: SessionHookArgs) -> anyhow::Result<()> {
    // Read stdin (hook event JSON). Done in a blocking task to avoid blocking the executor.
    let stdin_buf = tokio::task::spawn_blocking(|| {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map(|_| buf)
            .map_err(|e| anyhow::anyhow!("read stdin: {e}"))
    })
    .await
    .map_err(|e| anyhow::anyhow!("join error: {e}"))??;

    // Parse the hook event to get notification_type (may differ from --event for Notification).
    let notification_type = parse_hook_event(&stdin_buf)
        .ok()
        .and_then(|ev| ev.notification_type);

    // Map event → activity status. None = no-op, exit 0 without calling daemon.
    let Some(status) = activity_status_from_hook(&args.event, notification_type.as_deref()) else {
        return Ok(());
    };

    // Build and encode the protobuf request.
    let req = ReportSessionStatusRequest {
        session_id: args.session.clone(),
        hook_token: args.hook_token.clone(),
        os_user: args.os_user.clone(),
        status: status.as_wire().to_string(),
    };
    let body = req.encode_to_vec();

    // POST via Connect protocol (async reqwest with 2-second timeout).
    let url = format!(
        "{}/rpc/connection.ConnectionService/ReportSessionStatus",
        args.daemon.trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
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

    Ok(())
}
