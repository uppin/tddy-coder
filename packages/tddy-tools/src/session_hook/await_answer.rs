//! The awaiting half of `session-hook --await-answer`: the `AwaitQuestionAnswer` exchange and the
//! decision an `AskUserQuestion`'s answers become (`#home-inbox` 6/8, 8/9).

use prost::Message as _;
use tddy_service::proto::activity::{
    AwaitQuestionAnswerRequest, AwaitQuestionAnswerResponse, AwaitQuestionOutcome,
};

use super::question_decision::encode_question_decision;
use super::{exchange, SessionHookArgs};

/// Ask the daemon for the orchestrator's answers to the `AskUserQuestion` `request_id`, waiting at
/// most `--await-timeout-secs`, and map them to Claude's decision JSON (`#home-inbox` 8/9).
pub(super) async fn await_question_decision(
    args: &SessionHookArgs,
    stdin_json: &str,
    request_id: String,
) -> anyhow::Result<Option<String>> {
    let answer = await_answer(args, request_id, String::new()).await?;
    let tool_input = serde_json::from_str::<serde_json::Value>(stdin_json)
        .ok()
        .and_then(|event| event.get("tool_input").cloned())
        .unwrap_or(serde_json::Value::Null);
    Ok(question_decision_json(&answer, &tool_input))
}

/// The `AwaitQuestionAnswer` exchange for `request_id`: the daemon's answer, once it gives one.
pub(super) async fn await_answer(
    args: &SessionHookArgs,
    request_id: String,
    permission_suggestions_json: String,
) -> anyhow::Result<AwaitQuestionAnswerResponse> {
    let req = AwaitQuestionAnswerRequest {
        session_id: args.session.clone(),
        hook_token: args.hook_token.clone(),
        os_user: args.os_user.clone(),
        request_id,
        permission_suggestions_json,
    };
    let bytes = exchange(
        args,
        "AwaitQuestionAnswer",
        req.encode_to_vec(),
        std::time::Duration::from_secs(args.await_timeout_secs),
    )
    .await?;
    AwaitQuestionAnswerResponse::decode(&bytes[..])
        .map_err(|e| anyhow::anyhow!("decode AwaitQuestionAnswerResponse: {e}"))
}

/// How an orchestrator's answers to an `AskUserQuestion` reached the hook, as Claude's
/// `PreToolUse` decision: the call's own `tool_input` with the answers handed over
/// (`question_decision`). Nothing when the wait was not answered.
#[must_use]
pub fn question_decision_json(
    answer: &AwaitQuestionAnswerResponse,
    tool_input: &serde_json::Value,
) -> Option<String> {
    if AwaitQuestionOutcome::try_from(answer.outcome) != Ok(AwaitQuestionOutcome::Answered) {
        return None;
    }
    encode_question_decision(tool_input, &answer.answers)
}
