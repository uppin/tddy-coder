//! OpenAI `/v1/chat/completions` client for the multi-turn subagent loops.

use serde::{Deserialize, Serialize};

/// Token usage for one model call, normalized from the OpenAI/Ollama `usage` object
/// (`prompt_tokens` → input, `completion_tokens` → output).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl TokenUsage {
    /// Total tokens billed for the call — input plus output.
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

impl std::ops::Add for TokenUsage {
    type Output = TokenUsage;

    fn add(self, other: TokenUsage) -> TokenUsage {
        TokenUsage {
            input_tokens: self.input_tokens + other.input_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
        }
    }
}

/// A message in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn assistant(content: Option<String>, tool_calls: Option<Vec<ToolCall>>) -> Self {
        Self {
            role: "assistant".to_string(),
            content,
            tool_calls,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn tool_result(content: String, tool_call_id: String, name: String) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content),
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
            name: Some(name),
        }
    }
}

/// A single tool call returned by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

/// Tool definition sent to the model (READ/GLOB/GREP).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolFunctionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// The READ/GLOB/GREP tool schemas sent to the model on every turn — shared by
/// `SpecializedAgentBackend::invoke` (one-shot) and `SpecializedSubagentSession` (stateful), the
/// two turn loops that talk to an OpenAI-compatible endpoint. Deliberately read-only: the
/// mutation tools live in [`mutation_tool_definitions`] so a loop that advertises this whole set
/// unfiltered can never advertise them.
pub fn discovery_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "READ".to_string(),
                description: "Read a file and return its contents with line numbers. Long files \
                    are truncated to a line cap; page through them with offset/limit."
                    .to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to read." },
                        "offset": {
                            "type": "integer",
                            "description": "0-based line to start reading from (default 0)."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of lines to return."
                        }
                    },
                    "required": ["path"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "GLOB".to_string(),
                description: "Return file paths matching a glob pattern.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Glob pattern." }
                    },
                    "required": ["pattern"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "GREP".to_string(),
                description: "Search files with a regex pattern.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Regex pattern." },
                        "path": { "type": "string", "description": "Optional path to search in." }
                    },
                    "required": ["pattern"]
                }),
            },
        },
    ]
}

/// The WRITE/STR_REPLACE/DELETE tool schemas a coder-role subagent binds explicitly via its
/// def's `tools:` list (see `SubagentTool`). Kept out of [`discovery_tool_definitions`] on
/// purpose: only `SpecializedSubagentSession` — which filters by bound tools — ever sees these,
/// so a loop advertising the discovery set unfiltered stays read-only.
pub fn mutation_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "WRITE".to_string(),
                description: "Write the full contents of a file, creating it if missing and \
                    overwriting it otherwise."
                    .to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to write." },
                        "contents": { "type": "string", "description": "Full file contents." }
                    },
                    "required": ["path", "contents"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "STR_REPLACE".to_string(),
                description: "Replace one unique occurrence of a string in a file. Fails when \
                    the string is missing or matches more than once."
                    .to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to edit." },
                        "old_string": {
                            "type": "string",
                            "description": "Exact text to replace (must be unique in the file)."
                        },
                        "new_string": { "type": "string", "description": "Replacement text." }
                    },
                    "required": ["path", "old_string", "new_string"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "DELETE".to_string(),
                description: "Delete a file.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to delete." }
                    },
                    "required": ["path"]
                }),
            },
        },
    ]
}

/// The remaining exec-catalog tools a def may bind: `SHELL` (a mutating tool, gated exactly like
/// the three above) plus `AWAIT`/`READ_LINTS`/`SEMANTIC_SEARCH`, which the host tool engine
/// provides. Like [`mutation_tool_definitions`], only `SpecializedSubagentSession` — which filters
/// by bound tools — ever sees these, so a loop advertising the discovery set unfiltered stays
/// read-only.
pub fn engine_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "SHELL".to_string(),
                description: "Run a shell command in the workspace.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Command line to run." },
                        "block_until_ms": {
                            "type": "integer",
                            "description": "Milliseconds to wait before backgrounding the job."
                        }
                    },
                    "required": ["command"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "AWAIT".to_string(),
                description: "Wait for a background shell job to complete.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "job_id": { "type": "string", "description": "Job to wait for." },
                        "task_id": { "type": "string", "description": "Task to wait for." },
                        "timeout_ms": {
                            "type": "integer",
                            "description": "Give up after this many milliseconds."
                        },
                        "block_until_ms": {
                            "type": "integer",
                            "description": "Milliseconds to block before returning progress."
                        }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "READ_LINTS".to_string(),
                description: "Read linting diagnostics for the workspace.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Optional path to scope to." }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "SEMANTIC_SEARCH".to_string(),
                description: "Search the codebase semantically.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Natural-language query." },
                        "path": { "type": "string", "description": "Optional path to scope to." }
                    },
                    "required": ["query"]
                }),
            },
        },
    ]
}

