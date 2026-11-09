use crate::permission_records::{deferred_request_record, record_answer};
use crate::session::settle;
use crate::session::turn_rules::answer_delivered_while_held;
use crate::session::turn_rules::new_prompt;
use crate::session::turn_rules::refuse_loop_only;
use crate::session::CodingAgentSession;
use crate::session::InFlightPrompt;
use crate::session::Settled;
use async_trait::async_trait;
use std::sync::Arc;
use tddy_agent_backend::AcpUpdate;
use tddy_discovery::openai::TokenUsage;
use tddy_discovery::subagent::{
    PermissionAnswering, PromptOutcome, StopReason, SubagentError, SubagentSession, TurnCancelling,
    TurnRequest,
};
use tokio::sync::broadcast;

#[async_trait]
impl SubagentSession for CodingAgentSession {
    async fn take_turn(&mut self, request: TurnRequest) -> Result<PromptOutcome, SubagentError> {
        refuse_loop_only(&request)?;
        let answered = self.deliver_main_agent_answer(&request)?;
        let in_flight = Arc::clone(&self.in_flight);
        let mut slot = match answered {
            // Not waited on. A turn holding the slot is the one waiting for this answer, so waiting
            // behind it would never return; the answer is already with the coding agent.
            Some(request_id) => in_flight
                .try_lock()
                .map_err(|_| answer_delivered_while_held(request_id))?,
            None => in_flight.lock().await,
        };
        let mut flight = match slot.take() {
            // A turn a cancel drove to its end is over, not open: a new prompt closes it into the
            // history and starts the next turn. Its cancelled outcome was the cancel's to report.
            Some(ended) if ended.result.is_some() && answered.is_none() && request.prompt_text().is_some() => {
                self.close_cancelled_turn(ended);
                let prompt = new_prompt(&request)?;
                self.begin_prompt(prompt)
            }
            Some(flight) => {
                if answered.is_none() {
                    match self.resume(&request) {
                        Ok(Some(held)) => (flight.sink)(deferred_request_record(&held)),
                        Ok(None) => {}
                        Err(refusal) => {
                            *slot = Some(flight);
                            return Err(refusal);
                        }
                    }
                }
                flight
            }
            None => match answered {
                Some(request_id) => {
                    return Err(SubagentError::from(format!(
                        "permission request '{request_id}' was answered, but no prompt is in flight to continue"
                    )))
                }
                None => {
                    let prompt = new_prompt(&request)?;
                    self.begin_prompt(prompt)
                }
            },
        };
        // The answer this call delivered is recorded on the stream by the turn it continues. A turn holding a
        // deferred request records that answer itself, when it is given (see `settle`).
        if let Some(request_id) = answered {
            record_answer(&flight.sink, &self.permissions, request_id);
        }
        match settle(
            &mut flight,
            &self.permissions,
            request.permission_deferred_to_user(),
        )
        .await
        {
            Settled::Paused(pending) => {
                *slot = Some(flight);
                let mut outcome = PromptOutcome::new(
                    StopReason::PermissionRequested,
                    Vec::new(),
                    TokenUsage::default(),
                );
                outcome.permission_request = Some(pending);
                Ok(outcome)
            }
            Settled::Ended(end) => {
                drop(slot);
                let InFlightPrompt {
                    prompt, updates, ..
                } = flight;
                let updates =
                    std::mem::take(&mut *updates.lock().unwrap_or_else(|e| e.into_inner()));
                self.finish(&prompt, updates, end?)
            }
        }
    }

    async fn prompt(&mut self, text: &str) -> Result<PromptOutcome, SubagentError> {
        self.take_turn(TurnRequest::prompting(text)).await
    }

    fn model(&self) -> &str {
        &self.model
    }

    /// ACP reports no token usage, so there is nothing to total.
    fn cumulative_usage(&self) -> TokenUsage {
        TokenUsage::default()
    }

    /// ACP reports no context occupancy.
    fn context_tokens(&self) -> u64 {
        0
    }

    fn tail(&self, max_messages: usize) -> Vec<String> {
        let skip = self.history.len().saturating_sub(max_messages);
        self.history[skip..].to_vec()
    }

    fn updates(&self) -> Option<broadcast::Receiver<AcpUpdate>> {
        Some(self.update_stream.subscribe())
    }

    fn permission_answerer(&self) -> Option<Arc<dyn PermissionAnswering>> {
        Some(self.permission_answering())
    }

    fn turn_canceller(&self) -> Option<Arc<dyn TurnCancelling>> {
        Some(Arc::new(self.cancel_handle()))
    }
}
