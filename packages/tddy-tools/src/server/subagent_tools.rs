use super::replacement_of;

use tddy_discovery::subagent::MessageId;

use super::take_a_turn;

use super::yield_conditions_of;

use tddy_discovery::subagent::TurnRequest;

use super::turn_budget;

use tddy_discovery::subagent_runtime::report_local_conversation_state;

use crate::mcp_primitives::subagent_config_for_conversation;

use tddy_discovery::subagent::SubagentRegistry;

use tddy_discovery::subagent_runtime::SubagentConversation;

use tddy_discovery::subagent_runtime::subagent_sessions;

use crate::mcp_primitives::open_remote_agent_session;

use tddy_discovery::subagent_runtime::subagent_error_json;

use super::subagent_tool_router;

pub(crate) fn subagent_tool_names() -> Vec<String> {
    subagent_tool_router()
        .list_all()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect()
}

/// How long a conversation tool blocks before it hands the caller a receipt rather than an answer:
/// the default for `subagent_prompt`'s `graceMs` and for `subagent_await`'s `timeoutMs`.
///
/// Not a turn budget — nothing is cancelled when it elapses. It is how long the *main agent* waits,
/// which is a different question from how long the subagent may take
/// (docs/ft/coder/managed-codebase-subagents.md § Long turns).
pub(crate) const SUBAGENT_PROMPT_GRACE: std::time::Duration = std::time::Duration::from_secs(25);

/// Read a caller-named blocking budget (`graceMs`, `timeoutMs`) in milliseconds.
///
/// A malformed value is refused naming the field rather than replaced by `default`: substituting 25
/// seconds for a budget the caller got wrong would block a call that asked for something else, with
/// nothing saying so. `0` is a budget of its own — "hand me the id, I will collect it myself" — and
/// not an absence; only an absent field (or JSON's own spelling of one, `null`) takes `default`.
pub(crate) fn blocking_budget(
    args: &serde_json::Value,
    field: &str,
    default: std::time::Duration,
) -> Result<std::time::Duration, String> {
    match args.get(field) {
        None | Some(serde_json::Value::Null) => Ok(default),
        Some(value) => match value.as_u64() {
            Some(millis) => Ok(std::time::Duration::from_millis(millis)),
            None => Err(format!(
                "{field} must be a whole number of milliseconds to block for, got: {value}"
            )),
        },
    }
}

