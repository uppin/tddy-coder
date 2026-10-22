//! The operator channel: the port a session's elicitations and attention notifications reach an
//! operator's chat through, and the core's one rule for "this presenter mode waits on a human".
//!
//! PRD `docs/ft/daemon/1-WIP/PRD-2026-10-22-telegram-satellite-integration.md`, changes 1 and 2.
//! The rule is the core's because it must hold on a daemon with no channel at all: it is what turns
//! a workflow session's drawer dot yellow.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_daemon_kernel::operator_channel::{
    awaits_operator, NoOperatorChannel, OperatorChannel, SharedOperatorChannel,
};
use tddy_daemon_kernel::session_notification::{
    SessionNotification, SessionNotificationKind, SessionNotificationSource,
};
use tddy_service::gen::app_mode_proto::Variant;
use tddy_service::gen::server_message::Event;
use tddy_service::gen::{
    AppModeDocumentReview, AppModeDone, AppModeFeatureInput, AppModeMarkdownViewer,
    AppModeMultiSelect, AppModeProto, AppModeRunning, AppModeSelect, AppModeTextInput, ModeChanged,
    ServerMessage,
};

const SESSION_ID: &str = "01900000-0000-7000-8000-CCDD00000001";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

fn a_mode_change_to(variant: Variant) -> ModeChanged {
    ModeChanged {
        mode: Some(AppModeProto {
            variant: Some(variant),
        }),
    }
}

fn a_mode_change_with_no_mode() -> ModeChanged {
    ModeChanged { mode: None }
}

fn a_presenter_event(mode_changed: ModeChanged) -> ServerMessage {
    ServerMessage {
        event: Some(Event::ModeChanged(mode_changed)),
        ..Default::default()
    }
}

fn an_attention_notification() -> SessionNotification {
    SessionNotification {
        session_id: SESSION_ID.to_string(),
        os_user: "testuser".to_string(),
        label: "my-feature-branch".to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::ActivityStatus,
        text: "🔔 Session my-feature-branch: Claude Code needs your input.".to_string(),
        at_unix_ms: 1_756_000_000_000,
    }
}

/// A channel that records the presenter events handed to it, in order.
#[derive(Default)]
struct ARecordingChannel {
    presenter_events: Mutex<Vec<(String, ServerMessage)>>,
}

#[async_trait]
impl OperatorChannel for ARecordingChannel {
    fn id(&self) -> &'static str {
        "recording"
    }

    async fn on_presenter_event(
        &self,
        session_id: &str,
        event: &ServerMessage,
    ) -> anyhow::Result<()> {
        self.presenter_events
            .lock()
            .unwrap()
            .push((session_id.to_string(), event.clone()));
        Ok(())
    }

    fn wants_notification(&self, _notification: &SessionNotification) -> bool {
        false
    }

    async fn on_notification(&self, _notification: &SessionNotification) -> anyhow::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The presenter-gate rule
// ---------------------------------------------------------------------------

#[rstest]
#[case::document_review(Variant::DocumentReview(AppModeDocumentReview::default()))]
#[case::markdown_viewer(Variant::MarkdownViewer(AppModeMarkdownViewer::default()))]
#[case::feature_input(Variant::FeatureInput(AppModeFeatureInput::default()))]
#[case::select(Variant::Select(AppModeSelect::default()))]
#[case::multi_select(Variant::MultiSelect(AppModeMultiSelect::default()))]
#[case::text_input(Variant::TextInput(AppModeTextInput::default()))]
fn every_mode_that_asks_a_question_awaits_the_operator(#[case] mode: Variant) {
    // Given
    let mode_changed = a_mode_change_to(mode);

    // When
    let awaits = awaits_operator(&mode_changed);

    // Then
    assert!(awaits, "{mode_changed:?} asks a question");
}

#[rstest]
#[case::running(a_mode_change_to(Variant::Running(AppModeRunning::default())))]
#[case::done(a_mode_change_to(Variant::Done(AppModeDone::default())))]
#[case::no_mode(a_mode_change_with_no_mode())]
fn an_autonomous_mode_does_not_await_the_operator(#[case] mode_changed: ModeChanged) {
    // When
    let awaits = awaits_operator(&mode_changed);

    // Then
    assert!(!awaits, "{mode_changed:?} asks nothing");
}

// ---------------------------------------------------------------------------
// The null object a daemon with no chat surface is wired with
// ---------------------------------------------------------------------------

#[test]
fn no_operator_channel_wants_no_notification() {
    // Given
    let channel = NoOperatorChannel;

    // When
    let wants = channel.wants_notification(&an_attention_notification());

    // Then
    assert!(!wants);
}

#[tokio::test]
async fn no_operator_channel_accepts_every_presenter_event() {
    // Given
    let channel: SharedOperatorChannel = Arc::new(NoOperatorChannel);
    let a_select_prompt =
        a_presenter_event(a_mode_change_to(Variant::Select(AppModeSelect::default())));

    // When
    let outcome = channel.on_presenter_event(SESSION_ID, &a_select_prompt).await;

    // Then — an error would end the session's observer loop, and with it the drawer indicator
    assert!(outcome.is_ok(), "{outcome:?}");
}

// ---------------------------------------------------------------------------
// The port as the session host holds it
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_presenter_event_handed_to_the_shared_channel_reaches_the_channel_behind_it() {
    // Given
    let recorder = Arc::new(ARecordingChannel::default());
    let channel: SharedOperatorChannel = recorder.clone();
    let a_review_gate = a_presenter_event(a_mode_change_to(Variant::DocumentReview(
        AppModeDocumentReview {
            content: "# Plan".to_string(),
        },
    )));

    // When
    channel
        .on_presenter_event(SESSION_ID, &a_review_gate)
        .await
        .expect("the recording channel accepts every event");

    // Then
    assert_eq!(
        *recorder.presenter_events.lock().unwrap(),
        vec![(SESSION_ID.to_string(), a_review_gate)]
    );
}
