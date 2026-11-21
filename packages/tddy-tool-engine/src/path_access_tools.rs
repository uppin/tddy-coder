//! `RequestPathAccess`: an orchestrator names the paths beyond its reach it needs, read or write,
//! and why; the operator allows or denies them from its session pane (`#orchestra` 6/9).
//!
//! A path the caller can already use at the asked mode — in its read reach, or inside a write grant
//! it holds — is answered at once and never put to the operator. For the rest the call waits for the
//! answer. Allowed paths are granted for the rest of the session, so the `Read`, `Grep`, `Glob` —
//! and, for a write grant, `Write`, `StrReplace` and `Delete` — that follow succeed there; denied or
//! unanswered ones stay out of reach. Feature doc:
//! docs/ft/daemon/project-concept.md § Orchestrator sessions.

use std::path::Path;

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
    ToolDef {
        name: REQUEST_PATH_ACCESS_TOOL.to_string(),
        description: "Ask the operator for access to paths outside your reach. Name each path in \
                      full (absolute), the mode (`read`, or `write` for read and write) and why. \
                      Paths you can already use at that mode are answered `in_reach` at once; for \
                      the rest the call waits for the operator's answer. An allowed path is yours for the \
                      rest of the session, together with everything under it: `Read`, `Grep` and \
                      `Glob` reach it and, for `write`, `Write`, `StrReplace` and `Delete` act \
                      there. A denied or unanswered request leaves the paths out of reach."
            .to_string(),
        input_schema_json: serde_json::json!({
            "type": "object",
            "properties": {
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1,
                    "description": "Absolute paths; each grant covers the path and everything under it."
                },
                "mode": {
                    "type": "string",
                    "enum": ["read", "write"],
                    "description": "`write` includes read."
                },
                "reason": {
                    "type": "string",
                    "description": "Why you need them, in your own words; the operator decides on it."
                }
            },
            "required": ["paths", "mode", "reason"],
            "additionalProperties": false
        })
        .to_string(),
    }
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

/// What the tool needs from the daemon: the caller's current reach, and holding the request for the
/// operator and waiting.
#[async_trait]
pub trait PathAccessPort: Send + Sync {
    /// Which of `paths` (absolute) `caller` cannot already use at `mode` — outside its read reach
    /// for `read`, outside every write grant it holds for `write`. Only these are put to the
    /// operator.
    async fn paths_outside_reach(
        &self,
        caller: &HomeCaller,
        paths: &[String],
        mode: RequestedAccess,
    ) -> Result<Vec<String>, String>;

    /// Record `args` for `caller`'s operator, announce it, and wait for the answer.
    async fn request_path_access(
        &self,
        caller: &HomeCaller,
        args: RequestPathAccessArgs,
    ) -> Result<PathAccessAnswer, String>;
}

/// Run [`REQUEST_PATH_ACCESS_TOOL`] for `caller`. A relative path, an empty list or an empty reason
/// is refused before anything is asked. Paths already usable at the asked mode are not asked for;
/// the answer comes back as `{"answer": "allowed" | "denied" | "unanswered" | "in_reach", "paths":
/// [the paths asked for], "in_reach": [the paths that were usable already]}` — `in_reach` when
/// nothing needed asking.
pub async fn execute_path_access_tool(
    port: &dyn PathAccessPort,
    caller: &HomeCaller,
    args_json: &str,
) -> ToolOutcome {
    let args: RequestPathAccessArgs = match serde_json::from_str(args_json) {
        Ok(args) => args,
        Err(e) => {
            return ToolOutcome::err(format!("{REQUEST_PATH_ACCESS_TOOL}: invalid args: {e}"))
        }
    };
    if args.paths.is_empty() {
        return ToolOutcome::err(format!(
            "{REQUEST_PATH_ACCESS_TOOL}: name at least one path"
        ));
    }
    if let Some(relative) = args.paths.iter().find(|p| !Path::new(p).is_absolute()) {
        return ToolOutcome::err(format!(
            "{REQUEST_PATH_ACCESS_TOOL}: {relative} is not an absolute path; name every path in full"
        ));
    }
    if args.reason.trim().is_empty() {
        return ToolOutcome::err(format!(
            "{REQUEST_PATH_ACCESS_TOOL}: say why you need the access; the operator decides on it"
        ));
    }
    match ask_for_what_is_outside(port, caller, args).await {
        Ok(answered) => ToolOutcome::ok(answered.to_string()),
        Err(e) => ToolOutcome::err(format!("{REQUEST_PATH_ACCESS_TOOL}: {e}")),
    }
}

