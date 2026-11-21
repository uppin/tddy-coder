//! Acceptance: a session started by an orchestrator is told how to report back, over exactly one
//! channel chosen by where it runs and what both agents can do — natively between two agents with
//! native peer messaging on one host, `tddy-tools send` for any other local child, and a message
//! line in its hook log for a child on a target — and a target child's message line reaches the
//! orchestrator through the daemon's hook follower (`#orchestra` 2/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Session messages.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_agent_launch::back_channel::{
    back_channel_for, back_channel_guidance, native_peer_name_of, BackChannel, BackChannelInputs,
    ChildPlacement, RemoteMessage,
};
use tddy_agent_launch::remote_hook_follower::{
    ActivityHookSink, HookLine, HookReportDelivery, HookSink,
};
use tddy_coding_agent_specs::{embedded_spec, CodingAgentSpec};
use tddy_service::proto::activity::{
    AwaitQuestionAnswerRequest, AwaitQuestionAnswerResponse, ReportAgentActivityRequest,
    ReportSessionStatusRequest, ReportSessionStatusResponse,
};

const ORCHESTRATOR: &str = "01a1fdd8-dea7-7e70-9945-d66127579ec2";
const CHILD: &str = "01a1fe01-0000-7000-8000-000000000001";
const HOOK_TOKEN: &str = "hook-token-1";
const HOOK_LOG: &str = "/home/coder/work/.tddy/hooks.log";

fn agent(id: &str) -> CodingAgentSpec {
    embedded_spec(id).unwrap_or_else(|e| panic!("{id} is built in: {e}"))
}

fn a_child_of_a_claude_orchestrator<'a>(
    orchestrator: &'a CodingAgentSpec,
    child: &'a CodingAgentSpec,
    placement: ChildPlacement<'a>,
) -> BackChannelInputs<'a> {
    BackChannelInputs {
        orchestrator_session_id: ORCHESTRATOR,
        orchestrator_agent: orchestrator,
        child_agent: child,
        placement,
    }
}

/// The daemon end of a target's followed hooks, recording the messages it is asked to post.
#[derive(Default)]
struct AnOrchestratorsDaemon {
    posted: Mutex<Vec<RemoteMessage>>,
}

#[async_trait]
impl HookReportDelivery for AnOrchestratorsDaemon {
    async fn report_session_status(
        &self,
        _request: ReportSessionStatusRequest,
    ) -> Result<ReportSessionStatusResponse, String> {
        Ok(ReportSessionStatusResponse::default())
    }

    async fn await_question_answer(
        &self,
        _request: AwaitQuestionAnswerRequest,
    ) -> Result<AwaitQuestionAnswerResponse, String> {
        Ok(AwaitQuestionAnswerResponse::default())
    }

