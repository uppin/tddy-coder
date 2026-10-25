//! A conversation's worktree operation over the daemon's HTTP relay reads the daemon's protobuf reply.
//!
//! The operation runs through the public entry a conversation asks with, under the transport the
//! session's environment selects. The daemon is the real router on a loopback port (see `support`), so
//! the reply the test reads is the one the generated handler encoded.

mod support;

use serial_test::serial;
use support::{a_session_reaching_its_daemon_over_http, ADaemonOnARealPort, ExecToolsDaemon};
use tddy_service::proto::exec_tools::{
    conversation_worktree_request::Op, ConversationWorktreeResponse, ResetOp,
};
use tddy_session_tool_client::reset_conversation_worktree;

#[tokio::test]
#[serial]
async fn a_reset_whose_reply_carries_no_result_json_is_an_error_naming_the_missing_result() {
    // Given a daemon on a real port whose reply carries no result_json, and a session that reaches it
    // over HTTP
    let daemon = ADaemonOnARealPort::serving(
        ExecToolsDaemon::answering(Default::default(), &[])
            .with_conversation_answer(ConversationWorktreeResponse::default()),
    )
    .await;
    a_session_reaching_its_daemon_over_http(&daemon.rpc_base_url());

    // When the conversation resets its worktree
    let output = reset_conversation_worktree("explore", Some("3f9c2ab")).await;

    // Then the answer is an error naming the missing result, not an empty result
    let body: serde_json::Value =
        serde_json::from_str(&output).expect("an error answer is a JSON body");
    assert_eq!(
        body,
        serde_json::json!({"error": "the reply carries no result_json", "is_error": true})
    );
}

#[tokio::test]
#[serial]
async fn a_reset_whose_reply_carries_a_result_json_returns_it_verbatim() {
    // Given a daemon on a real port whose reply carries a reset result, and a session that reaches it
    // over HTTP
    let result = r#"{"reset":{"to":"3f9c2ab","droppedCommits":0}}"#;
    let daemon = ADaemonOnARealPort::serving(
        ExecToolsDaemon::answering(Default::default(), &[]).with_conversation_answer(
            ConversationWorktreeResponse {
                result_json: result.to_string(),
            },
        ),
    )
    .await;
    a_session_reaching_its_daemon_over_http(&daemon.rpc_base_url());

    // When the conversation resets its worktree
    let output = reset_conversation_worktree("explore", Some("3f9c2ab")).await;

    // Then the answer is the result JSON the daemon encoded, verbatim
    assert_eq!(output, result);
}

#[tokio::test]
#[serial]
async fn the_reset_reaches_conversation_worktree_as_one_decoded_request_naming_the_commit() {
    // Given a daemon on a real port, and a session that reaches it over HTTP
    let daemon = ADaemonOnARealPort::serving(
        ExecToolsDaemon::answering(Default::default(), &[]).with_conversation_answer(
            ConversationWorktreeResponse {
                result_json: "{}".to_string(),
            },
        ),
    )
    .await;
    a_session_reaching_its_daemon_over_http(&daemon.rpc_base_url());

    // When the conversation resets its worktree to a commit
    reset_conversation_worktree("explore", Some("3f9c2ab")).await;

    // Then the daemon decoded exactly one ConversationWorktree request: the conversation, the session,
    // and a reset to that commit
    let conversations = daemon.daemon.decoded_conversations();
    assert_eq!(
        conversations.len(),
        1,
        "the daemon must decode one ConversationWorktree request"
    );
    let request = &conversations[0];
    assert_eq!(
        (
            request.session_id.as_str(),
            request.session_token.as_str(),
            request.daemon_instance_id.as_str(),
            request.conversation_id.as_str(),
        ),
        ("sess-http-1", "tok-http", "codebase-host", "explore")
    );
    assert_eq!(
        request.op,
        Some(Op::Reset(ResetOp {
            commit: "3f9c2ab".to_string()
        }))
    );
}
