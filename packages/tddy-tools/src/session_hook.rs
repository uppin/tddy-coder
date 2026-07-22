//! `session-hook` subcommand: report granular session activity to the daemon.
//!
//! Claude Code invokes this binary as a hook (stdin = hook event JSON). The subcommand issues up
//! to two independent, fail-quiet daemon reports:
//! 1. **`ReportSessionStatus`** — maps the event to a [`SessionActivityStatus`] via
//!    [`activity_status_from_hook`]; skipped when the mapping is `None` (no-op event).
//! 2. **`ReportAgentActivity`** — for `PreToolUse` / `PostToolUse` events, carries the tool
//!    payload (name, input, response) so claude-cli sessions populate `agent-activity.jsonl`;
//!    skipped for every other event.
//!
//! **Fail-quiet contract**: any error (parse, network, daemon rejection) is printed to stderr and
//! swallowed — the process always exits 0. Claude Code must never be blocked by a failing hook,
//! and a failure in one report must not prevent the other.

use clap::Args;
use prost::Message as _;
use serde::Deserialize;
use std::io::Read;
use tddy_core::{activity_status_from_hook, parse_hook_event};
use tddy_service::proto::connection::{ReportAgentActivityRequest, ReportSessionStatusRequest};

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

    /// Claude Code hook event name (e.g. SessionStart, Stop). Optional for Cursor hooks that
    /// rely on stdin `hook_event_name` only.
    #[arg(long, default_value = "")]
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

    // Parse the hook event from stdin. Primary mapping uses `hook_event_name`; `--event` is a
    // backward-compat fallback for Claude hooks that still bake the flag into the command.
    let parsed = parse_hook_event(&stdin_buf).ok();
    let notification_type = parsed.as_ref().and_then(|ev| ev.notification_type.clone());
    let hook_event_name = parsed
        .as_ref()
        .map(|ev| ev.hook_event_name.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(args.event.as_str());

    // Agent-activity report (PreToolUse / PostToolUse only). Independent of the status report: a
    // failure here is logged and swallowed so it never blocks the status report or Claude Code.
    if let Err(e) = report_agent_activity(&args, hook_event_name, &stdin_buf).await {
        eprintln!("[session-hook] agent-activity report failed (ignored, fail-quiet): {e}");
    }

    report_session_status(&args, hook_event_name, notification_type.as_deref()).await
}

/// POST a `ReportSessionStatus` RPC for the mapped activity status.
///
/// Returns `Ok(())` without contacting the daemon when the event maps to no status (no-op event).
async fn report_session_status(
    args: &SessionHookArgs,
    hook_event_name: &str,
    notification_type: Option<&str>,
) -> anyhow::Result<()> {
    // Map event → activity status. None = no-op, return without calling the daemon.
    let Some(status) = activity_status_from_hook(hook_event_name, notification_type) else {
        return Ok(());
    };

    let req = ReportSessionStatusRequest {
        session_id: args.session.clone(),
        hook_token: args.hook_token.clone(),
        os_user: args.os_user.clone(),
        status: status.as_wire().to_string(),
    };

    post_proto(&args.daemon, "ReportSessionStatus", req.encode_to_vec()).await
}

/// POST a `ReportAgentActivity` RPC carrying the tool payload of a tool-use hook.
///
/// Returns `Ok(())` without contacting the daemon for non-tool events (the mapping yields `None`).
async fn report_agent_activity(
    args: &SessionHookArgs,
    hook_event_name: &str,
    stdin_json: &str,
) -> anyhow::Result<()> {
    let Some(req) = agent_activity_request(
        hook_event_name,
        stdin_json,
        &args.session,
        &args.hook_token,
        &args.os_user,
    ) else {
        return Ok(());
    };

    post_proto(&args.daemon, "ReportAgentActivity", req.encode_to_vec()).await
}

