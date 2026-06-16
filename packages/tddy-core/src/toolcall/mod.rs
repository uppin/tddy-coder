//! IPC for tddy-tools relay: Unix socket listener, request/response types.
//!
//! tddy-tools connects to the socket, sends JSON line requests, receives JSON line responses.
//! **`submit`** is acknowledged on the wire immediately after persisting the payload; the listener
//! then `try_send`s [`ToolCallRequest::SubmitActivity`] for the activity log only. **`ask`** and
//! **`approve`** block until the presenter responds via oneshot channels (requires
//! [`crate::presenter::Presenter::poll_tool_calls`]).

pub mod build;
mod listener;

pub use build::{
    build_executor, register_build_executor, BuildExecutor, BuildListQuery, BuildOptions,
};
pub use listener::{set_toolcall_log_dir, start_toolcall_listener};

use std::sync::{Arc, Mutex};

/// Per-instance channel for submit results. Isolates parallel workflows (tests).
#[derive(Clone, Debug, Default)]
pub struct SubmitResultChannel {
    inner: Arc<Mutex<Option<(String, String)>>>,
}

impl SubmitResultChannel {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    pub fn store(&self, goal: &str, json_str: &str) {
        log::debug!(
            "[toolcall] channel store goal={} json_len={}",
            goal,
            json_str.len()
        );
        if let Ok(mut guard) = self.inner.lock() {
            *guard = Some((goal.to_string(), json_str.to_string()));
        }
    }

    pub fn take_for_goal(&self, goal: &str) -> Option<String> {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some((g, s)) = guard.take() {
                if g == goal {
                    log::debug!(
                        "[toolcall] channel take goal={} matched (len={})",
                        goal,
                        s.len()
                    );
                    return Some(s);
                }
                log::debug!(
                    "[toolcall] channel take goal={} no match (stored goal={})",
                    goal,
                    g
                );
                *guard = Some((g, s));
            } else {
                log::debug!("[toolcall] channel take goal={} empty", goal);
            }
        }
        None
    }
}

/// Process-global fallback for presenter/RPC flow (single workflow per process).
static TOOL_CALL_SUBMIT_RESULT: Mutex<Option<(String, String)>> = Mutex::new(None);

/// Store a submit result in the global (presenter/RPC path).
pub fn store_submit_result(goal: &str, json_str: &str) {
    log::debug!(
        "[toolcall] global store_submit_result goal={} json_len={}",
        goal,
        json_str.len()
    );
    if let Ok(mut guard) = TOOL_CALL_SUBMIT_RESULT.lock() {
        *guard = Some((goal.to_string(), json_str.to_string()));
    }
}

/// Take from the global (fallback when no per-instance channel).
pub fn take_submit_result_for_goal(goal: &str) -> Option<String> {
    if let Ok(mut guard) = TOOL_CALL_SUBMIT_RESULT.lock() {
        if let Some((g, s)) = guard.take() {
            if g == goal {
                log::debug!(
                    "[toolcall] global take goal={} matched (len={})",
                    goal,
                    s.len()
                );
                return Some(s);
            }
            log::debug!(
                "[toolcall] global take goal={} no match (stored goal={})",
                goal,
                g
            );
            *guard = Some((g, s));
        } else {
            log::debug!("[toolcall] global take goal={} empty", goal);
        }
    }
    None
}

use crate::ClarificationQuestion;
use serde::Deserialize;
use tokio::sync::oneshot;

/// Request from tddy-tools (internal, with response channel).
#[derive(Debug)]
pub enum ToolCallRequest {
    /// Presenter should log activity for a submit that was already acknowledged on the wire.
    /// The relay stores results and sends `SubmitOk` to `tddy-tools` immediately so the client
    /// never depends on the presenter loop being scheduled (avoids timeouts when `poll_workflow`
    /// holds the presenter lock for a long time).
    SubmitActivity {
        goal: String,
        data: serde_json::Value,
    },
    Ask {
        questions: Vec<ClarificationQuestion>,
        response_tx: oneshot::Sender<ToolCallResponse>,
    },
    Approve {
        tool_name: String,
        input: serde_json::Value,
        response_tx: oneshot::Sender<ToolCallResponse>,
    },
}

/// Response to tddy-tools (internal enum; serialized to wire format).
#[derive(Debug, Clone)]
pub enum ToolCallResponse {
    SubmitOk {
        goal: String,
    },
    SubmitError {
        errors: Vec<String>,
    },
    AskAnswer {
        answers: String,
    },
    ApproveResult {
        allow: bool,
    },
    Error {
        message: String,
    },
    /// Successful `list-actions` relay response.
    ActionsList {
        actions: serde_json::Value,
        total: usize,
    },
    /// Successful `invoke-action` relay response.
    ActionInvokeOk {
        record: serde_json::Value,
    },
    /// Failed `invoke-action` relay response (carries exit_code for the client).
    ActionInvokeError {
        message: String,
        exit_code: i32,
    },
    /// Successful `build` / `build-list` relay response (carries the full JSON
    /// object produced by the build executor; `status:"ok"` is ensured here).
    BuildJson {
        value: serde_json::Value,
    },
}

