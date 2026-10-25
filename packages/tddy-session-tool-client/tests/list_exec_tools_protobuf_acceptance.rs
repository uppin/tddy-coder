//! A session's question "which exec tools may I use" over the daemon's HTTP relay speaks protobuf to
//! `ExecToolService/ListExecTools`.
//!
//! The question runs through the public entry a session asks it with, under the transport the session's
//! environment selects. The daemon is the real router on a loopback port (see `support`), so a body the
//! generated handler cannot decode never reaches the service.

mod support;

use serial_test::serial;
use support::{a_session_reaching_its_daemon_over_http, ADaemonOnARealPort, ExecToolsDaemon};
use tddy_service::proto::exec_tools::ExecuteToolResponse;
use tddy_session_tool_client::{session_exec_tool_answer, SessionExecToolAnswer};

#[tokio::test]
#[serial]
async fn a_session_over_http_is_admitted_the_exec_tools_its_daemon_lists() {
    // Given a daemon on a real port that lists two exec tools, and a session that reaches it over HTTP
    let daemon = ADaemonOnARealPort::serving(ExecToolsDaemon::answering(
        ExecuteToolResponse::default(),
        &["Read", "ListProjectSessions"],
    ))
    .await;
    a_session_reaching_its_daemon_over_http(&daemon.rpc_base_url());

    // When the session asks which exec tools it may use
    let answer = session_exec_tool_answer().await;

    // Then the answer admits the two tool names the daemon listed, in its order
    assert_eq!(
        answer,
        SessionExecToolAnswer::Admitted(vec![
            "Read".to_string(),
            "ListProjectSessions".to_string(),
        ])
    );
}

#[tokio::test]
#[serial]
async fn the_question_reaches_list_exec_tools_as_one_decoded_request_naming_the_session() {
    // Given a daemon on a real port, and a session that reaches it over HTTP
    let daemon = ADaemonOnARealPort::serving(ExecToolsDaemon::answering(
        ExecuteToolResponse::default(),
        &["Read"],
    ))
    .await;
    a_session_reaching_its_daemon_over_http(&daemon.rpc_base_url());

    // When the session asks which exec tools it may use
    session_exec_tool_answer().await;

    // Then the daemon decoded exactly one ListExecTools request, naming the session
    let listed = daemon.daemon.decoded_lists();
    assert_eq!(
        listed.len(),
        1,
        "the daemon must decode one ListExecTools request"
    );
    assert_eq!(
        (
            listed[0].session_id.as_str(),
            listed[0].session_token.as_str(),
            listed[0].daemon_instance_id.as_str(),
        ),
        ("sess-http-1", "tok-http", "codebase-host")
    );
}
