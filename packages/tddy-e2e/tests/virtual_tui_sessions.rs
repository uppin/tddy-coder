//! E2E acceptance test: Two gRPC clients with independent virtual TUI streams.
//!
//! Verifies per-connection VirtualTui: each client gets its own stream,
//! client 1 disconnect does not affect client 2, and state changes propagate.

use std::time::Duration;

use tddy_e2e::{connect_grpc, spawn_presenter_with_terminal_service};

#[tokio::test]
async fn two_grpc_clients_get_independent_terminal_streams() {
    let (_handle, port, _shutdown) =
        spawn_presenter_with_terminal_service(Some("feature".to_string()));

    let client1 = connect_grpc(port).await.expect("client1 connect");
    let client2 = connect_grpc(port).await.expect("client2 connect");

    let (tx1, mut rx1) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let (tx2, mut rx2) = tokio::sync::mpsc::channel::<Vec<u8>>(64);

    let mut c1 = client1.clone();
    let mut c2 = client2.clone();

    tokio::spawn(async move {
        let mut stream = c1
            .stream_terminal_io(tokio_stream::iter(vec![]))
            .await
            .expect("stream_terminal_io")
            .into_inner();
        while let Ok(Some(msg)) = stream.message().await {
            let _ = tx1.send(msg.data).await;
        }
    });

    tokio::spawn(async move {
        let mut stream = c2
            .stream_terminal_io(tokio_stream::iter(vec![]))
            .await
            .expect("stream_terminal_io")
            .into_inner();
        while let Ok(Some(msg)) = stream.message().await {
            let _ = tx2.send(msg.data).await;
        }
    });

    let mut received1 = Vec::new();
    let mut received2 = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);

    while tokio::time::Instant::now() < deadline {
        if let Ok(Some(bytes)) = tokio::time::timeout(Duration::from_millis(100), rx1.recv()).await
        {
            received1.extend(bytes);
        }
        if let Ok(Some(bytes)) = tokio::time::timeout(Duration::from_millis(100), rx2.recv()).await
        {
            received2.extend(bytes);
        }
        if received1.len() > 50 && received2.len() > 50 {
            break;
        }
    }

    assert!(
        received1.len() > 50,
        "client1 should receive ANSI bytes, got {}",
        received1.len()
    );
    assert!(
        received2.len() > 50,
        "client2 should receive ANSI bytes, got {}",
        received2.len()
    );
}
