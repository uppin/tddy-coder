//! Manual verification tool — not part of the plan/tests. Replicates `FastContextBackend::invoke`'s
//! multi-turn loop by hand (with per-turn logging) against a real OpenAI-compatible endpoint
//! (e.g. local Ollama), bypassing the tddy-coder workflow engine (which does not yet consume
//! Discovery's citation-only output shape). Kept intentionally for future manual re-verification.
//!
//! Usage: cargo run --example discover -p tddy-discovery -- <base_url> <model> <prompt>

use tddy_core::backend::InvokeRequest;
use tddy_discovery::discovery::extract_final_answer;
use tddy_discovery::openai::{
    ChatCompletionRequest, ChatMessage, OpenAiClient, ToolDefinition, ToolFunctionDef,
};
use tddy_discovery::tools::ToolExecutor;

fn discovery_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "READ".to_string(),
                description: "Read a file and return its contents with line numbers.".to_string(),
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
                description: "Return file paths matching a glob pattern.".to_string(),
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

async fn dispatch_tool(executor: &ToolExecutor, tc: &tddy_discovery::openai::ToolCall) -> String {
    let args: serde_json::Value =
        serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null);
    let result = match tc.function.name.as_str() {
        "READ" => {
            executor
                .read(args["path"].as_str().unwrap_or(""), None, None)
                .await
        }
        "GLOB" => executor.glob(args["pattern"].as_str().unwrap_or("")).await,
        "GREP" => {
            executor
                .grep(
                    args["pattern"].as_str().unwrap_or(""),
                    args["path"].as_str(),
                )
                .await
        }
        unknown => return format!("{{\"error\": \"unknown tool: {unknown}\"}}"),
    };
    match result {
        Ok(output) => output.value.to_string(),
        Err(e) => format!("{{\"error\": \"{e}\"}}"),
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let base_url = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "http://localhost:11434".to_string());
    let model = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "fastcontext-tools:latest".to_string());
    let prompt = args.get(3).cloned().unwrap_or_else(|| {
        "Where is the FastContextBackend multi-turn loop implemented?".to_string()
    });
    let max_turns = 10u32;

    eprintln!("base_url={base_url} model={model}\nprompt={prompt}\n---");

    let client = OpenAiClient::new(&base_url);
    let executor = ToolExecutor::from_invoke_request(&InvokeRequest::default());
    let tools = discovery_tools();

    let mut messages = vec![
        ChatMessage::system(
            "You are a repository-exploration assistant. You have three tools: \
             READ(path), GLOB(pattern), GREP(pattern, path). Use them to locate the code relevant \
             to the user's query. When you have enough information, respond with a <final_answer> \
             block listing citations in the form `path:line-start-line-end`, one per line.",
        ),
        ChatMessage::user(prompt),
    ];

    for turn in 0..max_turns {
        let req = ChatCompletionRequest {
            model: model.clone(),
            messages: messages.clone(),
            tools: tools.clone(),
            tool_choice: serde_json::json!("auto"),
            temperature: 0.0,
        };
        let response = client
            .complete(req)
            .await
            .expect("chat completion request failed");
        let choice = response
            .choices
            .into_iter()
            .next()
            .expect("no choices in response");
        let msg = choice.message;

        eprintln!(
            "[turn {turn}] finish_reason={:?} content={:?} tool_calls={:?}",
            choice.finish_reason, msg.content, msg.tool_calls
        );

        if let Some(ref content) = msg.content {
            if let Some(answer) = extract_final_answer(content) {
                println!("--- final_answer ---\n{answer}");
                return;
            }
        }

        match &msg.tool_calls {
            Some(tool_calls) if !tool_calls.is_empty() => {
                messages.push(ChatMessage::assistant(
                    msg.content.clone(),
                    msg.tool_calls.clone(),
                ));
                for tc in tool_calls {
                    let result_str = dispatch_tool(&executor, tc).await;
                    eprintln!(
                        "  -> {} {} => {}",
                        tc.function.name, tc.function.arguments, result_str
                    );
                    messages.push(ChatMessage::tool_result(
                        result_str,
                        tc.id.clone(),
                        tc.function.name.clone(),
                    ));
                }
            }
            _ => {
                messages.push(ChatMessage::assistant(msg.content.clone(), None));
            }
        }
    }

    println!("--- max_turns reached without <final_answer> ---");
}