/// Put the paths `caller` cannot already use to the operator, and say how it ended.
async fn ask_for_what_is_outside(
    port: &dyn PathAccessPort,
    caller: &HomeCaller,
    args: RequestPathAccessArgs,
) -> Result<serde_json::Value, String> {
    let outside = port
        .paths_outside_reach(caller, &args.paths, args.mode)
        .await?;
    let in_reach: Vec<String> = args
        .paths
        .iter()
        .filter(|p| !outside.contains(p))
        .cloned()
        .collect();
    if outside.is_empty() {
        return Ok(serde_json::json!({ "answer": "in_reach", "paths": [], "in_reach": in_reach }));
    }
    let asked = RequestPathAccessArgs {
        paths: outside.clone(),
        ..args
    };
    let answer = match port.request_path_access(caller, asked).await? {
        PathAccessAnswer::Allowed => "allowed",
        PathAccessAnswer::Denied => "denied",
        PathAccessAnswer::Unanswered => "unanswered",
    };
    Ok(serde_json::json!({ "answer": answer, "paths": outside, "in_reach": in_reach }))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// A port that answers `answer`, holds `in_reach` already, and records what it was asked.
    struct AnOperator {
        answer: PathAccessAnswer,
        in_reach: Vec<String>,
        asked: Mutex<Vec<RequestPathAccessArgs>>,
    }

    #[async_trait]
    impl PathAccessPort for AnOperator {
        async fn paths_outside_reach(
            &self,
            _caller: &HomeCaller,
            paths: &[String],
            _mode: RequestedAccess,
        ) -> Result<Vec<String>, String> {
            Ok(paths
                .iter()
                .filter(|p| !self.in_reach.contains(p))
                .cloned()
                .collect())
        }

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
            in_reach: Vec::new(),
            asked: Mutex::default(),
        }
    }

    fn an_operator_with_in_reach(answer: PathAccessAnswer, in_reach: &[&str]) -> AnOperator {
        AnOperator {
            in_reach: in_reach.iter().map(|p| p.to_string()).collect(),
            ..an_operator(answer)
        }
    }

    fn the_answer(outcome: &ToolOutcome) -> serde_json::Value {
        assert!(!outcome.is_error, "{}", outcome.error_message);
        serde_json::from_str(&outcome.result_json).unwrap()
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

        assert_eq!(
            the_answer(&outcome),
            serde_json::json!({ "answer": "allowed", "paths": ["/srv/shared"], "in_reach": [] })
        );
    }

    #[tokio::test]
    async fn paths_already_in_reach_are_answered_without_asking_the_operator() {
        let operator = an_operator_with_in_reach(PathAccessAnswer::Denied, &["/srv/shared"]);

        let outcome = execute_path_access_tool(
            &operator,
            &the_orchestrator(),
            r#"{"paths":["/srv/shared"],"mode":"read","reason":"read the fixtures"}"#,
        )
        .await;

        assert_eq!(
            the_answer(&outcome),
            serde_json::json!({ "answer": "in_reach", "paths": [], "in_reach": ["/srv/shared"] })
        );
        assert_eq!(operator.asked.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn only_the_paths_outside_reach_are_put_to_the_operator() {
        let operator = an_operator_with_in_reach(PathAccessAnswer::Allowed, &["/srv/shared"]);

        let outcome = execute_path_access_tool(
            &operator,
            &the_orchestrator(),
            r#"{"paths":["/srv/shared","/srv/other"],"mode":"write","reason":"update both"}"#,
        )
        .await;

        assert_eq!(
            the_answer(&outcome),
            serde_json::json!({
                "answer": "allowed", "paths": ["/srv/other"], "in_reach": ["/srv/shared"]
            })
        );
        let asked = operator.asked.lock().unwrap();
        assert_eq!(
            asked.iter().map(|a| a.paths.clone()).collect::<Vec<_>>(),
            vec![vec!["/srv/other".to_string()]]
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