/// Request body for `/v1/chat/completions`.
#[derive(Debug, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDefinition>,
    pub tool_choice: serde_json::Value,
    pub temperature: f32,
}

/// Response from `/v1/chat/completions`.
#[derive(Debug, Deserialize)]
pub struct ChatCompletionResponse {
    pub choices: Vec<ChatChoice>,
    /// Token accounting for the call. Absent when the endpoint omits `usage`; a partial `usage`
    /// object counts missing counters as zero rather than failing the response parse.
    #[serde(default, deserialize_with = "deserialize_usage")]
    pub usage: Option<TokenUsage>,
}

/// Map the wire `usage {prompt_tokens, completion_tokens}` onto [`TokenUsage`], tolerating an
/// absent object (`None`) or missing individual counters (zero).
fn deserialize_usage<'de, D>(deserializer: D) -> Result<Option<TokenUsage>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct RawUsage {
        #[serde(default)]
        prompt_tokens: u64,
        #[serde(default)]
        completion_tokens: u64,
    }
    let raw: Option<RawUsage> = Option::deserialize(deserializer)?;
    Ok(raw.map(|r| TokenUsage {
        input_tokens: r.prompt_tokens,
        output_tokens: r.completion_tokens,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ChatChoice {
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

pub struct OpenAiClient {
    base_url: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl OpenAiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: None,
            http: reqwest::Client::new(),
        }
    }

    /// The bearer token to authenticate every call with. A local endpoint (Ollama, vLLM) needs
    /// none — `None` sends no `Authorization` header at all — while a cloud provider does.
    #[must_use]
    pub fn api_key(mut self, api_key: Option<String>) -> Self {
        self.api_key = api_key;
        self
    }

    /// Give the transport a deadline instead of the default, deadline-free one.
    ///
    /// A provider is a third-party endpoint: it can accept the connection and then say nothing at
    /// all, and `reqwest::Client::new()` waits for that forever. The caller owns the budget because
    /// it knows what the call is for — an interactive chat turn and a batch enumeration do not
    /// deserve the same patience.
    ///
    /// `expect` matches `reqwest::Client::new`'s own contract: the build fails only when the TLS
    /// backend cannot be initialized, which no request in this process could survive either.
    #[must_use]
    pub fn timeouts(
        mut self,
        connect_timeout: std::time::Duration,
        request_timeout: std::time::Duration,
    ) -> Self {
        self.http = reqwest::Client::builder()
            .connect_timeout(connect_timeout)
            .timeout(request_timeout)
            .build()
            .expect("an http client builds unless the TLS backend cannot initialize");
        self
    }

    /// Send a chat completion request and return the response.
    pub async fn complete(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
        // Full request transcript (prompt, tool results, tool schemas) — enable with
        // `--mcp-log-level debug` (RUST_LOG `tddy_discovery::openai=debug`) to capture exactly what
        // is sent to the model; Ollama's own logs record only token counts/timings, not content.
        if log::log_enabled!(log::Level::Debug) {
            log::debug!(
                target: "tddy_discovery::openai",
                "chat request → {url}: {}",
                serde_json::to_string(&request).unwrap_or_else(|e| format!("<unserializable: {e}>"))
            );
        }
        let mut post = self.http.post(&url).json(&request);
        if let Some(api_key) = &self.api_key {
            post = post.bearer_auth(api_key);
        }
        let response = post.send().await?;
        let status = response.status();
        // Read the body as text once, so we can log the full response transcript and still parse it
        // (and surface the body on both HTTP errors and parse failures).
        let body = response.text().await?;
        if !status.is_success() {
            return Err(format!("OpenAI API error {status}: {body}").into());
        }
        log::debug!(target: "tddy_discovery::openai", "chat response ← {url} [{status}]: {body}");
        let parsed: ChatCompletionResponse = serde_json::from_str(&body)
            .map_err(|e| format!("parse chat completion response: {e}; body: {body}"))?;
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests: OpenAI chat completion client parses and serialises correctly.
    //!
    //! Feature: docs/ft/coder/discovery-agent.md (Phase B criterion 8)

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn read_glob_grep_tools() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                tool_type: "function".to_string(),
                function: ToolFunctionDef {
                    name: "READ".to_string(),
                    description: "Read file contents with line numbers.".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": { "path": { "type": "string" } },
                        "required": ["path"]
                    }),
                },
            },
            ToolDefinition {
                tool_type: "function".to_string(),
                function: ToolFunctionDef {
                    name: "GLOB".to_string(),
                    description: "Discover paths matching a glob pattern.".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": { "pattern": { "type": "string" } },
                        "required": ["pattern"]
                    }),
                },
            },
            ToolDefinition {
                tool_type: "function".to_string(),
                function: ToolFunctionDef {
                    name: "GREP".to_string(),
                    description: "Search files with a regex pattern.".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "pattern": { "type": "string" },
                            "path": { "type": "string" }
                        },
                        "required": ["pattern"]
                    }),
                },
            },
        ]
    }

    /// The request body must include messages and tools in the standard OpenAI shape.
    /// The mock server verifies the POST body; the response must be deserialised correctly.
    #[tokio::test]
    async fn serializes_tools_and_messages_into_the_request_body() {
        // Given — a mock that captures the request and returns a minimal valid response
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": { "role": "assistant", "content": "ok" },
                    "finish_reason": "stop"
                }]
            })))
            .mount(&server)
            .await;

        let client = OpenAiClient::new(server.uri());
        let request = ChatCompletionRequest {
            model: "qwen2.5-coder:7b".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: Some("Find the auth module".to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            tools: read_glob_grep_tools(),
            tool_choice: serde_json::json!("auto"),
            temperature: 0.0,
        };

        // When
        let response = client
            .complete(request)
            .await
            .expect("complete must succeed against the mock server");

        // Then — at least one choice is returned
        assert!(
            !response.choices.is_empty(),
            "response must contain at least one choice"
        );
        assert_eq!(
            response.choices[0].message.role, "assistant",
            "first choice message role must be 'assistant'"
        );
    }

    /// The client correctly parses `tool_calls` from a chat completion response.
    #[tokio::test]
    async fn parses_tool_calls_from_a_chat_completion_response() {
        // Given — mock returns a tool_calls response
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_abc123",
                            "type": "function",
                            "function": {
                                "name": "READ",
                                "arguments": "{\"path\": \"src/lib.rs\"}"
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }]
            })))
            .mount(&server)
            .await;

        let client = OpenAiClient::new(server.uri());
        let request = ChatCompletionRequest {
            model: "qwen2.5-coder:7b".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: Some("Find auth module".to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            tools: read_glob_grep_tools(),
            tool_choice: serde_json::json!("auto"),
            temperature: 0.0,
        };

        // When
        let response = client
            .complete(request)
            .await
            .expect("complete must succeed");

        // Then — tool_calls is populated
        let tool_calls = response.choices[0]
            .message
            .tool_calls
            .as_ref()
            .expect("response must contain tool_calls when the model issues a tool call");
        assert_eq!(tool_calls.len(), 1, "exactly one tool call must be present");
        assert_eq!(tool_calls[0].function.name, "READ");
        assert_eq!(tool_calls[0].id, "call_abc123");
    }

    /// A cloud provider authenticates by bearer token; a local one (Ollama) is given no key and
    /// must therefore be sent no `Authorization` header at all.
    #[tokio::test]
    async fn authenticates_with_the_configured_api_key_as_a_bearer_token() {
        // Given — a server that answers anything, so the header is asserted rather than matched
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": { "role": "assistant", "content": "ok" },
                    "finish_reason": "stop"
                }]
            })))
            .mount(&server)
            .await;

        // When
        OpenAiClient::new(server.uri())
            .api_key(Some("sk-live-1".to_string()))
            .complete(a_hello_request())
            .await
            .expect("complete must succeed");

        // Then
        let sent = server.received_requests().await.expect("recorded requests");
        let authorization = sent[0]
            .headers
            .get("authorization")
            .expect("the request must carry an Authorization header")
            .to_str()
            .expect("an ascii Authorization header");
        assert_eq!(authorization, "Bearer sk-live-1");
    }

    #[tokio::test]
    async fn sends_no_authorization_header_when_no_api_key_is_configured() {
        // Given
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": { "role": "assistant", "content": "ok" },
                    "finish_reason": "stop"
                }]
            })))
            .mount(&server)
            .await;

        // When
        OpenAiClient::new(server.uri())
            .complete(a_hello_request())
            .await
            .expect("complete must succeed");

        // Then
        let sent = server.received_requests().await.expect("recorded requests");
        assert_eq!(sent[0].headers.get("authorization"), None);
    }

    /// The smallest well-formed request: one user message, no tools.
    fn a_hello_request() -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: "qwen2.5-coder:7b".to_string(),
            messages: vec![ChatMessage::user("Say hello")],
            tools: Vec::new(),
            tool_choice: serde_json::json!("auto"),
            temperature: 0.0,
        }
    }
}
