//! The `syncWorktree` argument of `subagent_prompt` and `subagent_resume`, read off the call's
//! arguments. In its own module because `server.rs` is over the file budget
//! (`packages/tddy-tools/docs/code-issues/oversized-file-server.md`).

use serde_json::Value;
use tddy_discovery::subagent::TurnRequest;

/// `request` with the caller's `syncWorktree` choice applied: absent or `true` keeps the default
/// (the turn first takes in the caller's current files), `false` runs it on the worktree as it
/// stands; anything else is refused by name.
#[allow(dead_code)] // TODO(caller-sync): applied in `subagent_prompt_tool` and `subagent_resume_tool`
pub(crate) fn with_sync_worktree_choice(
    request: TurnRequest,
    args: &Value,
) -> Result<TurnRequest, &'static str> {
    // TODO(caller-sync): implement
    let _ = (request, args);
    todo!("with_sync_worktree_choice")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_call_without_sync_worktree_keeps_the_default_of_syncing() {
        let request = with_sync_worktree_choice(TurnRequest::resuming(), &json!({})).unwrap();

        assert!(request.syncs_worktree());
    }

    #[test]
    fn sync_worktree_false_runs_the_turn_on_the_worktree_as_it_stands() {
        let args = json!({ "syncWorktree": false });

        let request = with_sync_worktree_choice(TurnRequest::resuming(), &args).unwrap();

        assert!(!request.syncs_worktree());
    }

    #[test]
    fn a_non_boolean_sync_worktree_is_refused_by_name() {
        let refused =
            with_sync_worktree_choice(TurnRequest::resuming(), &json!({ "syncWorktree": "no" }))
                .map(|_| ());

        assert_eq!(refused, Err("syncWorktree must be a boolean"));
    }
}