/// `subagent_new_session` (ACP `session/new`-shaped): opens a conversation with the named
/// subagent under the given `sessionId` — the caller decides the conversation id; one is generated
/// only when omitted.
///
/// The agent is resolved against the session's **live roster**, not against the spawn env: an agent
/// attached after this process started is callable, and one detached is refused naming the id (see
/// docs/ft/daemon/session-agent-roster.md § Invoking an agent).
///
/// A call naming no agent is an error listing the agents there are. There is no default: with any
/// number of agents attachable, picking one for the caller would make the choice depend on attach
/// order rather than on what the main agent asked for.
pub(crate) async fn subagent_new_session_tool(args: serde_json::Value) -> String {
    let roster = crate::session_agents::session_agent_roster();
    let agent_id = args
        .get("agent")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let session_id = args
        .get("sessionId")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let system_prompt = args
        .get("systemPrompt")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    // Refused before anything is opened, and before which host runs the agent is even known, so
    // the caller reads the same answer on both paths. It is checked again at each layer beneath —
    // this is the one that can still name the field the caller got wrong.
    if let Err(e) = tddy_discovery::subagent::refuse_blank_system_prompt(system_prompt.as_deref()) {
        return subagent_error_json(e.to_string());
    }

    let entry = match roster.open_conversation_as(&session_id, agent_id) {
        Ok(entry) => entry,
        Err(e) => return subagent_error_json(e),
    };
    // A roster entry carries no endpoint, credential or turn budget — deliberately, so editing a def
    // cannot change what a running session may call. This process can therefore only run the loop of
    // an agent it holds the def for; every other one is run by the facilitating daemon, over the
    // same `session_id` the caller chose, so a `subagent_cancel` addresses the same conversation on
    // both paths.
    let Some(def) = roster.local_def_for(&entry) else {
        // An agent this process holds no def for is run by the facilitating daemon, which is every
        // roster agent in a split session — the jail holds no defs at all. The override rides
        // `OpenAgentConversation` to that daemon, so the caller gets the same answer here as it
        // would for an agent this process runs itself.
        return match open_remote_agent_session(&entry, &session_id, system_prompt.as_deref()).await
        {
            Ok(opened) => {
                subagent_sessions().lock().await.open.insert(
                    session_id.clone(),
                    // No provider: the loop runs on the facilitating daemon, against an
                    // endpoint this process cannot see. Naming one here would report a queue
                    // the turn is not standing in.
                    {
                        let mut conversation = SubagentConversation::opened(
                            opened.agent_id,
                            None,
                            opened.session,
                            opened.remote,
                        );
                        conversation.clone_placed =
                            crate::worktree_route::is_clone_placement(entry.placement);
                        conversation
                    },
                );
                serde_json::json!({ "sessionId": session_id }).to_string()
            }
            Err(e) => {
                roster.close_conversation(&session_id);
                subagent_error_json(e)
            }
        };
    };
    let agent_name = def.name.clone();
    // The endpoint this conversation's turns will be sent to, read before the def is consumed by
    // the registry: it is what the turns queue on, and what their receipts name.
    let provider = def.base_url.clone();
    let registry = SubagentRegistry::from_defs(vec![def]);
    let mut config = subagent_config_for_conversation(&session_id);
    if let Some(prompt) = system_prompt {
        config = config.with_system_prompt(prompt);
    }
    match registry.create(&agent_name, config) {
        Ok(session) => {
            let agent_id = entry.agent_id.clone();
            subagent_sessions().lock().await.open.insert(
                session_id.clone(),
                SubagentConversation::opened(entry.agent_id, Some(provider), session, None),
            );
            // The daemon is never asked to open this one, so this is the first and only moment its
            // roster row can stop reporting UNSPECIFIED.
            report_local_conversation_state(
                &agent_id,
                tddy_service::proto::types::SessionAgentStatus::Idle,
                "conversation opened",
            )
            .await;
            serde_json::json!({ "sessionId": session_id }).to_string()
        }
        Err(e) => {
            roster.close_conversation(&session_id);
            subagent_error_json(e)
        }
    }
}

/// `subagent_prompt` (ACP `session/prompt`-shaped): sends one prompt turn to an already-open
/// session and returns `{stopReason, content, usage, messages}` once the subagent yields — or
/// `{responseId, pending: true}` when it has not yielded within the caller's grace period.
///
/// The *wait* is what `graceMs` bounds; `maxTurns` is what bounds the turn itself, and only when
/// the caller sends one — absent, the agent definition's own budget applies unchanged. A turn
/// still running when `graceMs` elapses keeps going in a background task, and `subagent_await`
/// collects its outcome under the returned id
/// (docs/ft/coder/managed-codebase-subagents.md § Long turns).
pub(crate) async fn subagent_prompt_tool(args: serde_json::Value) -> String {
    let Some(session_id) = args.get("sessionId").and_then(|v| v.as_str()) else {
        return subagent_error_json("missing required field: sessionId");
    };
    let Some(prompt_blocks) = args.get("prompt").and_then(|v| v.as_array()) else {
        return subagent_error_json("missing required field: prompt");
    };
    let prompt_text = prompt_blocks
        .iter()
        .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    if prompt_text.is_empty() {
        return subagent_error_json("prompt must contain at least one non-empty text block");
    }
    // Read before anything runs: a call whose blocking budget cannot be honored as written must not
    // leave a turn behind that its caller was never told the id of.
    let grace = match blocking_budget(&args, "graceMs", SUBAGENT_PROMPT_GRACE) {
        Ok(grace) => grace,
        Err(e) => return subagent_error_json(e),
    };
    let request = match turn_budget(&args, TurnRequest::prompting(prompt_text)) {
        Ok(request) => request,
        Err(e) => return subagent_error_json(e),
    };
    let request = match yield_conditions_of(&args, request) {
        Ok(request) => request,
        Err(e) => return subagent_error_json(e),
    };
    let request = match crate::sync_worktree_choice::with_sync_worktree_choice(request, &args) {
        Ok(request) => request,
        Err(e) => return subagent_error_json(e),
    };
    take_a_turn(session_id, request, grace).await
}

