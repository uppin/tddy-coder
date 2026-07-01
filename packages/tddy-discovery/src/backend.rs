//! `FastContextBackend: CodingBackend` — multi-turn OpenAI loop producing citations.
//!
//! The backend:
//! - Selects `ToolExecutor::Local` or `::Remote` from `InvokeRequest.remote`.
//! - Calls `OpenAiClient::complete` with the system prompt + user query + tool definitions.
//! - Parses the response: if `tool_calls`, dispatches to the executor and appends tool messages.
//! - Repeats until the response content contains `<final_answer>` or `max_turns` is reached.
//! - Returns `InvokeResponse { output: <citations text> }`.

use async_trait::async_trait;
use tddy_core::backend::{CodingBackend, InvokeRequest, InvokeResponse};
use tddy_core::BackendError;

use crate::discovery::extract_final_answer;
use crate::openai::{
    ChatCompletionRequest, ChatMessage, OpenAiClient, ToolDefinition, ToolFunctionDef,
};
use crate::tools::ToolExecutor;

/// FastContext Discovery backend — drives the multi-turn model loop.
pub struct FastContextBackend {
    base_url: String,
    model: String,
    max_turns: u32,
}

impl FastContextBackend {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>, max_turns: u32) -> Self {
        Self {
            base_url: base_url.into(),
            model: model.into(),
            max_turns,
        }
    }
}

fn discovery_tools() -> Vec<ToolDefinition> {
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

fn build_system_message(request: &InvokeRequest) -> Option<ChatMessage> {
    let sp = request
        .system_prompt
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            request
                .system_prompt_path
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
        });
    sp.map(ChatMessage::system)
}

async fn dispatch_tool(executor: &ToolExecutor, tc: &crate::openai::ToolCall) -> String {
    let args: serde_json::Value =
        serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null);

    let result = match tc.function.name.as_str() {
        "READ" => {
            let path = args["path"].as_str().unwrap_or("");
            executor.read(path, None, None).await
        }
        "GLOB" => {
            let pattern = args["pattern"].as_str().unwrap_or("");
            executor.glob(pattern).await
        }
        "GREP" => {
            let pattern = args["pattern"].as_str().unwrap_or("");
            let path = args["path"].as_str();
            executor.grep(pattern, path).await
        }
        unknown => return format!("{{\"error\": \"unknown tool: {unknown}\"}}"),
    };

    match result {
        Ok(output) => output.value.to_string(),
        Err(e) => format!("{{\"error\": \"{e}\"}}"),
    }
}

#[async_trait]
impl CodingBackend for FastContextBackend {
    fn name(&self) -> &str {
        "fastcontext"
    }

    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        let client = OpenAiClient::new(&self.base_url);
        let executor = ToolExecutor::from_invoke_request(&request);

        let mut messages: Vec<ChatMessage> = Vec::new();
        if let Some(sys) = build_system_message(&request) {
            messages.push(sys);
        }
        messages.push(ChatMessage::user(request.prompt.clone()));

        let tools = discovery_tools();
        let mut last_content = String::new();

        for _ in 0..self.max_turns {
            let req = ChatCompletionRequest {
                model: self.model.clone(),
                messages: messages.clone(),
                tools: tools.clone(),
                tool_choice: serde_json::json!("auto"),
                temperature: 0.0,
            };

            let response = client
                .complete(req)
                .await
                .map_err(|e| BackendError::InvocationFailed(format!("FastContextBackend: {e}")))?;

            let choice = response.choices.into_iter().next().ok_or_else(|| {
                BackendError::InvocationFailed("no choices in response".to_string())
            })?;

            let msg = choice.message;

            // Check for final answer in content.
            if let Some(ref content) = msg.content {
                last_content = content.clone();
                if extract_final_answer(content).is_some() {
                    // We have the final answer — return it.
                    let output = extract_final_answer(&last_content)
                        .unwrap_or(&last_content)
                        .to_string();
                    return Ok(InvokeResponse {
                        output,
                        exit_code: 0,
                        session_id: None,
                        questions: Vec::new(),
                        raw_stream: None,
                        stderr: None,
                    });
                }
            }

            // If there are tool calls, dispatch them.
            if let Some(ref tool_calls) = msg.tool_calls {
                if tool_calls.is_empty() {
                    messages.push(ChatMessage::assistant(msg.content.clone(), None));
                    continue;
                }

                messages.push(ChatMessage::assistant(
                    msg.content.clone(),
                    msg.tool_calls.clone(),
                ));

                for tc in tool_calls {
                    let result_str = dispatch_tool(&executor, tc).await;
                    messages.push(ChatMessage::tool_result(
                        result_str,
                        tc.id.clone(),
                        tc.function.name.clone(),
                    ));
                }
            } else {
                messages.push(ChatMessage::assistant(msg.content.clone(), None));
            }
        }

        // max_turns reached — return whatever we have.
        Ok(InvokeResponse {
            output: last_content,
            exit_code: 0,
            session_id: None,
            questions: Vec::new(),
            raw_stream: None,
            stderr: None,
        })
    }
}

#[cfg(test)]
mod tests {
    //! Integration tests: FastContextBackend multi-turn loop against a mock server.
    //!
    //! Feature: docs/ft/coder/discovery-agent.md (Phase B criteria 7–8)
    //! Changeset: docs/dev/1-WIP/2026-06-24-changeset-fastcontext-discovery.md

