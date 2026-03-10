//! E2E test helpers for tddy-coder TUI.
//!
//! Provides utilities for gRPC-driven and PTY-based end-to-end testing.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tokio::sync::broadcast;
use tonic::transport::Server;

use tddy_core::{AnyBackend, Presenter, PresenterHandle, SharedBackend, StubBackend};
use tddy_grpc::gen::tddy_remote_server::TddyRemoteServer;
use tddy_grpc::TddyRemoteService;
use tddy_tui::{render::draw, TuiView};

use crate::test_util::NoopView;

pub mod test_util;

/// Spawn a Presenter with StubBackend and gRPC server. Returns (join_handle, port, shutdown_flag).
/// The presenter waits for initial feature input (pass None for initial_prompt).
pub fn spawn_presenter_with_grpc(
    initial_prompt: Option<String>,
) -> (thread::JoinHandle<()>, u16, std::sync::Arc<AtomicBool>) {
    let (event_tx, _) = broadcast::channel(256);
    let (intent_tx, intent_rx) = mpsc::channel();
    let handle = PresenterHandle {
        event_tx: event_tx.clone(),
        intent_tx: intent_tx.clone(),
    };

    let view = NoopView;
    let mut presenter = Presenter::new(view, "stub", "opus").with_broadcast(event_tx);
    let backend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
    let output_dir = std::env::temp_dir().join(format!("tddy-e2e-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&output_dir).unwrap();
    presenter.start_workflow(backend, output_dir, initial_prompt, None, None, false, None);

    let shutdown = std::sync::Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    let presenter_handle = thread::spawn(move || {
        for _ in 0..1000 {
            if shutdown_clone.load(Ordering::Relaxed) {
                break;
            }
            while let Ok(intent) = intent_rx.try_recv() {
                presenter.handle_intent(intent);
            }
            presenter.poll_workflow();
            thread::sleep(Duration::from_millis(10));
        }
    });

    let service = TddyRemoteService::new(handle);
    let addr: std::net::SocketAddr = "[::1]:0".parse().unwrap();
    let (port_tx, port_rx) = std::sync::mpsc::channel();

    let _server_handle = thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let listener = rt.block_on(tokio::net::TcpListener::bind(addr)).unwrap();
        let port = listener.local_addr().unwrap().port();
        port_tx.send(port).unwrap();
        rt.block_on(async {
            Server::builder()
                .add_service(TddyRemoteServer::new(service))
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
        })
    });

    let port = port_rx.recv().unwrap();

    (presenter_handle, port, shutdown)
}

/// Connect a gRPC client to the given port.
pub async fn connect_grpc(
    port: u16,
) -> Result<
    tddy_grpc::gen::tddy_remote_client::TddyRemoteClient<tonic::transport::Channel>,
    tonic::transport::Error,
> {
    tddy_grpc::gen::tddy_remote_client::TddyRemoteClient::connect(format!("http://[::1]:{}", port))
        .await
}

/// Spawn Presenter with TuiView, gRPC server, and TestBackend. Runs in memory (no binary).
/// Returns (join_handle, port, shutdown, screen_buffer) where screen_buffer is the rendered TUI.
pub fn spawn_presenter_with_grpc_and_tui(
    initial_prompt: Option<String>,
) -> (
    thread::JoinHandle<()>,
    u16,
    Arc<AtomicBool>,
    Arc<Mutex<String>>,
) {
    let (event_tx, _) = broadcast::channel(256);
    let (intent_tx, intent_rx) = mpsc::channel();
    let handle = PresenterHandle {
        event_tx: event_tx.clone(),
        intent_tx: intent_tx.clone(),
    };

    let view = TuiView::new();
    let mut presenter = Presenter::new(view, "stub", "opus").with_broadcast(event_tx);
    let backend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
    let output_dir =
        std::env::temp_dir().join(format!("tddy-e2e-tui-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&output_dir).unwrap();
    presenter.start_workflow(backend, output_dir, initial_prompt, None, None, false, None);

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    let screen_buffer = Arc::new(Mutex::new(String::new()));
    let screen_buffer_clone = screen_buffer.clone();

    let presenter_handle = thread::spawn(move || {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        for _ in 0..1000 {
            if shutdown_clone.load(Ordering::Relaxed) {
                break;
            }
            while let Ok(intent) = intent_rx.try_recv() {
                presenter.handle_intent(intent);
            }
            let state = presenter.state();
            let view = presenter.view();
            terminal
                .draw(|f| draw(f, state, view.view_state(), false))
                .unwrap();
            if let Ok(mut buf) = screen_buffer_clone.lock() {
                *buf = format!("{}", terminal.backend());
            }
            presenter.poll_workflow();
            thread::sleep(Duration::from_millis(10));
        }
    });

    let service = TddyRemoteService::new(handle);
    let addr: std::net::SocketAddr = "[::1]:0".parse().unwrap();
    let (port_tx, port_rx) = std::sync::mpsc::channel();

    let _server_handle = thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let listener = rt.block_on(tokio::net::TcpListener::bind(addr)).unwrap();
        let port = listener.local_addr().unwrap().port();
        port_tx.send(port).unwrap();
        rt.block_on(async {
            Server::builder()
                .add_service(TddyRemoteServer::new(service))
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
        })
    });

    let port = port_rx.recv().unwrap();

    (presenter_handle, port, shutdown, screen_buffer)
}
