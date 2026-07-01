//! Stateful subagent sessions exposed over MCP by `tddy-tools` (see
//! docs/ft/coder/managed-codebase-subagents.md). Unlike `FastContextBackend::invoke` (one-shot per
//! `InvokeRequest`), a `SubagentSession` is a long-lived conversation: `prompt()` can be called
//! repeatedly and each call sees the prior turns.
//!
//! `CodebaseAccess` lets the internal READ/GLOB/GREP tool loop read either the local filesystem
//! (`Local`) or a proxied codebase through an injected dispatch function (`Managed`) — the same
//! function `tddy-tools` uses for its exec-tool proxying — without `tddy-discovery` depending on
//! `tddy-tools`/`tddy-rpc`/`tddy-stdio`.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use regex::Regex;

use crate::discovery::extract_final_answer;
use crate::openai::{
    discovery_tool_definitions, ChatCompletionRequest, ChatMessage, OpenAiClient, ToolCall,
};

/// A single block of subagent response content — currently text-only, mirroring ACP's
/// `ContentBlock`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: String,
}

impl ContentBlock {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            block_type: "text".to_string(),
            text: text.into(),
        }
    }
}

/// Mirrors ACP's `PromptResponse.stopReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTurnRequests,
    Cancelled,
}

/// Result of one [`SubagentSession::prompt`] call — the loop's yield point.
#[derive(Debug, Clone)]
pub struct PromptOutcome {
    pub stop_reason: StopReason,
    pub content: Vec<ContentBlock>,
}

/// Error from a subagent session or the codebase-access layer it uses internally.
#[derive(Debug)]
pub struct SubagentError(String);

impl std::fmt::Display for SubagentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SubagentError {}

impl From<String> for SubagentError {
    fn from(message: String) -> Self {
        SubagentError(message)
    }
}

impl From<&str> for SubagentError {
    fn from(message: &str) -> Self {
        SubagentError(message.to_string())
    }
}

/// A live, stateful conversation with a subagent. One instance per conversation id.
#[async_trait]
pub trait SubagentSession: Send {
    async fn prompt(&mut self, text: &str) -> Result<PromptOutcome, SubagentError>;
}

/// Boxed async dispatch fn injected by the caller (`tddy-tools`) for managed codebase access.
/// Takes the capitalized tool name (`"Read"`/`"Glob"`/`"Grep"`) and its JSON args, and returns the
/// raw result JSON as a string (mirroring `session_tool_client::dispatch_session_tool`'s shape).
type ManagedDispatchFn = Arc<
    dyn Fn(String, serde_json::Value) -> Pin<Box<dyn Future<Output = String> + Send>> + Send + Sync,
>;

/// How a subagent's internal READ/GLOB/GREP tool calls reach the codebase.
pub enum CodebaseAccess {
    /// Direct host filesystem access (a co-located subagent).
    Local,
    /// Proxied through an injected dispatch function, keeping `tddy-discovery` free of any
    /// dependency on `tddy-tools`/`tddy-rpc`/`tddy-stdio`.
    Managed(ManagedDispatchFn),
}

impl CodebaseAccess {
    /// Build a [`CodebaseAccess::Managed`] from an async dispatch closure.
    pub fn managed<F>(dispatch: F) -> Self
    where
        F: Fn(String, serde_json::Value) -> Pin<Box<dyn Future<Output = String> + Send>>
            + Send
            + Sync
            + 'static,
    {
        CodebaseAccess::Managed(Arc::new(dispatch))
    }

