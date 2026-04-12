//! gRPC-driven E2E test: full workflow.
//!
//! full_workflow_with_clarification_completes: Uses "Build auth", answers interview Select +
//! MultiSelect, then all later Select prompts (Scope, Permission, Demo) with index 0. Verifies
//! workflow completes successfully.

use std::time::Duration;

use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;

use tddy_e2e::{connect_grpc, spawn_presenter_with_grpc};
use tddy_service::gen::app_mode_proto;
use tddy_service::gen::server_message;
use tddy_service::gen::{
    client_message, AnswerMultiSelect, AnswerSelect, ApproveSessionDocument, ClientMessage,
    SubmitFeatureInput,
};

#[tokio::test]
async fn full_workflow_with_clarification_completes() {
    // Use None so workflow waits for SubmitFeatureInput; avoids race where events are
    // broadcast before the test connects and subscribes.
    let (presenter_handle, port, shutdown) = spawn_presenter_with_grpc(None);

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
    let mut workflow_message = String::new();

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
                                            AnswerSelect {
                                                index: 0,
                                                clarification_question_index: None,
                                            },
                                        )),
                                    })
                                    .await
                                    .ok();
                                } else if let Some(app_mode_proto::Variant::MultiSelect(_)) =
                                    &mode.variant
                                {
                                    tx.send(ClientMessage {
                                        intent: Some(client_message::Intent::AnswerMultiSelect(
                                            AnswerMultiSelect {
                                                indices: vec![0],
                                                other: String::new(),
                                            },
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
                            workflow_message = wc.message.clone();
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
                    workflow_message = wc.message.clone();
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
    assert!(
        workflow_ok,
        "Expected workflow to complete successfully, got error: {}",
        workflow_message
    );

    // Acceptance: gRPC clients see FeatureInput after completion (not Done)
    let mut seen_feature_input_after_complete = false;
    for _ in 0..50 {
        match tokio::time::timeout(Duration::from_millis(100), stream.message()).await {
            Ok(Ok(Some(msg))) => {
                if let Some(server_message::Event::ModeChanged(mc)) = msg.event {
                    if let Some(mode) = &mc.mode {
                        if matches!(mode.variant, Some(app_mode_proto::Variant::FeatureInput(_))) {
                            seen_feature_input_after_complete = true;
                            break;
                        }
                    }
                }
            }
            Ok(Ok(None)) => break,
            _ => {}
        }
    }
    assert!(
        seen_feature_input_after_complete,
        "gRPC clients should receive ModeChanged(FeatureInput) after WorkflowComplete"
    );
}

/// Full workflow with StubBackend: interview → plan → acceptance-tests → red → green → demo → evaluate → validate → refactor.
/// Asserts each state transition in order. Answers interview (Feature scope + Constraints), plan clarification (Scope),
/// acceptance-tests permission (Permission), and demo (Create & run). StubBackend plan includes demo_plan, so demo step runs.
#[tokio::test]
async fn full_workflow_asserts_each_state_transition() {
    /// `TddWorkflowHooks::before_task` persists the transitional state in `changeset.yaml` before
    /// emitting `WorkflowEvent::StateChange`, so the UI often sees identity transitions
    /// (`Planning→Planning`, `AcceptanceTesting→AcceptanceTesting`, …). PlanReview resync still sends
    /// `Planning→Planned`. Captured from StubBackend full graph (interview → plan handoff skips extra plan Select).
    const EXPECTED_WITH_DEMO: &[(&str, &str)] = &[
        ("Interviewing", "Interviewing"),
        ("Interviewing", "Interviewing"),
        ("Interviewing", "Interviewed"),
        ("Planning", "Planned"),
        ("Planning", "Planned"),
        ("Interviewing", "Interviewing"),
        ("Interviewing", "Interviewing"),
        ("Interviewing", "Interviewed"),
        ("Planning", "Interviewed"),
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
        ("Interviewing", "Interviewing"),
        ("Interviewing", "Interviewing"),
        ("Interviewing", "Interviewed"),
        ("Planning", "Planned"),
        ("Planning", "Planned"),
        ("Interviewing", "Interviewing"),
        ("Interviewing", "Interviewing"),
        ("Interviewing", "Interviewed"),
        ("Planning", "Interviewed"),
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
                                            AnswerSelect {
                                                index: 0,
                                                clarification_question_index: None,
                                            },
                                        )),
                                    })
                                    .await
                                    .ok();
                                } else if let Some(app_mode_proto::Variant::MultiSelect(_)) =
                                    &mode.variant
                                {
                                    tx.send(ClientMessage {
                                        intent: Some(client_message::Intent::AnswerMultiSelect(
                                            AnswerMultiSelect {
                                                indices: vec![0],
                                                other: String::new(),
                                            },
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
