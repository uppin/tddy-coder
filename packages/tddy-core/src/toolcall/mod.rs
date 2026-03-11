//! IPC for tddy-tools relay: Unix socket listener, request/response types.
//!
//! tddy-tools connects to the socket, sends JSON line requests, receives JSON line responses.
//! The listener forwards requests to the presenter via mpsc; responses are sent via oneshot.

mod listener;

pub use listener::start_toolcall_listener;

use std::sync::Mutex;

/// Shared storage for Submit results. Presenter writes, workflow reads.
/// Key: goal name, Value: JSON string of the submitted data.
static TOOL_CALL_SUBMIT_RESULT: Mutex<Option<(String, String)>> = Mutex::new(None);

/// Store a submit result for the workflow to use instead of parsing the stream.
pub fn store_submit_result(goal: &str, json_str: &str) {
    log::debug!(
        "[toolcall] store_submit_result goal={} json_len={}",
        goal,
        json_str.len()
    );
    if let Ok(mut guard) = TOOL_CALL_SUBMIT_RESULT.lock() {
        *guard = Some((goal.to_string(), json_str.to_string()));
    }
}

/// Take the stored submit result if it matches the goal.
pub fn take_submit_result_for_goal(goal: &str) -> Option<String> {
    if let Ok(mut guard) = TOOL_CALL_SUBMIT_RESULT.lock() {
        if let Some((g, s)) = guard.take() {
            if g == goal {
                log::debug!(
                    "[toolcall] take_submit_result_for_goal goal={} matched (len={})",
                    goal,
                    s.len()
                );
                return Some(s);
            }
            log::debug!(
                "[toolcall] take_submit_result_for_goal goal={} no match (stored goal={})",
                goal,
                g
            );
            *guard = Some((g, s));
        } else {
            log::debug!(
                "[toolcall] take_submit_result_for_goal goal={} store empty",
                goal
            );
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
    Submit {
        goal: String,
        data: serde_json::Value,
        response_tx: oneshot::Sender<ToolCallResponse>,
    },
    Ask {
        questions: Vec<ClarificationQuestion>,
        response_tx: oneshot::Sender<ToolCallResponse>,
    },
}

/// Response to tddy-tools (internal enum; serialized to wire format).
#[derive(Debug, Clone)]
pub enum ToolCallResponse {
    SubmitOk { goal: String },
    SubmitError { errors: Vec<String> },
    AskAnswer { answers: String },
    Error { message: String },
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
            ToolCallResponse::Error { message } => {
                serde_json::json!({"status":"error","message":message})
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