    /// Parse a dispatch fn's raw result string, surfacing `is_error: true` responses as `Err`
    /// rather than returning the error envelope as if it were a successful result.
    fn parse_dispatch_result(result: &str) -> Result<serde_json::Value, SubagentError> {
        let value: serde_json::Value = serde_json::from_str(result)
            .map_err(|e| SubagentError(format!("invalid dispatch response JSON: {e}")))?;
        if value
            .get("is_error")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            let message = value
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("managed dispatch error");
            return Err(SubagentError(message.to_string()));
        }
        Ok(value)
    }

    pub async fn read(&self, path: &str) -> Result<serde_json::Value, SubagentError> {
        match self {
            CodebaseAccess::Local => {
                let content = std::fs::read_to_string(path)
                    .map_err(|e| SubagentError(format!("READ {path}: {e}")))?;
                Ok(serde_json::json!({ "content": content }))
            }
            CodebaseAccess::Managed(dispatch) => {
                let result =
                    dispatch("Read".to_string(), serde_json::json!({ "path": path })).await;
                Self::parse_dispatch_result(&result)
            }
        }
    }

    pub async fn glob(&self, pattern: &str) -> Result<serde_json::Value, SubagentError> {
        match self {
            CodebaseAccess::Local => {
                let mut paths: Vec<String> = Vec::new();
                for entry in glob::glob(pattern)
                    .map_err(|e| SubagentError(format!("GLOB pattern error: {e}")))?
                    .flatten()
                {
                    if let Some(s) = entry.to_str() {
                        paths.push(s.to_string());
                    }
                }
                Ok(serde_json::json!({ "paths": paths }))
            }
            CodebaseAccess::Managed(dispatch) => {
                let result = dispatch(
                    "Glob".to_string(),
                    serde_json::json!({ "pattern": pattern }),
                )
                .await;
                Self::parse_dispatch_result(&result)
            }
        }
    }

    pub async fn grep(
        &self,
        pattern: &str,
        path: Option<&str>,
    ) -> Result<serde_json::Value, SubagentError> {
        match self {
            CodebaseAccess::Local => {
                let re = Regex::new(pattern)
                    .map_err(|e| SubagentError(format!("GREP invalid regex {pattern:?}: {e}")))?;
                let mut matches: Vec<serde_json::Value> = Vec::new();
                let search_path = path.unwrap_or(".");
                let is_file = std::fs::metadata(search_path)
                    .map(|m| m.is_file())
                    .unwrap_or(false);
                if is_file {
                    grep_file(&re, search_path, &mut matches);
                } else {
                    grep_dir(&re, search_path, &mut matches);
                }
                Ok(serde_json::json!({ "matches": matches }))
            }
            CodebaseAccess::Managed(dispatch) => {
                let mut args = serde_json::json!({ "pattern": pattern });
                if let Some(p) = path {
                    args["path"] = serde_json::Value::String(p.to_string());
                }
                let result = dispatch("Grep".to_string(), args).await;
                Self::parse_dispatch_result(&result)
            }
        }
    }
}

fn grep_file(re: &Regex, path: &str, matches: &mut Vec<serde_json::Value>) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    for (i, line) in content.lines().enumerate() {
        if re.is_match(line) {
            matches.push(serde_json::json!({
                "type": "match",
                "data": {
                    "path": { "text": path },
                    "line_number": i + 1,
                    "lines": { "text": line }
                }
            }));
        }
    }
}

fn grep_dir(re: &Regex, dir: &str, matches: &mut Vec<serde_json::Value>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(path_str) = path.to_str() else {
            continue;
        };
        if path.is_file() {
            grep_file(re, path_str, matches);
        } else if path.is_dir() {
            grep_dir(re, path_str, matches);
        }
    }
}

/// Configuration for constructing a subagent session via [`SubagentRegistry::create`].
pub struct SubagentConfig {
    pub base_url: String,
    pub model: String,
    pub max_turns: u32,
    pub access: CodebaseAccess,
}

/// Dispatch one model-issued tool call against `access`, returning the raw JSON result (or a JSON
/// error envelope) as a string, ready to carry back as a `tool`-role message.
async fn dispatch_tool_call(access: &CodebaseAccess, tool_call: &ToolCall) -> String {
    let args: serde_json::Value =
        serde_json::from_str(&tool_call.function.arguments).unwrap_or(serde_json::Value::Null);

    let result = match tool_call.function.name.as_str() {
        "READ" => {
            let path = args["path"].as_str().unwrap_or("");
            access.read(path).await
        }
        "GLOB" => {
            let pattern = args["pattern"].as_str().unwrap_or("");
            access.glob(pattern).await
        }
        "GREP" => {
            let pattern = args["pattern"].as_str().unwrap_or("");
            let path = args["path"].as_str();
            access.grep(pattern, path).await
        }
        unknown => return format!("{{\"error\": \"unknown tool: {unknown}\"}}"),
    };

    match result {
        Ok(value) => value.to_string(),
        Err(e) => format!("{{\"error\": \"{e}\"}}"),
    }
}

