//! The daemon renews a session agent's own token when the agent's `tddy-tools --mcp` asks with the
//! reserved `tddy.token.RenewAgentSession` call, and refuses any token that is not that agent's.
//!
//! Incident: orchestrator 01a20f10 lost every tool when its 24 h agent token expired (2026-11-27).

use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_rpc::{Code, Request};
use tddy_service::proto::exec_tools::{ExecToolService as _, ExecuteToolRequest};
use tddy_session_lifecycle::test_util::{signing_test_host, SignedTestCredentials};
use tddy_session_tool_client::session_token::RENEW_AGENT_SESSION_TOKEN_TOOL;

const SESSION: &str = "01a20f10-orchestrator";

fn a_signing_daemon() -> (TestDaemon, SignedTestCredentials, tempfile::TempDir) {
    let sessions = tempfile::tempdir().expect("a sessions base");
    let (host, credentials) = signing_test_host(sessions.path().to_path_buf());
    (TestDaemon::from_host(host), credentials, sessions)
}

fn a_renewal(session_token: &str) -> Request<ExecuteToolRequest> {
    Request::direct(ExecuteToolRequest {
        session_token: session_token.to_string(),
        session_id: SESSION.to_string(),
        tool_name: RENEW_AGENT_SESSION_TOKEN_TOOL.to_string(),
        args_json: "{}".to_string(),
        ..Default::default()
    })
}

fn renewed_token_in(result_json: &str) -> String {
    let body: serde_json::Value = serde_json::from_str(result_json).expect("a JSON answer");
    body["session_token"]
        .as_str()
        .expect("the answer carries a token")
        .to_string()
}

#[tokio::test]
async fn an_agent_renews_its_token_and_the_renewed_token_is_accepted_in_turn() {
    // Given the daemon that signed the agent's token
    let (daemon, credentials, _sessions) = a_signing_daemon();
    let exec_tools = daemon.handlers().exec_tool_service();

    // When the agent renews its token with the reserved call
    let first = exec_tools
        .execute_tool(a_renewal(&credentials.agent_token_for(SESSION)))
        .await
        .expect("the agent's own token is renewed")
        .into_inner();
    let renewed = renewed_token_in(&first.result_json);

    // Then the renewed token is a working agent token: renewing with it succeeds as well
    let second = exec_tools
        .execute_tool(a_renewal(&renewed))
        .await
        .expect("the renewed token is accepted")
        .into_inner();
    assert!(!renewed_token_in(&second.result_json).is_empty());
}

#[tokio::test]
async fn the_operators_own_token_cannot_be_renewed_into_an_agent_token() {
    // Given the operator's own access token
    let (daemon, credentials, _sessions) = a_signing_daemon();

    // When it is presented to the reserved renewal call
    let refusal = daemon
        .handlers()
        .exec_tool_service()
        .execute_tool(a_renewal(&credentials.user_token))
        .await
        .expect_err("a user's token is refused");

    // Then the daemon refuses: a user's token buys no agent token
    assert_eq!(refusal.code, Code::Unauthenticated);
    assert!(
        refusal.message.contains("not an agent's access token"),
        "{}",
        refusal.message
    );
}
