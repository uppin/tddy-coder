//! A coding subagent's permission requests, parked until the caller answers them.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard};

use tddy_agent_backend::{PermissionDecision, PermissionHandler, PermissionRequest};
use tddy_discovery::subagent::{
    PermissionAnswer, PermissionChoice, PermissionRequestView, PermissionToolCall,
};
use tokio::sync::{oneshot, Notify};

/// The [`PermissionHandler`] a coding-agent conversation installs: a request is parked — the
/// agent's `session/request_permission` stays unanswered — and the tddy turn ends with it; the
/// caller's [`PermissionAnswer`] later resolves it, and the agent carries on.
///
/// The request the caller answers is the oldest parked one. An agent that asks again before that
/// one is answered has its next request wait behind it, so the caller is always shown one request.
#[derive(Debug, Default)]
pub struct ParkedPermissions {
    state: Mutex<State>,
    /// Woken when a request parks, so a turn waiting on the agent can yield it.
    parked: Notify,
}

#[derive(Debug, Default)]
struct State {
    /// Requests the agent is waiting on, oldest first.
    queue: VecDeque<Parked>,
    /// How many requests have been parked, so each one is named on its own.
    issued: u64,
}

#[derive(Debug)]
struct Parked {
    view: PermissionRequestView,
    /// Resolves the agent's `session/request_permission` with the decision.
    decided: oneshot::Sender<PermissionDecision>,
}

impl ParkedPermissions {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The request the caller is to answer now, if the agent is waiting on one.
    pub fn pending(&self) -> Option<PermissionRequestView> {
        self.state().queue.front().map(|parked| parked.view.clone())
    }

    /// Resolves once a request has parked. A wake-up can be for a request answered before the
    /// waiter looks, so a waiter re-checks [`Self::pending`] rather than trusting the wake-up.
    pub async fn parked(&self) {
        self.parked.notified().await;
    }

    /// Deliver `answer` to the pending request it names. A different `request_id`, a selected
    /// option the request did not offer, or no pending request at all is refused — naming the
    /// pending request where there is one — and leaves the pending request parked.
    pub fn answer(&self, answer: &PermissionAnswer) -> Result<(), String> {
        let mut state = self.state();
        let decision = decision_for(state.queue.front(), answer)?;
        let parked = state
            .queue
            .pop_front()
            .ok_or_else(|| nothing_pending(answer))?;
        parked.decided.send(decision).map_err(|_| {
            format!(
                "the coding agent stopped waiting for permission request '{}'",
                answer.request_id()
            )
        })
    }

    /// Answer every parked request as cancelled, oldest first. ACP asks a client whose prompt is
    /// cancelled to answer each outstanding permission request, so this runs before
    /// `session/cancel` is sent.
    pub fn cancel_pending(&self) {
        for parked in self.state().queue.drain(..) {
            // A closed receiver means the agent's connection is already gone, so no agent is left
            // waiting on this answer.
            if parked.decided.send(PermissionDecision::Cancelled).is_err() {
                log::debug!(
                    "permission request '{}' had no waiting agent to cancel",
                    parked.view.request_id
                );
            }
        }
    }
}

/// Why `answer` is refused when no permission request is pending.
pub(crate) fn nothing_pending(answer: &PermissionAnswer) -> String {
    format!(
        "no permission request is pending, so the answer to '{}' has nothing to answer",
        answer.request_id()
    )
}

/// The decision `answer` delivers to the request at the front of the queue, or why it cannot.
fn decision_for(
    front: Option<&Parked>,
    answer: &PermissionAnswer,
) -> Result<PermissionDecision, String> {
    let Some(parked) = front else {
        return Err(nothing_pending(answer));
    };
    let pending = &parked.view.request_id;
    if pending != answer.request_id() {
        return Err(format!(
            "permission request '{pending}' is the one pending; the answer names '{}'",
            answer.request_id()
        ));
    }
    match answer {
        PermissionAnswer::Selected { option_id, .. } => {
            if parked
                .view
                .options
                .iter()
                .any(|option| &option.option_id == option_id)
            {
                Ok(PermissionDecision::Selected {
                    option_id: option_id.clone(),
                })
            } else {
                Err(format!(
                    "'{option_id}' is not one of the options of permission request '{pending}'"
                ))
            }
        }
        PermissionAnswer::Cancelled { .. } => Ok(PermissionDecision::Cancelled),
    }
}

/// The view the caller reads of an agent's request, named by its ordinal in this conversation.
fn view_of(ordinal: u64, request: &PermissionRequest) -> PermissionRequestView {
    PermissionRequestView {
        request_id: format!("perm-{ordinal}"),
        tool_call: PermissionToolCall {
            // FIXME(permission-yield): the driver's `PermissionRequest` does not carry the ACP tool
            // kind (its mapping drops it), so every request reads as `other` until the driver passes
            // it through. Deferred to #720's driver: docs/dev/todo/2026-10-24-acp-permission-request-drops-tool-kind.md
            title: request
                .title
                .clone()
                .unwrap_or_else(|| request.tool_call_id.clone()),
            kind: "other".to_string(),
            raw_input: request.raw_input.clone(),
        },
        options: request
            .options
            .iter()
            .map(|option| PermissionChoice {
                option_id: option.option_id.clone(),
                name: option.name.clone(),
                kind: option.kind.clone(),
            })
            .collect(),
    }
}

impl ParkedPermissions {
    /// Park `request` at the back of the queue. Returns the id it is named by, and the receiver its
    /// answer arrives on, so a caller can cancel this request alone (see [`Self::cancel_request`])
    /// before it has been answered.
    pub(crate) fn park(
        &self,
        request: PermissionRequest,
    ) -> (String, oneshot::Receiver<PermissionDecision>) {
        let (decided, answered) = oneshot::channel();
        let request_id = {
            let mut state = self.state();
            state.issued += 1;
            let view = view_of(state.issued, &request);
            let request_id = view.request_id.clone();
            state.queue.push_back(Parked { view, decided });
            request_id
        };
        self.parked.notify_one();
        (request_id, answered)
    }

    /// Answer the one parked request named `request_id` as cancelled, and leave every other parked
    /// request as it is. Returns whether that request was parked: a request already answered or
    /// cancelled is not, and is left alone.
    pub fn cancel_request(&self, request_id: &str) -> bool {
        let removed = {
            let mut state = self.state();
            let position = state
                .queue
                .iter()
                .position(|parked| parked.view.request_id == request_id);
            position.and_then(|position| state.queue.remove(position))
        };
        match removed {
            Some(parked) => {
                // A closed receiver means the approval that parked it is already gone, so no agent is
                // left waiting on this answer.
                if parked.decided.send(PermissionDecision::Cancelled).is_err() {
                    log::debug!(
                        "permission request '{}' had no waiting agent to cancel",
                        parked.view.request_id
                    );
                }
                true
            }
            None => false,
        }
    }
}

#[async_trait::async_trait]
impl PermissionHandler for ParkedPermissions {
    async fn decide(&self, request: PermissionRequest) -> PermissionDecision {
        let (_, answered) = self.park(request);
        // The sender waits in the queue until `answer` or `cancel_pending` takes it. The queue
        // lives as long as this handler, which this call borrows, so the sender is not dropped
        // unsent; cancelling is the protocol's answer should that ever change.
        answered.await.unwrap_or(PermissionDecision::Cancelled)
    }
}
