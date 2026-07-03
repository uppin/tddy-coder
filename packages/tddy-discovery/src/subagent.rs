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

/// Canonical exec-tool names a subagent can declare as replaced (mirrors
/// `tddy_sandbox::workspace_exec_tool_names()`; kept local to avoid a cross-crate dependency for a
/// name list).
const CANONICAL_EXEC_TOOL_NAMES: &[&str] = &[
    "Read",
    "Write",
    "StrReplace",
    "Delete",
    "Grep",
    "Glob",
    "Shell",
    "Await",
    "ReadLints",
    "SemanticSearch",
];

/// Normalize a list of free-form tool-name tokens against the canonical exec-tool catalog: trim,
/// case-insensitive match, canonical casing, drop unrecognized tokens (never fabricate a tool
/// name), de-duplicate preserving first-occurrence order.
pub fn normalize_replaced_tools(tools: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for token in tools {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if let Some(canonical) = CANONICAL_EXEC_TOOL_NAMES
            .iter()
            .find(|canonical| canonical.eq_ignore_ascii_case(token))
        {
            let canonical = canonical.to_string();
            if !out.contains(&canonical) {
                out.push(canonical);
            }
        }
    }
    out
}

/// Tools this subagent replaces on the main agent. Empty for unknown names — no fabricated tool
/// name, no panic. The `"fastcontext"` set derives from
/// [`crate::agent_def::builtin_fastcontext_def`]'s own `replaces` field (single source of truth —
/// no separate hardcoded literal to drift out of sync).
pub fn subagent_replaced_tools(name: &str) -> Vec<String> {
    match name {
        "fastcontext" => {
            normalize_replaced_tools(&crate::agent_def::builtin_fastcontext_def().replaces)
        }
        _ => Vec::new(),
    }
}

/// Effective replaced set for `name`: a non-empty `override_csv` replaces the declared default
/// outright (never merges with it); `None` or an empty override falls back to the default.
///
/// Override tokens are comma-separated, trimmed, matched case-insensitively against the
/// canonical exec-tool names, and normalized to that canonical casing. A token that doesn't match
/// a known exec tool is dropped rather than passed through — a typo must not silently disable
/// enforcement or invent a tool name.
pub fn resolve_replaced_tools(name: &str, override_csv: Option<&str>) -> Vec<String> {
    match override_csv.map(str::trim).filter(|s| !s.is_empty()) {
        Some(csv) => {
            let tokens: Vec<String> = csv.split(',').map(str::trim).map(str::to_string).collect();
            normalize_replaced_tools(&tokens)
        }
        None => subagent_replaced_tools(name),
    }
}

