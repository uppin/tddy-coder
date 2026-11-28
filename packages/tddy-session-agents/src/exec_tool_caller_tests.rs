//! What an exec-tool caller is told when its session token is refused.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_github::{GitHubUser, SessionTokenSigner};
use tddy_rpc::Code;
use tddy_service::proto::exec_tools::ExecuteToolRequest;

use super::authorize_exec_tool_caller;

fn a_resolver_that_accepts_nothing() -> SessionUserResolver {
    Arc::new(|_| None)
}

fn a_call_carrying(token: &str) -> ExecuteToolRequest {
    ExecuteToolRequest {
        session_token: token.to_string(),
        session_id: "01a20f10-orchestrator".to_string(),
        tool_name: "ReadSessionTerminal".to_string(),
        ..Default::default()
    }
}

fn an_agent_token_that_expired_an_hour_ago() -> String {
    SessionTokenSigner::new(
        tddy_github::session_token_v2::Ed25519SigningKey::from_bytes(&[7u8; 32]),
    )
    .mint_agent_with_issued_at(
        &GitHubUser {
            id: 7,
            login: "operator".to_string(),
            avatar_url: String::new(),
            name: String::new(),
        },
        SystemTime::now() - Duration::from_secs(86_400 + 3_600),
        Duration::from_secs(86_400),
        "01a20f10-orchestrator",
    )
}

#[test]
fn an_expired_token_is_refused_as_expired_with_the_way_out() {
    // Given a call carrying an agent token that expired an hour ago
    let refusal = authorize_exec_tool_caller(
        &DaemonConfig::default(),
        &a_resolver_that_accepts_nothing(),
        &a_call_carrying(&an_agent_token_that_expired_an_hour_ago()),
    )
    .expect_err("an expired token is refused");

    // Then the refusal names the expiry and the way out, not a split-session misconfiguration
    assert_eq!(refusal.code, Code::Unauthenticated);
    assert!(refusal.message.contains("expired"), "{}", refusal.message);
    assert!(refusal.message.contains("resum"), "{}", refusal.message);
    assert!(
        !refusal.message.contains("split session"),
        "an expiry is not a split-session misconfiguration: {}",
        refusal.message
    );
}

#[test]
fn a_token_that_does_not_parse_keeps_the_verification_hint() {
    // Given a call carrying something that is not a session token at all
    let refusal = authorize_exec_tool_caller(
        &DaemonConfig::default(),
        &a_resolver_that_accepts_nothing(),
        &a_call_carrying("not-a-token"),
    )
    .expect_err("an unreadable token is refused");

    // Then the refusal is the token-verification one
    assert_eq!(refusal.code, Code::Unauthenticated);
    assert!(
        refusal.message.contains("could not verify"),
        "{}",
        refusal.message
    );
}
