//! `FastContextBackend: CodingBackend` — multi-turn OpenAI loop producing citations.
//!
//! The backend:
//! - Selects `ToolExecutor::Local` or `::Remote` from `InvokeRequest.remote`.
//! - Calls `OpenAiClient::complete` with the system prompt + user query + tool definitions, then
//!   emits `ProgressEvent::TaskProgress` on `InvokeRequest.progress_sink` marking that turn's
//!   request/response round-trip as complete, with elapsed time — without this, a slow or hung
//!   model request and a finished turn look identical in the activity log (both silent).
//! - Parses the response: if `tool_calls`, emits `ProgressEvent::ToolUse` on
//!   `InvokeRequest.progress_sink` (same mechanism `ClaudeCodeBackend`/`CursorBackend` use to
//!   drive the TUI activity log), then dispatches to the executor and appends tool messages. If
//!   the model instead produced plain text with no tool call and no `<final_answer>` (e.g.
//!   "thinking" prose from a reasoning model), that text streams to
//!   `InvokeRequest.agent_output_sink` — otherwise those turns are silent end-to-end.
//! - Repeats until the response content contains `<final_answer>` or `max_turns` is reached.
//! - Returns `InvokeResponse { output: <citations text> }`.

use async_trait::async_trait;
use tddy_core::backend::{CodingBackend, InvokeRequest, InvokeResponse};
use tddy_core::{BackendError, ProgressEvent};

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