/// Stateful FastContext discovery session: owns its message history across `prompt()` calls,
/// unlike `FastContextBackend::invoke`'s one-shot-per-`InvokeRequest` loop.
pub struct FastContextSession {
    client: OpenAiClient,
    model: String,
    max_turns: u32,
    access: CodebaseAccess,
    messages: Vec<ChatMessage>,
}

impl FastContextSession {
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        max_turns: u32,
        access: CodebaseAccess,
    ) -> Self {
        Self {
            client: OpenAiClient::new(base_url),
            model: model.into(),
            max_turns,
            access,
            messages: Vec::new(),
        }
    }
}

impl FastContextSession {
    /// One model round-trip: sends the current history, appends the response, and dispatches any
    /// tool calls. Returns `Some(outcome)` once the model yields a `<final_answer>`, or `None` to
    /// keep looping.
    async fn run_one_turn(&mut self) -> Result<Option<PromptOutcome>, SubagentError> {
        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: self.messages.clone(),
            tools: discovery_tool_definitions(),
            tool_choice: serde_json::json!("auto"),
            temperature: 0.0,
        };
        let response = self
            .client
            .complete(request)
            .await
            .map_err(|e| SubagentError(format!("FastContextSession: {e}")))?;
        let message = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| SubagentError("no choices in response".to_string()))?
            .message;

        if let Some(answer) = message
            .content
            .as_deref()
            .and_then(extract_final_answer)
            .filter(|a| !a.is_empty())
        {
            let answer = answer.to_string();
            self.messages
                .push(ChatMessage::assistant(message.content.clone(), None));
            return Ok(Some(PromptOutcome {
                stop_reason: StopReason::EndTurn,
                content: vec![ContentBlock::text(answer)],
            }));
        }

        match message.tool_calls {
            Some(ref tool_calls) if !tool_calls.is_empty() => {
                self.messages.push(ChatMessage::assistant(
                    message.content.clone(),
                    message.tool_calls.clone(),
                ));
                for tool_call in tool_calls {
                    let result_str = dispatch_tool_call(&self.access, tool_call).await;
                    self.messages.push(ChatMessage::tool_result(
                        result_str,
                        tool_call.id.clone(),
                        tool_call.function.name.clone(),
                    ));
                }
            }
            // No tool calls: either the field was absent, or present-but-empty (the same shape
            // Ollama sends for a plain-text turn) — both are a no-op assistant turn.
            _ => {
                self.messages
                    .push(ChatMessage::assistant(message.content.clone(), None));
            }
        }
        Ok(None)
    }
}

#[async_trait]
impl SubagentSession for FastContextSession {
    async fn prompt(&mut self, text: &str) -> Result<PromptOutcome, SubagentError> {
        self.messages.push(ChatMessage::user(text.to_string()));

        for _turn in 0..self.max_turns {
            if let Some(outcome) = self.run_one_turn().await? {
                return Ok(outcome);
            }
        }

        Ok(PromptOutcome {
            stop_reason: StopReason::MaxTurnRequests,
            content: Vec::new(),
        })
    }
}

type SessionFactory = Box<dyn Fn(SubagentConfig) -> Box<dyn SubagentSession> + Send + Sync>;

/// Name → factory registry for subagent sessions. Pluggable: `"fastcontext"` ships built in;
/// future subagents register under their own name.
pub struct SubagentRegistry {
    factories: HashMap<String, SessionFactory>,
}

impl SubagentRegistry {
    pub fn new() -> Self {
        let mut factories: HashMap<String, SessionFactory> = HashMap::new();
        factories.insert(
            "fastcontext".to_string(),
            Box::new(|config: SubagentConfig| -> Box<dyn SubagentSession> {
                Box::new(FastContextSession::new(
                    config.base_url,
                    config.model,
                    config.max_turns,
                    config.access,
                ))
            }) as SessionFactory,
        );
        Self { factories }
    }

    /// Create a session for `name`, or a [`SubagentError`] naming the unknown subagent.
    pub fn create(
        &self,
        name: &str,
        config: SubagentConfig,
    ) -> Result<Box<dyn SubagentSession>, SubagentError> {
        match self.factories.get(name) {
            Some(factory) => Ok(factory(config)),
            None => Err(SubagentError(format!("unknown subagent: {name}"))),
        }
    }
}

impl Default for SubagentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
