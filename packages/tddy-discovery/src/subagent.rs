//! Stateful subagent sessions exposed over MCP by `tddy-tools` (see
//! docs/ft/coder/managed-codebase-subagents.md). Unlike `SpecializedAgentBackend::invoke`
//! (one-shot per `InvokeRequest`), a `SubagentSession` is a long-lived conversation: `prompt()`
//! can be called repeatedly and each call sees the prior turns.
//!
//! `CodebaseAccess` lets the internal READ/GLOB/GREP tool loop read either the local filesystem
//! (`Local`) or a proxied codebase through an injected dispatch function (`Managed`) — the same
//! function `tddy-tools` uses for its exec-tool proxying — without `tddy-discovery` depending on
//! `tddy-tools`/`tddy-rpc`/`tddy-stdio`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use regex::Regex;

use crate::discovery::extract_final_answer;
use crate::openai::{
    discovery_tool_definitions, ChatCompletionRequest, ChatMessage, OpenAiClient, TokenUsage,
    ToolCall,
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
    /// Tokens spent by this `prompt()` call — the sum across every model turn it ran.
    pub usage: TokenUsage,
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

    /// The model this conversation talks to (e.g. an Ollama tag or a hosted model id).
    fn model(&self) -> &str;

    /// Running token total across every `prompt()` call made on this session.
    fn cumulative_usage(&self) -> TokenUsage;
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

    /// Read a file with the default line cap and no explicit window — a thin alias for
    /// `read_window(path, None, None)`.
    pub async fn read(&self, path: &str) -> Result<serde_json::Value, SubagentError> {
        self.read_window(path, None, None).await
    }

    /// Read a line window of a file, bounding how much content flows back into the model's context.
    ///
    /// `offset` is a 0-based starting line (default 0); `limit` is the maximum number of lines
    /// (default [`DEFAULT_READ_LINE_CAP`]). The result carries `truncated` (true when more lines
    /// follow the returned window) and `total_lines` (the file's true length) so the model can page
    /// with a follow-up `offset` instead of blindly re-reading. An un-windowed read of a file within
    /// the cap returns its bytes verbatim.
    pub async fn read_window(
        &self,
        path: &str,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> Result<serde_json::Value, SubagentError> {
        match self {
            CodebaseAccess::Local => {
                let content = std::fs::read_to_string(path)
                    .map_err(|e| SubagentError(format!("READ {path}: {e}")))?;
                Ok(window_content(&content, offset, limit))
            }
            CodebaseAccess::Managed(dispatch) => {
                let mut args = serde_json::json!({ "path": path });
                if let Some(offset) = offset {
                    args["offset"] = serde_json::json!(offset);
                }
                if let Some(limit) = limit {
                    args["limit"] = serde_json::json!(limit);
                }
                let result = dispatch("Read".to_string(), args).await;
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

    /// The mutation tools are Managed-only: the host tool engine confines every path to the
    /// session/repo roots, exactly as it does for the main agent's own writes. `Local` has no
    /// confinement layer, so a local-mode subagent must not be grantable unrestricted host
    /// writes by a YAML `tools:` entry alone.
    fn reject_local_mutation(tool: &str) -> SubagentError {
        SubagentError(format!(
            "{tool}: write tools require managed codebase access (local subagents are read-only)"
        ))
    }

    /// Write `contents` to `path` (Managed-only; see [`Self::reject_local_mutation`]).
    pub async fn write(
        &self,
        path: &str,
        contents: &str,
    ) -> Result<serde_json::Value, SubagentError> {
        match self {
            CodebaseAccess::Local => Err(Self::reject_local_mutation("WRITE")),
            CodebaseAccess::Managed(dispatch) => {
                let args = serde_json::json!({ "path": path, "contents": contents });
                let result = dispatch("Write".to_string(), args).await;
                Self::parse_dispatch_result(&result)
            }
        }
    }

    /// Replace the unique occurrence of `old_string` in `path` with `new_string` (Managed-only).
    pub async fn str_replace(
        &self,
        path: &str,
        old_string: &str,
        new_string: &str,
    ) -> Result<serde_json::Value, SubagentError> {
        match self {
            CodebaseAccess::Local => Err(Self::reject_local_mutation("STR_REPLACE")),
            CodebaseAccess::Managed(dispatch) => {
                let args = serde_json::json!({
                    "path": path,
                    "old_string": old_string,
                    "new_string": new_string,
                });
                let result = dispatch("StrReplace".to_string(), args).await;
                Self::parse_dispatch_result(&result)
            }
        }
    }

    /// Delete the file at `path` (Managed-only).
    pub async fn delete(&self, path: &str) -> Result<serde_json::Value, SubagentError> {
        match self {
            CodebaseAccess::Local => Err(Self::reject_local_mutation("DELETE")),
            CodebaseAccess::Managed(dispatch) => {
                let args = serde_json::json!({ "path": path });
                let result = dispatch("Delete".to_string(), args).await;
                Self::parse_dispatch_result(&result)
            }
        }
    }

    /// Run a shell command in the workspace (Managed-only, like the file-mutation tools: a command
    /// is free to write anywhere, so it needs the host tool engine's confinement).
    pub async fn shell(
        &self,
        command: &str,
        block_until_ms: Option<u64>,
    ) -> Result<serde_json::Value, SubagentError> {
        match self {
            CodebaseAccess::Local => Err(Self::reject_local_mutation("SHELL")),
            CodebaseAccess::Managed(dispatch) => {
                let mut args = serde_json::json!({ "command": command });
                if let Some(ms) = block_until_ms {
                    args["block_until_ms"] = serde_json::json!(ms);
                }
                let result = dispatch("Shell".to_string(), args).await;
                Self::parse_dispatch_result(&result)
            }
        }
    }

    /// The tools that exist only as host-engine capabilities: a `Local` subagent has no job table,
    /// no diagnostics feed and no semantic index of its own, so the honest answer is a typed
    /// refusal rather than an empty result that reads like "nothing found".
    fn reject_local_engine_tool(tool: &str) -> SubagentError {
        SubagentError(format!(
            "{tool}: this tool requires managed codebase access (the host tool engine provides it; \
             local subagents have no equivalent)"
        ))
    }

    /// Wait for a background shell job started by [`Self::shell`] (Managed-only; see
    /// [`Self::reject_local_engine_tool`]).
    pub async fn await_job(
        &self,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, SubagentError> {
        match self {
            CodebaseAccess::Local => Err(Self::reject_local_engine_tool("AWAIT")),
            CodebaseAccess::Managed(dispatch) => {
                let result = dispatch("Await".to_string(), args).await;
                Self::parse_dispatch_result(&result)
            }
        }
    }

    /// Read linting diagnostics for the workspace (Managed-only).
    pub async fn read_lints(&self, path: Option<&str>) -> Result<serde_json::Value, SubagentError> {
        match self {
            CodebaseAccess::Local => Err(Self::reject_local_engine_tool("READ_LINTS")),
            CodebaseAccess::Managed(dispatch) => {
                let mut args = serde_json::json!({});
                if let Some(p) = path {
                    args["path"] = serde_json::Value::String(p.to_string());
                }
                let result = dispatch("ReadLints".to_string(), args).await;
                Self::parse_dispatch_result(&result)
            }
        }
    }

    /// Search the codebase semantically (Managed-only).
    pub async fn semantic_search(
        &self,
        query: &str,
        path: Option<&str>,
    ) -> Result<serde_json::Value, SubagentError> {
        match self {
            CodebaseAccess::Local => Err(Self::reject_local_engine_tool("SEMANTIC_SEARCH")),
            CodebaseAccess::Managed(dispatch) => {
                let mut args = serde_json::json!({ "query": query });
                if let Some(p) = path {
                    args["path"] = serde_json::Value::String(p.to_string());
                }
                let result = dispatch("SemanticSearch".to_string(), args).await;
                Self::parse_dispatch_result(&result)
            }
        }
    }
}

/// Default number of lines a single un-windowed READ returns before truncating. Bounds how much
/// file content a subagent can pull into the model's context in one tool call.
const DEFAULT_READ_LINE_CAP: usize = 200;

/// Apply a line window to file `content`, returning `{content, truncated, total_lines}`.
///
/// `offset` (default 0) and `limit` (default [`DEFAULT_READ_LINE_CAP`]) select the returned lines.
/// When the whole file fits in the window (offset 0, all lines within `limit`), the original bytes
/// are returned verbatim so callers see the file exactly as-is; otherwise the selected lines are
/// re-joined with `\n`. `truncated` is true when lines follow the returned window.
fn window_content(content: &str, offset: Option<u64>, limit: Option<u64>) -> serde_json::Value {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let start = (offset.unwrap_or(0) as usize).min(total_lines);
    let max_lines = limit.map(|l| l as usize).unwrap_or(DEFAULT_READ_LINE_CAP);
    let end = start.saturating_add(max_lines).min(total_lines);
    let truncated = end < total_lines;

    // Whole file within the window: return the bytes verbatim (preserves trailing newline etc.).
    let windowed = if start == 0 && !truncated {
        content.to_string()
    } else {
        lines[start..end].join("\n")
    };

    serde_json::json!({
        "content": windowed,
        "truncated": truncated,
        "total_lines": total_lines,
    })
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

/// Deduped, canonical union of every def's own `replaces` list. Pure aggregation: a def's own
/// `replaces` is the only source of a withdrawn tool — no name is special-cased and no override
/// widens the set (see docs/ft/daemon/session-agent-roster.md § Tool replacement, without
/// behaviour).
pub fn resolve_replaced_tools_for_defs(
    defs: &[crate::agent_def::SpecializedAgentDef],
) -> Vec<String> {
    let combined: Vec<String> = defs.iter().flat_map(|def| def.replaces.clone()).collect();
    normalize_replaced_tools(&combined)
}

/// Configuration for constructing a subagent session via [`SubagentRegistry::create`]: everything
/// the *caller* knows and the def does not. The endpoint, model, credential and turn budget are
/// not here — they are the def's, and a caller able to override them could run a session against a
/// model the operator never configured while still reporting the def's name.
pub struct SubagentConfig {
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
            let offset = args["offset"].as_u64();
            let limit = args["limit"].as_u64();
            access.read_window(path, offset, limit).await
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
        "WRITE" => {
            let path = args["path"].as_str().unwrap_or("");
            let contents = args["contents"].as_str().unwrap_or("");
            access.write(path, contents).await
        }
        "STR_REPLACE" => {
            let path = args["path"].as_str().unwrap_or("");
            let old_string = args["old_string"].as_str().unwrap_or("");
            let new_string = args["new_string"].as_str().unwrap_or("");
            access.str_replace(path, old_string, new_string).await
        }
        "DELETE" => {
            let path = args["path"].as_str().unwrap_or("");
            access.delete(path).await
        }
        "SHELL" => {
            let command = args["command"].as_str().unwrap_or("");
            let block_until_ms = args["block_until_ms"].as_u64();
            access.shell(command, block_until_ms).await
        }
        // The engine's `Await` takes several optional selectors (job_id/task_id/timeouts); pass
        // the model's arguments through rather than re-deriving a subset here.
        "AWAIT" => access.await_job(args.clone()).await,
        "READ_LINTS" => {
            let path = args["path"].as_str();
            access.read_lints(path).await
        }
        "SEMANTIC_SEARCH" => {
            let query = args["query"].as_str().unwrap_or("");
            let path = args["path"].as_str();
            access.semantic_search(query, path).await
        }
        unknown => return format!("{{\"error\": \"unknown tool: {unknown}\"}}"),
    };

    match result {
        Ok(value) => value.to_string(),
        Err(e) => format!("{{\"error\": \"{e}\"}}"),
    }
}

/// Shared prefix of a subagent turn loop: send the current history, then short-circuit with
/// `EndTurn` if the model produced a non-empty `<final_answer>`. Returns `Ok(Some(outcome))` on a
/// final answer (the assistant message has already been appended to `messages`); returns
/// `Ok(None)` with `messages` unchanged otherwise, leaving the model's `ChatMessage` in
/// `last_message` for the caller to handle tool-calls / plain prose itself.
async fn send_turn_and_check_final_answer(
    client: &OpenAiClient,
    model: &str,
    messages: &mut Vec<ChatMessage>,
    tools: Vec<crate::openai::ToolDefinition>,
    error_context: &str,
) -> Result<(TurnStep, TokenUsage), SubagentError> {
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
    let turn_usage = response.usage.unwrap_or_default();
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
        return Ok((
            TurnStep::FinalAnswer(PromptOutcome {
                stop_reason: StopReason::EndTurn,
                content: vec![ContentBlock::text(answer)],
                usage: turn_usage,
            }),
            turn_usage,
        ));
    }
    Ok((TurnStep::Continue(message), turn_usage))
}

/// Result of [`send_turn_and_check_final_answer`] — either the loop is done, or the caller must
/// still handle the model's tool-calls / plain-prose message itself.
enum TurnStep {
    FinalAnswer(PromptOutcome),
    Continue(ChatMessage),
}

/// Maps a bound-tool kind to the model-facing tool name — the SCREAMING_SNAKE spelling the tool
/// loop advertises and [`dispatch_tool_call`] matches on (the same spelling a def's YAML `tools:`
/// list uses, unlike [`crate::agent_def::SubagentTool::catalog_name`]'s exec-catalog casing).
fn tool_name(tool: crate::agent_def::SubagentTool) -> &'static str {
    match tool {
        crate::agent_def::SubagentTool::Read => "READ",
        crate::agent_def::SubagentTool::Glob => "GLOB",
        crate::agent_def::SubagentTool::Grep => "GREP",
        crate::agent_def::SubagentTool::Write => "WRITE",
        crate::agent_def::SubagentTool::StrReplace => "STR_REPLACE",
        crate::agent_def::SubagentTool::Delete => "DELETE",
        crate::agent_def::SubagentTool::Shell => "SHELL",
        crate::agent_def::SubagentTool::Await => "AWAIT",
        crate::agent_def::SubagentTool::ReadLints => "READ_LINTS",
        crate::agent_def::SubagentTool::SemanticSearch => "SEMANTIC_SEARCH",
    }
}

/// A subagent session built from a [`crate::agent_def::SpecializedAgentDef`] — the only kind
/// there is, since every agent comes from a def an operator wrote. An optional system prompt seeds
/// the conversation, only the def's bound tools are advertised to (and dispatchable by) the model,
/// a plain-prose turn with no tool call and no `<final_answer>` terminates `EndTurn` rather than
/// continuing toward `max_turns` (a def is free not to follow any citation convention), and an
/// exhausted budget ends in one tool-less synthesis turn rather than in silence
/// ([`Self::run_synthesis_turn`]).
pub struct SpecializedSubagentSession {
    client: OpenAiClient,
    model: String,
    max_turns: u32,
    access: CodebaseAccess,
    messages: Vec<ChatMessage>,
    tools: Vec<crate::agent_def::SubagentTool>,
    cumulative: TokenUsage,
}

impl SpecializedSubagentSession {
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
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
            client: OpenAiClient::new(base_url).api_key(api_key),
            model: model.into(),
            max_turns,
            access,
            messages,
            tools,
            cumulative: TokenUsage::default(),
        }
    }

    /// Only the def's bound tools are advertised to the model. The filter base is the whole exec
    /// catalog — the read-only discovery trio, the mutation tools and the remaining engine tools —
    /// so a def that doesn't bind `WRITE`/`SHELL`/… never advertises them.
    fn tool_definitions(&self) -> Vec<crate::openai::ToolDefinition> {
        discovery_tool_definitions()
            .into_iter()
            .chain(crate::openai::mutation_tool_definitions())
            .chain(crate::openai::engine_tool_definitions())
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

    async fn run_one_turn(&mut self) -> Result<(Option<PromptOutcome>, TokenUsage), SubagentError> {
        let tools = self.tool_definitions();
        let (step, turn_usage) = send_turn_and_check_final_answer(
            &self.client,
            &self.model,
            &mut self.messages,
            tools,
            "SpecializedSubagentSession",
        )
        .await?;
        let message = match step {
            TurnStep::FinalAnswer(outcome) => return Ok((Some(outcome), turn_usage)),
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
                Ok((None, turn_usage))
            }
            // No tool call and no <final_answer> — plain prose. A specialized agent may simply
            // answer in prose on a single turn, so that prose is the answer rather than a reason
            // to keep spending turns.
            _ => {
                let content = message.content.clone().unwrap_or_default();
                self.messages
                    .push(ChatMessage::assistant(message.content.clone(), None));
                Ok((
                    Some(PromptOutcome {
                        stop_reason: StopReason::EndTurn,
                        content: vec![ContentBlock::text(content)],
                        usage: turn_usage,
                    }),
                    turn_usage,
                ))
            }
        }
    }

    /// The turn budget is spent with no `<final_answer>`. Rather than discard everything gathered
    /// so far and answer with nothing, spend one final turn asking the model to summarize what it
    /// already read. In incident 019f2d14 the model burned every turn tool-calling (each turn
    /// `content=0 chars`) and the caller got an empty answer back.
    ///
    /// The turn advertises **no tools**, so the model cannot keep searching — without that the
    /// budget is not a budget, it is one more search turn.
    ///
    /// The instruction is spliced into *this request* and never retained: the session is long-lived
    /// and multi-prompt, so an instruction left in the history would be replayed as prior context
    /// on the next `subagent_prompt` — and a model that honours it would stop calling tools for the
    /// rest of the conversation, on a budget that was just refilled. The summary it produces *is*
    /// kept: it answers the user prompt that is still in the history.
    async fn run_synthesis_turn(&mut self) -> Result<PromptOutcome, SubagentError> {
        let mut request_messages = self.messages.clone();
        request_messages.push(ChatMessage::user(
            "You have reached your search budget and may not call any more tools. \
             Summarize your findings now from what you have already read, citing the specific \
             file:line locations you found."
                .to_string(),
        ));

        let (step, turn_usage) = send_turn_and_check_final_answer(
            &self.client,
            &self.model,
            &mut request_messages,
            Vec::new(),
            "SpecializedSubagentSession synthesis",
        )
        .await?;
        let content = match step {
            TurnStep::FinalAnswer(outcome) => outcome.content,
            TurnStep::Continue(message) => {
                vec![ContentBlock::text(message.content.unwrap_or_default())]
            }
        };
        self.messages.push(ChatMessage::assistant(
            Some(
                content
                    .iter()
                    .map(|block| block.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            None,
        ));
        Ok(PromptOutcome {
            stop_reason: StopReason::MaxTurnRequests,
            content,
            usage: turn_usage,
        })
    }
}

#[async_trait]
impl SubagentSession for SpecializedSubagentSession {
    async fn prompt(&mut self, text: &str) -> Result<PromptOutcome, SubagentError> {
        self.messages.push(ChatMessage::user(text.to_string()));

        let mut call_usage = TokenUsage::default();
        for _turn in 0..self.max_turns {
            let (maybe_outcome, turn_usage) = self.run_one_turn().await?;
            call_usage = call_usage + turn_usage;
            if let Some(mut outcome) = maybe_outcome {
                outcome.usage = call_usage;
                self.cumulative = self.cumulative + call_usage;
                return Ok(outcome);
            }
        }

        // Budget exhausted — one tool-less turn, so the caller gets what was gathered rather than
        // nothing (see [`Self::run_synthesis_turn`]).
        //
        // The turns already spent are charged *before* that call: they were spent whatever it does,
        // and folding them in afterwards discards the whole prompt's usage when the one extra call
        // times out or is refused — the conversation's running total would then under-report every
        // turn the prompt paid for.
        self.cumulative = self.cumulative + call_usage;
        let synthesis = self.run_synthesis_turn().await?;
        self.cumulative = self.cumulative + synthesis.usage;
        let call_usage = call_usage + synthesis.usage;
        Ok(PromptOutcome {
            stop_reason: StopReason::MaxTurnRequests,
            content: synthesis.content,
            usage: call_usage,
        })
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn cumulative_usage(&self) -> TokenUsage {
        self.cumulative
    }
}

/// Name → def registry for subagent sessions: any number of
/// [`crate::agent_def::SpecializedAgentDef`]s, each resolved by its own `name`. Defs are the only
/// source — a registry cannot hold an agent nobody defined (see
/// docs/ft/daemon/session-agent-roster.md § Removing the hardcoded agents).
pub struct SubagentRegistry {
    defs: Vec<crate::agent_def::SpecializedAgentDef>,
}

impl SubagentRegistry {
    pub fn from_defs(defs: Vec<crate::agent_def::SpecializedAgentDef>) -> Self {
        Self { defs }
    }

    /// Create a session for `name`, or a [`SubagentError`] naming the unknown subagent.
    ///
    /// `config.access` is the whole configuration a caller supplies: it depends on the runtime
    /// transport rather than on the agent, while base URL, model, credential, turn budget, system
    /// prompt and bound tools all come from the def.
    pub fn create(
        &self,
        name: &str,
        config: SubagentConfig,
    ) -> Result<Box<dyn SubagentSession>, SubagentError> {
        if let Some(def) = self.defs.iter().find(|d| d.name == name) {
            return Ok(Box::new(SpecializedSubagentSession::new(
                def.base_url.clone(),
                def.model.clone(),
                def.api_key.clone(),
                def.max_turns,
                config.access,
                def.system_prompt.clone(),
                def.tools.clone(),
            )));
        }
        Err(SubagentError(format!("unknown subagent: {name}")))
    }
}
