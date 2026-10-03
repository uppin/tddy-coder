//! Reading the daemon's answer to a conversation-worktree operation a subagent's port asked for —
//! the reset ([`crate::worktree_reset_port`]) and the sync ([`crate::worktree_sync_port`]) answer in
//! the same envelope: an operation's JSON, or `{"error", "is_error": true}`.

use serde_json::Value;
use tddy_discovery::subagent::SubagentError;

/// The operation an answer belongs to, as its refusals name it.
pub(crate) struct WorktreeOperation {
    /// The noun in "the daemon's answer to the worktree `<name>`" — `reset`, `sync`.
    pub(crate) name: &'static str,
    /// What a daemon-reported error is prefixed with, e.g. "could not reset the conversation's
    /// worktree".
    pub(crate) failure: &'static str,
}

impl WorktreeOperation {
    /// The answer as JSON: refused when it is not JSON (quoting it) or when it is the daemon's error
    /// envelope (naming the daemon's error).
    pub(crate) fn parse(&self, answer: &str) -> Result<Value, SubagentError> {
        let parsed: Value = serde_json::from_str(answer).map_err(|e| {
            SubagentError::from(format!(
                "the daemon's answer to the worktree {} was not JSON ({e}): {answer}",
                self.name
            ))
        })?;
        if parsed.get("is_error") == Some(&Value::Bool(true)) {
            return Err(SubagentError::from(format!(
                "{}: {}",
                self.failure,
                parsed["error"].as_str().unwrap_or("unknown error")
            )));
        }
        Ok(parsed)
    }

    /// The refusal for an answer whose operation object does not have the expected shape.
    pub(crate) fn malformed(&self, error: serde_json::Error) -> SubagentError {
        SubagentError::from(format!(
            "the daemon's worktree {} was malformed: {error}",
            self.name
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYNC: WorktreeOperation = WorktreeOperation {
        name: "sync",
        failure: "could not sync",
    };

    #[test]
    fn an_answer_that_is_not_json_is_refused_quoting_it() {
        let refused = SYNC.parse("daemon unreachable").unwrap_err();

        assert_eq!(
            refused.to_string(),
            "the daemon's answer to the worktree sync was not JSON (expected value at line 1 \
             column 1): daemon unreachable"
        );
    }

    #[test]
    fn an_error_envelope_is_refused_naming_the_daemons_error() {
        let refused = SYNC
            .parse(r#"{"error":"no transport","is_error":true}"#)
            .unwrap_err();

        assert_eq!(refused.to_string(), "could not sync: no transport");
    }

    #[test]
    fn an_error_envelope_without_an_error_is_refused_as_unknown() {
        let refused = SYNC.parse(r#"{"is_error":true}"#).unwrap_err();

        assert_eq!(refused.to_string(), "could not sync: unknown error");
    }

    #[test]
    fn any_other_json_is_the_answer() {
        assert_eq!(
            SYNC.parse(r#"{"sync":null}"#).unwrap(),
            serde_json::json!({ "sync": null })
        );
    }
}
