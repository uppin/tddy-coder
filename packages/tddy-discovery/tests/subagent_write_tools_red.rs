//! Unit tests: write-capable subagent tools (`WRITE`/`STR_REPLACE`/`DELETE`) — the coder-role
//! extension of the internal subagent tool loop.
//!
//! Feature: docs/ft/coder/no-bash-mode.md (no-write mode: coder subagent)
//!
//! The mutation tools are opt-in per def (`tools:` list), Managed-access only (path confinement
//! comes from the host tool engine), and never advertised by a def that doesn't bind them.

use std::sync::{Arc, Mutex};

use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{CodebaseAccess, SubagentConfig, SubagentRegistry};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

type RecordedCalls = Arc<Mutex<Vec<(String, serde_json::Value)>>>;

/// A `CodebaseAccess::Managed` backed by a fake dispatch fn that records every call and always
/// returns `response` (same harness as `codebase_access_red.rs`).
fn managed_access_with(response: &'static str) -> (RecordedCalls, CodebaseAccess) {
    let calls: RecordedCalls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_closure = calls.clone();
    let access = CodebaseAccess::managed(move |tool_name: String, args: serde_json::Value| {
        let calls = calls_for_closure.clone();
        Box::pin(async move {
            calls.lock().unwrap().push((tool_name, args));
            response.to_string()
        })
    });
    (calls, access)
}

// ─── Def schema: the new tool variants parse from YAML ─────────────────────────

/// A coder-role def binds the mutation tools by their model-facing names in the YAML `tools:`
/// list — including `STR_REPLACE`, whose serde name is explicit (plain UPPERCASE renaming would
/// yield `STRREPLACE`).
#[test]
fn a_def_yaml_binds_the_mutation_tools_by_their_model_facing_names() {
    // Given / When
    let def: SpecializedAgentDef = serde_yaml::from_str(
        "\
name: coder
model: some-coder-model
tools: [READ, GLOB, GREP, WRITE, STR_REPLACE, DELETE]
",
    )
    .expect("a coder def binding the mutation tools must parse");

    // Then
    assert_eq!(
        def.tools,
        vec![
            SubagentTool::Read,
            SubagentTool::Glob,
            SubagentTool::Grep,
            SubagentTool::Write,
            SubagentTool::StrReplace,
            SubagentTool::Delete,
        ]
    );
}

/// The defaults stay read-only: a def that omits `tools:` must not silently gain write access.
#[test]
fn a_def_without_a_tools_list_stays_read_only() {
    // Given / When
    let def: SpecializedAgentDef = serde_yaml::from_str("name: reader\nmodel: m\n")
        .expect("a minimal def must parse");

    // Then
    assert!(
        def.tools.iter().all(|t| !t.is_mutating()),
        "default tools must be read-only; got: {:?}",
        def.tools
    );
}

// ─── Managed: name mapping + arg shapes ────────────────────────────────────────

/// A managed WRITE dispatches the capitalized `"Write"` tool name with the exec-catalog
/// `{"path", "contents"}` payload — the same shape the host tool engine validates and confines.
#[tokio::test]
async fn managed_write_dispatches_the_exec_catalog_write_shape() {
    // Given
    let (calls, access) = managed_access_with(r#"{"ok":true}"#);

    // When
    access
        .write("src/new.rs", "fn hello() {}")
        .await
        .expect("managed WRITE must succeed on a success payload");

    // Then
    let recorded = calls.lock().unwrap();
    assert_eq!(recorded[0].0, "Write", "tool name must be 'Write'");
    assert_eq!(
        recorded[0].1,
        serde_json::json!({"path": "src/new.rs", "contents": "fn hello() {}"})
    );
}

/// A managed STR_REPLACE dispatches `"StrReplace"` with `{"path", "old_string", "new_string"}`.
#[tokio::test]
async fn managed_str_replace_dispatches_the_exec_catalog_str_replace_shape() {
    // Given
    let (calls, access) = managed_access_with(r#"{"ok":true}"#);

    // When
    access
        .str_replace("src/lib.rs", "old()", "new()")
        .await
        .expect("managed STR_REPLACE must succeed on a success payload");

    // Then
    let recorded = calls.lock().unwrap();
    assert_eq!(recorded[0].0, "StrReplace");
    assert_eq!(
        recorded[0].1,
        serde_json::json!({
            "path": "src/lib.rs",
            "old_string": "old()",
            "new_string": "new()",
        })
    );
}

/// A managed DELETE dispatches `"Delete"` with `{"path"}`.
#[tokio::test]
async fn managed_delete_dispatches_the_exec_catalog_delete_shape() {
    // Given
    let (calls, access) = managed_access_with(r#"{"ok":true}"#);

    // When
    access
        .delete("src/tmp.rs")
        .await
        .expect("managed DELETE must succeed on a success payload");

    // Then
    let recorded = calls.lock().unwrap();
    assert_eq!(recorded[0].0, "Delete");
    assert_eq!(recorded[0].1, serde_json::json!({"path": "src/tmp.rs"}));
}

// ─── Local: mutation is rejected, not silently granted ─────────────────────────

/// Local codebase access has no path-confinement layer, so every mutation tool returns a typed
/// error instead of touching the host filesystem — a YAML `tools:` entry alone must not grant
/// unrestricted host writes to a co-located subagent.
#[tokio::test]
async fn local_codebase_access_rejects_every_mutation_tool() {
    // Given
    let access = CodebaseAccess::Local;

    // When / Then
    let write_err = access
        .write("/etc/passwd", "oops")
        .await
        .expect_err("local WRITE must be rejected");
    assert!(
        write_err.to_string().contains("managed"),
        "the error must say mutation requires managed access; got: {write_err}"
    );
    access
        .str_replace("/etc/passwd", "a", "b")
        .await
        .expect_err("local STR_REPLACE must be rejected");
    access
        .delete("/etc/passwd")
        .await
        .expect_err("local DELETE must be rejected");
}

// ─── Session loop: bound coder writes; unbound defs cannot ─────────────────────

fn tool_call_response(tool_name: &str, args: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": { "name": tool_name, "arguments": args.to_string() }
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
                "content": format!("<final_answer>\n{answer}\n</final_answer>")
            },
            "finish_reason": "stop"
        }]
    })
}

