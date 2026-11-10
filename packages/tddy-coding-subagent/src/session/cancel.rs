use crate::parked_permissions::ParkedPermissions;
use crate::session::InFlightSlot;
use async_trait::async_trait;
use std::sync::Weak;
use std::time::Duration;
use tddy_agent_backend::ConversationDriver;
use tddy_discovery::subagent::{SubagentError, TurnCancelling};

use crate::session::InFlightPrompt;

/// How long [`CancelHandle::cancel`] waits for the cancelled turn to end. Past it the call returns
/// `Ok` and the turn is left to the session's drop, which stops the agent process, so an agent that
/// never ends its turn cannot hang `subagent_cancel`.
pub const CANCEL_SETTLE_DEADLINE: Duration = Duration::from_secs(10);

/// Cancels a [`CodingAgentSession`](super::CodingAgentSession)'s running turn. Every permission
/// request the turn is parked on is answered as cancelled first, then the turn is cancelled
/// (`session/cancel` over ACP). The call returns once the turn has ended, or after
/// [`CANCEL_SETTLE_DEADLINE`]; the turn then ends as cancelled. Cloneable and `Send`, so it can be
/// held apart from the session. It does not keep the agent process alive: once the session is
/// dropped there is nothing left to cancel.
#[derive(Clone)]
pub struct CancelHandle {
    pub(super) conversation: Weak<dyn ConversationDriver>,
    pub(super) permissions: Weak<ParkedPermissions>,
    pub(super) in_flight: Weak<InFlightSlot>,
}

impl CancelHandle {
    pub async fn cancel(&self) -> Result<(), SubagentError> {
        // The session owns all three; once it is gone there is no turn left to cancel.
        let (Some(conversation), Some(permissions), Some(in_flight)) = (
            self.conversation.upgrade(),
            self.permissions.upgrade(),
            self.in_flight.upgrade(),
        ) else {
            return Ok(());
        };
        permissions.cancel_pending();
        conversation
            .cancel_turn()
            .await
            .map_err(|e| SubagentError::from(format!("could not cancel the coding agent: {e}")))?;
        // A turn that is running holds the slot, so this waits for it to end. A turn paused on a
        // request is driven to its end here, and its result waits for the next turn to take.
        let mut slot = in_flight.lock().await;
        if let Some(flight) = slot.as_mut().filter(|flight| flight.result.is_none()) {
            drive_cancelled_turn_to_its_end(flight, &permissions).await;
        }
        Ok(())
    }
}

/// Wait, up to [`CANCEL_SETTLE_DEADLINE`], for a cancelled prompt to end, keeping its result for
/// the next turn to take. Past the deadline the prompt is left to the session's drop.
async fn drive_cancelled_turn_to_its_end(
    flight: &mut InFlightPrompt,
    permissions: &ParkedPermissions,
) {
    let waited = tokio::time::timeout(CANCEL_SETTLE_DEADLINE, async {
        loop {
            // A request the agent parks while it winds down is answered as cancelled too, so the
            // wait cannot stall on a question nothing will answer.
            permissions.cancel_pending();
            tokio::select! {
                end = flight.ended.as_mut() => break end,
                () = permissions.parked() => {}
            }
        }
    })
    .await;
    match waited {
        Ok(end) => flight.result = Some(end),
        Err(_) => log::warn!(
            "the coding agent did not end its cancelled turn within {CANCEL_SETTLE_DEADLINE:?}; \
             the turn is left to the session's drop"
        ),
    }
}
#[async_trait]
impl TurnCancelling for CancelHandle {
    async fn cancel_turn(&self) -> Result<(), SubagentError> {
        self.cancel().await
    }
}
