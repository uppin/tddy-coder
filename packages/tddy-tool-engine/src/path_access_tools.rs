//! `RequestPathAccess`: an orchestrator names the paths beyond its reach it needs, read or write,
//! and why; the operator allows or denies them from its session pane (`#orchestra` 6/9).
//!
//! The call waits for the answer. Allowed paths are granted for the rest of the session, so the
//! `Read`, `Grep`, `Glob` — and, for a write grant, `Write`, `StrReplace` and `Delete` — that follow
//! succeed there; denied or unanswered ones stay out of reach. Feature doc:
//! docs/ft/daemon/project-concept.md § Orchestrator sessions.

use async_trait::async_trait;
use serde::Deserialize;
use tddy_worktree_service::session_target::HomeCaller;

use crate::catalog::ToolDef;
use crate::reach::REQUEST_PATH_ACCESS_TOOL;
use crate::ToolOutcome;

/// Whether `name` is [`REQUEST_PATH_ACCESS_TOOL`].
pub fn is_path_access_tool(name: &str) -> bool {
    name == REQUEST_PATH_ACCESS_TOOL
}

/// [`REQUEST_PATH_ACCESS_TOOL`]'s definition.
pub fn request_path_access_tool_def() -> ToolDef {
    // TODO(access-elicitation): implement
    unimplemented!("request_path_access_tool_def")
}

/// What access is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestedAccess {
    Read,
    /// Read and write.
    Write,
}

/// [`REQUEST_PATH_ACCESS_TOOL`]'s arguments.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestPathAccessArgs {
    /// Absolute paths; each grant covers the path and everything under it.
    pub paths: Vec<String>,
    pub mode: RequestedAccess,
    /// Why, in the agent's words: the operator decides on it.
    pub reason: String,
}

/// How the operator answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathAccessAnswer {
    Allowed,
    Denied,
    /// Nobody answered within the wait; treated as denied.
    Unanswered,
}

/// What the tool needs from the daemon: holding the request for the operator and waiting.
#[async_trait]
pub trait PathAccessPort: Send + Sync {
    /// Record `args` for `caller`'s operator, announce it, and wait for the answer.
    ///
    /// The default refuses, so a port that asks no operator need not name it.
    async fn request_path_access(
        &self,
        caller: &HomeCaller,
        args: RequestPathAccessArgs,
    ) -> Result<PathAccessAnswer, String> {
        let _ = (caller, args);
        Err("this host asks no operator for access".to_string())
    }
}

/// Run [`REQUEST_PATH_ACCESS_TOOL`] for `caller`. A relative path, an empty list or an empty reason
/// is refused before anything is asked; the answer comes back as `{"answer": "allowed" | "denied" |
/// "unanswered", "paths": [...]}`.
pub async fn execute_path_access_tool(
    port: &dyn PathAccessPort,
    caller: &HomeCaller,
    args_json: &str,
) -> ToolOutcome {
    // TODO(access-elicitation): implement
    let _ = (port, caller, args_json);
    unimplemented!("execute_path_access_tool")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// A port that answers `answer` and records what it was asked.
    struct AnOperator {
        answer: PathAccessAnswer,
        asked: Mutex<Vec<RequestPathAccessArgs>>,
    }

    #[async_trait]
    impl PathAccessPort for AnOperator {
        async fn request_path_access(
            &self,
            _caller: &HomeCaller,
            args: RequestPathAccessArgs,
        ) -> Result<PathAccessAnswer, String> {
            self.asked.lock().unwrap().push(args);
            Ok(self.answer)
        }
    }

    fn an_operator(answer: PathAccessAnswer) -> AnOperator {
        AnOperator {
            answer,
            asked: Mutex::default(),
        }
    }

    fn the_orchestrator() -> HomeCaller {
        HomeCaller {
            session_id: "01a1fdd8-0000-7000-8000-00000000000a".to_string(),
            project_id: "019d105b-ac0f-78d3-9a89-409731145a3a".to_string(),
        }
    }

    #[tokio::test]
    async fn an_allowed_request_answers_allowed_with_its_paths() {
        let operator = an_operator(PathAccessAnswer::Allowed);

        let outcome = execute_path_access_tool(
            &operator,
            &the_orchestrator(),
            r#"{"paths":["/srv/shared"],"mode":"read","reason":"read the fixtures"}"#,
        )
        .await;

        assert!(!outcome.is_error, "{}", outcome.error_message);
        let answer: serde_json::Value = serde_json::from_str(&outcome.result_json).unwrap();
        assert_eq!(
            answer,
            serde_json::json!({ "answer": "allowed", "paths": ["/srv/shared"] })
        );
    }

    #[tokio::test]
    async fn a_relative_path_is_refused_before_the_operator_is_asked() {
        let operator = an_operator(PathAccessAnswer::Allowed);

        let outcome = execute_path_access_tool(
            &operator,
            &the_orchestrator(),
            r#"{"paths":["shared"],"mode":"read","reason":"read the fixtures"}"#,
        )
        .await;

        assert!(outcome.is_error, "{}", outcome.result_json);
        assert_eq!(operator.asked.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn an_empty_reason_is_refused_before_the_operator_is_asked() {
        let operator = an_operator(PathAccessAnswer::Allowed);

        let outcome = execute_path_access_tool(
            &operator,
            &the_orchestrator(),
            r#"{"paths":["/srv/shared"],"mode":"write","reason":"  "}"#,
        )
        .await;

        assert!(outcome.is_error, "{}", outcome.result_json);
        assert_eq!(operator.asked.lock().unwrap().len(), 0);
    }

    #[test]
    fn the_tool_is_defined_under_its_name() {
        assert_eq!(
            request_path_access_tool_def().name,
            REQUEST_PATH_ACCESS_TOOL
        );
    }
}
