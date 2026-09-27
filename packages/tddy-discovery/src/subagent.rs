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

mod grep_context;
mod repeated_calls;
mod result_summary;
mod tool_arguments;
mod transcript;
mod turn_request;

use transcript::Transcript;

pub use grep_context::{ContextLine, GrepContext, GREP_CONTEXT_LINE_CEILING};
pub use repeated_calls::{RepeatedCall, RepeatedCalls, IDENTICAL_CALL_LIMIT};
pub use result_summary::{summarize, ResultSummary, SUMMARY_FIRST_LINE_CHARS};
pub use tool_arguments::{validate_tool_arguments, ArgumentProblem, ArgumentViolation};
pub use transcript::{
    MessageDescriptor, MessageId, MessageRole, ToolCallDescriptor, MESSAGE_PREVIEW_CHARS,
};
pub use turn_request::{TurnRequest, SUBAGENT_MAX_TURNS_CEILING, SUBAGENT_MIN_TURNS};

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
    /// The model refused the turn because its context window is full.
    ///
    /// A stop reason rather than an error, for the same reason [`Self::MaxTurnRequests`] is one:
    /// the caller should get the work that was gathered instead of a failure string. It is not a
    /// condition this conversation can recover from — its history is still oversized, so every
    /// further prompt re-sends it — so the outcome carries a handoff brief for a fresh one.
    ContextExhausted,
    /// The provider stopped generating because the turn hit [`SUBAGENT_MAX_OUTPUT_TOKENS`].
    ///
    /// Mirrors ACP's `max_tokens`. `finish_reason` was parsed and only logged until this existed,
    /// so a model cut off mid-sentence produced an outcome shaped exactly like a finished one —
    /// a truncated answer a caller had no way to tell from a whole one. What the model did
    /// produce still comes back: a cut answer is still work, and discarding it would make the
    /// caller re-run the turn to recover what it already said.
    MaxTokens,
}

/// Result of one [`SubagentSession::take_turn`] call — the loop's yield point.
#[derive(Debug, Clone)]
pub struct PromptOutcome {
    pub stop_reason: StopReason,
    pub content: Vec<ContentBlock>,
    /// Tokens spent by this call — the sum across every model turn it ran.
    pub usage: TokenUsage,
    /// The messages **this turn appended**, in the order they happened — not the whole history.
    ///
    /// What a caller reads to see what the agent actually did, and the only way it can learn an id
    /// to rewind to. A turn that is a `{stopReason, content, usage}` and nothing else cannot say
    /// that its 54 tool calls were all refused, which is how incident 2026-09-26 ran to two
    /// fabricated summaries with two readers watching.
    pub messages: Vec<MessageDescriptor>,
    /// The budget actually applied, when the caller asked for more than
    /// [`SUBAGENT_MAX_TURNS_CEILING`] and was given the ceiling instead; `None` when the caller got
    /// what it asked for.
    pub clamped_max_turns: Option<u32>,
}

impl PromptOutcome {
    /// An outcome with no per-turn transcript and no clamp to report — the shape of every
    /// construction site that is not a subagent turn loop.
    pub fn new(stop_reason: StopReason, content: Vec<ContentBlock>, usage: TokenUsage) -> Self {
        Self {
            stop_reason,
            content,
            usage,
            messages: Vec::new(),
            clamped_max_turns: None,
        }
    }
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
    /// Take one turn on this conversation: ask something new, carry on from where it stopped, or
    /// rewind to a named message and try again — see [`TurnRequest`].
    async fn take_turn(&mut self, request: TurnRequest) -> Result<PromptOutcome, SubagentError>;

    /// Ask this conversation a new question under the agent definition's own turn budget — the
    /// plain case, and the one nearly every caller wants.
    async fn prompt(&mut self, text: &str) -> Result<PromptOutcome, SubagentError>;

    /// The model this conversation talks to (e.g. an Ollama tag or a hosted model id).
    fn model(&self) -> &str;

    /// Running token total across every `prompt()` call made on this session.
    fn cumulative_usage(&self) -> TokenUsage;

    /// What this conversation's history currently costs to send — the most recent turn's prompt
    /// tokens.
    ///
    /// Occupancy, not spend. [`Self::cumulative_usage`]'s input figure is the sum of every turn's
    /// prompt, and every turn re-sends the whole history, so it is a sum of growing prefixes that
    /// over-counts the window several times over: a caller watching it cannot tell a nearly-full
    /// conversation from one that has merely run many cheap turns.
    fn context_tokens(&self) -> u64;

