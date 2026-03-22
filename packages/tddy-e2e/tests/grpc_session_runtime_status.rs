//! Acceptance: `ServerMessage` stream emits `SessionRuntimeStatus` after workflow transitions.

use std::time::Duration;

use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;

use tddy_e2e::{connect_grpc, spawn_presenter_with_grpc};
use tddy_service::gen::app_mode_proto;
use tddy_service::gen::server_message;
use tddy_service::gen::{
    client_message, AnswerSelect, ApprovePlan, ClientMessage, SubmitFeatureInput,
};

#[tokio::test]
async fn server_message_emits_runtime_status_on_state_change() {
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

    let mut runtime_status_events = Vec::new();
    let mut state_change_count = 0u32;

    for _ in 0..600 {
        match tokio::time::timeout(Duration::from_millis(50), stream.message()).await {
            Ok(Ok(Some(msg))) => {
                if let Some(event) = msg.event {
                    match &event {
                        server_message::Event::StateChanged(_) => {
                            state_change_count += 1;
                        }
                        server_message::Event::SessionRuntimeStatus(s) => {
                            runtime_status_events.push(s.clone());
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
                        server_message::Event::WorkflowComplete(_) => {
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
    let _ = presenter_handle.join();

    assert!(
        state_change_count >= 1,
        "expected at least one StateChanged (workflow moved); got {}",
        state_change_count
    );
    assert!(
        runtime_status_events.len() >= 2,
        "expected at least two SessionRuntimeStatus events (initial snapshot + after a transition); got {} \
         — wire presenter/daemon emission and periodic tick",
        runtime_status_events.len()
    );
    assert!(
        runtime_status_events.iter().any(|s| {
            !s.session_id.is_empty()
                && (s.goal.contains("Build auth") || s.goal.contains("plan"))
        }),
        "expected SessionRuntimeStatus with non-empty session_id and goal related to the feature; got {:?}",
        runtime_status_events
    );
}