    async fn report_agent_activity(
        &self,
        _request: ReportAgentActivityRequest,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn post_message_to_orchestrator(&self, message: RemoteMessage) -> Result<(), String> {
        self.posted.lock().unwrap().push(message);
        Ok(())
    }
}

/// One line of a target child's hook log, as the message command writes it.
fn a_message_line(payload: &str) -> String {
    format!(
        "{}\t{HOOK_TOKEN}\t{payload}",
        tddy_core::REMOTE_MESSAGE_EVENT
    )
}

#[test]
fn a_local_codex_child_of_an_orchestrator_reports_with_tddy_tools_send() {
    // Given a Claude orchestrator and a Codex child on the same host
    let (orchestrator, child) = (agent("claude"), agent("codex"));

    // When its back channel and guidance are chosen
    let channel = back_channel_for(&a_child_of_a_claude_orchestrator(
        &orchestrator,
        &child,
        ChildPlacement::Local,
    ));
    let guidance = back_channel_guidance(ORCHESTRATOR, &channel);

    // Then it reports with tddy-tools send, to its orchestrator's session
    assert_eq!(channel, BackChannel::ToolsSend);
    assert!(guidance.contains("tddy-tools send"), "{guidance}");
    assert!(guidance.contains(ORCHESTRATOR), "{guidance}");
}

#[test]
fn a_local_claude_child_of_a_claude_orchestrator_reports_natively_to_its_peer_name() {
    // Given a Claude orchestrator and a Claude child on the same host
    let (orchestrator, child) = (agent("claude"), agent("claude"));

    // When its back channel and guidance are chosen
    let channel = back_channel_for(&a_child_of_a_claude_orchestrator(
        &orchestrator,
        &child,
        ChildPlacement::Local,
    ));
    let guidance = back_channel_guidance(ORCHESTRATOR, &channel);

    // Then it messages the orchestrator natively, by the name the orchestrator was launched under
    let orchestrator_peer_name = native_peer_name_of(ORCHESTRATOR);
    assert_eq!(
        channel,
        BackChannel::NativePeer {
            orchestrator_peer_name: orchestrator_peer_name.clone()
        }
    );
    assert!(guidance.contains(&orchestrator_peer_name), "{guidance}");
}

#[test]
fn a_claude_child_on_a_target_reports_through_its_hook_log_not_natively() {
    // Given a Claude orchestrator and a Claude child on a Coder workspace
    let (orchestrator, child) = (agent("claude"), agent("claude"));

    // When its back channel and guidance are chosen
    let channel = back_channel_for(&a_child_of_a_claude_orchestrator(
        &orchestrator,
        &child,
        ChildPlacement::Target {
            hook_token: HOOK_TOKEN,
            hook_log_path: HOOK_LOG,
        },
    ));
    let guidance = back_channel_guidance(ORCHESTRATOR, &channel);

    // Then it appends a message line to its hook log, with the very command its hooks use
    assert_eq!(
        channel,
        BackChannel::HookLog {
            command: tddy_core::remote_hook_command(
                tddy_core::REMOTE_MESSAGE_EVENT,
                HOOK_TOKEN,
                HOOK_LOG
            )
        }
    );
    assert!(
        guidance.contains(tddy_core::REMOTE_MESSAGE_EVENT),
        "{guidance}"
    );
    assert!(guidance.contains(HOOK_LOG), "{guidance}");
}

#[test]
fn every_channels_guidance_asks_for_periodic_progress_and_status_reports() {
    // Given each kind of channel
    let channels = [
        BackChannel::NativePeer {
            orchestrator_peer_name: native_peer_name_of(ORCHESTRATOR),
        },
        BackChannel::ToolsSend,
        BackChannel::HookLog {
            command: tddy_core::remote_hook_command(
                tddy_core::REMOTE_MESSAGE_EVENT,
                HOOK_TOKEN,
                HOOK_LOG,
            ),
        },
    ];

    // When each one's guidance is written
    let missing: Vec<String> = channels
        .iter()
        .map(|channel| back_channel_guidance(ORCHESTRATOR, channel))
        .filter(|guidance| !(guidance.contains("progress") && guidance.contains("status")))
        .collect();

    // Then every one asks the child to report its progress and status
    assert_eq!(missing, Vec::<String>::new());
}

#[test]
fn a_message_line_is_a_hook_the_follower_accepts() {
    // Given a message line a target child appended
    let line = a_message_line(r#"{"text":"tests written, 2 of 5 failing"}"#);

    // When the follower parses it
    let parsed = HookLine::parse(&line, HOOK_TOKEN);

    // Then it is the message event, with its payload
    let parsed = parsed.expect("a message line is a hook line");
    assert_eq!(
        (parsed.event.as_str(), parsed.payload["text"].as_str()),
        (
            tddy_core::REMOTE_MESSAGE_EVENT,
            Some("tests written, 2 of 5 failing")
        )
    );
}

#[tokio::test]
async fn a_followed_message_line_is_posted_to_the_orchestrator() {
    // Given the follower's sink for a target child
    let daemon = Arc::new(AnOrchestratorsDaemon::default());
    let sink = ActivityHookSink::new(
        Arc::clone(&daemon) as Arc<dyn HookReportDelivery>,
        CHILD,
        HOOK_TOKEN,
        "coder",
    );
    let line = HookLine::parse(
        &a_message_line(r#"{"text":"PR pushed, CI running"}"#),
        HOOK_TOKEN,
    )
    .expect("a message line is a hook line");

    // When it delivers the line
    sink.deliver(line).await;

    // Then the child's text is posted to its orchestrator, as the child
    assert_eq!(
        daemon.posted.lock().unwrap().clone(),
        vec![RemoteMessage {
            session_id: CHILD.to_string(),
            os_user: "coder".to_string(),
            text: "PR pushed, CI running".to_string(),
        }]
    );
}

#[tokio::test]
async fn a_message_line_without_text_is_not_posted() {
    // Given the follower's sink and a message line whose payload has no text
    let daemon = Arc::new(AnOrchestratorsDaemon::default());
    let sink = ActivityHookSink::new(
        Arc::clone(&daemon) as Arc<dyn HookReportDelivery>,
        CHILD,
        HOOK_TOKEN,
        "coder",
    );
    let line = HookLine::parse(&a_message_line(r#"{"note":"no text"}"#), HOOK_TOKEN)
        .expect("a message line is a hook line");

    // When it delivers the line
    sink.deliver(line).await;

    // Then nothing is posted
    assert_eq!(
        daemon.posted.lock().unwrap().clone(),
        Vec::<RemoteMessage>::new()
    );
}

#[test]
fn a_native_peer_name_is_stable_and_names_the_session() {
    // When the same session's peer name is asked twice
    let (first, second) = (native_peer_name_of(CHILD), native_peer_name_of(CHILD));

    // Then it is the same, and carries the session id
    assert_eq!(first, second);
    assert!(first.contains(CHILD), "{first}");
}
