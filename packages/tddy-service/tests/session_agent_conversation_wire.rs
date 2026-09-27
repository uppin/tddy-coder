//! What an `OpenAgentConversation` carries, and where on the wire it carries it.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md — the agent opening a conversation may
//! replace the def's system prompt for that conversation alone. A conversation whose turn loop
//! runs on another daemon is opened over this RPC, so an override that cannot ride this request
//! is an override that does not exist for every session that runs its tools in a jail.
//!
//! The field *number* is pinned here as well as the field: two daemons of different builds talk
//! over these bytes, and a renumbering would have the newer one send a prompt the older one reads
//! as some other field — or as nothing at all, which is the failure this override must never have.

use prost::Message;
use tddy_service::proto::session_agents_svc::OpenAgentConversationRequest;

// ─── builders ───────────────────────────────────────────────────────────────────────────────────

/// Short enough that its protobuf length prefix is one byte, which
/// [`the_system_prompt_rides_field_six_where_every_build_looks_for_it`] hand-encodes.
const AN_OVERRIDE: &str = "Answer only with file paths.";

/// An open of a conversation with one attached agent, carrying no override.
fn an_open_request() -> OpenAgentConversationRequest {
    OpenAgentConversationRequest {
        session_token: "session-token-for-the-facilitating-daemon".to_string(),
        session_id: "01a04d08-2bbf-7850-ae60-0df89791608a".to_string(),
        daemon_instance_id: "udoo".to_string(),
        agent_id: "FastContext@mac".to_string(),
        conversation_id: "conv-1".to_string(),
        system_prompt: String::new(),
    }
}

// ─── tests ──────────────────────────────────────────────────────────────────────────────────────

#[test]
fn an_open_carries_the_system_prompt_override_it_was_given() {
    // Given an open that replaces the def's prompt for this conversation
    let request = OpenAgentConversationRequest {
        system_prompt: AN_OVERRIDE.to_string(),
        ..an_open_request()
    };

    // When it crosses the wire
    let decoded = OpenAgentConversationRequest::decode(request.encode_to_vec().as_slice())
        .expect("an open request should decode as one");

    // Then the override is what the serving daemon reads
    assert_eq!(decoded.system_prompt, AN_OVERRIDE);
}

#[test]
fn the_system_prompt_rides_field_six_where_every_build_looks_for_it() {
    // Given bytes carrying nothing but a length-delimited value in field 6
    let mut bytes = vec![(6 << 3) | 2, AN_OVERRIDE.len() as u8];
    bytes.extend_from_slice(AN_OVERRIDE.as_bytes());

    // When they are read as an open request
    let decoded = OpenAgentConversationRequest::decode(bytes.as_slice())
        .expect("field 6 should decode as the open request's system prompt");

    // Then field 6 is the system prompt
    assert_eq!(decoded.system_prompt, AN_OVERRIDE);
}
