//! A resume continues the conversation a session's agent is in now, not the one it started with
//! (`#orchestrators` 6/11): after `/clear` the session's `.session.yaml` lists the post-clear
//! conversation last, and a resume of a `daemon_session_id` agent binds that id. A session recorded
//! before the list existed resumes by its own id, as it always has.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-10-conversation-rotation.md`.

use std::path::PathBuf;

use pretty_assertions::assert_eq;
use tddy_actions::tool::{render_argv, ResolvedTool};
use tddy_agent_launch::current_conversation_id;
use tddy_agent_launch::harness_from_spec::{harness_template_params, HarnessLaunch};
use tddy_coding_agent_specs::{embedded_spec, CodingAgentSpec};
use tddy_core::agent_conversation::{AgentConversation, AgentConversationReason};
use tddy_core::SessionMetadata;
use tddy_testing_commons::builders::a_session_metadata;

const THE_SESSION: &str = "01900000-0000-7000-8000-0000000006c1";
const THE_POST_CLEAR_CONVERSATION: &str = "6c1e0a00-0000-4000-8000-0000000006c2";

fn claude() -> CodingAgentSpec {
    embedded_spec("claude").expect("claude is built in")
}

/// A stopped claude-cli session of [`THE_SESSION`] listing `conversations`, oldest first.
fn a_session_listing(conversations: Vec<AgentConversation>) -> SessionMetadata {
    SessionMetadata {
        agent: Some("claude".to_string()),
        model: Some("opus".to_string()),
        agent_conversations: conversations,
        ..a_session_metadata()
            .with_session_id(THE_SESSION)
            .with_session_type("claude-cli")
            .with_status("stopped")
            .build()
    }
}

fn a_conversation(id: &str, reason: AgentConversationReason) -> AgentConversation {
    AgentConversation {
        id: id.to_string(),
        started_at_unix_ms: 1_762_771_200_000,
        reason,
    }
}

/// The argv `spec` renders to resume `meta`: the conversation the resume binds is the session's
/// current one, as the generic launch binds it.
fn the_resume_argv(spec: &CodingAgentSpec, meta: &SessionMetadata) -> Vec<String> {
    let params = harness_template_params(&HarnessLaunch {
        model: "opus",
        resume: true,
        session_id: current_conversation_id(meta, spec),
        ..HarnessLaunch::default()
    })
    .with_list("slots", Vec::new());
    let binary = ResolvedTool {
        id: spec.id.clone(),
        command: "claude".to_string(),
        binary: PathBuf::from("/usr/local/bin/claude"),
        reads: Vec::new(),
        network: Default::default(),
    };
    render_argv(&spec.run, &binary, &params).expect("claude's resume argv renders")
}

/// The id `argv` passes to `--resume`, if it resumes at all.
fn resumed_id(argv: &[String]) -> Option<&str> {
    argv.windows(2)
        .find(|pair| pair[0] == "--resume")
        .map(|pair| pair[1].as_str())
}

#[test]
fn resuming_after_a_clear_continues_the_post_clear_conversation() {
    // Given a claude-cli session whose agent cleared once
    let session = a_session_listing(vec![
        a_conversation(THE_SESSION, AgentConversationReason::Start),
        a_conversation(THE_POST_CLEAR_CONVERSATION, AgentConversationReason::Rotate),
    ]);

    // When it is resumed
    let argv = the_resume_argv(&claude(), &session);

    // Then Claude Code resumes the conversation after the clear
    assert_eq!(resumed_id(&argv), Some(THE_POST_CLEAR_CONVERSATION));
}

#[test]
fn a_session_with_no_conversation_list_resumes_by_its_session_id() {
    // Given a claude-cli session recorded before conversations were listed
    let session = a_session_listing(Vec::new());

    // When it is resumed
    let argv = the_resume_argv(&claude(), &session);

    // Then Claude Code resumes the conversation of the session's own id
    assert_eq!(resumed_id(&argv), Some(THE_SESSION));
}
