//! The requests the worker serves on one agent connection: initialize once, open a session, prompt,
//! cancel and list models. Every one runs on the worker's `LocalSet`, beside the connection's I/O.

use std::future::Future;
use std::sync::Arc;

use agent_client_protocol::{self as acp, Agent as _};
use tokio::sync::OnceCell;

use super::client::Turns;
use super::mapping;
use super::{
    AcpAgentHooks, AcpSession, AcpTurnEnd, AcpUpdateSink, OpenSession, SessionErrorVerdict,
};
use crate::backend::{BackendModels, ProgressSink};
use crate::error::BackendError;

/// The connection to one agent, served on the worker's `LocalSet`.
pub(super) struct AgentConnection {
    conn: acp::ClientSideConnection,
    turns: Turns,
    hooks: Arc<dyn AcpAgentHooks>,
    initialized: OnceCell<()>,
}

impl AgentConnection {
    pub(super) fn new(
        conn: acp::ClientSideConnection,
        turns: Turns,
        hooks: Arc<dyn AcpAgentHooks>,
    ) -> Self {
        Self {
            conn,
            turns,
            hooks,
            initialized: OnceCell::new(),
        }
    }

    /// `initialize`, once. Requests that arrive before it completes wait for that one call.
    async fn ensure_initialized(&self) -> Result<(), BackendError> {
        self.initialized
            .get_or_try_init(|| async {
                let request = acp::InitializeRequest::new(acp::ProtocolVersion::V1).client_info(
                    acp::Implementation::new("tddy-coder", env!("CARGO_PKG_VERSION"))
                        .title("TDDY Coder"),
                );
                self.conn
                    .initialize(request)
                    .await
                    .map(|_| ())
                    .map_err(|e| {
                        BackendError::InvocationFailed(format!("ACP initialize failed: {e}"))
                    })
            })
            .await
            .map(|_| ())
    }

    /// `session/load` for a resumed session, else `session/new`; then `session/set_model` when the
    /// request names a model the agent offers.
    pub(super) async fn open_session(&self, open: OpenSession) -> Result<AcpSession, BackendError> {
        self.ensure_initialized().await?;
        let OpenSession {
            cwd,
            resume,
            model,
            mcp_servers,
        } = open;
        let mcp_servers: Vec<acp::McpServer> =
            mcp_servers.into_iter().map(mapping::mcp_server).collect();
        let (session_id, models) = match resume {
            Some(session_id) => {
                let response = self
                    .with_one_retry("load_session", || {
                        self.conn.load_session(
                            acp::LoadSessionRequest::new(session_id.clone(), cwd.clone())
                                .mcp_servers(mcp_servers.clone()),
                        )
                    })
                    .await?;
                (session_id, response.models)
            }
            None => {
                let response = self
                    .with_one_retry("new_session", || {
                        self.conn.new_session(
                            acp::NewSessionRequest::new(cwd.clone())
                                .mcp_servers(mcp_servers.clone()),
                        )
                    })
                    .await?;
                (response.session_id.0.to_string(), response.models)
            }
        };

        let (offered, current) = mapping::offered_models(models.as_ref());
        let current_model = match model {
            Some(requested) => {
                check_offered(&requested, &offered)?;
                self.conn
                    .set_session_model(acp::SetSessionModelRequest::new(
                        session_id.clone(),
                        requested.clone(),
                    ))
                    .await
                    .map_err(|e| {
                        BackendError::InvocationFailed(format!("ACP set_session_model failed: {e}"))
                    })?;
                Some(requested)
            }
            None => current,
        };
        Ok(AcpSession {
            session_id,
            available_models: offered,
            current_model,
        })
    }

    /// One `session/prompt`. Its updates reach the turn's sinks as they arrive.
    pub(super) async fn prompt(
        &self,
        session_id: String,
        prompt: String,
        updates: Option<AcpUpdateSink>,
        progress: Option<ProgressSink>,
    ) -> Result<AcpTurnEnd, BackendError> {
        self.ensure_initialized().await?;
        self.turns.begin(&session_id, updates, progress)?;
        let answer = self
            .conn
            .prompt(acp::PromptRequest::new(
                session_id.clone(),
                vec![prompt.into()],
            ))
            .await;
        let text = self.turns.end(&session_id);
        let response = answer
            .map_err(|e| BackendError::InvocationFailed(format!("ACP prompt failed: {e}")))?;
        Ok(AcpTurnEnd {
            stop_reason: mapping::stop_reason(response.stop_reason)?,
            text,
        })
    }

    /// `session/cancel`. The turn it names ends when the agent answers its prompt.
    pub(super) async fn cancel(&self, session_id: String) -> Result<(), BackendError> {
        self.ensure_initialized().await?;
        self.conn
            .cancel(acp::CancelNotification::new(session_id))
            .await
            .map_err(|e| BackendError::InvocationFailed(format!("ACP cancel failed: {e}")))
    }

    /// The models a fresh session in the current directory offers.
    pub(super) async fn list_models(&self) -> Result<BackendModels, BackendError> {
        self.ensure_initialized().await?;
        let cwd = std::env::current_dir().map_err(|e| {
            BackendError::InvocationFailed(format!("no working directory to list models in: {e}"))
        })?;
        let response = self
            .conn
            .new_session(acp::NewSessionRequest::new(cwd))
            .await
            .map_err(|e| BackendError::InvocationFailed(format!("ACP new_session failed: {e}")))?;
        crate::backend::acp_models_from_session_state(response.models.as_ref())
    }

    /// Run one session call. When it fails, the hooks may recover (Codex runs `codex login`), and
    /// the call is retried once; a second failure is the error.
    async fn with_one_retry<T, Fut>(
        &self,
        call: &str,
        mut attempt: impl FnMut() -> Fut,
    ) -> Result<T, BackendError>
    where
        Fut: Future<Output = acp::Result<T>>,
    {
        let first = match attempt().await {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };
        if self.hooks.on_session_error(&first.to_string()).await == SessionErrorVerdict::RetryOnce {
            return attempt().await.map_err(|e| session_failed(call, &e));
        }
        Err(session_failed(call, &first))
    }
}

/// A requested model must be one the agent offers. Nothing is set otherwise, and the error names
/// what is offered.
fn check_offered(requested: &str, offered: &[String]) -> Result<(), BackendError> {
    if offered.iter().any(|model| model == requested) {
        return Ok(());
    }
    let listed = if offered.is_empty() {
        "none".to_string()
    } else {
        offered.join(", ")
    };
    Err(BackendError::InvocationFailed(format!(
        "model {requested} is not offered by the agent (offered: {listed})"
    )))
}

fn session_failed(call: &str, error: &acp::Error) -> BackendError {
    BackendError::InvocationFailed(format!("ACP {call} failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_the_agent_offers_is_accepted() {
        // Given
        let offered = vec!["model-a".to_string(), "model-b".to_string()];

        // When / Then
        assert!(check_offered("model-b", &offered).is_ok());
    }

    #[test]
    fn a_model_the_agent_does_not_offer_is_refused_naming_the_offered_ones() {
        // Given
        let offered = vec!["model-a".to_string(), "model-b".to_string()];

        // When
        let refusal = check_offered("model-z", &offered).expect_err("refused");

        // Then
        let message = refusal.to_string();
        assert!(message.contains("model-z"), "{message}");
        assert!(message.contains("model-a, model-b"), "{message}");
    }

    #[test]
    fn a_model_is_refused_when_the_agent_offers_none() {
        // When
        let refusal = check_offered("model-a", &[]).expect_err("refused");

        // Then
        assert!(refusal.to_string().contains("(offered: none)"));
    }
}
