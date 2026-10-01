//! The `resetWorktree` argument of `subagent_resume`, read off the call's arguments. In its own
//! module because `server.rs` is over the file budget
//! (`packages/tddy-tools/docs/code-issues/oversized-file-server.md`).

use serde_json::Value;
use tddy_discovery::subagent::TurnRequest;

/// The message a non-boolean `resetWorktree` is refused with.
const NOT_A_BOOLEAN: &str = "resetWorktree must be a boolean";

/// `request` with the caller's `resetWorktree` choice applied: absent or `true` keeps the default
/// (a rewind resets the worktree), `false` keeps every file.
pub(crate) fn with_reset_worktree_choice(
    request: TurnRequest,
    args: &Value,
) -> Result<TurnRequest, &'static str> {
    match args.get("resetWorktree") {
        None | Some(Value::Bool(true)) => Ok(request),
        Some(Value::Bool(false)) => Ok(request.keeping_worktree()),
        Some(_) => Err(NOT_A_BOOLEAN),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn a_rewinding_request() -> TurnRequest {
        TurnRequest::resuming()
    }

    #[test]
    fn a_call_without_reset_worktree_keeps_the_default_of_resetting() {
        let request = with_reset_worktree_choice(a_rewinding_request(), &json!({})).unwrap();

        assert!(request.resets_worktree());
    }

    #[test]
    fn reset_worktree_true_resets_the_worktree() {
        let args = json!({ "resetWorktree": true });

        let request = with_reset_worktree_choice(a_rewinding_request(), &args).unwrap();

        assert!(request.resets_worktree());
    }

    #[test]
    fn reset_worktree_false_keeps_the_worktree() {
        let args = json!({ "resetWorktree": false });

        let request = with_reset_worktree_choice(a_rewinding_request(), &args).unwrap();

        assert!(!request.resets_worktree());
    }

    #[test]
    fn a_non_boolean_reset_worktree_is_refused_by_name() {
        let args = json!({ "resetWorktree": "no" });

        let refused = with_reset_worktree_choice(a_rewinding_request(), &args).unwrap_err();

        assert_eq!(refused, "resetWorktree must be a boolean");
    }
}