    use tddy_core::backend::{CodingBackend, InvokeRequest};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::FastContextBackend;

    fn tool_call_response(tool_name: &str, args: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": tool_name,
                            "arguments": args.to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        })
    }

    fn final_answer_response(answer: &str) -> serde_json::Value {
        serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": format!("Looked at the code.\n<final_answer>\n{answer}\n</final_answer>")
                },
                "finish_reason": "stop"
            }]
        })
    }

    /// `name()` must return `"fastcontext"` as the backend identifier.
    #[test]
    fn invoke_reports_name_as_fastcontext() {
        // Given
        let backend = FastContextBackend::new(
            "http://localhost:30000",
            "microsoft/FastContext-1.0-4B-RL",
            6,
        );

        // Then
        assert_eq!(
            backend.name(),
            "fastcontext",
            "FastContextBackend::name() must return \"fastcontext\""
        );
    }

    /// A custom (non-default) model id passed to `FastContextBackend::new` must be sent verbatim
    /// as the `model` field of the outgoing `/v1/chat/completions` request body — this is what
    /// lets the backend target a locally-served model tag (e.g. an Ollama model) instead of the
    /// hardcoded `microsoft/FastContext-1.0-4B-RL` default.
    #[tokio::test]
    async fn invoke_sends_the_configured_model_name_in_the_request_body() {
        // Given — a mock server that immediately returns a final answer
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(final_answer_response("src/lib.rs:1-1")),
            )
            .mount(&server)
            .await;

        let custom_model = "hf.co/mitkox/FastContext-1.0-4B-SFT-Q4_K_M-GGUF:Q4_K_M";
        let backend = FastContextBackend::new(server.uri(), custom_model, 2);
        let request = InvokeRequest {
            prompt: "Where is the entry point?".to_string(),
            ..InvokeRequest::default()
        };

        // When
        backend
            .invoke(request)
            .await
            .expect("invoke must succeed when the model produces a final answer");

        // Then — the request body's `model` field is the custom tag, not the microsoft default
        let calls = server.received_requests().await.unwrap();
        assert_eq!(calls.len(), 1, "exactly one model call must be made");
        let body: serde_json::Value =
            serde_json::from_slice(&calls[0].body).expect("request body must be valid JSON");
        assert_eq!(
            body["model"].as_str(),
            Some(custom_model),
            "request body must carry the configured model id, not the microsoft default"
        );
    }

    /// The multi-turn loop: mock returns a tool_call on turn 1 and `<final_answer>` on turn 2.
    /// Assert: exactly two model calls were made, the tool was executed between them, and the
    /// output contains the citations from the final answer.
    #[tokio::test]
    async fn invoke_runs_the_multi_turn_loop_until_final_answer() {
        // Given — two sequential responses from the model
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(tool_call_response(
                "GLOB",
                serde_json::json!({"pattern": "src/**/*.rs"}),
            )))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(final_answer_response(
                    "src/auth.rs:1-50\nsrc/auth/mod.rs:1-30",
                )),
            )
            .mount(&server)
            .await;

        let backend = FastContextBackend::new(server.uri(), "microsoft/FastContext-1.0-4B-RL", 6);
        let request = InvokeRequest {
            prompt: "Where is the authentication logic?".to_string(),
            ..InvokeRequest::default()
        };

        // When
        let response = backend
            .invoke(request)
            .await
            .expect("invoke must succeed when the model produces a final answer");

        // Then — the output contains the citations from <final_answer>
        assert!(
            response.output.contains("src/auth.rs:1-50"),
            "output must contain citations from <final_answer>; got: {:?}",
            response.output
        );

        // And — exactly 2 model calls were made (1 tool call turn + 1 final answer turn)
        let calls = server.received_requests().await.unwrap();
        assert_eq!(
            calls.len(),
            2,
            "the loop must make exactly 2 model calls (tool call + final answer)"
        );
    }

    /// When `max_turns` is reached with no `<final_answer>`, the loop terminates and returns
    /// a defined result (not an infinite loop, not a panic).
    #[tokio::test]
    async fn invoke_stops_at_max_turns_when_no_final_answer() {
        // Given — mock always returns a tool_call (no final_answer ever)
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(tool_call_response(
                "GLOB",
                serde_json::json!({"pattern": "**/*.rs"}),
            )))
            .mount(&server)
            .await;

        let max_turns = 3u32;
        let backend =
            FastContextBackend::new(server.uri(), "microsoft/FastContext-1.0-4B-RL", max_turns);
        let request = InvokeRequest {
            prompt: "Find all Rust files".to_string(),
            ..InvokeRequest::default()
        };

        // When
        let result = backend.invoke(request).await;

        // Then — invoke returns Ok (partial output) after exhausting max_turns; does not loop forever
        assert!(
            result.is_ok(),
            "invoke must return Ok when max_turns is reached; got: {:?}",
            result.err()
        );

        // And — exactly max_turns model calls were made (one per loop iteration)
        let call_count = server.received_requests().await.unwrap().len();
        assert_eq!(
            call_count, max_turns as usize,
            "the loop must make exactly max_turns model calls; made {call_count}"
        );
    }
}
