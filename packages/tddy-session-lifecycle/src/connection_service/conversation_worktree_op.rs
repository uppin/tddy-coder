//! `ConversationWorktree`'s operations — `Pull`, `Remove` and `Reset` — against a session's worktree.
//!
//! Shared by the exec-tool RPC (`tddy-daemon-rpc`, which authorizes the caller's token first) and
//! the host bridge a jail's relayed call arrives on (which has no token and is bound to its session
//! by the runner instead).

use std::path::Path;

use prost::Message as _;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::conversation_worktree_request::Op;
use tddy_service::proto::exec_tools::{ConversationWorktreeRequest, ConversationWorktreeResponse};
use tddy_subagent_worktree::{ConversationId, ConversationWorktrees, ResetTarget, WorktreeError};

use super::DaemonSessionHost;

/// Run `op` on `conversation_id`'s worktree under `session_worktree` and answer with the
/// operation's `result_json`: `{"pulled": {files, lines, conflicts} | null}` for a pull (null when
/// the conversation never created a worktree), `{"removed": bool}` for a remove, or
/// `{"reset": {to, droppedCommits} | null}` for a reset (null likewise when there is no worktree).
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
        (Op::Remove(_), None) => serde_json::json!({ "removed": false }),
        (Op::Remove(_), Some(worktree)) => {
            worktree.remove().await.map_err(internal)?;
            serde_json::json!({ "removed": true })
        }
        (Op::Reset(_), None) => serde_json::json!({ "reset": null }),
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
