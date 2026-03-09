//! gRPC-driven E2E test: clarification question flow.
//!
//! Submit feature with CLARIFY keyword → wait for Select mode → answer → WorkflowComplete.

use std::time::Duration;

use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;

use tddy_e2e::{connect_grpc, spawn_presenter_with_grpc};
use tddy_grpc::gen::app_mode_proto;
use tddy_grpc::gen::server_message;
use tddy_grpc::gen::{client_message, AnswerSelect, ClientMessage, QueuePrompt};

#[tokio::test]
async fn clarification_flow_submit_answer_select_workflow_completes() {
    // Use initial_prompt so workflow starts immediately (matches presenter_integration).
    // SubmitFeatureInput path is covered by grpc_full_workflow.
    let (presenter_handle, port, shutdown) =
        spawn_presenter_with_grpc(Some("CLARIFY test feature".to_string()));

    let mut client = connect_grpc(port).await.unwrap();

    let (tx, rx) = tokio::sync::mpsc::channel(64);
    // Send benign intent to establish stream; workflow already has input from initial_prompt
    tx.send(ClientMessage {
        intent: Some(client_message::Intent::QueuePrompt(QueuePrompt {
            text: String::new(),
        })),
    })
    .await
    .unwrap();

    let request_stream = ReceiverStream::new(rx);
    let mut stream = client
        .stream(Request::new(request_stream))
        .await
        .unwrap()
        .into_inner();

    let mut seen_select_mode = false;
    let mut seen_workflow_complete = false;
    let mut workflow_ok = false;
    let mut workflow_message = String::new();

    let mut event_count = 0u32;
    for _ in 0..500 {
        match tokio::time::timeout(Duration::from_millis(100), stream.message()).await {
            Ok(Ok(Some(msg))) => {
                if let Some(event) = msg.event {
                    event_count += 1;
                    match &event {
                        server_message::Event::ModeChanged(mc) => {
                            if let Some(mode) = &mc.mode {
                                if let Some(app_mode_proto::Variant::Select(select)) = &mode.variant
                                {
                                    seen_select_mode = true;
                                    let q = select.question.as_ref().unwrap();
                                    // StubBackend: plan clarification (Scope), acceptance-tests permission (Permission), or demo (Demo)
                                    assert!(
                                        q.header == "Scope"
                                            || q.header == "Permission"
                                            || q.header == "Demo",
                                        "expected Scope, Permission, or Demo, got {}",
                                        q.header
                                    );

                                    tx.send(ClientMessage {
                                        intent: Some(client_message::Intent::AnswerSelect(
                                            AnswerSelect { index: 0 },
                                        )),
                                    })
                                    .await
                                    .unwrap();
                                    tokio::time::sleep(Duration::from_millis(1000)).await;
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

    assert!(
        seen_select_mode,
        "Did not see Select mode - clarification question was not shown"
    );
    assert!(seen_workflow_complete, "Expected WorkflowComplete event");
    assert!(
        workflow_ok,
        "Expected workflow to complete successfully, got: {} (events: {})",
        workflow_message, event_count
    );
}