    /// The last `max_messages` messages of the history, oldest first, each truncated to a bound.
    ///
    /// Verbatim where the handoff brief is lossy: sometimes the useful thing is exactly where the
    /// agent was standing when it stopped. Answers for a conversation that has already exhausted
    /// its context — such a conversation is unpromptable, but it is not unreadable, and it stays in
    /// the open table until it is cancelled.
    ///
    /// Bounded per message so that reading the tail of one full context cannot fill the reader's
    /// own.
    fn tail(&self, max_messages: usize) -> Vec<String>;
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
    /// (default: every line that remains). The result carries `truncated` (true when more lines
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
                // Forwarded exactly as the model wrote it — including the absence of a window.
                // This layer used to substitute a 200-line cap for a missing `limit`, which made
                // an un-windowed read of a 960-line file return its first 200 lines with
                // `truncated: true`. A model that does not then page re-reads the same window
                // instead, and session 01a0e285 did precisely that nine times on one file while
                // its turn latency grew from 6s to 168s. Choosing the window is the model's job;
                // this layer's job is to carry the choice faithfully, and `truncated` /
                // `total_lines` come back either way so a model that does page still can.
                let mut args = serde_json::json!({ "path": path });
                if let Some(offset) = offset {
                    args["offset"] = offset.into();
                }
                if let Some(limit) = limit {
                    args["limit"] = limit.into();
                }
                let result = dispatch("Read".to_string(), args).await;
                Self::parse_dispatch_result(&result)
            }
        }
    }

    /// Match a glob with the default path cap and no explicit window — a thin alias for
    /// `glob_limited(pattern, None)`.
    pub async fn glob(&self, pattern: &str) -> Result<serde_json::Value, SubagentError> {
        self.glob_limited(pattern, None).await
    }

    /// Match a glob, bounding how many paths flow back into the model's context.
    ///
    /// `limit` is the greatest number of paths to return (default [`DEFAULT_GLOB_PATH_CAP`]). The
    /// result carries `truncated` (true when more paths follow the window) and `total_paths` (the
    /// match set's true size) so the model can narrow its pattern, or ask for a different window,
    /// instead of re-searching blind.
    ///
    /// The window resolves exactly the way [`Self::read_window`]'s does, and for the same reason:
    /// on the **managed** path the cap goes *into the request*, because the paths cross the wire
    /// before anything here could trim them — a cap applied after the transfer bounds the context
    /// but not the wire, and a cap the daemon never hears about bounds neither. On **Local** the
    /// paths are already in hand, so the cap is applied to them.
    pub async fn glob_limited(
        &self,
        pattern: &str,
        limit: Option<u64>,
    ) -> Result<serde_json::Value, SubagentError> {
        match self {
            CodebaseAccess::Local => {
                let mut paths: Vec<serde_json::Value> = Vec::new();
                for entry in glob::glob(pattern)
                    .map_err(|e| SubagentError(format!("GLOB pattern error: {e}")))?
                    .flatten()
                {
                    if let Some(s) = entry.to_str() {
                        paths.push(serde_json::Value::String(s.to_string()));
                    }
                }
                Ok(capped_results(
                    paths,
                    limit,
                    DEFAULT_GLOB_PATH_CAP,
                    "paths",
                    "total_paths",
                ))
            }
            CodebaseAccess::Managed(dispatch) => {
                let args = serde_json::json!({
                    "pattern": pattern,
                    "limit": limit.unwrap_or(DEFAULT_GLOB_PATH_CAP as u64),
                });
                let result = dispatch("Glob".to_string(), args).await;
                Self::parse_dispatch_result(&result)
            }
        }
    }

    /// Search with the default match cap and no explicit window — a thin alias for
    /// `grep_limited(pattern, path, None)`.
    pub async fn grep(
        &self,
        pattern: &str,
        path: Option<&str>,
    ) -> Result<serde_json::Value, SubagentError> {
        self.grep_limited(pattern, path, None).await
    }

    /// Search, bounding how many matches flow back into the model's context.
    ///
    /// `limit` is the greatest number of matches to return (default
    /// [`DEFAULT_GREP_MATCH_CAP`]); `truncated` and `total_matches` report what the window left
    /// out. The window resolves the same two ways [`Self::glob_limited`]'s does, for the same
    /// reason.
    pub async fn grep_limited(
        &self,
        pattern: &str,
        path: Option<&str>,
        limit: Option<u64>,
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
                Ok(capped_results(
                    matches,
                    limit,
                    DEFAULT_GREP_MATCH_CAP,
                    "matches",
                    "total_matches",
                ))
            }
            CodebaseAccess::Managed(dispatch) => {
                let mut args = serde_json::json!({
                    "pattern": pattern,
                    "limit": limit.unwrap_or(DEFAULT_GREP_MATCH_CAP as u64),
                });
                if let Some(p) = path {
                    args["path"] = serde_json::Value::String(p.to_string());
                }
                let result = dispatch("Grep".to_string(), args).await;
                Self::parse_dispatch_result(&result)
            }
        }
    }

    /// Search with context lines around each match — [`Self::grep_limited`] plus the
    /// `before`/`after` window the call asked for ([`grep_context::GrepContext`]).
    ///
    /// Every match entry gains its `context` lines — `{lineNumber, text, relation}` each,
    /// `before` then `after` in file order — while `truncated`/`total_matches` keep counting
    /// matches, exactly as they do without context.
    pub async fn grep_with_context(
        &self,
        pattern: &str,
        path: Option<&str>,
        limit: Option<u64>,
        context: grep_context::GrepContext,
    ) -> Result<serde_json::Value, SubagentError> {
        match self {
            CodebaseAccess::Local => {
                // TODO(grep-context): compute the context windows from the file's lines, in the
                // same shape the engine's event folding produces.
                let _ = context;
                self.grep_limited(pattern, path, limit).await
            }
            CodebaseAccess::Managed(dispatch) => {
                let mut args = serde_json::json!({
                    "pattern": pattern,
                    "before": context.before,
                    "after": context.after,
                });
                if let Some(p) = path {
                    args["path"] = serde_json::Value::String(p.to_string());
                }
                if let Some(l) = limit {
                    args["limit"] = serde_json::Value::from(l);
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

/// Default number of paths a single un-windowed GLOB returns before truncating.
///
/// 200 because a path is about the size of a line of source, so 200 of them cost a few thousand
/// tokens — affordable in the 32k window a fast local agent runs in, and enough to see the shape
/// of a directory. Session 01a0e200's `GLOB **/*` returned 408,282 bytes of tree into exactly
/// such a window.
///
/// Unlike `READ`, this one is still applied when the model names no `limit`: `GLOB` has no
/// `offset`, so a truncated match set cannot be paged and the cap is the only bound there is.
/// See `docs/dev/todo/2026-09-27-glob-and-grep-cannot-be-paged.md`.
pub const DEFAULT_GLOB_PATH_CAP: usize = 200;

/// Default number of matches a single un-windowed GREP returns before truncating.
///
/// Half [`DEFAULT_GLOB_PATH_CAP`] because a match is not a path: each one carries the matching
/// line's whole text alongside the file and line number, so it costs several times what a path
/// does, and 100 of them is the same order of context as 200 paths. The `GREP` that ran in
/// session 01a0e200's fatal turn returned 180,475 bytes.
pub const DEFAULT_GREP_MATCH_CAP: usize = 100;

/// Apply a result window to a search's `results`, as `{<field>, truncated, <total_field>}`.
///
/// `limit` defaults to `default_cap`, and the total is always the search's true size rather than
/// the window's — without it a model told its answer was cut has no idea by how much, and a
/// caller cannot tell a finished search from a clipped one.
fn capped_results(
    mut results: Vec<serde_json::Value>,
    limit: Option<u64>,
    default_cap: usize,
    field: &str,
    total_field: &str,
) -> serde_json::Value {
    let total = results.len();
    let wanted = limit.map_or(default_cap, |l| usize::try_from(l).unwrap_or(usize::MAX));
    let truncated = total > wanted;
    results.truncate(wanted);

    serde_json::json!({
        field: results,
        "truncated": truncated,
        total_field: total,
    })
}

/// Apply a line window to file `content`, returning `{content, truncated, total_lines}`.
///
/// `offset` (default 0) and `limit` (default: every line that remains) select the returned lines.
/// When the whole file fits in the window (offset 0, all lines within `limit`), the original bytes
/// are returned verbatim so callers see the file exactly as-is; otherwise the selected lines are
/// re-joined with `\n`. `truncated` is true when lines follow the returned window.
fn window_content(content: &str, offset: Option<u64>, limit: Option<u64>) -> serde_json::Value {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let start = (offset.unwrap_or(0) as usize).min(total_lines);
    let max_lines = limit.map(|l| l as usize).unwrap_or(usize::MAX);
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
    /// The system prompt for **this conversation**, replacing the def's own.
    ///
    /// A def's `system_prompt` is authored once, for every use of that agent. A caller delegating
    /// a specific piece of work knows things the def's author could not: which repository this
    /// is, what shape the answer has to take, and — after a turn that went wrong — what the agent
    /// must stop doing. The alternative is another user message, which the model weighs against
    /// everything already in its window; a system prompt is not.
    ///
    /// `None` keeps the def's prompt. The override replaces it rather than appending to it, so
    /// the model is never left weighing two sets of instructions, and it is scoped to the session
    /// it opens — the def is not edited by being used.
    pub system_prompt: Option<String>,
    /// The admission gate every model call this conversation makes waits at, keyed by the def's
    /// endpoint.
    ///
    /// The caller's because the contention is: which conversations share a provider is a fact
    /// about the host that opened them, not about any one def. A host that gives two
    /// conversations the same queue has made them contend on this side of the socket instead of
    /// inside the provider's.
    ///
    /// `None` reaches the model directly, exactly as every conversation did before this existed.
    pub provider_queue: Option<crate::subagent_runtime::ProviderQueue>,
}

impl SubagentConfig {
    /// The plain case: how this process reaches the codebase, and nothing else overridden.
    pub fn new(access: CodebaseAccess) -> Self {
        Self {
            access,
            system_prompt: None,
            provider_queue: None,
        }
    }

    /// Make this conversation's model calls queue on `queue`, one at a time per endpoint.
    #[must_use]
    pub fn with_provider_queue(mut self, queue: crate::subagent_runtime::ProviderQueue) -> Self {
        self.provider_queue = Some(queue);
        self
    }

    /// Replace the def's system prompt for the conversation this config opens.
    #[must_use]
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }
}

/// Refuse a system-prompt override that is present but says nothing.
///
/// The caller meant to send something, and an empty prefix reads in the transcript as a deliberate
/// silence. It matters beyond that on the wire: `OpenAgentConversationRequest.system_prompt` is a
/// proto3 string, where "absent" and "empty" are the same bytes — so a blank override handed to a
/// remotely-run agent would arrive as *no* override and the conversation would quietly run under
/// the def's prompt instead.
///
/// One function rather than a check at each site, because there are three: the registry that
/// builds a local turn loop, the client that sends an open to the facilitating daemon, and the
/// daemon that serves it. Three spellings of this rule would be three answers to the same call.
pub fn refuse_blank_system_prompt(prompt: Option<&str>) -> Result<(), SubagentError> {
    match prompt.is_some_and(|prompt| prompt.trim().is_empty()) {
        true => Err(SubagentError(
            "the system prompt override is blank: a conversation is opened with the agent's own \
             system prompt when none is given, so send the prompt you meant or omit it"
                .to_string(),
        )),
        false => Ok(()),
    }
}

/// What one model-issued tool call produced — and the thing a bare result string cannot say:
/// whether the tool **ran**.
///
/// The distinction is load-bearing, not cosmetic. A tool that ran and produced a result describes
/// a search that happened, whatever the result says. A call that produced no result at all — the
/// jail's tool channel refusing it, which [`CodebaseAccess::parse_dispatch_result`] surfaces from
/// the `is_error` envelope; a path that could not be read; a tool this subagent cannot run — read
/// nothing, and [`SpecializedSubagentSession::run_turn_loop`] must recognise a whole prompt of
/// those before it asks a model to summarize findings that do not exist.
///
/// Honest limit: on the managed path the `is_error` envelope is today the *only* signal there is,
/// so a tool that ran and reported its own failure is counted here as having produced no result.
/// That is why the rule built on this is "**nothing at all** came back", never "something failed":
/// a single unreadable file among successful reads is an ordinary result, and one prompt in which
/// every call failed is the case worth refusing whichever half of the ambiguity produced it.
enum ToolDispatch {
    /// The tool ran and produced a result.
    Ran(serde_json::Value),
    /// No result came back, and the reason why.
    NeverRan(SubagentError),
    /// The call was stopped before dispatch because its arguments do not match the schema the
    /// model was given — a rejection rather than a failure, and the only one of these three the
    /// model can fix by itself.
    ///
    /// Its own variant so the breakdown survives to the tool result: a rejection collapsed into
    /// a single error string can name the argument but cannot name the problem *and* the value
    /// for each of several at once, and one round trip per defect would cost a 32k agent its
    /// whole budget.
    Rejected {
        tool: String,
        violations: Vec<ArgumentViolation>,
    },
    /// The call was stopped before dispatch because this conversation has already made it to the
    /// limit and nothing has changed since (see [`RepeatedCalls`]).
    ///
    /// A rejection like [`Self::Rejected`] rather than a failure — the arguments are fine, and
    /// the model can fix this by asking something it does not already know the answer to.
    Repeated(RepeatedCall),
}

impl ToolDispatch {
    /// A tool this subagent cannot run at all — unbound by its def, or not in the catalog. Not a
    /// transport failure, but not a result either: nothing was read.
    fn unavailable(reason: String) -> Self {
        ToolDispatch::NeverRan(SubagentError(reason))
    }

    /// The reason no result came back, or `None` when one did.
    ///
    /// A rejected call is one of these: the codebase was never asked, so nothing was read.
    fn produced_nothing(&self) -> Option<String> {
        match self {
            ToolDispatch::Ran(_) => None,
            ToolDispatch::NeverRan(error) => Some(error.0.clone()),
            ToolDispatch::Rejected { tool, violations } => {
                Some(tool_arguments::rejection_reason(tool, violations))
            }
            ToolDispatch::Repeated(repeated) => Some(repeated.to_string()),
        }
    }

    /// The `tool`-role message body carried back to the model.
    ///
    /// Built with `serde_json` rather than `format!`: a failure text carrying a quote or a newline
    /// — a jail refusal quoting the command it would not run, say — would otherwise produce a tool
    /// result that is not valid JSON, and the model would be handed a broken payload on the one
    /// turn it most needs to read the reason.
    fn tool_result_payload(&self) -> String {
        match self {
            ToolDispatch::Ran(value) => value.to_string(),
            ToolDispatch::NeverRan(error) => serde_json::json!({ "error": error.0 }).to_string(),
            ToolDispatch::Rejected { tool, violations } => {
                tool_arguments::rejection_payload(tool, violations).to_string()
            }
            ToolDispatch::Repeated(repeated) => repeated.payload().to_string(),
        }
    }

    /// The structured facts of this dispatch's result, read from the tool the model called.
    ///
    /// Only a `Ran` dispatch carries a result with facts in it; every other shape produced
    /// nothing, and [`ResultSummary::Error`] is the summary that says so.
    fn summary(&self, tool: &str) -> ResultSummary {
        match self {
            ToolDispatch::Ran(value) => summarize(tool, value),
            ToolDispatch::NeverRan(_)
            | ToolDispatch::Rejected { .. }
            | ToolDispatch::Repeated(_) => ResultSummary::Error,
        }
    }
}

/// What one `prompt()` call's tool calls have done so far — enough to answer, when the budget runs
/// out, whether this was a search or an outage.
#[derive(Default)]
struct ToolCallTally {
    ran: usize,
    produced_nothing: usize,
    /// The most recent reason a call that **reached the codebase** came back with nothing — the
    /// state the prompt ended in, and the one worth quoting when every call ended that way.
    last_dispatch_failure: Option<String>,
    /// The most recent reason a call was stopped **before** the codebase was asked.
    ///
    /// Kept apart from [`Self::last_dispatch_failure`] because a refusal describes the
    /// conversation rather than the codebase, and it is usually downstream of the thing that
    /// actually went wrong: an agent whose tool channel is dead reaches for the same file until
    /// the repeat guard stops it, and a report quoting that guard would name the symptom and
    /// bury the closed channel that caused it.
    last_refusal: Option<String>,
}

impl ToolCallTally {
    fn note(&mut self, dispatch: &ToolDispatch) {
        let Some(reason) = dispatch.produced_nothing() else {
            self.ran += 1;
            return;
        };
        self.produced_nothing += 1;
        match dispatch {
            ToolDispatch::Rejected { .. } | ToolDispatch::Repeated(_) => {
                self.last_refusal = Some(reason)
            }
            ToolDispatch::Ran(_) | ToolDispatch::NeverRan(_) => {
                self.last_dispatch_failure = Some(reason)
            }
        }
    }

    /// The reason this prompt has nothing to summarize, when it has nothing: every tool call it
    /// made produced no result, so nothing was read and any citation would be invented.
    ///
    /// `None` when anything ran — one unreadable file among successful reads is an ordinary
    /// result — and `None` when the prompt called no tool at all, which is a model that answered
    /// in prose rather than a search that failed.
    ///
    /// Read at exactly one place — immediately before
    /// [`SpecializedSubagentSession::run_synthesis_turn`], whose instruction asks for "the
    /// specific file:line locations you found" and checks nothing. Asking that of a model that
    /// read nothing is an invitation to invent one, whichever way the reading failed, which is why
    /// the reasons are not sifted further. A prompt the **model** ended never reaches here: it
    /// answered of its own accord, having been told in its own tool results what came back, and
    /// that is a conversation rather than an outage.
    ///
    /// The reason quoted is the last **dispatch** failure when there was one, and a refusal only
    /// when nothing was ever dispatched — see [`Self::last_refusal`] for why that order and not
    /// simply the most recent of the two.
    fn total_outage(&self) -> Option<String> {
        if self.ran > 0 {
            return None;
        }
        let reason = self
            .last_dispatch_failure
            .as_deref()
            .or(self.last_refusal.as_deref())?;
        Some(format!(
            "nothing was read: all {} tool calls in this prompt produced no result, the last of \
             them failing with — {reason}",
            self.produced_nothing
        ))
    }
}

/// Dispatch one model-issued tool call against `access`, returning its result — or the reason no
/// result came back — as a [`ToolDispatch`], ready to carry back as a `tool`-role message.
async fn dispatch_tool_call(access: &CodebaseAccess, tool_call: &ToolCall) -> ToolDispatch {
    let args: serde_json::Value =
        serde_json::from_str(&tool_call.function.arguments).unwrap_or(serde_json::Value::Null);

    // Checked **before** dispatch, so a call the schema rejects never reaches the codebase. A
    // malformed path asked of the jail comes back as `file not found`, which is the same answer
    // a real missing file gives — the reading that sent session 01a0e200 round the identical
    // broken path twice.
    let violations = validate_tool_arguments(&tool_call.function.name, &args);
    if !violations.is_empty() {
        return ToolDispatch::Rejected {
            tool: tool_call.function.name.clone(),
            violations,
        };
    }

    let result = match tool_call.function.name.as_str() {
        "READ" => {
            let path = args["path"].as_str().unwrap_or("");
            let offset = args["offset"].as_u64();
            let limit = args["limit"].as_u64();
            access.read_window(path, offset, limit).await
        }
        "GLOB" => {
            let pattern = args["pattern"].as_str().unwrap_or("");
            let limit = args["limit"].as_u64();
            access.glob_limited(pattern, limit).await
        }
        "GREP" => {
            let pattern = args["pattern"].as_str().unwrap_or("");
            let path = args["path"].as_str();
            let limit = args["limit"].as_u64();
            access.grep_limited(pattern, path, limit).await
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
        unknown => return ToolDispatch::unavailable(format!("unknown tool: {unknown}")),
    };

    match result {
        Ok(value) => ToolDispatch::Ran(value),
        Err(e) => ToolDispatch::NeverRan(e),
    }
}

/// The greatest number of tokens a subagent may generate in one turn.
///
/// Session 01a0e200 had no such bound. Its 32k model entered a degenerate generation and never
/// emitted a stop token: `llama-server` reported `n_gen = 19790` and climbing at 24 t/s thirteen
/// minutes in, having already context-shifted once (`n_discard = 16381`, `n_keep = 4`) — which
/// discarded the whole prompt and guaranteed it would never recover. Nothing upstream could end
/// it: the turn budget counts *turns*, so it cannot fire inside one, and the in-jail tool timeout
/// bounds a tool call, not inference.
///
/// 4096 because it has to be comfortably above every legitimate turn and far below a runaway
/// one. A tool-calling turn is a sentence and a JSON object; the longest honest answer a subagent
/// gives is the synthesis turn's findings list, a few hundred tokens. 4096 is an eighth of the
/// 32k window a fast local agent runs in, so even a turn that spends the whole allowance leaves
/// the history room to survive it — and it ends a runaway in under three minutes at that
/// session's observed 24 t/s instead of never.
pub const SUBAGENT_MAX_OUTPUT_TOKENS: u32 = 4096;

/// Whether the provider stopped generating because the turn hit its token cap.
///
/// `"length"` is the OpenAI spelling, and the one Ollama and llama-server follow. Any other
/// reason — including none at all — is a turn the model ended itself.
fn was_cut_at_the_token_cap(finish_reason: Option<&str>) -> bool {
    finish_reason == Some("length")
}

/// Shared prefix of a subagent turn loop: send `messages` as the history, then short-circuit with
/// `EndTurn` if the model produced a non-empty `<final_answer>`.
///
/// Appends nothing itself. Both [`TurnStep`] variants hand the model's message back for the caller
/// to record, because the caller is the only one that knows where it belongs: the turn loop mints
/// an id for every message it keeps, and the synthesis turn keeps its instruction out of the
/// history on purpose.
async fn send_turn_and_check_final_answer(
    client: &OpenAiClient,
    model: &str,
    messages: &[ChatMessage],
    tools: Vec<crate::openai::ToolDefinition>,
    admission: Option<&crate::subagent_runtime::ProviderAdmission>,
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
        messages: messages.to_vec(),
        tools,
        tool_choice: serde_json::json!("auto"),
        temperature: 0.0,
        // Every turn this loop sends, the synthesis turn included — that one runs after the turn
        // budget is spent, which is exactly the position session 01a0e200's runaway occupied.
        max_tokens: Some(SUBAGENT_MAX_OUTPUT_TOKENS),
    };
    // Taken here and released the moment the call returns — including on the `?` below, which
    // drops it on the way out. The wait for it is this process's, and counted: a request handed
    // to a busy single-slot provider instead waits inside that provider's socket, which is where
    // session 01a0e200's second turn spent 38 minutes without executing a token.
    let slot = match admission {
        Some(admission) => Some(admission.hold().await.map_err(|e| {
            log::warn!(
                target: "tddy_discovery::subagent",
                "{error_context}: model={model} never reached the provider: {e}"
            );
            SubagentError(format!("{error_context}: {e}"))
        })?),
        None => None,
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
    drop(slot);
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
    // Carried out of here rather than only logged: a turn the provider cut at the cap otherwise
    // produces an outcome shaped exactly like a completed one.
    let cut_at_token_cap = was_cut_at_the_token_cap(choice.finish_reason.as_deref());

    if let Some(answer) = message
        .content
        .as_deref()
        .and_then(extract_final_answer)
        .filter(|a| !a.is_empty())
    {
        let answer = answer.to_string();
        return Ok((
            TurnStep::FinalAnswer {
                outcome: PromptOutcome::new(
                    turn_stop_reason(cut_at_token_cap),
                    vec![ContentBlock::text(answer)],
                    turn_usage,
                ),
                message: ChatMessage::assistant(message.content.clone(), None),
            },
            turn_usage,
        ));
    }
    Ok((
        TurnStep::Continue {
            message,
            cut_at_token_cap,
        },
        turn_usage,
    ))
}

/// How a turn that the model itself ended reports its stop reason — cut at the cap, or finished.
fn turn_stop_reason(cut_at_token_cap: bool) -> StopReason {
    match cut_at_token_cap {
        true => StopReason::MaxTokens,
        false => StopReason::EndTurn,
    }
}

/// Result of [`send_turn_and_check_final_answer`] — either the loop is done, or the caller must
/// still handle the model's tool-calls / plain-prose message itself. Neither has been recorded in
/// any history yet.
enum TurnStep {
    FinalAnswer {
        outcome: PromptOutcome,
        /// The assistant message that carried the answer, for the caller to record.
        message: ChatMessage,
    },
    Continue {
        message: ChatMessage,
        /// Whether the provider stopped generating this message at the token cap rather than
        /// because the model was done — so a caller that turns it into an outcome can say the
        /// answer is cut instead of passing a half-written one off as finished.
        cut_at_token_cap: bool,
    },
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

/// The phrasings providers use to refuse a turn because the context window is full.
///
/// Matching prose is fragile by nature, so the list is short and every entry is a **real** string a
/// provider emits: the first two are OpenAI's (`code: "context_length_exceeded"`, and the message
/// body that carries it), the third is Ollama's. A phrasing that is not here keeps today's
/// behaviour exactly — an error — so the cost of missing one is that a caller sees the provider's
/// own words, while the cost of over-reaching would be telling a caller to rebuild a conversation
/// that had nothing wrong with it.
const CONTEXT_REFUSAL_PHRASES: &[&str] = &[
    "context_length_exceeded",
    "maximum context length",
    "input length exceeds context length",
];

/// Whether a provider error is a context-length refusal rather than any other failure.
///
/// The error text is the whole surface there is: `OpenAiClient::complete` returns a
/// `Box<dyn Error>` wrapping `"OpenAI API error {status}: {body}"`, with the provider's JSON in the
/// body and no typed code of its own.
fn is_context_length_refusal(error: &SubagentError) -> bool {
    let text = error.0.to_lowercase();
    CONTEXT_REFUSAL_PHRASES
        .iter()
        .any(|phrase| text.contains(phrase))
}

/// How many messages the verbatim tail of an exhausted conversation carries.
///
/// The brief above it is lossy on purpose; this is the other half — the last exchanges exactly as
/// they happened, so the caller can see where the agent was standing when it stopped without a
/// second round trip to ask.
const EXHAUSTION_TAIL_MESSAGES: usize = 6;

/// How much of one message a [`SubagentSession::tail`] entry carries before it is cut.
///
/// A tail is read to recover from a context that filled; reading a few unbounded tool results to do
/// it would fill the reader's own.
const TAIL_MESSAGE_CHAR_CAP: usize = 2_000;

/// Truncate `text` to [`TAIL_MESSAGE_CHAR_CAP`] characters, saying how much was left out.
///
/// Counted in `char`s, not bytes: a byte slice through a multi-byte character panics.
fn truncate_for_tail(text: &str) -> String {
    let total = text.chars().count();
    if total <= TAIL_MESSAGE_CHAR_CAP {
        return text.to_string();
    }
    let kept: String = text.chars().take(TAIL_MESSAGE_CHAR_CAP).collect();
    let dropped = total - TAIL_MESSAGE_CHAR_CAP;
    format!("{kept}… [{dropped} more characters]")
}

/// Render one history message for a tail read: who said it, what they said (bounded), and any tool
/// calls it issued, by name and arguments.
///
/// The tool calls are appended after the bound rather than folded into it — they are a line each
/// and they are the part that says what the agent was doing, so a long message must not push them
/// out.
fn render_tail_message(message: &ChatMessage) -> String {
    let mut rendered = match (message.role.as_str(), message.name.as_deref()) {
        ("tool", Some(name)) => format!("tool ({name}): "),
        (role, _) => format!("{role}: "),
    };
    rendered.push_str(&truncate_for_tail(message.content.as_deref().unwrap_or("")));
    for call in message.tool_calls.iter().flatten() {
        rendered.push_str(&format!(
            "\n  → {} {}",
            call.function.name, call.function.arguments
        ));
    }
    rendered
}

/// One `## `-headed section of a handoff brief, or `when_empty` in place of an empty list — a
/// heading with nothing under it reads as lost content rather than as absent content.
fn brief_section(title: &str, entries: &[String], when_empty: &str) -> String {
    let mut section = format!("\n## {title}\n");
    if entries.is_empty() {
        section.push_str(when_empty);
        section.push('\n');
    } else {
        for entry in entries {
            section.push_str(entry);
            section.push('\n');
        }
    }
    section
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
    /// The agent definition's turn budget — what a call that names none of its own runs under.
    max_turns: u32,
    access: CodebaseAccess,
    /// The conversation so far, every message of it addressable by a [`MessageId`] so a caller can
    /// be told what a turn did and can send the conversation back to a point in it.
    transcript: Transcript,
    tools: Vec<crate::agent_def::SubagentTool>,
    cumulative: TokenUsage,
    /// Prompt tokens the most recent model turn reported — what the history costs to send now.
    /// See [`SubagentSession::context_tokens`].
    context_tokens: u64,
    /// Where this conversation queues for its endpoint, when the host that opened it gave it one.
    /// `None` calls the model directly.
    admission: Option<crate::subagent_runtime::ProviderAdmission>,
    /// What this conversation has already asked, so it cannot spend its budget asking again.
    ///
    /// On the conversation rather than inside the loop: session 01a0e285's nine identical reads
    /// were spread across the *turns* of one question, and a ledger scoped to a single turn
    /// would have seen one fresh call each time. It is cleared when the caller brings something
    /// new — see [`SubagentSession::take_turn`].
    repeated_calls: RepeatedCalls,
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
        let mut transcript = Transcript::default();
        if let Some(prompt) = system_prompt {
            transcript.push(ChatMessage::system(prompt));
        }
        Self {
            client: OpenAiClient::new(base_url).api_key(api_key),
            model: model.into(),
            max_turns,
            access,
            transcript,
            tools,
            cumulative: TokenUsage::default(),
            context_tokens: 0,
            admission: None,
            repeated_calls: RepeatedCalls::new(),
        }
    }

    /// Send every model call this conversation makes through `admission` — one call at a time on
    /// the endpoint it names, with the waiting counted here rather than inside the provider's
    /// socket (see [`crate::subagent_runtime::ProviderQueue`]).
    ///
    /// A builder rather than a constructor argument: a conversation with no queue is the plain
    /// case and stays the plain call.
    #[must_use]
    pub fn queued_on(mut self, admission: crate::subagent_runtime::ProviderAdmission) -> Self {
        self.admission = Some(admission);
        self
    }

    /// Record what the turn just sent cost, as this conversation's current occupancy.
    ///
    /// A turn whose provider reported no usage at all leaves the figure alone rather than zeroing
    /// it: silence about occupancy is not evidence of an empty window, and a conversation that
    /// reported 1200 tokens and then reported nothing has not shrunk.
    fn note_context_occupancy(&mut self, turn_usage: TokenUsage) {
        if turn_usage.input_tokens > 0 {
            self.context_tokens = turn_usage.input_tokens;
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
    /// (a typed error tool-result, not a silent execution and not a panic), and one this
    /// conversation has already made to the limit (see [`RepeatedCalls`]).
    ///
    /// Takes `&mut self` because the ledger is conversation state and the turn loop holds the
    /// conversation: a shared-mutability cell here would only be hiding that from the borrow
    /// checker, and there is nothing concurrent to justify it — a conversation runs one turn at
    /// a time, and one tool call at a time within it.
    async fn dispatch_bounded(&mut self, tool_call: &ToolCall) -> ToolDispatch {
        let tool = tool_call.function.name.clone();
        let bound = self.tools.iter().any(|t| tool_name(*t) == tool);
        if !bound {
            return ToolDispatch::unavailable(format!(
                "tool '{tool}' is not bound for this subagent"
            ));
        }
        if let Err(repeated) = self
            .repeated_calls
            .admit(&tool, &tool_call.function.arguments)
        {
            log::warn!(
                target: "tddy_discovery::subagent",
                "SpecializedSubagentSession: model={} refused a repeated call — {repeated}",
                self.model
            );
            return ToolDispatch::Repeated(repeated);
        }

        let dispatch = dispatch_tool_call(&self.access, tool_call).await;
        self.repeated_calls
            .record_outcome(&tool, dispatch.produced_nothing().is_none());
        dispatch
    }

    /// One pass of the turn loop, adding what its tool calls did to `tools_called`.
    async fn run_one_turn(
        &mut self,
        tools_called: &mut ToolCallTally,
    ) -> Result<(Option<PromptOutcome>, TokenUsage), SubagentError> {
        let tools = self.tool_definitions();
        let (step, turn_usage) = send_turn_and_check_final_answer(
            &self.client,
            &self.model,
            &self.transcript.messages(),
            tools,
            self.admission.as_ref(),
            "SpecializedSubagentSession",
        )
        .await?;
        self.note_context_occupancy(turn_usage);
        let (message, cut_at_token_cap) = match step {
            TurnStep::FinalAnswer { outcome, message } => {
                self.transcript.push(message);
                return Ok((Some(outcome), turn_usage));
            }
            TurnStep::Continue {
                message,
                cut_at_token_cap,
            } => (message, cut_at_token_cap),
        };

        match message.tool_calls {
            Some(ref tool_calls) if !tool_calls.is_empty() => {
                self.transcript.push(ChatMessage::assistant(
                    message.content.clone(),
                    message.tool_calls.clone(),
                ));
                for tool_call in tool_calls {
                    let dispatch = self.dispatch_bounded(tool_call).await;
                    tools_called.note(&dispatch);
                    let tool = tool_call.function.name.clone();
                    let result_summary = dispatch.summary(&tool);
                    self.transcript.push_tool_result(
                        ChatMessage::tool_result(
                            dispatch.tool_result_payload(),
                            tool_call.id.clone(),
                            tool,
                        ),
                        dispatch.produced_nothing().is_some(),
                        result_summary,
                    );
                }
                Ok((None, turn_usage))
            }
            // No tool call and no <final_answer> — plain prose. A specialized agent may simply
            // answer in prose on a single turn, so that prose is the answer rather than a reason
            // to keep spending turns.
            _ => {
                let content = message.content.clone().unwrap_or_default();
                self.transcript
                    .push(ChatMessage::assistant(message.content.clone(), None));
                Ok((
                    Some(PromptOutcome::new(
                        turn_stop_reason(cut_at_token_cap),
                        vec![ContentBlock::text(content)],
                        turn_usage,
                    )),
                    turn_usage,
                ))
            }
        }
    }

    /// The handoff brief for a conversation whose context window filled: what a fresh conversation
    /// needs to carry this work on without redoing it.
    ///
    /// Built mechanically from the history, because it must be: the context that would summarise
    /// this conversation is the context that is full, so there is no model call to make. One rule
    /// decides what survives —
    ///
    /// > Drop the payloads. Keep the conclusions and the index of what was already examined.
    ///
    /// so the goal, every assistant text block, and every tool call by name and arguments are kept,
    /// while tool **results** — the file contents and search output that are the bulk, and that
    /// filled the window — are dropped. The already-examined index is the half that actually saves
    /// the work: told only that it ran out of context, a replacement re-reads the same files and
    /// refills the same window.
    ///
    /// The system prompt is not carried: a new conversation with this same agent is seeded from the
    /// same def, so repeating it here would spend the new window on something already in it.
    ///
    /// Honest limit: an agent that narrated little and mostly called tools leaves thin conclusions.
    /// The index still prevents the repeated reads, which is the expensive half, but the new
    /// conversation may have to re-derive reasoning a faithful compaction cannot invent.
    fn handoff_brief(&self) -> String {
        let mut goals: Vec<String> = Vec::new();
        let mut findings: Vec<String> = Vec::new();
        let mut examined: Vec<String> = Vec::new();
        for message in self.transcript.iter() {
            let text = message.content.as_deref().unwrap_or("").trim();
            match message.role.as_str() {
                "user" => {
                    if !text.is_empty() {
                        goals.push(text.to_string());
                    }
                }
                "assistant" => {
                    if !text.is_empty() {
                        findings.push(format!("- {text}"));
                    }
                    for call in message.tool_calls.iter().flatten() {
                        examined.push(format!(
                            "- {} {}",
                            call.function.name, call.function.arguments
                        ));
                    }
                }
                // "tool" — the results. This is what is dropped, and it is the reason the brief
                // fits: a finding derived from a file is worth carrying where the file is not.
                // "system" — the def seeds the replacement conversation with it already.
                _ => {}
            }
        }

        let where_it_got_to = match self.transcript.last() {
            Some(message) if message.role == "tool" => format!(
                "The window filled on the turn after {}, so that result is not in this brief.",
                message.name.as_deref().unwrap_or("its last tool call")
            ),
            Some(message) if message.role == "assistant" => {
                "The window filled on the turn after the last finding above.".to_string()
            }
            _ => "The window filled before the agent acted on the request above.".to_string(),
        };

        let mut brief = String::from(
            "This specialized agent ran out of context. Its history is already larger than the \
             model's window, so this conversation cannot be continued — every further prompt \
             re-sends the same oversized history and fails the same way. Below is what it had done, \
             compacted: the tool results it read are dropped, because they are what filled the \
             window.\n",
        );
        brief.push_str(&brief_section("Goal", &goals, "(no prompt was recorded)"));
        brief.push_str(&brief_section(
            "Findings so far",
            &findings,
            "(the agent recorded no findings in prose)",
        ));
        brief.push_str(&brief_section(
            "Already examined — do not repeat",
            &examined,
            "(no tool calls were made)",
        ));
        brief.push_str(&brief_section(
            "Where it got to",
            &[where_it_got_to],
            "(unknown)",
        ));
        brief.push_str(&brief_section(
            "What to do next",
            &["Open a NEW conversation with this same specialized agent and pass this brief as its \
               first prompt, then continue from the open question. Do not repeat anything on the \
               already-examined list: those lookups have been made, and re-running them refills the \
               same window this one ran out of."
                .to_string()],
            "(unknown)",
        ));
        brief
    }

    /// The soft landing for a context refusal: the work gathered so far plus the brief that lets a
    /// fresh conversation carry it on, under [`StopReason::ContextExhausted`].
    ///
    /// The same shape [`Self::run_synthesis_turn`] gives a spent turn budget, for the same reason —
    /// the caller should receive what was gathered rather than a failure string.
    fn context_exhausted_outcome(&self, call_usage: TokenUsage) -> PromptOutcome {
        log::warn!(
            target: "tddy_discovery::subagent",
            "SpecializedSubagentSession: model={} refused the turn for a full context after {} messages; \
             landing with a handoff brief",
            self.model,
            self.transcript.len(),
        );
        // Both halves in one response: the compacted brief, and the verbatim tail under it. A
        // caller that has just been told its conversation is unusable should not need another call
        // to find out where it stopped.
        let mut tail = self.tail(EXHAUSTION_TAIL_MESSAGES);
        tail.insert(
            0,
            "## The last exchanges, verbatim (long messages cut)".to_string(),
        );
        PromptOutcome::new(
            StopReason::ContextExhausted,
            vec![
                ContentBlock::text(self.handoff_brief()),
                ContentBlock::text(tail.join("\n")),
            ],
            call_usage,
        )
    }

    /// The turn budget is spent with no `<final_answer>`. Rather than discard everything gathered
    /// so far and answer with nothing, spend one final turn asking the model to summarize what it
    /// already read. In incident 019f2d14 the model burned every turn tool-calling (each turn
    /// `content=0 chars`) and the caller got an empty answer back.
    ///
    /// The turn advertises **no tools**, so the model cannot keep searching — without that the
    /// budget is not a budget, it is one more search turn.
    ///
    /// It is reached only when at least one tool call produced a result. The instruction asks for
    /// "the specific file:line locations you found" and checks nothing, so on a prompt that read
    /// nothing it is an invitation to invent one — which is why [`Self::run_turn_loop`] errors out
    /// before calling this rather than sifting what it returns.
    ///
    /// The instruction is spliced into *this request* and never retained: the session is long-lived
    /// and multi-prompt, so an instruction left in the history would be replayed as prior context
    /// on the next `subagent_prompt` — and a model that honours it would stop calling tools for the
    /// rest of the conversation, on a budget that was just refilled. The summary it produces *is*
    /// kept: it answers the user prompt that is still in the history.
    async fn run_synthesis_turn(&mut self) -> Result<PromptOutcome, SubagentError> {
        let mut request_messages = self.transcript.messages();
        request_messages.push(ChatMessage::user(
            "You have reached your search budget and may not call any more tools. \
             Summarize your findings now from what you have already read, citing the specific \
             file:line locations you found."
                .to_string(),
        ));

        let (step, turn_usage) = send_turn_and_check_final_answer(
            &self.client,
            &self.model,
            &request_messages,
            Vec::new(),
            self.admission.as_ref(),
            "SpecializedSubagentSession synthesis",
        )
        .await?;
        self.note_context_occupancy(turn_usage);
        // A synthesis turn the provider cut is the one that matters most: it *is* the answer on
        // this path, so reporting it as a spent budget alone would hand the caller a summary
        // that stops mid-sentence with nothing saying so.
        let (content, cut_at_token_cap) = match step {
            TurnStep::FinalAnswer { outcome, .. } => {
                let cut = outcome.stop_reason == StopReason::MaxTokens;
                (outcome.content, cut)
            }
            TurnStep::Continue {
                message,
                cut_at_token_cap,
            } => (
                vec![ContentBlock::text(message.content.unwrap_or_default())],
                cut_at_token_cap,
            ),
        };
        self.transcript.push(ChatMessage::assistant(
            Some(
                content
                    .iter()
                    .map(|block| block.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            None,
        ));
        Ok(PromptOutcome::new(
            match cut_at_token_cap {
                true => StopReason::MaxTokens,
                false => StopReason::MaxTurnRequests,
            },
            content,
            turn_usage,
        ))
    }

    /// The turn loop proper, over a history [`SubagentSession::take_turn`] has already put in the
    /// shape this call should run against (prompted, resumed, or rewound and corrected).
    async fn run_turn_loop(&mut self, max_turns: u32) -> Result<PromptOutcome, SubagentError> {
        let mut call_usage = TokenUsage::default();
        let mut tools_called = ToolCallTally::default();
        for _turn in 0..max_turns {
            let (maybe_outcome, turn_usage) = match self.run_one_turn(&mut tools_called).await {
                Ok(turn) => turn,
                // A full context is a stop condition, not a failure: the caller gets what was
                // gathered, with a brief for a fresh conversation. The turns already spent are
                // charged first, exactly as the budget-exhausted path below charges them — they
                // were spent whatever this turn did.
                Err(e) if is_context_length_refusal(&e) => {
                    self.cumulative = self.cumulative + call_usage;
                    return Ok(self.context_exhausted_outcome(call_usage));
                }
                Err(e) => return Err(e),
            };
            call_usage = call_usage + turn_usage;
            if let Some(mut outcome) = maybe_outcome {
                outcome.usage = call_usage;
                self.cumulative = self.cumulative + call_usage;
                return Ok(outcome);
            }
        }

        // Budget exhausted — one tool-less turn, so the caller gets what was gathered rather than
        // nothing (see [`Self::run_synthesis_turn`]), unless nothing was gathered at all.
        //
        // The turns already spent are charged *before* that call: they were spent whatever it does,
        // and folding them in afterwards discards the whole prompt's usage when the one extra call
        // times out or is refused — the conversation's running total would then under-report every
        // turn the prompt paid for.
        self.cumulative = self.cumulative + call_usage;

        // A budget spent entirely on tool calls that produced nothing is a search that never
        // started, and it is the one case the synthesis turn must not be reached on: it asks for
        // "the specific file:line locations you found" without checking that anything was found,
        // and a model at `temperature: 0.0` obliges with an invented one (incident 2026-09-26).
        // So this is an error rather than a soft landing — and it is returned *before* any model
        // call, because the guarantee is that nothing was asked to summarize, not that its answer
        // was discarded afterwards.
        //
        // The conversation is left exactly as the failed prompt built it, and stays promptable:
        // its history is the record of what was attempted, and the caller that fixes the tool
        // channel can carry on from here.
        if let Some(nothing_was_read) = tools_called.total_outage() {
            log::warn!(
                target: "tddy_discovery::subagent",
                "SpecializedSubagentSession: model={} {nothing_was_read}",
                self.model
            );
            return Err(SubagentError(nothing_was_read));
        }

        let synthesis = match self.run_synthesis_turn().await {
            Ok(synthesis) => synthesis,
            // The synthesis turn re-sends the same history plus an instruction, so it can be
            // refused for the same reason a loop turn can. `call_usage` is already charged above.
            Err(e) if is_context_length_refusal(&e) => {
                return Ok(self.context_exhausted_outcome(call_usage))
            }
            Err(e) => return Err(e),
        };
        self.cumulative = self.cumulative + synthesis.usage;
        let call_usage = call_usage + synthesis.usage;
        Ok(PromptOutcome::new(
            synthesis.stop_reason,
            synthesis.content,
            call_usage,
        ))
    }
}

#[async_trait]
impl SubagentSession for SpecializedSubagentSession {
    /// Shape the history this call runs against — rewind, prompt, correct — then run the loop and
    /// report what it appended.
    ///
    /// The order is the contract. A rewind happens **before** anything is appended, so a
    /// correction lands after the rewind point rather than after a history the rewind was about to
    /// discard; and an unknown rewind point is refused here, before a single model call, because
    /// spending a turn on a request that was never valid is the silent-continue this refuses to be
    /// (AC17).
    async fn take_turn(&mut self, request: TurnRequest) -> Result<PromptOutcome, SubagentError> {
        let budget = request.budget_within(self.max_turns);
        // Anything that changes the conversation from outside the loop — a new question, a
        // correction, a rewind that discards answers it was holding — invalidates the premise
        // the repeat ledger rests on (see [`RepeatedCalls::forget_earlier_calls`]).
        if request.prompt_text().is_some()
            || request.correction().is_some()
            || request.rewind_point().is_some()
        {
            self.repeated_calls.forget_earlier_calls();
        }
        if let Some(rewind_point) = request.rewind_point() {
            self.transcript
                .rewind_to(rewind_point)
                .map_err(|e| SubagentError(e.to_string()))?;
        }
        // Taken after the rewind: what this turn appended is what is new relative to the history
        // it actually ran against.
        let appended_from = self.transcript.len();
        if let Some(text) = request.prompt_text() {
            self.transcript.push(ChatMessage::user(text.to_string()));
        }
        if let Some(correction) = request.correction() {
            self.transcript
                .push(ChatMessage::user(correction.to_string()));
        }

        let mut outcome = self.run_turn_loop(budget.turns).await?;
        outcome.messages = self.transcript.descriptors_from(appended_from);
        outcome.clamped_max_turns = budget.clamped_to;
        Ok(outcome)
    }

    async fn prompt(&mut self, text: &str) -> Result<PromptOutcome, SubagentError> {
        self.take_turn(TurnRequest::prompting(text)).await
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn cumulative_usage(&self) -> TokenUsage {
        self.cumulative
    }

    fn context_tokens(&self) -> u64 {
        self.context_tokens
    }

    fn tail(&self, max_messages: usize) -> Vec<String> {
        let messages = self.transcript.messages();
        let start = messages.len().saturating_sub(max_messages);
        messages[start..].iter().map(render_tail_message).collect()
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
    /// Base URL, model, credential, turn budget and bound tools all come from the def. The caller
    /// supplies how this process reaches the codebase, and may replace the def's system prompt
    /// for this one conversation ([`SubagentConfig::with_system_prompt`]) — the def itself is
    /// never edited, so a second conversation opened from this registry gets its prompt back.
    ///
    /// A blank override is refused rather than seeding an empty system message: the caller meant
    /// to send something, and an empty prefix reads in the transcript as a deliberate silence.
    pub fn create(
        &self,
        name: &str,
        config: SubagentConfig,
    ) -> Result<Box<dyn SubagentSession>, SubagentError> {
        refuse_blank_system_prompt(config.system_prompt.as_deref())?;
        if let Some(def) = self.defs.iter().find(|d| d.name == name) {
            let mut session = SpecializedSubagentSession::new(
                def.base_url.clone(),
                def.model.clone(),
                def.api_key.clone(),
                def.max_turns,
                config.access,
                config.system_prompt.or_else(|| def.system_prompt.clone()),
                def.tools.clone(),
            );
            if let Some(queue) = config.provider_queue {
                // The def's endpoint, because that is what this conversation will actually call:
                // two agents on one local Ollama contend, and two on different endpoints do not.
                // The waiter name is this conversation's alone — a name two conversations shared
                // would make the queue's own position report ambiguous between them.
                session = session.queued_on(crate::subagent_runtime::ProviderAdmission::new(
                    queue,
                    def.base_url.clone(),
                    format!("{}-{}", def.name, uuid::Uuid::new_v4()),
                ));
            }
            return Ok(Box::new(session));
        }
        Err(SubagentError(format!("unknown subagent: {name}")))
    }
}
