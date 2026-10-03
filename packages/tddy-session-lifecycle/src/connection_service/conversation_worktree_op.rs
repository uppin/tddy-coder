//! `ConversationWorktree`'s operations — `Pull`, `PullRange`, `Remove`, `Reset`, `Diff` and `Sync` —
//! against a session's worktree.
//!
//! Shared by the exec-tool RPC (`tddy-daemon-rpc`, which authorizes the caller's token first) and
//! the host bridge a jail's relayed call arrives on (which has no token and is bound to its session
//! by the runner instead).

use std::collections::BTreeSet;
use std::path::Path;

use prost::Message as _;
use serde_json::{json, Value};
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::conversation_worktree_request::Op;
use tddy_service::proto::exec_tools::{
    ConversationWorktreeRequest, ConversationWorktreeResponse, DiffOp, PullRangeOp, ResetOp,
};
use tddy_subagent_worktree::{
    ConversationId, ConversationWorktree, ConversationWorktrees, PullRange, ResetTarget,
    SyncOutcome, WorktreeError, WorktreeSync, SYNC_NOTICE_PATHS,
};

use super::daemon_rpc_handler::BoundJailSession;
use super::DaemonSessionHost;

/// Run `op` on `conversation_id`'s worktree under `session_worktree` and answer with the
/// operation's `result_json`: `{"pulled": {files, lines, conflicts} | null}` for a pull (null when
/// the conversation never created a worktree), `{"removed": bool}` for a remove, or
/// `{"reset": {to, droppedCommits} | null}` for a reset (null likewise when there is no worktree),
/// `{"pulled": {commits, skipped, files, lines, conflicts} | null}` for a range pull (a bound the
/// conversation does not have is `FailedPrecondition`), or
/// `{"diff": {from, to, files, lines, diff, truncated, includesCallerChanges}}` for a diff (a
/// conversation without a worktree is `FailedPrecondition`), or
/// `{"sync": {commit, files, lines, paths, morePaths} | null}` for a sync — null when there is no
/// worktree, when the session worktree's files are what the conversation last took in, or when a
/// merge was recorded but changed no file in the conversation worktree (the session worktree only
/// took in the conversation's own work) — and
/// `{"conflicts": [paths], "moreConflicts": n}` when the merge conflicts and nothing moved: at most
/// [`SYNC_NOTICE_PATHS`] paths, `n` counting the rest.
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
    let answer = match op {
        Op::Pull(_) => pull(existing).await?,
        Op::PullRange(range) => pull_range(existing, range).await?,
        Op::Remove(_) => remove(existing).await?,
        Op::Reset(target) => reset(existing, target).await?,
        Op::Diff(diff) => diff_of(conversation_id, existing, diff).await?,
        Op::Sync(_) => sync(conversation_id, existing).await?,
    };
    Ok(answer.to_string())
}

/// Everything the conversation committed, applied to the session worktree.
async fn pull(existing: Option<ConversationWorktree>) -> Result<Value, Status> {
    let Some(worktree) = existing else {
        return Ok(json!({ "pulled": null }));
    };
    let outcome = worktree.pull_into_caller().await.map_err(internal)?;
    Ok(json!({ "pulled": outcome }))
}

/// The chosen range of the conversation's commits, applied to the session worktree.
async fn pull_range(
    existing: Option<ConversationWorktree>,
    pull: PullRangeOp,
) -> Result<Value, Status> {
    let Some(worktree) = existing else {
        return Ok(json!({ "pulled": null }));
    };
    // An empty bound is an omitted one: the first commit not yet pulled, the tip.
    let range = PullRange {
        from: omitted_if_empty(pull.from),
        to: omitted_if_empty(pull.to),
    };
    let already_pulled: BTreeSet<String> = pull.already_pulled.into_iter().collect();
    let outcome = worktree
        .pull_range(&range, &already_pulled)
        .await
        .map_err(refused)?;
    Ok(json!({ "pulled": outcome }))
}

async fn remove(existing: Option<ConversationWorktree>) -> Result<Value, Status> {
    let Some(worktree) = existing else {
        return Ok(json!({ "removed": false }));
    };
    worktree.remove().await.map_err(internal)?;
    Ok(json!({ "removed": true }))
}

async fn reset(existing: Option<ConversationWorktree>, reset: ResetOp) -> Result<Value, Status> {
    let Some(worktree) = existing else {
        return Ok(json!({ "reset": null }));
    };
    // An empty commit names the base.
    let target = match omitted_if_empty(reset.commit) {
        None => ResetTarget::Base,
        Some(commit) => ResetTarget::Commit(commit),
    };
    let outcome = worktree.reset_to(&target).await.map_err(internal)?;
    Ok(json!({ "reset": outcome }))
}

