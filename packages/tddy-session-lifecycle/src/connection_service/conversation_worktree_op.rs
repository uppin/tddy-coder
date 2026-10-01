//! `ConversationWorktree`'s operations — `Pull`, `PullRange`, `Remove`, `Reset` and `Diff` — against a session's worktree.
//!
//! Shared by the exec-tool RPC (`tddy-daemon-rpc`, which authorizes the caller's token first) and
//! the host bridge a jail's relayed call arrives on (which has no token and is bound to its session
//! by the runner instead).

use std::path::Path;

use prost::Message as _;
use std::collections::BTreeSet;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::conversation_worktree_request::Op;
use tddy_service::proto::exec_tools::{ConversationWorktreeRequest, ConversationWorktreeResponse};

use tddy_subagent_worktree::{
    ConversationId, ConversationWorktrees, PullRange, ResetTarget, WorktreeError,
};

use super::DaemonSessionHost;

/// Run `op` on `conversation_id`'s worktree under `session_worktree` and answer with the
/// operation's `result_json`: `{"pulled": {files, lines, conflicts} | null}` for a pull (null when
/// the conversation never created a worktree), `{"removed": bool}` for a remove, or
/// `{"reset": {to, droppedCommits} | null}` for a reset (null likewise when there is no worktree),
/// `{"pulled": {commits, skipped, files, lines, conflicts} | null}` for a range pull (a bound the
/// conversation does not have is `FailedPrecondition`), or `{"diff": {from, to, files, lines, diff, truncated}}` for a diff (a conversation without a
/// worktree is `FailedPrecondition`).
///
/// An id that could escape its directory or branch namespace is refused before anything is looked
/// up.
pub async fn run_conversation_worktree_op(
    session_worktree: &Path,
    session_id: &str,
    conversation_id: &str,
    op: Op,
) -> Result<String, Status> {
    let conversation = ConversationId::parse(conversation_id)
        .map_err(|e| Status::invalid_argument(e.to_string()))?;
    let worktrees = ConversationWorktrees::new(session_worktree, session_id);
    let existing = worktrees.existing(&conversation).await.map_err(internal)?;
    let answer = match (op, existing) {
        (Op::Pull(_), None) => serde_json::json!({ "pulled": null }),
        (Op::Pull(_), Some(worktree)) => {
            let outcome = worktree.pull_into_caller().await.map_err(internal)?;
            serde_json::json!({ "pulled": outcome })
        }
        (Op::PullRange(_), None) => serde_json::json!({ "pulled": null }),
        (Op::PullRange(pull), Some(worktree)) => {
            // An empty bound is an omitted one: the first commit not yet pulled, the tip.
            let bound = |bound: String| (!bound.is_empty()).then_some(bound);
            let range = PullRange {
                from: bound(pull.from),
                to: bound(pull.to),
            };
            let already_pulled: BTreeSet<String> = pull.already_pulled.into_iter().collect();
            let outcome = worktree
                .pull_range(&range, &already_pulled)
                .await
                .map_err(refused)?;
            serde_json::json!({ "pulled": outcome })
        }
        (Op::Remove(_), None) => serde_json::json!({ "removed": false }),
        (Op::Remove(_), Some(worktree)) => {
            worktree.remove().await.map_err(internal)?;
            serde_json::json!({ "removed": true })
        }
        (Op::Reset(_), None) => serde_json::json!({ "reset": null }),
        (Op::Diff(_), None) => {
            return Err(Status::failed_precondition(format!(
                "conversation {conversation_id} has no worktree: nothing has been committed yet"
            )))
        }
        (Op::Diff(diff), Some(worktree)) => {
            // An empty bound is an omitted one: the base for `from`, the tip for `to`.
            let bound = |bound: &str| (!bound.is_empty()).then_some(bound.to_string());
            let outcome = worktree
                .diff(bound(&diff.from).as_deref(), bound(&diff.to).as_deref())
                .await
                .map_err(refused)?;
            serde_json::json!({ "diff": outcome })
        }
        (Op::Reset(reset), Some(worktree)) => {
            // An empty commit names the base.
            let target = match reset.commit.as_str() {
                "" => ResetTarget::Base,
                commit => ResetTarget::Commit(commit.to_string()),
            };
            let outcome = worktree.reset_to(&target).await.map_err(internal)?;
            serde_json::json!({ "reset": outcome })
        }
    };
    Ok(answer.to_string())
}

/// A diff or pull bound the conversation does not have is the caller's mistake, not the daemon's.
fn refused(error: WorktreeError) -> Status {
    log::warn!("ConversationWorktree: {error}");
    Status::failed_precondition(error.to_string())
}

fn internal(error: WorktreeError) -> Status {
    log::warn!("ConversationWorktree: {error}");
    Status::internal(error.to_string())
}

impl DaemonSessionHost {
    /// Serve a `ConversationWorktree` request relayed out of a jail.
    ///
    /// Unlike the exec-tool RPC this carries no session token to check: the jail's runner stamped
    /// `session_id` from its own environment, which is the same identity every `ToolRequest` of that
    /// jail is served under, and the worktree is resolved from this daemon's own record of the
    /// session.
    pub(crate) async fn conversation_worktree_from_jail(
        &self,
        payload: &[u8],
    ) -> Result<Vec<u8>, Status> {
        let req = ConversationWorktreeRequest::decode(payload).map_err(|e| {
            Status::invalid_argument(format!("decode ConversationWorktreeRequest: {e}"))
        })?;
        let op = req
            .op
            .ok_or_else(|| Status::invalid_argument("ConversationWorktree carries no operation"))?;
        // The same lookup the roster RPCs on this bridge make: the session's own record under this
        // daemon's data dir.
        let session_dir = self.session_dir_for(&req.session_id)?;
        let worktree_root = tddy_core::read_session_metadata(&session_dir)
            .ok()
            .and_then(|meta| meta.repo_path)
            .map(std::path::PathBuf::from)
            .ok_or_else(|| {
                Status::failed_precondition(
                    "session not found or its .session.yaml has no repo_path",
                )
            })?;
        let result_json =
            run_conversation_worktree_op(&worktree_root, &req.session_id, &req.conversation_id, op)
                .await?;
        Ok(ConversationWorktreeResponse { result_json }.encode_to_vec())
    }
}