impl ToolCallResponse {
    pub fn to_json_line(&self) -> String {
        let wire = match self {
            ToolCallResponse::SubmitOk { goal } => {
                serde_json::json!({"status":"ok","goal":goal})
            }
            ToolCallResponse::SubmitError { errors } => {
                serde_json::json!({"status":"error","errors":errors})
            }
            ToolCallResponse::AskAnswer { answers } => {
                serde_json::json!({"status":"ok","answers":answers})
            }
            ToolCallResponse::ApproveResult { allow } => {
                serde_json::json!({"status":"ok","decision":if *allow { "allow" } else { "deny" }})
            }
            ToolCallResponse::Error { message } => {
                serde_json::json!({"status":"error","message":message})
            }
            ToolCallResponse::ActionsList { actions, total } => {
                serde_json::json!({"status":"ok","actions":actions,"total":total})
            }
            ToolCallResponse::ActionInvokeOk { record } => {
                serde_json::json!({"status":"ok","record":record})
            }
            ToolCallResponse::ActionInvokeError { message, exit_code } => {
                serde_json::json!({"status":"error","message":message,"exit_code":exit_code})
            }
            ToolCallResponse::BuildJson { value } => {
                let mut value = value.clone();
                if let serde_json::Value::Object(map) = &mut value {
                    map.entry("status".to_string())
                        .or_insert_with(|| serde_json::Value::String("ok".to_string()));
                }
                value
            }
        };
        wire.to_string()
    }
}

/// Wire format for submit request (from tddy-tools).
#[derive(Debug, Deserialize)]
pub struct SubmitRequestWire {
    pub r#type: String,
    pub goal: String,
    pub data: serde_json::Value,
}

/// Wire format for ask request (from tddy-tools).
#[derive(Debug, Deserialize)]
pub struct AskRequestWire {
    pub r#type: String,
    pub questions: Vec<ClarificationQuestion>,
}

/// Wire format for approve request (from MCP approval_prompt tool).
#[derive(Debug, Deserialize)]
pub struct ApproveRequestWire {
    pub r#type: String,
    pub tool_name: String,
    pub input: serde_json::Value,
}

/// Wire format for `list-actions` request (from tddy-tools).
#[derive(Debug, Deserialize)]
pub struct ListActionsRequestWire {
    pub r#type: String,
    /// Filter by relative-path prefix (e.g. `"packages/foo"`).
    #[serde(default)]
    pub path_prefix: Option<String>,
    /// Case-insensitive substring filter on id, summary, or path.
    #[serde(default)]
    pub query: Option<String>,
    /// Maximum actions to return (pagination).
    #[serde(default)]
    pub limit: Option<usize>,
    /// Zero-based offset into the sorted, filtered result.
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Wire format for `invoke-action` request (from tddy-tools).
#[derive(Debug, Deserialize)]
pub struct InvokeActionRequestWire {
    pub r#type: String,
    /// Relative path identifier of the action (e.g. `packages/foo/build` or `run-tests`).
    pub action: String,
    /// JSON-encoded arguments object.
    pub data: String,
}

/// Wire format for `build-list` request (from tddy-tools).
#[derive(Debug, Deserialize)]
pub struct BuildListRequestWire {
    pub r#type: String,
    /// Repository root to discover `BUILD.yaml` manifests in.
    pub repo_dir: String,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Wire format for `build` request (from tddy-tools).
#[derive(Debug, Deserialize)]
pub struct BuildRequestWire {
    pub r#type: String,
    /// Repository root to discover `BUILD.yaml` manifests in.
    pub repo_dir: String,
    /// Target id to build.
    pub target: String,
    #[serde(default)]
    pub no_cache: bool,
    #[serde(default)]
    pub dry_run: bool,
}

#[cfg(test)]
mod build_wire_tests {
    use super::{build_executor, BuildListRequestWire, BuildRequestWire, ToolCallResponse};
    use serde_json::json;

    #[test]
    fn build_list_request_wire_parses_full_and_minimal() {
        let full: BuildListRequestWire = serde_json::from_value(json!({
            "type": "build-list", "repo_dir": "/repo", "query": "q", "limit": 5, "offset": 2
        }))
        .unwrap();
        assert_eq!(full.repo_dir, "/repo");
        assert_eq!(full.query.as_deref(), Some("q"));
        assert_eq!(full.limit, Some(5));
        assert_eq!(full.offset, Some(2));

        let minimal: BuildListRequestWire =
            serde_json::from_value(json!({ "type": "build-list", "repo_dir": "/repo" })).unwrap();
        assert_eq!(minimal.query, None);
        assert_eq!(minimal.limit, None);
        assert_eq!(minimal.offset, None);
    }

    #[test]
    fn build_request_wire_parses_flags_with_defaults() {
        let w: BuildRequestWire = serde_json::from_value(json!({
            "type": "build", "repo_dir": "/repo", "target": "pkg:bin", "no_cache": true, "dry_run": true
        }))
        .unwrap();
        assert_eq!(w.target, "pkg:bin");
        assert!(w.no_cache && w.dry_run);

        let defaults: BuildRequestWire = serde_json::from_value(json!({
            "type": "build", "repo_dir": "/repo", "target": "pkg:bin"
        }))
        .unwrap();
        assert!(!defaults.no_cache && !defaults.dry_run);
    }

    #[test]
    fn build_json_response_ensures_status_ok() {
        let line = ToolCallResponse::BuildJson {
            value: json!({ "targets": [], "total": 0 }),
        }
        .to_json_line();
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["status"], "ok");
        assert_eq!(v["total"], 0);
    }

    #[test]
    fn build_json_response_preserves_existing_status() {
        let line = ToolCallResponse::BuildJson {
            value: json!({ "status": "error", "message": "boom" }),
        }
        .to_json_line();
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["status"], "error");
        assert_eq!(v["message"], "boom");
    }

    #[test]
    fn build_executor_unset_by_default_in_this_crate() {
        // tddy-core never registers an executor (that happens in tddy-coder), so the
        // relay handler reports "build support not enabled" here.
        assert!(build_executor().is_none());
    }
}