/// `subagent_resume`: take another turn on an open conversation **without** asking it anything
/// new, optionally after sending it back to a message it already holds and optionally with one
/// corrective instruction.
///
/// A rewind alone changes nothing — turns go out at `temperature: 0.0`, so an identical history
/// reproduces an identical turn — which is why `correction` exists beside `fromMessageId` rather
/// than as a separate tool.
///
/// Queues exactly as a prompt does: a conversation runs one turn at a time, so a resume accepted
/// while a turn is in flight waits behind it and reports `queuePosition` / `queueSize`.
pub async fn subagent_resume_tool(args: serde_json::Value) -> String {
    let Some(session_id) = args.get("sessionId").and_then(|v| v.as_str()) else {
        return subagent_error_json("missing required field: sessionId");
    };
    let grace = match blocking_budget(&args, "graceMs", SUBAGENT_PROMPT_GRACE) {
        Ok(grace) => grace,
        Err(e) => return subagent_error_json(e),
    };
    let mut request = TurnRequest::resuming();
    // Present-but-empty is refused rather than treated as absent, for both fields and for the
    // reason `subagent_prompt` refuses an empty prompt: a caller that sent a field meant to send
    // something in it, and honouring the request as though the field were missing would silently
    // do less than it asked for. An empty id addresses no message; an empty correction corrects
    // nothing, which makes the rewind it accompanies a no-op.
    match args.get("fromMessageId").and_then(|v| v.as_str()) {
        Some("") => {
            return subagent_error_json(
                "fromMessageId must name a message to rewind to; omit it to continue from the end",
            )
        }
        Some(id) => request = request.from_message(MessageId::from(id)),
        None => {}
    }
    match args.get("correction").and_then(|v| v.as_str()) {
        Some("") => {
            return subagent_error_json(
                "correction must say what to do differently; omit it to resume unchanged",
            )
        }
        Some(correction) => request = request.with_correction(correction),
        None => {}
    }
    let request = match crate::reset_worktree_choice::with_reset_worktree_choice(request, &args) {
        Ok(request) => request,
        Err(e) => return subagent_error_json(e),
    };
    let request = match crate::sync_worktree_choice::with_sync_worktree_choice(request, &args) {
        Ok(request) => request,
        Err(e) => return subagent_error_json(e),
    };
    let request = match turn_budget(&args, request) {
        Ok(request) => request,
        Err(e) => return subagent_error_json(e),
    };
    let request = match yield_conditions_of(&args, request) {
        Ok(request) => request,
        Err(e) => return subagent_error_json(e),
    };
    let request = match replacement_of(&args, request) {
        Ok(request) => request,
        Err(e) => return subagent_error_json(e),
    };
    // A deferral hands the pending request to the user: the daemon holds the turn until the user
    // answers it, and the call returns pending with `awaitingUser` (see `take_a_turn`). Combined
    // with an answer it is refused, naming both, by the parser.
    let request = match crate::permission_deferral_arg::parse_permission_deferral(&args) {
        Ok(Some(request_id)) => request.deferring_permission_to_user(request_id),
        Ok(None) => request,
        Err(e) => return subagent_error_json(e),
    };
    let request = match crate::permission_answer_arg::parse_permission_answer(&args) {
        Ok(Some(answer)) => request.with_permission_answer(answer),
        Ok(None) => request,
        Err(e) => return subagent_error_json(e),
    };
    take_a_turn(session_id, request, grace).await
}
