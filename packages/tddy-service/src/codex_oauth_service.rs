//! LiveKit / Connect-RPC `CodexOAuthService`: relay OAuth callback to Codex loopback (Variant A).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};

use crate::codex_oauth_scan::CodexOAuthDetected;
use crate::codex_oauth_validate::codex_callback_url;
use crate::proto::codex_oauth::{
    CodexOAuthService as CodexOAuthServiceTrait, DeliverCallbackRequest, DeliverCallbackResponse,
};

/// Pending OAuth session on the agent host (memory only).
#[derive(Debug, Clone)]
pub struct CodexOAuthPending {
    pub detected: CodexOAuthDetected,
}

/// Shared state between terminal output scanner and `DeliverCallback`.
#[derive(Default)]
pub struct CodexOAuthSessionState {
    pub pending: Option<CodexOAuthPending>,
}

pub type CodexOAuthSession = Arc<Mutex<CodexOAuthSessionState>>;

pub struct CodexOAuthServiceImpl {
    session: CodexOAuthSession,
    /// When set, successful callback clears LiveKit participant metadata for UIs.
    metadata_tx: Option<tokio::sync::watch::Sender<String>>,
}

impl CodexOAuthServiceImpl {
    pub fn new(session: CodexOAuthSession) -> Self {
        Self {
            session,
            metadata_tx: None,
        }
    }

    pub fn with_metadata_watch(
        session: CodexOAuthSession,
        metadata_tx: tokio::sync::watch::Sender<String>,
    ) -> Self {
        Self {
            session,
            metadata_tx: Some(metadata_tx),
        }
    }
}

#[async_trait]
impl CodexOAuthServiceTrait for CodexOAuthServiceImpl {
    async fn deliver_callback(
        &self,
        request: Request<DeliverCallbackRequest>,
    ) -> Result<Response<DeliverCallbackResponse>, Status> {
        let req = request.into_inner();
        let code = req.code.trim();
        let state = req.state.trim();
        if code.is_empty() || state.is_empty() {
            return Ok(Response::new(DeliverCallbackResponse {
                success: false,
                error_message: "missing code or state".into(),
            }));
        }

        let port = {
            let g = self
                .session
                .lock()
                .map_err(|e| Status::internal(e.to_string()))?;
            let Some(p) = g.pending.as_ref() else {
                return Ok(Response::new(DeliverCallbackResponse {
                    success: false,
                    error_message: "no pending OAuth on this participant".into(),
                }));
            };
            if !p.detected.state.is_empty() && p.detected.state != state {
                return Ok(Response::new(DeliverCallbackResponse {
                    success: false,
                    error_message: "state mismatch".into(),
                }));
            }
            p.detected.callback_port
        };

        let target = codex_callback_url(port, code, state).map_err(Status::invalid_argument)?;

        let url_clone = target.clone();
        let result = tokio::task::spawn_blocking(move || {
            reqwest::blocking::get(&url_clone).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

        match result {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(mut g) = self.session.lock() {
                        g.pending = None;
                    }
                    if let Some(tx) = &self.metadata_tx {
                        let _ = tx.send(r#"{"codex_oauth":{"pending":false}}"#.to_string());
                    }
                    Ok(Response::new(DeliverCallbackResponse {
                        success: true,
                        error_message: String::new(),
                    }))
                } else {
                    Ok(Response::new(DeliverCallbackResponse {
                        success: false,
                        error_message: format!("callback HTTP status {}", resp.status()),
                    }))
                }
            }
            Err(e) => Ok(Response::new(DeliverCallbackResponse {
                success: false,
                error_message: e,
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex_oauth_scan::CodexOAuthDetected;

    #[tokio::test]
    async fn deliver_callback_rejects_without_pending() {
        let s: CodexOAuthSession = Arc::new(Mutex::new(CodexOAuthSessionState::default()));
        let svc = CodexOAuthServiceImpl::new(s);
        let r = svc
            .deliver_callback(Request::new(DeliverCallbackRequest {
                code: "c".into(),
                state: "s".into(),
                session_id: String::new(),
            }))
            .await
            .unwrap()
            .into_inner();
        assert!(!r.success);
    }

    #[tokio::test]
    async fn deliver_callback_state_mismatch() {
        let s: CodexOAuthSession = Arc::new(Mutex::new(CodexOAuthSessionState {
            pending: Some(CodexOAuthPending {
                detected: CodexOAuthDetected {
                    authorize_url: "https://auth.openai.com/x".into(),
                    callback_port: 9,
                    state: "expect".into(),
                },
            }),
        }));
        let svc = CodexOAuthServiceImpl::new(s);
        let r = svc
            .deliver_callback(Request::new(DeliverCallbackRequest {
                code: "c".into(),
                state: "wrong".into(),
                session_id: String::new(),
            }))
            .await
            .unwrap()
            .into_inner();
        assert!(!r.success);
    }
}
