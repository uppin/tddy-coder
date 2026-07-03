//! OpenAI `/v1/chat/completions` client for the FastContext multi-turn loop.

use serde::{Deserialize, Serialize};

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
/// `FastContextBackend::invoke` (one-shot) and `FastContextSession` (stateful), the two turn
/// loops that both talk to a FastContext-compatible endpoint.
pub fn discovery_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "READ".to_string(),
                description: "Read a file and return its contents with line numbers.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to read." }
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
}

#[derive(Debug, Deserialize)]
pub struct ChatChoice {
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

pub struct OpenAiClient {
    base_url: String,
    http: reqwest::Client,
}

impl OpenAiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
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
        let response = self.http.post(&url).json(&request).send().await?;
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
    //! Changeset: docs/dev/1-WIP/2026-06-24-changeset-fastcontext-discovery.md

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
            model: "microsoft/FastContext-1.0-4B-RL".to_string(),
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
            model: "microsoft/FastContext-1.0-4B-RL".to_string(),
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
}