/// Streams a turn's plain-text assistant content (no tool call) to `InvokeRequest.agent_output_sink`
/// — the same mechanism `ClaudeCodeBackend`/`CursorBackend` use for raw text — gated by
/// `InvokeRequest.agent_output`, same as those backends. A no-op for `None`/empty content.
fn emit_agent_output(request: &InvokeRequest, content: Option<&str>) {
    let Some(text) = content.filter(|s| !s.is_empty()) else {
        return;
    };
    if !request.agent_output {
        return;
    }
    if let Some(ref sink) = request.agent_output_sink {
        sink.emit(text);
    }
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

        for turn in 0..self.max_turns {
            let req = ChatCompletionRequest {
                model: self.model.clone(),
                messages: messages.clone(),
                tools: tools.clone(),
                tool_choice: serde_json::json!("auto"),
                temperature: 0.0,
            };

            let request_started = std::time::Instant::now();
            let response = client
                .complete(req)
                .await
                .map_err(|e| BackendError::InvocationFailed(format!("FastContextBackend: {e}")))?;
            let request_elapsed = request_started.elapsed();

            if let Some(ref sink) = request.progress_sink {
                sink.emit(&ProgressEvent::TaskProgress {
                    description: format!(
                        "turn {}/{}: model request complete ({:.1}s)",
                        turn + 1,
                        self.max_turns,
                        request_elapsed.as_secs_f64()
                    ),
                    last_tool: None,
                });
            }

            let choice = response.choices.into_iter().next().ok_or_else(|| {
                BackendError::InvocationFailed("no choices in response".to_string())
            })?;

            let msg = choice.message;

            // Check for final answer in content. Ollama returns `content: ""` (not null) on
            // tool-call-only turns, so both checks below require non-empty content: otherwise
            // a trailing tool-call turn silently clobbers `last_content` back to empty (wiping
            // out real reasoning from an earlier turn), and an empty `<final_answer></final_answer>`
            // block would short-circuit with a blank result instead of continuing the loop.
            if let Some(ref content) = msg.content {
                if let Some(answer) = extract_final_answer(content).filter(|a| !a.is_empty()) {
                    // We have a non-empty final answer — return it.
                    return Ok(InvokeResponse {
                        output: answer.to_string(),
                        exit_code: 0,
                        session_id: None,
                        questions: Vec::new(),
                        raw_stream: None,
                        stderr: None,
                    });
                }
                if !content.trim().is_empty() {
                    last_content = content.clone();
                }
            }

            // If there are tool calls, dispatch them.
            if let Some(ref tool_calls) = msg.tool_calls {
                if tool_calls.is_empty() {
                    emit_agent_output(&request, msg.content.as_deref());
                    messages.push(ChatMessage::assistant(msg.content.clone(), None));
                    continue;
                }

                messages.push(ChatMessage::assistant(
                    msg.content.clone(),
                    msg.tool_calls.clone(),
                ));

                for tc in tool_calls {
                    if let Some(ref sink) = request.progress_sink {
                        sink.emit(&ProgressEvent::ToolUse {
                            name: tc.function.name.clone(),
                            detail: Some(tc.function.arguments.clone()),
                        });
                    }
                    let result_str = dispatch_tool(&executor, tc).await;
                    messages.push(ChatMessage::tool_result(
                        result_str,
                        tc.id.clone(),
                        tc.function.name.clone(),
                    ));
                }
            } else {
                // No tool call and no <final_answer> — the model produced plain text (often
                // "thinking" prose). Without this, these turns are indistinguishable from a
                // hang: a `TaskProgress` line appears, then nothing, for as long as this turn
                // took.
                emit_agent_output(&request, msg.content.as_deref());
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

    use std::sync::{Arc, Mutex};

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

    /// A turn with plain assistant text: no `tool_calls`, and no `<final_answer>` block.
    fn plain_text_response(text: &str) -> serde_json::Value {
        serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": text
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

    /// Each dispatched tool call must emit a `ProgressEvent::ToolUse` on `InvokeRequest.progress_sink`
    /// — the same mechanism `ClaudeCodeBackend`/`CursorBackend` use to drive the TUI activity log —
    /// so a user running `--agent fastcontext` interactively sees tool activity as it happens,
    /// instead of the CLI going silent for the whole multi-turn loop.
    #[tokio::test]
    async fn invoke_emits_a_tool_use_progress_event_for_each_dispatched_tool_call() {
        // Given — a mock server that issues one GLOB tool call, then a final answer
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
                ResponseTemplate::new(200).set_body_json(final_answer_response("src/lib.rs:1-1")),
            )
            .mount(&server)
            .await;

        let backend = FastContextBackend::new(server.uri(), "microsoft/FastContext-1.0-4B-RL", 6);
        let recorded: Arc<Mutex<Vec<tddy_core::ProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let recorded_for_sink = recorded.clone();
        let progress_sink = tddy_core::backend::ProgressSink::new(move |ev| {
            recorded_for_sink.lock().unwrap().push(ev.clone());
        });
        let request = InvokeRequest {
            prompt: "Where is the authentication logic?".to_string(),
            progress_sink: Some(progress_sink),
            ..InvokeRequest::default()
        };

        // When
        backend
            .invoke(request)
            .await
            .expect("invoke must succeed when the model produces a final answer");

        // Then — exactly one ToolUse event, naming GLOB with the model's arguments as detail
        // (TaskProgress events also fire per completed request; this test only cares about ToolUse)
        let events = recorded.lock().unwrap();
        let tool_use_events: Vec<_> = events
            .iter()
            .filter(|ev| matches!(ev, tddy_core::ProgressEvent::ToolUse { .. }))
            .collect();
        assert_eq!(
            tool_use_events.len(),
            1,
            "exactly one ToolUse progress event must be emitted; got {events:?}"
        );
        match tool_use_events[0] {
            tddy_core::ProgressEvent::ToolUse { name, detail } => {
                assert_eq!(name, "GLOB", "ToolUse event must name the dispatched tool");
                assert_eq!(
                    detail.as_deref(),
                    Some(r#"{"pattern":"src/**/*.rs"}"#),
                    "ToolUse detail must carry the model's raw tool-call arguments"
                );
            }
            other => panic!("expected ProgressEvent::ToolUse, got {other:?}"),
        }
    }

    /// Each completed model request must emit a `ProgressEvent::TaskProgress` naming the turn
    /// number and the elapsed wall-clock time for that round-trip — without this, a slow/hung
    /// model request and an already-finished turn are indistinguishable in the activity log
    /// (both silent).
    #[tokio::test]
    async fn invoke_emits_a_task_progress_event_with_elapsed_time_after_each_request() {
        // Given — a mock server that delays its final-answer response by 150ms
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(final_answer_response("src/lib.rs:1-1"))
                    .set_delay(std::time::Duration::from_millis(150)),
            )
            .mount(&server)
            .await;

        let backend = FastContextBackend::new(server.uri(), "microsoft/FastContext-1.0-4B-RL", 6);
        let recorded: Arc<Mutex<Vec<tddy_core::ProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let recorded_for_sink = recorded.clone();
        let progress_sink = tddy_core::backend::ProgressSink::new(move |ev| {
            recorded_for_sink.lock().unwrap().push(ev.clone());
        });
        let request = InvokeRequest {
            prompt: "Where is the entry point?".to_string(),
            progress_sink: Some(progress_sink),
            ..InvokeRequest::default()
        };

        // When
        backend
            .invoke(request)
            .await
            .expect("invoke must succeed when the model produces a final answer");

        // Then — exactly one TaskProgress event, naming turn 1/6 with a plausible elapsed time
        let events = recorded.lock().unwrap();
        assert_eq!(
            events.len(),
            1,
            "exactly one TaskProgress progress event must be emitted; got {events:?}"
        );
        match &events[0] {
            tddy_core::ProgressEvent::TaskProgress { description, .. } => {
                assert!(
                    description.starts_with("turn 1/6: model request complete ("),
                    "description must name the turn and total; got {description:?}"
                );
                // The mock delayed its response by 150ms; the reported elapsed time must reflect
                // real wall-clock time, not a hardcoded placeholder.
                assert!(
                    description.contains("0.1") || description.contains("0.2"),
                    "elapsed time must reflect the mock's ~150ms delay; got {description:?}"
                );
            }
            other => panic!("expected ProgressEvent::TaskProgress, got {other:?}"),
        }
    }

    /// A turn where the model produces plain text — no tool call, no `<final_answer>` (e.g.
    /// "thinking" prose from a reasoning model) — must stream that text to
    /// `InvokeRequest.agent_output_sink`, the same mechanism Claude/Cursor use for raw assistant
    /// text. Without this, such turns are completely silent in the activity log.
    #[tokio::test]
    async fn invoke_streams_plain_text_turns_to_agent_output_sink() {
        // Given — turn 1 is plain text (no tool call, no final answer), turn 2 is the final answer
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(plain_text_response(
                    "Let me think about where docker support might live in this repo.",
                )),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(final_answer_response("src/lib.rs:1-1")),
            )
            .mount(&server)
            .await;

        let backend = FastContextBackend::new(server.uri(), "microsoft/FastContext-1.0-4B-RL", 6);
        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let recorded_for_sink = recorded.clone();
        let agent_output_sink = tddy_core::backend::AgentOutputSink::new(move |s: &str| {
            recorded_for_sink.lock().unwrap().push(s.to_string());
        });
        let request = InvokeRequest {
            prompt: "Where is the entry point?".to_string(),
            agent_output: true,
            agent_output_sink: Some(agent_output_sink),
            ..InvokeRequest::default()
        };

        // When
        backend
            .invoke(request)
            .await
            .expect("invoke must succeed when the model produces a final answer");

        // Then — the plain-text turn's content was streamed verbatim
        let streamed = recorded.lock().unwrap();
        assert_eq!(
            streamed.as_slice(),
            &["Let me think about where docker support might live in this repo.".to_string()],
            "plain-text turn content must stream to agent_output_sink"
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

    /// A tool-call turn's `content` field, matching Ollama's OpenAI-compatible response shape,
    /// is `""` (empty string), not `null`.
    fn tool_call_response_with_empty_content(
        tool_name: &str,
        args: serde_json::Value,
    ) -> serde_json::Value {
        serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "",
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

    /// Ollama returns `content: ""` (empty string, not `null`) on tool-call-only turns. When
    /// `max_turns` is reached, the fallback output must be the last *meaningful* plain-text turn,
    /// not clobbered back to empty by a later tool-call turn's empty content.
    #[tokio::test]
    async fn invoke_preserves_last_meaningful_content_across_a_trailing_tool_call_turn() {
        // Given — turn 1 is substantial plain text, turn 2 is a tool call with empty content
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(plain_text_response(
                    "Docker support looks like it lives under packages/tddy-build-docker.",
                )),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                tool_call_response_with_empty_content(
                    "GLOB",
                    serde_json::json!({"pattern": "**/*docker*"}),
                ),
            ))
            .mount(&server)
            .await;

        let max_turns = 2u32;
        let backend =
            FastContextBackend::new(server.uri(), "microsoft/FastContext-1.0-4B-RL", max_turns);
        let request = InvokeRequest {
            prompt: "Where is docker support implemented?".to_string(),
            ..InvokeRequest::default()
        };

        // When
        let response = backend
            .invoke(request)
            .await
            .expect("invoke must return Ok when max_turns is reached");

        // Then — the fallback output is turn 1's text, not clobbered to empty by turn 2
        assert_eq!(
            response.output, "Docker support looks like it lives under packages/tddy-build-docker.",
            "max-turns fallback must preserve the last non-empty content, not the trailing \
             tool-call turn's empty content"
        );
    }

    /// An empty `<final_answer></final_answer>` block must not be treated as a real answer —
    /// otherwise the loop returns a blank result and discards all prior reasoning/tool work
    /// instead of continuing toward a genuine answer.
    #[tokio::test]
    async fn invoke_does_not_treat_an_empty_final_answer_block_as_a_real_answer() {
        // Given — turn 1 has an empty <final_answer> block, turn 2 has a real one
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(plain_text_response(
                    "Let me check.\n<final_answer>\n</final_answer>",
                )),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(final_answer_response("src/lib.rs:1-1")),
            )
            .mount(&server)
            .await;

        let backend = FastContextBackend::new(server.uri(), "microsoft/FastContext-1.0-4B-RL", 6);
        let request = InvokeRequest {
            prompt: "Where is the entry point?".to_string(),
            ..InvokeRequest::default()
        };

        // When
        let response = backend
            .invoke(request)
            .await
            .expect("invoke must succeed once the model produces a real final answer");

        // Then — the loop continued past the empty final_answer and returned the real one
        assert_eq!(
            response.output, "src/lib.rs:1-1",
            "an empty <final_answer> block must not short-circuit the loop with a blank result"
        );
        let calls = server.received_requests().await.unwrap();
        assert_eq!(
            calls.len(),
            2,
            "the loop must continue past the empty final_answer to a second turn"
        );
    }
}