fn a_def_with_tools(name: &str, base_url: &str, tools: Vec<SubagentTool>) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: name.to_string(),
        label: None,
        model: "some-model".to_string(),
        base_url: base_url.to_string(),
        system_prompt: None,
        system_prompt_path: None,
        tools,
        max_turns: 4,
        replaces: vec![],
    }
}

fn managed_config(access: CodebaseAccess) -> SubagentConfig {
    SubagentConfig {
        base_url: String::new(),
        model: String::new(),
        max_turns: 0,
        access,
    }
}

/// A coder def that binds WRITE executes a model-issued WRITE through the managed dispatch fn —
/// the full delegation path a no-write session relies on.
#[tokio::test]
async fn a_write_bound_coder_session_dispatches_a_model_issued_write() {
    // Given — turn 1 issues WRITE, turn 2 answers
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tool_call_response(
            "WRITE",
            serde_json::json!({"path": "src/new.rs", "contents": "fn f() {}"}),
        )))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(final_answer_response("done")))
        .mount(&server)
        .await;
    let (calls, access) = managed_access_with(r#"{"ok":true}"#);
    let registry = SubagentRegistry::from_defs(vec![a_def_with_tools(
        "coder",
        &server.uri(),
        vec![SubagentTool::Read, SubagentTool::Write],
    )]);
    let mut session = registry
        .create("coder", managed_config(access))
        .expect("the coder def must resolve");

    // When
    let outcome = session
        .prompt("create src/new.rs")
        .await
        .expect("prompt must succeed");

    // Then
    assert_eq!(outcome.content[0].text, "done");
    let recorded = calls.lock().unwrap();
    assert_eq!(recorded.len(), 1, "exactly one managed dispatch expected");
    assert_eq!(recorded[0].0, "Write");
}

/// A read-only def never advertises the mutation tools to the model, and a WRITE the model
/// hallucinates anyway is rejected as unbound — without reaching the dispatch fn.
#[tokio::test]
async fn a_read_only_session_neither_advertises_nor_executes_write() {
    // Given — the model (mis)issues WRITE on turn 1, then answers
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tool_call_response(
            "WRITE",
            serde_json::json!({"path": "src/x.rs", "contents": "x"}),
        )))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(final_answer_response("ok")))
        .mount(&server)
        .await;
    let (calls, access) = managed_access_with(r#"{"ok":true}"#);
    let registry = SubagentRegistry::from_defs(vec![a_def_with_tools(
        "reader",
        &server.uri(),
        vec![SubagentTool::Read, SubagentTool::Glob, SubagentTool::Grep],
    )]);
    let mut session = registry
        .create("reader", managed_config(access))
        .expect("the reader def must resolve");

    // When
    session.prompt("look around").await.expect("prompt must succeed");

    // Then — no dispatch reached the codebase
    assert!(
        calls.lock().unwrap().is_empty(),
        "an unbound WRITE must never reach the managed dispatch fn"
    );
    // And — the advertised tool list on every request stayed read-only
    for request in server.received_requests().await.unwrap() {
        let body: serde_json::Value = request.body_json().unwrap();
        if let Some(tools) = body["tools"].as_array() {
            for tool in tools {
                let name = tool["function"]["name"].as_str().unwrap_or("");
                assert!(
                    !["WRITE", "STR_REPLACE", "DELETE"].contains(&name),
                    "a read-only def must not advertise {name}"
                );
            }
        }
    }
}
