//! gRPC-driven E2E test: full workflow.
//!
//! Uses SKIP_QUESTIONS to avoid blocking on clarification/permission. StubBackend still shows
//! Select for demo (Run/Skip) after green; workflow completes without client answering.

use std::time::Duration;

use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;

use tddy_e2e::{connect_grpc, spawn_presenter_with_grpc};
use tddy_grpc::gen::app_mode_proto;
use tddy_grpc::gen::server_message;
use tddy_grpc::gen::{client_message, AnswerSelect, ClientMessage, SubmitFeatureInput};

#[tokio::test]
async fn full_workflow_without_clarification_completes() {
    let (presenter_handle, port, shutdown) =
        spawn_presenter_with_grpc(Some("SKIP_QUESTIONS simple feature".to_string()));

    let mut client = connect_grpc(port).await.unwrap();

    let (tx, rx) = tokio::sync::mpsc::channel(64);
    tx.send(ClientMessage {
        intent: Some(client_message::Intent::SubmitFeatureInput(
            SubmitFeatureInput {
                text: "SKIP_QUESTIONS simple feature".to_string(),
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
                                            AnswerSelect { index: 1 },
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
