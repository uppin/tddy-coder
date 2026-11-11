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

    /// The conversation's totals under its agent's usage binding; zero when none is attached.
    fn cumulative_usage(&self) -> TokenUsage {
        self.usage_reading
            .as_ref()
            .map(crate::session::UsageReading::totals)
            .unwrap_or_default()
    }

    /// TODO(usage-binding): context occupancy is the last record's *input* (its input and cache
    /// counters together), and a binding aggregates whole counters only — `aggregate: last` was found
    /// not to answer it (`#usage-binding 4/5`); it needs a per-record input read. Zero until then.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::UsageReading;
    use tddy_agent_backend::HeadlessCommand;
    use tddy_usage::{UsageContext, UsageSection};

    const A_TAPPED_BINDING: &str = r#"
sources:
  - format: jsonl
    main:
      path: "{session_dir}/usage/{conversation_id}.jsonl"
    record:
      where: [{ field: type, equals: result }]
      aggregate: sum
      tokens: { input: usage.input_tokens, output: usage.output_tokens }
"#;

    async fn a_session_reading(
        session_dir: &std::path::Path,
        conversation_id: &str,
    ) -> CodingAgentSession {
        let section: UsageSection = serde_yaml::from_str(A_TAPPED_BINDING).expect("a binding");
        let reading = UsageReading {
            section,
            ctx: UsageContext {
                agent_home: None,
                session_dir: session_dir.to_path_buf(),
                conversation_id: Some(conversation_id.to_string()),
                worktree: None,
                agent_label: "an-agent".to_string(),
                fallback_model: String::new(),
            },
        };
        CodingAgentSession::start_headless(HeadlessCommand::default(), "a-model".to_string())
            .await
            .expect("a headless session")
            .with_usage_reading(reading)
    }

    #[tokio::test]
    async fn cumulative_usage_reads_the_tapped_totals() {
        // Given — two tapped turns of the conversation
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("usage")).unwrap();
        std::fs::write(
            dir.path().join("usage").join("conv-1.jsonl"),
            concat!(
                r#"{"type":"result","usage":{"input_tokens":100,"output_tokens":10}}"#,
                "\n",
                r#"{"type":"result","usage":{"input_tokens":50,"output_tokens":5}}"#,
                "\n"
            ),
        )
        .unwrap();
        let session = a_session_reading(dir.path(), "conv-1").await;

        // When
        let usage = session.cumulative_usage();

        // Then
        assert_eq!(
            usage,
            TokenUsage {
                input_tokens: 150,
                output_tokens: 15
            }
        );
    }

    #[tokio::test]
    async fn a_conversation_that_has_tapped_nothing_yet_reports_zero() {
        let dir = tempfile::tempdir().unwrap();
        let session = a_session_reading(dir.path(), "conv-1").await;

        assert_eq!(session.cumulative_usage(), TokenUsage::default());
    }
}
