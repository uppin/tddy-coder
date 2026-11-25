//! Answering a hook that waits on a target: the daemon end of
//! `tddy_core::remote_answering_hook_command` (`#target-parity` 1/7).
//!
//! The hook on the target appended its line with a nonce and polls
//! `<worktree>/<REMOTE_ANSWERS_DIR>/<nonce>`. [`hold_and_answer`] holds for the orchestrator's
//! answer to the recorded request, builds Claude's decision from it and writes the answer file
//! through the session's shell. No decision is an **empty** file, which ends the hook's wait at
//! once so Claude asks in its own terminal.
//!
//! The hook removes only an answer it reads. One written after the hook gave up — a Coder write
//! takes 6–9 s — would stay on the target until the session ends, so once the hook's window has
//! passed the daemon removes the answer it wrote ([`HeldHook::window_ends`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_remote_shell::{ExecRequest, RemoteShell};

use crate::remote_hook_delivery::HookReportDelivery;
use crate::remote_hook_follower::shell_quote;

/// Who a held hook belongs to: the identity `AwaitQuestionAnswer` authenticates with.
pub(crate) struct HeldHook {
    pub session_id: String,
    pub hook_token: String,
    pub os_user: String,
    /// The `request_id` the report was recorded under; `None` when the report was refused.
    pub request_id: Option<String>,
    pub permission_suggestions: Option<serde_json::Value>,
    /// The held call's `tool_input` when it is an `AskUserQuestion`: the hook's answer is then a
    /// question decision, not a permission one.
    pub question_input: Option<serde_json::Value>,
    /// The hook event holding the call (`PreToolUse`, `PermissionRequest`): a question decision is
    /// written in that hook's shape, the only one Claude reads from it.
    pub event: String,
    /// When the hook has certainly stopped polling for its answer (see [`hook_window_end`]).
    pub window_ends: tokio::time::Instant,
}

/// How far past its own count of seconds a waiting hook's poll can run: its loop polls once more
/// than it counts, and each `sleep 1` takes a little over a second.
const HOOK_POLL_OVERRUN_SECS: u64 = 3;

/// When a waiting hook whose line has just been delivered stops polling, at the latest. The hook
/// waits `answer_wait_secs + AWAIT_CLIENT_MARGIN_SECS` from before its line was written, so a
/// window counted from now ends after it.
pub(crate) fn hook_window_end(answer_wait_secs: u64) -> tokio::time::Instant {
    tokio::time::Instant::now()
        + Duration::from_secs(
            answer_wait_secs + tddy_core::AWAIT_CLIENT_MARGIN_SECS + HOOK_POLL_OVERRUN_SECS,
        )
}

impl crate::remote_hook_follower::HookLine {
    /// The `tool_input` of a hook line that holds a question — a `PreToolUse` of `AskUserQuestion`,
    /// or the `PermissionRequest` Claude fires for it once that hook's hold has ended. Kept beside
    /// the hold rather than in `remote_hook_follower.rs`, which is over its length budget
    /// (`docs/code-issues/oversized-file-remote-hook-follower.md`).
    pub(crate) fn question_input(&self) -> Option<serde_json::Value> {
        (matches!(self.event.as_str(), "PreToolUse" | "PermissionRequest")
            && self.payload.get("tool_name").and_then(|t| t.as_str()) == Some("AskUserQuestion"))
        .then(|| {
            self.payload
                .get("tool_input")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        })
    }
}

/// Hold for the orchestrator's answer to `hook`, then write the decision — or an empty file — as
/// the answer for `nonce` in `worktree` on `shell`'s target.
pub(crate) async fn hold_and_answer(
    delivery: Arc<dyn HookReportDelivery>,
    shell: Arc<dyn RemoteShell>,
    worktree: PathBuf,
    nonce: String,
    hook: HeldHook,
) {
    let window_ends = hook.window_ends;
    let decision = match hook.request_id.filter(|id| !id.is_empty()) {
        Some(request_id) => {
            let suggestions = hook.permission_suggestions;
            let question_input = hook.question_input;
            let request = tddy_service::proto::activity::AwaitQuestionAnswerRequest {
                session_id: hook.session_id.clone(),
                hook_token: hook.hook_token,
                os_user: hook.os_user,
                request_id,
                permission_suggestions_json: suggestions
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            };
            match delivery.await_question_answer(request).await {
                Ok(answer) => match &question_input {
                    Some(tool_input) => {
                        tddy_session_activity::hook_decision::question_decision_json_for(
                            &hook.event,
                            &answer,
                            tool_input,
                        )
                    }
                    None => tddy_session_activity::hook_decision::permission_decision_json(
                        &answer,
                        suggestions.as_ref(),
                    ),
                },
                Err(error) => {
                    log::warn!(
                        "holding for the answer of session {}'s waiting hook refused: {}",
                        hook.session_id,
                        error.message()
                    );
                    None
                }
            }
        }
        None => None,
    };
    if let Err(error) = write_answer(
        shell.as_ref(),
        &worktree,
        &nonce,
        decision.as_deref().unwrap_or(""),
    )
    .await
    {
        log::warn!(
            "writing the answer of session {}'s waiting hook on its target: {error}",
            hook.session_id
        );
        return;
    }
    // A hook that read its answer removed it; one that gave up first never will.
    tokio::time::sleep_until(window_ends).await;
    if let Err(error) = remove_answer(shell.as_ref(), &worktree, &nonce).await {
        log::warn!(
            "removing the answer of session {}'s waiting hook from its target: {error}",
            hook.session_id
        );
    }
}

/// The path of the answer for `nonce` in `worktree`, refused unless `nonce` is one a hook mints.
fn answer_path(worktree: &Path, nonce: &str) -> Result<String, String> {
    if nonce.is_empty()
        || !nonce
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("{nonce:?} is not a nonce a hook mints"));
    }
    let dir = worktree.join(tddy_core::REMOTE_ANSWERS_DIR);
    Ok(format!("{}/{nonce}", dir.to_string_lossy()))
}

/// Remove the answer for `nonce` from `worktree` on the target. An answer already gone is not an
/// error.
async fn remove_answer(
    shell: &dyn RemoteShell,
    worktree: &Path,
    nonce: &str,
) -> Result<(), String> {
    let answer = answer_path(worktree, nonce)?;
    shell
        .exec(ExecRequest::new(format!("rm -f {}", shell_quote(&answer))))
        .await
        .map_err(|error| error.to_string())?
        .success_or_exited()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Write `content` as `<worktree>/<REMOTE_ANSWERS_DIR>/<nonce>`: into a temporary file beside it,
/// then moved into place, so the hook polling for the name never reads half an answer.
async fn write_answer(
    shell: &dyn RemoteShell,
    worktree: &Path,
    nonce: &str,
    content: &str,
) -> Result<(), String> {
    let answer = answer_path(worktree, nonce)?;
    let dir = worktree.join(tddy_core::REMOTE_ANSWERS_DIR);
    let dir = dir.to_string_lossy();
    let destination = shell_quote(&answer);
    let temporary = shell_quote(&format!("{dir}/.{nonce}.tmp"));
    let command = format!(
        "umask 077; mkdir -p {dir} && cat > {temporary} && mv {temporary} {destination}",
        dir = shell_quote(&dir),
    );
    shell
        .exec(ExecRequest::new(command).with_stdin(content.as_bytes()))
        .await
        .map_err(|error| error.to_string())?
        .success_or_exited()
        .map(|_| ())
        .map_err(|error| error.to_string())
}
