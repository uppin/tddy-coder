use crate::connection_service::svc_close_session::close_task::CloseRun;

use tddy_rpc::Request;
use tddy_rpc::Status;
use tddy_service::proto::session::CloseSessionEvent;
use tddy_service::proto::session::{close_session_event, CloseSessionRequest};
use tddy_session_agents::peer_session_answer::peer_has_no_such_session;
use tokio::sync::mpsc::UnboundedReceiver;

impl CloseRun {
    /// Close the workspace session holding a split session's checkout on its codebase daemon,
    /// confirming the preview that daemon gave. A daemon that no longer has the session has no
    /// checkout left to remove.
    pub(super) async fn close_codebase(
        &self,
        daemon_instance_id: &str,
        session_id: &str,
        fingerprint: &str,
        here: bool,
    ) -> Result<String, String> {
        let request = CloseSessionRequest {
            session_token: self.ctx.session_token.clone(),
            session_id: session_id.to_string(),
            confirmed: true,
            confirmed_fingerprint: fingerprint.to_string(),
        };
        let events = if here {
            Box::pin(self.host.close_session_at_host(Request::direct(request)))
                .await
                .map(|stream| CodebaseEvents::Here(stream.into_inner()))
        } else {
            self.host
                .forward_close(daemon_instance_id, &request)
                .await
                .map(CodebaseEvents::Peer)
        };
        let ended = match events {
            Ok(events) => events.terminal().await,
            Err(refusal) => Err(refusal),
        };
        match ended {
            Ok(close_session_event::Event::Outcome(outcome)) if outcome.ok => {
                Ok(format!("closed on daemon {daemon_instance_id}"))
            }
            Ok(close_session_event::Event::Outcome(outcome)) => Err(format!(
                "the close on daemon {daemon_instance_id} did not finish: {}",
                outcome
                    .failed_steps
                    .iter()
                    .map(|step| format!("{} — {}", step.subject, step.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            )),
            Ok(_) => Err(format!(
                "the checkout on daemon {daemon_instance_id} changed since it was checked; \
                 close the session again"
            )),
            Err(refusal) if peer_has_no_such_session(&refusal) => Ok(format!(
                "daemon {daemon_instance_id} no longer has workspace session {session_id}"
            )),
            Err(refusal) => Err(refusal.message().to_string()),
        }
    }
}

/// The events of a codebase daemon's close: this daemon's own stream, or the peer's relay.
enum CodebaseEvents {
    Here(crate::CloseSessionEventStream),
    Peer(UnboundedReceiver<Result<CloseSessionEvent, Status>>),
}

impl CodebaseEvents {
    /// Read to the close's terminal event. A stream that ends without one, or with an error, is
    /// the error.
    async fn terminal(self) -> Result<close_session_event::Event, Status> {
        let mut last = None;
        match self {
            CodebaseEvents::Here(mut stream) => {
                while let Some(item) = futures_util::StreamExt::next(&mut stream).await {
                    last = item?.event;
                }
            }
            CodebaseEvents::Peer(mut events) => {
                while let Some(item) = events.recv().await {
                    last = item?.event;
                }
            }
        }
        match last {
            Some(
                event @ (close_session_event::Event::Outcome(_)
                | close_session_event::Event::NeedsConfirmation(_)),
            ) => Ok(event),
            _ => Err(Status::internal(
                "the codebase daemon's close ended without saying how",
            )),
        }
    }
}
