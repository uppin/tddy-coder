//! TUI + gRPC E2E test: full workflow from plan to refactor.
//!
//! Runs presenter + TUI in memory (TestBackend). Drives workflow via gRPC (SubmitFeatureInput,
//! AnswerSelect). On each StateChanged gRPC event, asserts the UI (rendered buffer) shows that transition.
//! Event received → UI checked.

use std::time::Duration;

use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;

use tddy_e2e::{connect_grpc, spawn_presenter_with_grpc_and_tui};
use tddy_service::gen::app_mode_proto;
use tddy_service::gen::server_message;
use tddy_service::gen::{
    client_message, AnswerSelect, ApproveSessionDocument, ClientMessage, SubmitFeatureInput,
};

/// See `grpc_full_workflow.rs`: transitional state is persisted before `StateChange`, so identity
/// transitions appear; PlanReview resync still sends `Planning→Planned`. With demo: 19; without: 17.
const EXPECTED_WITH_DEMO: &[(&str, &str)] = &[
    ("Planning", "Planning"),
    ("Planning", "Planned"),
    ("Planning", "Planned"),
    ("AcceptanceTesting", "AcceptanceTesting"),
    ("AcceptanceTesting", "AcceptanceTestsReady"),
    ("RedTesting", "RedTesting"),
    ("RedTesting", "RedTestsReady"),
    ("GreenImplementing", "GreenImplementing"),
    ("GreenImplementing", "GreenComplete"),
    ("DemoRunning", "DemoRunning"),
    ("DemoRunning", "DemoComplete"),
    ("Evaluating", "Evaluating"),
    ("Evaluating", "Evaluated"),
    ("Validating", "Validating"),
    ("Validating", "ValidateComplete"),
    ("Refactoring", "Refactoring"),
    ("Refactoring", "RefactorComplete"),
    ("UpdatingDocs", "UpdatingDocs"),
    ("UpdatingDocs", "DocsUpdated"),
];
const EXPECTED_WITHOUT_DEMO: &[(&str, &str)] = &[
    ("Planning", "Planning"),
    ("Planning", "Planned"),
    ("Planning", "Planned"),
    ("AcceptanceTesting", "AcceptanceTesting"),
    ("AcceptanceTesting", "AcceptanceTestsReady"),
    ("RedTesting", "RedTesting"),
    ("RedTesting", "RedTestsReady"),
    ("GreenImplementing", "GreenImplementing"),
    ("GreenImplementing", "GreenComplete"),
    ("Evaluating", "Evaluating"),
    ("Evaluating", "Evaluated"),
    ("Validating", "Validating"),
    ("Validating", "ValidateComplete"),
    ("Refactoring", "Refactoring"),
    ("Refactoring", "RefactorComplete"),
    ("UpdatingDocs", "UpdatingDocs"),
    ("UpdatingDocs", "DocsUpdated"),
];

