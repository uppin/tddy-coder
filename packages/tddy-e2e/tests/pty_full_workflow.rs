//! TUI + gRPC E2E test: full workflow from plan to refactor.
//!
//! Runs presenter + TUI in memory (TestBackend). Drives workflow via gRPC (SubmitFeatureInput,
//! AnswerSelect). On each StateChanged gRPC event, asserts the UI (rendered buffer) shows that transition.
//! Event received → UI checked.

use std::time::Duration;

use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;

use tddy_e2e::{connect_grpc, spawn_presenter_with_grpc_and_tui};
use tddy_grpc::gen::app_mode_proto;
use tddy_grpc::gen::server_message;
use tddy_grpc::gen::{
    client_message, AnswerSelect, ApprovePlan, ClientMessage, SubmitFeatureInput,
};

/// With demo: 18 transitions. Without demo: 16. Plan approval adds (Planned→Planning→Planned).
const EXPECTED_WITH_DEMO: &[(&str, &str)] = &[
    ("Init", "Planning"),
    ("Planning", "Planned"),
    ("Planned", "Planning"),
    ("Planning", "Planned"),
    ("Planned", "AcceptanceTesting"),
    ("AcceptanceTesting", "AcceptanceTestsReady"),
    ("AcceptanceTestsReady", "RedTesting"),
    ("RedTesting", "RedTestsReady"),
    ("RedTestsReady", "GreenImplementing"),
    ("GreenImplementing", "GreenComplete"),
    ("GreenComplete", "DemoRunning"),
    ("DemoRunning", "DemoComplete"),
    ("DemoComplete", "Evaluating"),
    ("Evaluating", "Evaluated"),
    ("Evaluated", "Validating"),
    ("Validating", "ValidateComplete"),
    ("ValidateComplete", "Refactoring"),
    ("Refactoring", "RefactorComplete"),
];
const EXPECTED_WITHOUT_DEMO: &[(&str, &str)] = &[
    ("Init", "Planning"),
    ("Planning", "Planned"),
    ("Planned", "Planning"),
    ("Planning", "Planned"),
    ("Planned", "AcceptanceTesting"),
    ("AcceptanceTesting", "AcceptanceTestsReady"),
    ("AcceptanceTestsReady", "RedTesting"),
    ("RedTesting", "RedTestsReady"),
    ("RedTestsReady", "GreenImplementing"),
    ("GreenImplementing", "GreenComplete"),
    ("GreenComplete", "Evaluating"),
    ("Evaluating", "Evaluated"),
    ("Evaluated", "Validating"),
    ("Validating", "ValidateComplete"),
    ("ValidateComplete", "Refactoring"),
    ("Refactoring", "RefactorComplete"),
];

/// Full workflow: plan → acceptance-tests → red → green → demo → evaluate → validate → refactor.
/// gRPC events drive the flow; after each StateChanged, UI buffer is asserted.
#[tokio::test]
async fn pty_full_workflow_asserts_each_state_transition() {
    let (_presenter_handle, port, shutdown, screen_buffer) =
        spawn_presenter_with_grpc_and_tui(Some("Build auth".to_string()));

    let mut client = connect_grpc(port).await.expect("connect gRPC");

    let (tx, rx) = tokio::sync::mpsc::channel(64);
    tx.send(ClientMessage {
        intent: Some(client_message::Intent::SubmitFeatureInput(
            SubmitFeatureInput {
                text: "Build auth".to_string(),
            },
        )),
    })
    .await
    .unwrap();

    let request_stream = ReceiverStream::new(rx);
    let mut stream = client
        .stream(Request::new(request_stream))
        .await
        .unwrap()
        .into_inner();

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
                            let expected = format!("State: {} → {}", sc.from, sc.to);
                            let mut seen = false;
                            for _ in 0..50 {
                                let screen = screen_buffer.lock().unwrap().clone();
                                if screen.contains(&expected) {
                                    seen = true;
                                    break;
                                }
                                tokio::time::sleep(Duration::from_millis(5)).await;
                            }
                            assert!(
                                seen,
                                "UI should show '{}' after gRPC StateChanged",
                                expected
                            );
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
                                } else if let Some(app_mode_proto::Variant::PlanReview(_)) =
                                    &mode.variant
                                {
                                    tx.send(ClientMessage {
                                        intent: Some(client_message::Intent::ApprovePlan(
                                            ApprovePlan {},
                                        )),
                                    })
                                    .await
                                    .ok();
                                }
                            }
                        }
                        server_message::Event::WorkflowComplete(wc) => {
                            seen_workflow_complete = true;
                            workflow_ok = wc.ok;
                            let mut seen = false;
                            for _ in 0..50 {
                                let screen = screen_buffer.lock().unwrap().clone();
                                if screen.contains("Plan dir:")
                                    || screen.contains("Workflow complete")
                                {
                                    seen = true;
                                    break;
                                }
                                tokio::time::sleep(Duration::from_millis(5)).await;
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
