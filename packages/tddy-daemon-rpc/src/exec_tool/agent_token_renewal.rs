//! The daemon's answer to an agent renewing its own session token
//! ([`tddy_session_tool_client::session_token::RENEW_AGENT_SESSION_TOKEN_TOOL`]).
//!
//! It is not a tool: no listing offers it, and it is answered here, before routing, so it is never
//! forwarded to a peer. Only the daemon that signed a token renews it
//! ([`tddy_daemon_auth::SessionTokens::renew_agent`]).

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_rpc::{Request, RequestTransport, Status};
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};

use super::ExecToolRpcHandler;

/// Whether `tool_name` is the reserved token-renewal call this module answers before anything
/// routes it (`ports.rs`).
pub(super) fn is_reserved_renewal_call(tool_name: &str) -> bool {
    tool_name == tddy_session_tool_client::session_token::RENEW_AGENT_SESSION_TOKEN_TOOL
}

impl ExecToolRpcHandler {
    pub(super) fn renew_agent_session_token(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<ExecuteToolResponse, Status> {
        let arrived_from_a_peer = request.metadata().transport() == RequestTransport::LiveKit;
        let req = request.into_inner();
        let local = local_instance_id_for_config(&self.config);
        if arrived_from_a_peer {
            return Err(Status::permission_denied(format!(
                "{} is answered only to an agent of daemon {local}, never over the common room",
                req.tool_name
            )));
        }
        if !(req.daemon_instance_id.is_empty() || req.daemon_instance_id == local) {
            return Err(Status::permission_denied(format!(
                "{} is answered only by the daemon that signed the token; this is {local}, the call \
                 names {}",
                req.tool_name, req.daemon_instance_id
            )));
        }
        let tokens = self
            .project_home_host
            .session_tokens
            .as_ref()
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "daemon {local} signs no session tokens, so it cannot renew one"
                ))
            })?;
        let renewed = tokens
            .renew_agent(&req.session_token, &req.session_id)
            .map_err(|refusal| {
                log::warn!(
                    "session {session}: agent token renewal refused on daemon {local}: {refusal}",
                    session = req.session_id
                );
                Status::unauthenticated(format!(
                    "daemon {local} refused to renew the session token: {refusal}"
                ))
            })?;
        log::info!(
            "session {session}: agent token renewed on daemon {local}, now expiring at unix {at}",
            session = req.session_id,
            at = renewed.expires_at_unix
        );
        Ok(ExecuteToolResponse {
            result_json: serde_json::json!({
                "session_token": renewed.session_token,
                "expires_at_unix": renewed.expires_at_unix,
            })
            .to_string(),
            ..Default::default()
        })
    }
}
