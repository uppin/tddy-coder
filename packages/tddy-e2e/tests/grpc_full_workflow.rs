//! gRPC-driven E2E test: full workflow.
//!
//! full_workflow_with_clarification_completes: Uses "Build auth", answers all Select prompts
//! (Scope, Permission, Demo) with index 0. Verifies workflow completes successfully.

use std::time::Duration;

use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;

use tddy_e2e::{connect_grpc, spawn_presenter_with_grpc};
use tddy_grpc::gen::app_mode_proto;
use tddy_grpc::gen::server_message;
use tddy_grpc::gen::{
    client_message, AnswerSelect, ApprovePlan, ClientMessage, SubmitFeatureInput,
};

#[tokio::test]
async fn full_workflow_with_clarification_completes() {
    let (presenter_handle, port, shutdown) =
        spawn_presenter_with_grpc(Some("Build auth".to_string()));

    let mut client = connect_grpc(port).await.unwrap();

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

    let mut seen_goal_started = false;
    let mut seen_workflow_complete = false;
    let mut workflow_ok = false;

    for _ in 0..500 {
        match tokio::time::timeout(Duration::from_millis(100), stream.message()).await {
            Ok(Ok(Some(msg))) => {
                if let Some(event) = msg.event {
                    match &event {
                        server_message::Event::GoalStarted(_) => seen_goal_started = true,
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

    for _ in 0..200 {
        if seen_workflow_complete {
            break;
        }
        match tokio::time::timeout(Duration::from_millis(100), stream.message()).await {
            Ok(Ok(Some(msg))) => {
                if let Some(server_message::Event::WorkflowComplete(wc)) = msg.event {
                    seen_workflow_complete = true;
                    workflow_ok = wc.ok;
                    break;
                }
            }
            Ok(Ok(None)) => break,
            _ => {}
        }
    }

    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = presenter_handle.join();

    assert!(seen_goal_started, "Expected GoalStarted event");
    assert!(seen_workflow_complete, "Expected WorkflowComplete event");
    assert!(workflow_ok, "Expected workflow to complete successfully");
}

/// Full workflow with StubBackend: plan → acceptance-tests → red → green → demo → evaluate → validate → refactor.
/// Asserts each state transition in order. Answers plan clarification (Scope), acceptance-tests permission (Permission),
/// and demo (Create & run). StubBackend plan includes demo_plan, so demo step runs.
#[tokio::test]
async fn full_workflow_asserts_each_state_transition() {
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
        ("RefactorComplete", "UpdatingDocs"),
        ("UpdatingDocs", "DocsUpdated"),
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
        ("RefactorComplete", "UpdatingDocs"),
        ("UpdatingDocs", "DocsUpdated"),
    ];

    let (presenter_handle, port, shutdown) =
        spawn_presenter_with_grpc(Some("Build auth".to_string()));

    let mut client = connect_grpc(port).await.unwrap();

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

    for _ in 0..1000 {
        match tokio::time::timeout(Duration::from_millis(100), stream.message()).await {
            Ok(Ok(Some(msg))) => {
                if let Some(event) = msg.event {
                    match &event {
                        server_message::Event::StateChanged(sc) => {
                            state_transitions.push((sc.from.clone(), sc.to.clone()));
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

    for _ in 0..300 {
        if seen_workflow_complete {
            break;
        }
        match tokio::time::timeout(Duration::from_millis(100), stream.message()).await {
            Ok(Ok(Some(msg))) => {
                if let Some(server_message::Event::StateChanged(sc)) = msg.event {
                    state_transitions.push((sc.from.clone(), sc.to.clone()));
                } else if let Some(server_message::Event::WorkflowComplete(wc)) = msg.event {
                    seen_workflow_complete = true;
                    workflow_ok = wc.ok;
                    break;
                }
            }
            Ok(Ok(None)) => break,
            _ => {}
        }
    }

    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = presenter_handle.join();

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
        assert_eq!(
            from,
            exp_from,
            "Transition {}: expected from='{}', got from='{}'",
            i + 1,
            exp_from,
            from
        );
        assert_eq!(
            to,
            exp_to,
            "Transition {}: expected to='{}', got to='{}'",
            i + 1,
            exp_to,
            to
        );
    }
}