/// Deduped, canonical union of every def's own `replaces` list. Pure aggregation — no CSV-override
/// concept here, since each def's YAML is itself the user-editable source of truth (unlike the
/// single-name [`resolve_replaced_tools`], which supports a runtime override).
pub fn resolve_replaced_tools_for_defs(
    defs: &[crate::agent_def::SpecializedAgentDef],
) -> Vec<String> {
    let combined: Vec<String> = defs.iter().flat_map(|def| def.replaces.clone()).collect();
    normalize_replaced_tools(&combined)
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

/// Shared prefix of every subagent turn loop (`FastContextSession`/`SpecializedSubagentSession`):
/// send the current history, then short-circuit with `EndTurn` if the model produced a non-empty
/// `<final_answer>`. Returns `Ok(Some(outcome))` on a final answer (the assistant message has
/// already been appended to `messages`); returns `Ok(None)` with `messages` unchanged otherwise,
/// leaving the model's `ChatMessage` in `last_message` for the caller to handle tool-calls / plain
/// prose itself (those two cases differ between the two session types).
async fn send_turn_and_check_final_answer(
    client: &OpenAiClient,
    model: &str,
    messages: &mut Vec<ChatMessage>,
    tools: Vec<crate::openai::ToolDefinition>,
    error_context: &str,
) -> Result<TurnStep, SubagentError> {
    let message_count = messages.len();
    let tool_count = tools.len();
    log::info!(
        target: "tddy_discovery::subagent",
        "{error_context}: model={model} sending turn ({message_count} messages, {tool_count} tools)"
    );
    let request = ChatCompletionRequest {
        model: model.to_string(),
        messages: messages.clone(),
        tools,
        tool_choice: serde_json::json!("auto"),
        temperature: 0.0,
    };
    let started = std::time::Instant::now();
    let response = client.complete(request).await.map_err(|e| {
        log::warn!(
            target: "tddy_discovery::subagent",
            "{error_context}: model={model} request failed after {:.1?}: {e}",
            started.elapsed()
        );
        SubagentError(format!("{error_context}: {e}"))
    })?;
    let elapsed = started.elapsed();
    let choice = response.choices.into_iter().next().ok_or_else(|| {
        log::warn!(
            target: "tddy_discovery::subagent",
            "{error_context}: model={model} returned no choices after {elapsed:.1?}"
        );
        SubagentError("no choices in response".to_string())
    })?;
    let message = choice.message;
    log::info!(
        target: "tddy_discovery::subagent",
        "{error_context}: model={model} turn completed in {elapsed:.1?} (finish_reason={:?}, content={} chars, tool_calls={})",
        choice.finish_reason.as_deref().unwrap_or("<none>"),
        message.content.as_deref().map(str::len).unwrap_or(0),
        message.tool_calls.as_ref().map(Vec::len).unwrap_or(0),
    );

    if let Some(answer) = message
        .content
        .as_deref()
        .and_then(extract_final_answer)
        .filter(|a| !a.is_empty())
    {
        let answer = answer.to_string();
        messages.push(ChatMessage::assistant(message.content.clone(), None));
        return Ok(TurnStep::FinalAnswer(PromptOutcome {
            stop_reason: StopReason::EndTurn,
            content: vec![ContentBlock::text(answer)],
        }));
    }
    Ok(TurnStep::Continue(message))
}

/// Result of [`send_turn_and_check_final_answer`] — either the loop is done, or the caller must
/// still handle the model's tool-calls / plain-prose message itself.
enum TurnStep {
    FinalAnswer(PromptOutcome),
    Continue(ChatMessage),
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
        let message = match send_turn_and_check_final_answer(
            &self.client,
            &self.model,
            &mut self.messages,
            discovery_tool_definitions(),
            "FastContextSession",
        )
        .await?
        {
            TurnStep::FinalAnswer(outcome) => return Ok(Some(outcome)),
            TurnStep::Continue(message) => message,
        };

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

/// Maps a bound-tool kind to the model-facing tool name (`"READ"`/`"GLOB"`/`"GREP"`).
fn tool_name(tool: crate::agent_def::SubagentTool) -> &'static str {
    match tool {
        crate::agent_def::SubagentTool::Read => "READ",
        crate::agent_def::SubagentTool::Glob => "GLOB",
        crate::agent_def::SubagentTool::Grep => "GREP",
    }
}

/// A subagent session built from a YAML-defined [`crate::agent_def::SpecializedAgentDef`] rather
/// than the single hardcoded `"fastcontext"` factory (see [`FastContextSession`]). Generalizes
/// that session in three ways: an optional system prompt seeds the conversation, only the def's
/// bound tools are advertised to (and dispatchable by) the model, and a plain-prose turn with no
/// tool call and no `<final_answer>` terminates `EndTurn` instead of continuing toward
/// `max_turns` — useful for a model that doesn't follow FastContext's citation convention.
pub struct SpecializedSubagentSession {
    client: OpenAiClient,
    model: String,
    max_turns: u32,
    access: CodebaseAccess,
    messages: Vec<ChatMessage>,
    tools: Vec<crate::agent_def::SubagentTool>,
}

impl SpecializedSubagentSession {
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        max_turns: u32,
        access: CodebaseAccess,
        system_prompt: Option<String>,
        tools: Vec<crate::agent_def::SubagentTool>,
    ) -> Self {
        let mut messages = Vec::new();
        if let Some(prompt) = system_prompt {
            messages.push(ChatMessage::system(prompt));
        }
        Self {
            client: OpenAiClient::new(base_url),
            model: model.into(),
            max_turns,
            access,
            messages,
            tools,
        }
    }

    /// Only the def's bound tools are advertised to the model.
    fn tool_definitions(&self) -> Vec<crate::openai::ToolDefinition> {
        discovery_tool_definitions()
            .into_iter()
            .filter(|d| self.tools.iter().any(|t| tool_name(*t) == d.function.name))
            .collect()
    }

    /// Dispatches a model-issued tool call, rejecting one that names a tool the def did not bind
    /// (a typed error tool-result, not a silent execution and not a panic).
    async fn dispatch_bounded(&self, tool_call: &ToolCall) -> String {
        let bound = self
            .tools
            .iter()
            .any(|t| tool_name(*t) == tool_call.function.name);
        if !bound {
            return format!(
                "{{\"error\": \"tool '{}' is not bound for this subagent\"}}",
                tool_call.function.name
            );
        }
        dispatch_tool_call(&self.access, tool_call).await
    }

    async fn run_one_turn(&mut self) -> Result<Option<PromptOutcome>, SubagentError> {
        let tools = self.tool_definitions();
        let message = match send_turn_and_check_final_answer(
            &self.client,
            &self.model,
            &mut self.messages,
            tools,
            "SpecializedSubagentSession",
        )
        .await?
        {
            TurnStep::FinalAnswer(outcome) => return Ok(Some(outcome)),
            TurnStep::Continue(message) => message,
        };

        match message.tool_calls {
            Some(ref tool_calls) if !tool_calls.is_empty() => {
                self.messages.push(ChatMessage::assistant(
                    message.content.clone(),
                    message.tool_calls.clone(),
                ));
                for tool_call in tool_calls {
                    let result_str = self.dispatch_bounded(tool_call).await;
                    self.messages.push(ChatMessage::tool_result(
                        result_str,
                        tool_call.id.clone(),
                        tool_call.function.name.clone(),
                    ));
                }
                Ok(None)
            }
            // No tool call and no <final_answer> — plain prose. Unlike FastContextSession (which
            // keeps looping toward max_turns on such a turn, matching the citation convention it
            // expects), a generic specialized agent may simply answer in prose on a single turn —
            // treat that prose as the answer instead of forcing max_turns.
            _ => {
                let content = message.content.clone().unwrap_or_default();
                self.messages
                    .push(ChatMessage::assistant(message.content.clone(), None));
                Ok(Some(PromptOutcome {
                    stop_reason: StopReason::EndTurn,
                    content: vec![ContentBlock::text(content)],
                }))
            }
        }
    }
}

