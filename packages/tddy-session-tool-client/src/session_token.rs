//! Keeping a session agent's own token alive for as long as its `tddy-tools --mcp` runs.
//!
//! The daemon hands the agent a token in `TDDY_REMOTE_SESSION_TOKEN` that lives
//! `tddy_github::AGENT_TOKEN_TTL` (24 h). Nothing used to replace it, so an agent that outlived it
//! lost every tool at once. An orchestrator has no shell, so that was a dead end (session
//! `01a20f10`, 2026-11-27). This module renews the token over the daemon route while it is still
//! valid, and every later call carries the renewed one ([`current_session_token`]).
//!
//! Renewal goes to the agent's **own** daemon, the one that signed the token. The request names no
//! `daemon_instance_id`, so nothing forwards it: a codebase host cannot renew a token another host
//! issued.

use std::sync::RwLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{detect_session_tool_transport, SessionToolEnvelope, SessionToolTransport};

/// The reserved exec-tool name the daemon answers with a renewed agent token. It is not a tool: no
/// listing offers it, and the daemon answers it before routing (`tddy-daemon-rpc`'s
/// `ExecToolRpcHandler::execute_tool`).
pub const RENEW_AGENT_SESSION_TOKEN_TOOL: &str = "tddy.token.RenewAgentSession";

/// Renew once less than this much life is left. A quarter of the 24 h TTL leaves room for a laptop
/// that sleeps through several checks.
pub const RENEWAL_MARGIN: Duration = Duration::from_secs(6 * 3600);

/// How often the renewal loop checks.
pub const RENEWAL_CHECK_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// What one renewal check did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenewalOutcome {
    /// The token has more than the margin left.
    NotDue,
    /// The daemon issued a new token, which every later call now carries.
    Renewed { expires_at_unix: u64 },
    /// This process reaches no daemon that issues agent tokens: a sandbox socket (authenticated by
    /// the connection) or a room transport.
    NotRenewable,
}

static RENEWED_TOKEN: RwLock<Option<String>> = RwLock::new(None);

/// The token tool calls carry: the latest renewal's, else the one the agent was spawned with.
pub fn current_session_token() -> String {
    RENEWED_TOKEN
        .read()
        .ok()
        .and_then(|renewed| renewed.clone())
        .unwrap_or_else(|| std::env::var("TDDY_REMOTE_SESSION_TOKEN").unwrap_or_default())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// Renew the token when `expires_at_unix` is less than `margin` away, or unknown.
pub async fn renew_session_token_if_due(
    expires_at_unix: Option<u64>,
    margin: Duration,
) -> Result<RenewalOutcome, String> {
    if let Some(expiry) = expires_at_unix {
        if expiry > now_unix() + margin.as_secs() {
            return Ok(RenewalOutcome::NotDue);
        }
    }
    let envelope_for = |session_id: String, session_token: String| SessionToolEnvelope {
        session_id,
        session_token,
        daemon_instance_id: String::new(),
    };
    let args = serde_json::json!({});
    let answer = match detect_session_tool_transport() {
        Some(SessionToolTransport::DaemonUds {
            socket_path,
            session_id,
            session_token,
            ..
        }) => {
            crate::rpc_transports::dispatch_via_daemon_uds(
                &socket_path,
                &envelope_for(session_id, session_token),
                RENEW_AGENT_SESSION_TOKEN_TOOL,
                &args,
            )
            .await
        }
        Some(SessionToolTransport::DaemonHttp {
            session_id,
            daemon_url,
            session_token,
            ..
        }) => {
            crate::rpc_transports::dispatch_via_daemon_http(
                &daemon_url,
                &session_id,
                &session_token,
                "",
                RENEW_AGENT_SESSION_TOKEN_TOOL,
                &args,
            )
            .await
        }
        _ => return Ok(RenewalOutcome::NotRenewable),
    };
    let body: serde_json::Value = serde_json::from_str(&answer)
        .map_err(|e| format!("token renewal answered something that is not JSON: {e}"))?;
    let (Some(token), Some(expires_at_unix)) = (
        body.get("session_token").and_then(|v| v.as_str()),
        body.get("expires_at_unix").and_then(|v| v.as_u64()),
    ) else {
        return Err(format!("token renewal refused: {answer}"));
    };
    *RENEWED_TOKEN
        .write()
        .map_err(|_| "the renewed-token slot is poisoned".to_string())? = Some(token.to_string());
    Ok(RenewalOutcome::Renewed { expires_at_unix })
}

/// Renew this process's agent token for as long as it runs: once at start, when its expiry is not
/// yet known, then whenever less than [`RENEWAL_MARGIN`] is left. A failed renewal is logged and
/// retried at the next check. The loop ends when the process reaches no daemon that renews.
pub fn spawn_session_token_renewal() -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut expires_at_unix = None;
        let mut checks = tokio::time::interval(RENEWAL_CHECK_INTERVAL);
        loop {
            checks.tick().await;
            match renew_session_token_if_due(expires_at_unix, RENEWAL_MARGIN).await {
                Ok(RenewalOutcome::NotDue) => {}
                Ok(RenewalOutcome::Renewed {
                    expires_at_unix: at,
                }) => {
                    log::info!("session token renewed; it now expires at unix {at}");
                    expires_at_unix = Some(at);
                }
                Ok(RenewalOutcome::NotRenewable) => {
                    log::debug!("no daemon route renews this session's token; not renewing");
                    return;
                }
                Err(e) => log::warn!("session token renewal failed, retrying next check: {e}"),
            }
        }
    })
}