async fn diff_of(
    conversation_id: &str,
    existing: Option<ConversationWorktree>,
    diff: DiffOp,
) -> Result<Value, Status> {
    let Some(worktree) = existing else {
        return Err(Status::failed_precondition(format!(
            "conversation {conversation_id} has no worktree: nothing has been committed yet"
        )));
    };
    // An empty bound is an omitted one: the base for `from`, the tip for `to`.
    let (from, to) = (omitted_if_empty(diff.from), omitted_if_empty(diff.to));
    let outcome = worktree
        .diff(from.as_deref(), to.as_deref())
        .await
        .map_err(refused)?;
    Ok(json!({ "diff": outcome }))
}

/// The session worktree's current files, merged into the conversation's branch.
async fn sync(
    conversation_id: &str,
    existing: Option<ConversationWorktree>,
) -> Result<Value, Status> {
    let outcome = match existing {
        Some(worktree) => worktree.sync_with_caller().await.map_err(internal)?,
        None => SyncOutcome::Unchanged,
    };
    Ok(match outcome {
        SyncOutcome::Unchanged => json!({ "sync": null }),
        SyncOutcome::Merged(sync) => {
            log_merge(conversation_id, &sync);
            json!({ "sync": sync })
        }
        SyncOutcome::Conflicted { paths } => {
            log::info!(
                "ConversationWorktree: sync of conversation {conversation_id} conflicts in {} \
                 path(s); nothing merged",
                paths.len()
            );
            conflicts_answer(paths)
        }
    })
}

fn log_merge(conversation_id: &str, sync: &WorktreeSync) {
    log::info!(
        "ConversationWorktree: synced conversation {conversation_id} at {}: files +{} ~{} -{}, \
         lines +{} -{}, {} path(s)",
        sync.commit,
        sync.files.created,
        sync.files.updated,
        sync.files.removed,
        sync.lines.added,
        sync.lines.removed,
        sync.paths.len() + sync.more_paths
    );
}

/// `{"conflicts": [...], "moreConflicts": n}`: the first [`SYNC_NOTICE_PATHS`] of `paths`, in the
/// order given, and how many were left out — the same cap the sync's own notice applies.
fn conflicts_answer(mut paths: Vec<String>) -> Value {
    let more = paths.len().saturating_sub(SYNC_NOTICE_PATHS);
    paths.truncate(SYNC_NOTICE_PATHS);
    json!({ "conflicts": paths, "moreConflicts": more })
}

/// An empty wire string is an omitted value.
fn omitted_if_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
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
    /// Serve a `ConversationWorktree` request relayed out of the jail built for `bound`.
    ///
    /// Unlike the exec-tool RPC this carries no session token to check. The jail is trusted for
    /// exactly the one session it was built for, so a request naming any other session is refused
    /// (`PermissionDenied`) — the runner rewrites every request to its own session, and one that
    /// names another did not come through the runner. The worktree is read from the session
    /// directory the daemon resolved for that jail under the session owner's sessions base, the
    /// same `.session.yaml` the token route reads.
    pub(crate) async fn conversation_worktree_from_jail(
        &self,
        bound: &BoundJailSession,
        payload: &[u8],
    ) -> Result<Vec<u8>, Status> {
        let req = ConversationWorktreeRequest::decode(payload).map_err(|e| {
            Status::invalid_argument(format!("decode ConversationWorktreeRequest: {e}"))
        })?;
        if req.session_id != bound.session_id {
            log::warn!(
                "ConversationWorktree: the jail of session {} named session {:?}; refused",
                bound.session_id,
                req.session_id
            );
            return Err(Status::permission_denied(format!(
                "this jail serves session {}, not {:?}",
                bound.session_id, req.session_id
            )));
        }
        let op = req
            .op
            .ok_or_else(|| Status::invalid_argument("ConversationWorktree carries no operation"))?;
        let worktree_root =
            crate::workspace_session::resolve_worktree_root_in_session_dir(&bound.session_dir)?;
        let result_json =
            run_conversation_worktree_op(&worktree_root, &req.session_id, &req.conversation_id, op)
                .await?;
        Ok(ConversationWorktreeResponse { result_json }.encode_to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(count: usize) -> Vec<String> {
        (0..count).map(|n| format!("src/f{n:02}.rs")).collect()
    }

    #[test]
    fn conflicts_within_the_limit_are_all_named() {
        assert_eq!(
            conflicts_answer(paths(2)),
            json!({ "conflicts": ["src/f00.rs", "src/f01.rs"], "moreConflicts": 0 })
        );
    }

    #[test]
    fn conflicts_past_the_limit_are_counted_not_named() {
        assert_eq!(
            conflicts_answer(paths(SYNC_NOTICE_PATHS + 3)),
            json!({ "conflicts": paths(SYNC_NOTICE_PATHS), "moreConflicts": 3 })
        );
    }
}