/// POST an encoded protobuf request to a daemon `ConnectionService` method via the Connect
/// protocol (async reqwest with a 2-second timeout).
async fn post_proto(daemon: &str, method: &str, body: Vec<u8>) -> anyhow::Result<()> {
    let url = format!(
        "{}/rpc/connection.ConnectionService/{method}",
        daemon.trim_end_matches('/')
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

/// Tool-payload subset of the Claude Code hook stdin JSON.
///
/// Parsed separately from [`tddy_core::HookEvent`] on purpose: `HookEvent` derives `Eq`, which
/// `serde_json::Value` does not implement, so these dynamic fields cannot live there. Every field
/// is optional — the event type determines which are present (`tool_response` only on
/// `PostToolUse`).
#[derive(Debug, Default, Deserialize)]
struct HookToolPayload {
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Option<serde_json::Value>,
    #[serde(default)]
    tool_response: Option<serde_json::Value>,
}

/// Pure mapping: Claude Code tool-use hook stdin JSON → a [`ReportAgentActivityRequest`].
///
/// Returns `None` for any event other than `PreToolUse` / `PostToolUse` (they carry no tool
/// payload, so nothing is reported). `input_json` is the `tool_input` object serialized to a JSON
/// string; `result_json` is the `tool_response` serialized (empty on `PreToolUse`, which has no
/// response); `is_error` / `error_message` are derived from the `tool_response`.
fn agent_activity_request(
    hook_event_name: &str,
    stdin_json: &str,
    session_id: &str,
    hook_token: &str,
    os_user: &str,
) -> Option<ReportAgentActivityRequest> {
    if hook_event_name != "PreToolUse" && hook_event_name != "PostToolUse" {
        return None;
    }

    // A malformed payload degrades to empty fields rather than failing the hook.
    let payload: HookToolPayload = serde_json::from_str(stdin_json).unwrap_or_default();

    let (is_error, error_message) = tool_response_error(payload.tool_response.as_ref());
    let input_json = payload
        .tool_input
        .map(|v| v.to_string())
        .unwrap_or_default();
    let result_json = payload
        .tool_response
        .map(|v| v.to_string())
        .unwrap_or_default();

    Some(ReportAgentActivityRequest {
        session_id: session_id.to_string(),
        hook_token: hook_token.to_string(),
        os_user: os_user.to_string(),
        event: hook_event_name.to_string(),
        tool_name: payload.tool_name.unwrap_or_default(),
        input_json,
        result_json,
        is_error,
        error_message,
    })
}

/// Derive `(is_error, error_message)` from a Claude Code `tool_response`.
///
/// Claude Code has no single standardized error schema; the common convention is a response object
/// carrying a non-empty `error` string. Anything else is treated as a success.
fn tool_response_error(tool_response: Option<&serde_json::Value>) -> (bool, String) {
    match tool_response
        .and_then(|v| v.get("error"))
        .and_then(|e| e.as_str())
        .filter(|s| !s.is_empty())
    {
        Some(err) => (true, err.to_string()),
        None => (false, String::new()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION_ID: &str = "sess-1";
    const HOOK_TOKEN: &str = "tok-1";
    const OS_USER: &str = "alice";

    /// Map a hook stdin JSON to an agent-activity request with the standard test identity.
    fn map(hook_event_name: &str, stdin_json: &str) -> Option<ReportAgentActivityRequest> {
        agent_activity_request(hook_event_name, stdin_json, SESSION_ID, HOOK_TOKEN, OS_USER)
    }

    /// A `PreToolUse` hook carries `tool_name` + `tool_input`, so it maps to a request whose
    /// `input_json` is the serialized tool input and whose `result_json` is empty (no response
    /// yet).
    #[test]
    fn pre_tool_use_maps_tool_input_with_empty_result() {
        // Given
        let stdin_json = r#"{"hook_event_name":"PreToolUse","tool_name":"Read","tool_input":{"path":"src/main.rs"}}"#;

        // When
        let req = map("PreToolUse", stdin_json).expect("PreToolUse must produce a request");

        // Then
        assert_eq!(req.event, "PreToolUse");
        assert_eq!(req.tool_name, "Read");
        assert_eq!(req.input_json, r#"{"path":"src/main.rs"}"#);
        assert_eq!(req.result_json, "");
        assert!(!req.is_error);
        assert_eq!(req.error_message, "");
        assert_eq!(req.session_id, SESSION_ID);
        assert_eq!(req.hook_token, HOOK_TOKEN);
        assert_eq!(req.os_user, OS_USER);
    }

    /// A `PostToolUse` hook carries `tool_response`, so it maps to a request whose `result_json`
    /// is the serialized response.
    #[test]
    fn post_tool_use_maps_tool_response_into_result_json() {
        // Given
        let stdin_json = r#"{"hook_event_name":"PostToolUse","tool_name":"Read","tool_input":{"path":"src/main.rs"},"tool_response":{"content":"fn main() {}"}}"#;

        // When
        let req = map("PostToolUse", stdin_json).expect("PostToolUse must produce a request");

        // Then
        assert_eq!(req.event, "PostToolUse");
        assert_eq!(req.tool_name, "Read");
        assert_eq!(req.result_json, r#"{"content":"fn main() {}"}"#);
        assert!(!req.is_error);
        assert_eq!(req.error_message, "");
    }

    /// A `PostToolUse` response carrying a non-empty `error` string is reported as an error.
    #[test]
    fn post_tool_use_with_error_response_is_reported_as_error() {
        // Given
        let stdin_json = r#"{"hook_event_name":"PostToolUse","tool_name":"Bash","tool_input":{"command":"false"},"tool_response":{"error":"command failed"}}"#;

        // When
        let req = map("PostToolUse", stdin_json).expect("PostToolUse must produce a request");

        // Then
        assert!(req.is_error);
        assert_eq!(req.error_message, "command failed");
    }

    /// A non-tool event (e.g. `Stop`) carries no tool payload, so no agent-activity report is
    /// produced.
    #[test]
    fn non_tool_event_produces_no_request() {
        // Given
        let stdin_json = r#"{"hook_event_name":"Stop","session_id":"s"}"#;

        // When
        let req = map("Stop", stdin_json);

        // Then
        assert_eq!(
            req, None,
            "Stop is not a tool event and must not be reported"
        );
    }
}