#[async_trait]
impl SubagentSession for SpecializedSubagentSession {
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
///
/// `defs` (populated via [`SubagentRegistry::from_defs`]) is the generalized path — see
/// docs/ft/coder/specialized-subagents.md: any number of YAML-defined
/// [`crate::agent_def::SpecializedAgentDef`]s, not just the one hardcoded `factories` entry.
pub struct SubagentRegistry {
    factories: HashMap<String, SessionFactory>,
    defs: Vec<crate::agent_def::SpecializedAgentDef>,
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
        Self {
            factories,
            defs: Vec::new(),
        }
    }

    /// Build a registry backed by YAML-defined [`crate::agent_def::SpecializedAgentDef`]s instead
    /// of the single hardcoded `"fastcontext"` factory — any number of defs, each resolved by its
    /// own `name`. See docs/ft/coder/specialized-subagents.md.
    pub fn from_defs(defs: Vec<crate::agent_def::SpecializedAgentDef>) -> Self {
        Self {
            factories: HashMap::new(),
            defs,
        }
    }

    /// Create a session for `name`, or a [`SubagentError`] naming the unknown subagent.
    ///
    /// `config.access` is always honored (it depends on the runtime transport, not on a static
    /// def); when `name` resolves through `defs` rather than the legacy `factories` map,
    /// `config.base_url`/`model`/`max_turns` are ignored in favor of the def's own values.
    pub fn create(
        &self,
        name: &str,
        config: SubagentConfig,
    ) -> Result<Box<dyn SubagentSession>, SubagentError> {
        if let Some(factory) = self.factories.get(name) {
            return Ok(factory(config));
        }
        if let Some(def) = self.defs.iter().find(|d| d.name == name) {
            return Ok(Box::new(SpecializedSubagentSession::new(
                def.base_url.clone(),
                def.model.clone(),
                def.max_turns,
                config.access,
                def.system_prompt.clone(),
                def.tools.clone(),
            )));
        }
        Err(SubagentError(format!("unknown subagent: {name}")))
    }
}

impl Default for SubagentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
