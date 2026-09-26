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

mod transcript;
mod turn_request;

use transcript::Transcript;

pub use transcript::{MessageDescriptor, MessageId, MessageRole, MESSAGE_PREVIEW_CHARS};
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
                // The window is **resolved here**, not forwarded as the caller wrote it. On this
                // path the file crosses the wire before anything on this side could trim it, so an
                // absent `limit` has to become the cap *in the request* — a cap applied after the
                // transfer bounds the context but not the wire, and a cap the daemon never hears
                // about bounds neither. `CodebaseAccess::Local` reaches the same defaults the other
                // way round, in `window_content`, because there the bytes are already in hand.
                let args = serde_json::json!({
                    "path": path,
                    "offset": offset.unwrap_or(0),
                    "limit": limit.unwrap_or(DEFAULT_READ_LINE_CAP as u64),
                });
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
}

impl ToolDispatch {
    /// A tool this subagent cannot run at all — unbound by its def, or not in the catalog. Not a
    /// transport failure, but not a result either: nothing was read.
    fn unavailable(reason: String) -> Self {
        ToolDispatch::NeverRan(SubagentError(reason))
    }

    /// The reason no result came back, or `None` when one did.
    fn produced_nothing(&self) -> Option<&SubagentError> {
        match self {
            ToolDispatch::Ran(_) => None,
            ToolDispatch::NeverRan(error) => Some(error),
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
        }
    }
}

/// What one `prompt()` call's tool calls have done so far — enough to answer, when the budget runs
/// out, whether this was a search or an outage.
#[derive(Default)]
struct ToolCallTally {
    ran: usize,
    produced_nothing: usize,
    /// The most recent reason no result came back — the state the prompt ended in, and the one
    /// worth quoting when every call ended that way.
    last_failure: Option<String>,
}

impl ToolCallTally {
    fn note(&mut self, dispatch: &ToolDispatch) {
        match dispatch.produced_nothing() {
            None => self.ran += 1,
            Some(reason) => {
                self.produced_nothing += 1;
                self.last_failure = Some(reason.0.clone());
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
    fn total_outage(&self) -> Option<String> {
        if self.ran > 0 {
            return None;
        }
        let reason = self.last_failure.as_deref()?;
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
        unknown => return ToolDispatch::unavailable(format!("unknown tool: {unknown}")),
    };

    match result {
        Ok(value) => ToolDispatch::Ran(value),
        Err(e) => ToolDispatch::NeverRan(e),
    }
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
        return Ok((
            TurnStep::FinalAnswer {
                outcome: PromptOutcome::new(
                    StopReason::EndTurn,
                    vec![ContentBlock::text(answer)],
                    turn_usage,
                ),
                message: ChatMessage::assistant(message.content.clone(), None),
            },
            turn_usage,
        ));
    }
    Ok((TurnStep::Continue(message), turn_usage))
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
        }
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
    /// (a typed error tool-result, not a silent execution and not a panic).
    async fn dispatch_bounded(&self, tool_call: &ToolCall) -> ToolDispatch {
        let bound = self
            .tools
            .iter()
            .any(|t| tool_name(*t) == tool_call.function.name);
        if !bound {
            return ToolDispatch::unavailable(format!(
                "tool '{}' is not bound for this subagent",
                tool_call.function.name
            ));
        }
        dispatch_tool_call(&self.access, tool_call).await
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
            "SpecializedSubagentSession",
        )
        .await?;
        self.note_context_occupancy(turn_usage);
        let message = match step {
            TurnStep::FinalAnswer { outcome, message } => {
                self.transcript.push(message);
                return Ok((Some(outcome), turn_usage));
            }
            TurnStep::Continue(message) => message,
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
                    self.transcript.push_marked(
                        ChatMessage::tool_result(
                            dispatch.tool_result_payload(),
                            tool_call.id.clone(),
                            tool_call.function.name.clone(),
                        ),
                        dispatch.produced_nothing().is_some(),
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
                        StopReason::EndTurn,
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
            "SpecializedSubagentSession synthesis",
        )
        .await?;
        self.note_context_occupancy(turn_usage);
        let content = match step {
            TurnStep::FinalAnswer { outcome, .. } => outcome.content,
            TurnStep::Continue(message) => {
                vec![ContentBlock::text(message.content.unwrap_or_default())]
            }
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
            StopReason::MaxTurnRequests,
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
            StopReason::MaxTurnRequests,
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
