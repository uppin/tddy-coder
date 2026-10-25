//! A session's tool call over the daemon's HTTP relay speaks protobuf to `ExecToolService/ExecuteTool`.
//!
//! The call runs through the public dispatch a session's tools use, with the transport the session's
//! environment selects. The daemon is the real router on a loopback port (see `support`), so a body
//! the generated handler cannot decode never reaches the service.

mod support;

use serde_json::json;
use serial_test::serial;
use support::{a_session_reaching_its_daemon_over_http, ADaemonOnARealPort, ExecToolsDaemon};
use tddy_service::proto::exec_tools::ExecuteToolResponse;
use tddy_session_tool_client::dispatch_session_tool;

#[tokio::test]
#[serial]
async fn a_tool_call_over_http_reaches_execute_tool_as_one_decoded_request() {
    // Given a daemon on a real port that answers the tool call, and a session that reaches it over HTTP
    let daemon = ADaemonOnARealPort::serving(ExecToolsDaemon::answering(
        ExecuteToolResponse {
            result_json: r#"{"ok":true}"#.to_string(),
            ..Default::default()
        },
        &[],
    ))
    .await;
    a_session_reaching_its_daemon_over_http(&daemon.rpc_base_url());

    // When the session calls a tool
    dispatch_session_tool("Read", json!({"path": "a.txt"})).await;

    // Then the daemon decoded exactly one ExecuteTool request, naming the session and the tool
    let executed = daemon.daemon.decoded_executes();
    assert_eq!(
        executed.len(),
        1,
        "the daemon must decode one ExecuteTool request"
    );
    let request = &executed[0];
    assert_eq!(
        (
            request.session_id.as_str(),
            request.session_token.as_str(),
            request.daemon_instance_id.as_str(),
            request.tool_name.as_str(),
        ),
        ("sess-http-1", "tok-http", "codebase-host", "Read")
    );
    assert_eq!(request.args_json, json!({"path": "a.txt"}).to_string());
}

#[tokio::test]
#[serial]
async fn a_tool_call_over_http_returns_the_result_json_the_daemon_answered() {
    // Given a daemon on a real port that answers the tool call with a result
    let daemon = ADaemonOnARealPort::serving(ExecToolsDaemon::answering(
        ExecuteToolResponse {
            result_json: r#"{"ok":true}"#.to_string(),
            ..Default::default()
        },
        &[],
    ))
    .await;
    a_session_reaching_its_daemon_over_http(&daemon.rpc_base_url());

    // When the session calls a tool
    let output = dispatch_session_tool("Read", json!({"path": "a.txt"})).await;

    // Then the call returns the result JSON the daemon answered, verbatim
    assert_eq!(output, r#"{"ok":true}"#);
}

#[tokio::test]
#[serial]
async fn a_tool_call_the_daemon_refuses_over_http_returns_its_error_message() {
    // Given a daemon on a real port that refuses the tool call with a message
    let daemon = ADaemonOnARealPort::serving(ExecToolsDaemon::answering(
        ExecuteToolResponse {
            is_error: true,
            error_message: "no such tool: Frobnicate".to_string(),
            ..Default::default()
        },
        &[],
    ))
    .await;
    a_session_reaching_its_daemon_over_http(&daemon.rpc_base_url());

    // When the session calls the tool
    let output = dispatch_session_tool("Frobnicate", json!({})).await;

    // Then the call returns an error body carrying the daemon's message
    let body: serde_json::Value =
        serde_json::from_str(&output).expect("an error answer is a JSON body");
    assert_eq!(
        body,
        json!({"error": "no such tool: Frobnicate", "is_error": true})
    );
}