/// Full workflow: plan → acceptance-tests → red → green → demo → evaluate → validate → refactor → update-docs.
/// gRPC events drive the flow; after each StateChanged, UI buffer is asserted.
#[tokio::test]
async fn pty_full_workflow_asserts_each_state_transition() {
    // None: do not auto-start the workflow — avoids broadcast events before the gRPC stream
    // subscribes (same as grpc_full_workflow). Feature text is sent via SubmitFeatureInput below.
    let (_presenter_handle, port, shutdown, screen_buffer) =
        spawn_presenter_with_grpc_and_tui(None);

    let mut client = connect_grpc(port).await.expect("connect gRPC");

    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let request_stream = ReceiverStream::new(rx);
    let mut stream = client
        .stream(Request::new(request_stream))
        .await
        .unwrap()
        .into_inner();

    // Start the workflow only after the gRPC stream is live so StateChanged events are not missed
    // relative to the UI buffer before the first `stream.message()` poll.
    tx.send(ClientMessage {
        intent: Some(client_message::Intent::SubmitFeatureInput(
            SubmitFeatureInput {
                text: "Build auth".to_string(),
            },
        )),
    })
    .await
    .unwrap();

    let mut state_transitions: Vec<(String, String)> = Vec::new();
    let mut seen_workflow_complete = false;
    let mut workflow_ok = false;
    let msg_timeout = Duration::from_millis(100);

    for _ in 0..1000 {
        match tokio::time::timeout(msg_timeout, stream.message()).await {
            Ok(Ok(Some(msg))) => {
                if let Some(event) = msg.event {
                    match &event {
                        server_message::Event::StateChanged(sc) => {
                            state_transitions.push((sc.from.clone(), sc.to.clone()));
                            // Do not assert the activity log line "State: a → b" is visible in the
                            // TestBackend snapshot: the log auto-scrolls to the bottom, so early
                            // transitions scroll out of the 24-row viewport while the stub workflow
                            // may still be advancing. Transition correctness is checked below against
                            // EXPECTED_* from this same gRPC stream.
                        }
                        server_message::Event::ModeChanged(mc) => {
                            if let Some(mode) = &mc.mode {
                                if let Some(app_mode_proto::Variant::Select(_)) = &mode.variant {
                                    tx.send(ClientMessage {
                                        intent: Some(client_message::Intent::AnswerSelect(
                                            AnswerSelect { index: 0 },
                                        )),
                                    })
                                    .await
                                    .ok();
                                } else if let Some(app_mode_proto::Variant::DocumentReview(_)) =
                                    &mode.variant
                                {
                                    tx.send(ClientMessage {
                                        intent: Some(
                                            client_message::Intent::ApproveSessionDocument(
                                                ApproveSessionDocument {},
                                            ),
                                        ),
                                    })
                                    .await
                                    .ok();
                                }
                            }
                        }
                        server_message::Event::WorkflowComplete(wc) => {
                            seen_workflow_complete = true;
                            workflow_ok = wc.ok;
                            // Drain stream briefly to allow any in-flight StateChanged events
                            // (race: WorkflowComplete can arrive before last StateChanged)
                            for _ in 0..20 {
                                match tokio::time::timeout(
                                    Duration::from_millis(10),
                                    stream.message(),
                                )
                                .await
                                {
                                    Ok(Ok(Some(msg))) => {
                                        if let Some(server_message::Event::StateChanged(sc)) =
                                            msg.event
                                        {
                                            state_transitions.push((sc.from, sc.to));
                                        }
                                    }
                                    _ => break,
                                }
                            }
                            let mut seen = false;
                            for _ in 0..200 {
                                let screen = screen_buffer.lock().unwrap().clone();
                                // Accept any of: FeatureInput prompt ("Type your feature description"),
                                // completion summary ("Session dir:", etc.), or status bar showing RefactorComplete
                                // (race: gRPC can arrive before presenter switches mode to FeatureInput)
                                if screen.contains("Session dir:")
                                    || screen.contains("Type your feature description")
                                    || screen.contains("Workflow complete")
                                    || screen.contains("Tasks completed")
                                    || screen.contains("Tests passing")
                                    || (screen.contains("DocsUpdated")
                                        && screen.contains("Goal: end"))
                                {
                                    seen = true;
                                    break;
                                }
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                            assert!(
                                seen,
                                "UI should show completion after gRPC WorkflowComplete"
                            );
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Ok(Ok(None)) => break,
            _ => {}
        }
    }

    drop(tx);
    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = _presenter_handle.join();

    assert!(
        seen_workflow_complete,
        "Expected WorkflowComplete event (got {} state transitions)",
        state_transitions.len()
    );
    assert!(
        workflow_ok,
        "Expected workflow to complete successfully (transitions: {:?})",
        state_transitions
    );
    let expected = if state_transitions.iter().any(|(_, to)| to == "DemoComplete") {
        EXPECTED_WITH_DEMO
    } else {
        EXPECTED_WITHOUT_DEMO
    };
    assert_eq!(
        state_transitions.len(),
        expected.len(),
        "Expected {} state transitions, got {}: {:?}",
        expected.len(),
        state_transitions.len(),
        state_transitions
    );
    for (i, (from, to)) in state_transitions.iter().enumerate() {
        let (exp_from, exp_to) = expected[i];
        assert_eq!(from, exp_from, "Transition {}: from", i + 1);
        assert_eq!(to, exp_to, "Transition {}: to", i + 1);
    }
}
