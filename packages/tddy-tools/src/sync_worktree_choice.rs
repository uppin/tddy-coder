//! The `syncWorktree` argument of `subagent_prompt` and `subagent_resume`, read off the call's
//! arguments, and the schema property that advertises it. In its own module because `server.rs` is
//! over the file budget (`packages/tddy-tools/docs/code-issues/oversized-file-server.md`).

use serde_json::Value;
use tddy_discovery::subagent::TurnRequest;

/// The argument's name on the wire.
pub(crate) const SYNC_WORKTREE_ARG: &str = "syncWorktree";

/// The message a non-boolean `syncWorktree` is refused with.
const NOT_A_BOOLEAN: &str = "syncWorktree must be a boolean";

/// `request` with the caller's `syncWorktree` choice applied: absent or `true` keeps the default
/// (the turn first takes in the caller's current files), `false` runs it on the worktree as it
/// stands; anything else is refused by name.
pub(crate) fn with_sync_worktree_choice(
    request: TurnRequest,
    args: &Value,
) -> Result<TurnRequest, &'static str> {
    match args.get(SYNC_WORKTREE_ARG) {
        None | Some(Value::Bool(true)) => Ok(request),
        Some(Value::Bool(false)) => Ok(request.without_sync()),
        Some(_) => Err(NOT_A_BOOLEAN),
    }
}

/// The `syncWorktree` property, spelled once for `subagent_prompt` and `subagent_resume`.
pub(crate) fn sync_worktree_property() -> Value {
    serde_json::json!({
        "type": "boolean",
        "description": "Before the turn, merge your current files — HEAD plus staged, unstaged and \
                        untracked changes — into the conversation's worktree, so the subagent \
                        works on what you have now rather than on the files it last saw. Its own \
                        unpulled commits are kept beside your changes, and it is told which files \
                        changed. Defaults to true; a conversation that has not written anything \
                        yet has nothing to merge. If your changes and its unpulled commits touch \
                        the same lines, the turn is refused naming the files: pull its commits \
                        first (subagent_pull), change the files, or pass false to run the turn on \
                        its worktree as it stands. The outcome's `worktreeSync` says what was \
                        merged."
    })
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
    fn sync_worktree_true_keeps_the_sync() {
        let args = json!({ "syncWorktree": true });

        let request = with_sync_worktree_choice(TurnRequest::resuming(), &args).unwrap();

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
