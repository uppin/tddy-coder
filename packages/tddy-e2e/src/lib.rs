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

#[cfg(feature = "livekit")]
use tddy_core::ViewConnection;
use tddy_core::{AnyBackend, Presenter, PresenterHandle, SharedBackend, StubBackend};
use tddy_service::gen::tddy_remote_server::TddyRemoteServer;
use tddy_service::TddyRemoteService;
use tddy_tui::{apply_event, render::draw, TuiView};
use tddy_workflow_recipes::TddRecipe;

use crate::test_util::temp_dir_with_git_repo;

pub mod rpc_frontend;
pub mod test_util;

/// `connect_view` callback used by VirtualTui and terminal services.
pub type ViewConnectionFactory = Arc<dyn Fn() -> Option<tddy_core::ViewConnection> + Send + Sync>;

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

    let mut presenter = Presenter::new("stub", "opus", Arc::new(TddRecipe))
        .with_broadcast(event_tx)
        .with_intent_sender(intent_tx);
    let backend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
    let output_dir = temp_dir_with_git_repo("grpc");
    presenter.start_workflow(
        backend,
        output_dir,
        None,
        initial_prompt,
        None,
        None,
        false,
        None,
        None,
        None,
    );

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
    tddy_service::gen::tddy_remote_client::TddyRemoteClient<tonic::transport::Channel>,
    tonic::transport::Error,
> {
    tddy_service::gen::tddy_remote_client::TddyRemoteClient::connect(format!(
        "http://[::1]:{}",
        port
    ))
    .await
}

/// Connect a tonic terminal gRPC client to the given port.
pub async fn connect_terminal_grpc(
    port: u16,
) -> Result<
    tddy_service::tonic_terminal::terminal_service_client::TerminalServiceClient<
        tonic::transport::Channel,
    >,
    tonic::transport::Error,
> {
    tddy_service::tonic_terminal::terminal_service_client::TerminalServiceClient::connect(format!(
        "http://[::1]:{}",
        port
    ))
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

    let mut presenter = Presenter::new("stub", "opus", Arc::new(TddRecipe))
        .with_broadcast(event_tx.clone())
        .with_intent_sender(intent_tx.clone());
    let backend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
    let output_dir = temp_dir_with_git_repo("grpc-tui");
    presenter.start_workflow(
        backend,
        output_dir,
        None,
        initial_prompt,
        None,
        None,
        false,
        None,
        None,
        None,
    );

    let conn = presenter.connect_view().expect("connect_view");
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    let screen_buffer = Arc::new(Mutex::new(String::new()));
    let screen_buffer_clone = screen_buffer.clone();

    let _tui_handle = thread::spawn(move || {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut state = conn.state_snapshot;
        let mut view = TuiView::new();
        let mut event_rx = conn.event_rx;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        for _ in 0..1000 {
            if shutdown_clone.load(Ordering::Relaxed) {
                break;
            }
            while let Ok(ev) = event_rx.try_recv() {
                apply_event(&mut state, &mut view, ev);
            }
            terminal
                .draw(|f| draw(f, &state, view.view_state_mut(), false, None))
                .unwrap();
            if let Ok(mut buf) = screen_buffer_clone.lock() {
                *buf = format!("{}", terminal.backend());
            }
            thread::sleep(Duration::from_millis(10));
        }
    });

    let shutdown_for_presenter = shutdown.clone();
    let presenter_handle = thread::spawn(move || {
        let mut p = presenter;
        for _ in 0..1000 {
            if shutdown_for_presenter.load(Ordering::Relaxed) {
                break;
            }
            while let Ok(intent) = intent_rx.try_recv() {
                p.handle_intent(intent);
            }
            p.poll_tool_calls();
            p.poll_workflow();
            if p.state().should_quit {
                break;
            }
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

/// Spawn Presenter with view connection factory. Returns (presenter_handle, factory, shutdown).
/// Use the factory to create TerminalServiceVirtualTui for LiveKit or tonic terminal server for gRPC.
#[cfg(feature = "livekit")]
pub fn spawn_presenter_with_view_connection_factory(
    initial_prompt: Option<String>,
) -> (
    thread::JoinHandle<()>,
    Arc<dyn Fn() -> Option<ViewConnection> + Send + Sync>,
    Arc<AtomicBool>,
) {
    let (event_tx, _) = broadcast::channel(256);
    let (intent_tx, intent_rx) = mpsc::channel();
    let output_dir =
        std::env::temp_dir().join(format!("tddy-e2e-livekit-tui-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&output_dir).unwrap();
    let mut presenter = Presenter::new("stub", "opus", Arc::new(TddRecipe))
        .with_broadcast(event_tx.clone())
        .with_intent_sender(intent_tx.clone())
        .with_worktree_dir(output_dir.clone());
    let backend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
    presenter.start_workflow(
        backend,
        output_dir,
        None,
        initial_prompt,
        None,
        None,
        false,
        None,
        None,
        None,
    );

    let presenter = Arc::new(Mutex::new(presenter));
    let presenter_for_factory = presenter.clone();
    let factory: Arc<dyn Fn() -> Option<ViewConnection> + Send + Sync> = Arc::new(move || {
        presenter_for_factory
            .lock()
            .ok()
            .and_then(|p| p.connect_view())
    });

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    let presenter_for_thread = presenter.clone();
    let presenter_handle = thread::spawn(move || {
        for _ in 0..1000 {
            if shutdown_clone.load(Ordering::Relaxed) {
                break;
            }
            while let Ok(intent) = intent_rx.try_recv() {
                if let Ok(mut p) = presenter_for_thread.lock() {
                    p.handle_intent(intent);
                }
            }
            if let Ok(mut p) = presenter_for_thread.lock() {
                p.poll_workflow();
            }
            thread::sleep(Duration::from_millis(10));
        }
    });

    (presenter_handle, factory, shutdown)
}

/// Stub Presenter workflow with a `connect_view` factory. No gRPC server.
/// Use with [`tddy_service::start_virtual_tui_session`] for in-process VirtualTui tests
/// that bypass tonic RPC.
pub fn spawn_presenter_with_view_factory(
    initial_prompt: Option<String>,
) -> (
    thread::JoinHandle<()>,
    ViewConnectionFactory,
    Arc<AtomicBool>,
) {
    let (presenter_handle, factory, shutdown, _remote) =
        spawn_presenter_stub_workflow(initial_prompt);
    (presenter_handle, factory, shutdown)
}

fn spawn_presenter_stub_workflow(
    initial_prompt: Option<String>,
) -> (
    thread::JoinHandle<()>,
    ViewConnectionFactory,
    Arc<AtomicBool>,
    PresenterHandle,
) {
    let (event_tx, _) = broadcast::channel(256);
    let (intent_tx, intent_rx) = mpsc::channel();
    let remote = PresenterHandle {
        event_tx: event_tx.clone(),
        intent_tx: intent_tx.clone(),
    };

    let presenter = Presenter::new("stub", "opus", Arc::new(TddRecipe))
        .with_broadcast(event_tx)
        .with_intent_sender(intent_tx);
    let output_dir = std::env::temp_dir().join(format!("tddy-e2e-vt-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&output_dir).unwrap();
    let backend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
    let mut presenter = presenter.with_worktree_dir(output_dir.clone());
    presenter.start_workflow(
        backend,
        output_dir,
        None,
        initial_prompt,
        None,
        None,
        false,
        None,
        None,
        None,
    );

    let presenter = Arc::new(Mutex::new(presenter));
    let presenter_for_factory = presenter.clone();
    let factory: ViewConnectionFactory = Arc::new(move || {
        presenter_for_factory
            .lock()
            .ok()
            .and_then(|p| p.connect_view())
    });

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    let presenter_for_thread = presenter.clone();
    let presenter_handle = thread::spawn(move || {
        for _ in 0..1000 {
            if shutdown_clone.load(Ordering::Relaxed) {
                break;
            }
            while let Ok(intent) = intent_rx.try_recv() {
                log::trace!("presenter: received intent={:?}", intent);
                if let Ok(mut p) = presenter_for_thread.lock() {
                    p.handle_intent(intent);
                }
            }
            if let Ok(mut p) = presenter_for_thread.lock() {
                p.poll_workflow();
            }
            thread::sleep(Duration::from_millis(10));
        }
    });

    (presenter_handle, factory, shutdown, remote)
}

/// Spawn Presenter with per-connection VirtualTui (stream_terminal_io creates a VirtualTui per client).
/// Serves both TddyRemoteServer and tonic TerminalServiceServer on the same gRPC port.
/// Returns (presenter_handle, port, shutdown).
pub fn spawn_presenter_with_terminal_service(
    initial_prompt: Option<String>,
) -> (thread::JoinHandle<()>, u16, Arc<AtomicBool>) {
    let (presenter_handle, factory, shutdown, handle) =
        spawn_presenter_stub_workflow(initial_prompt);

    let terminal_svc = tddy_service::TerminalServiceVirtualTui::new(factory, false);
    let addr: std::net::SocketAddr = "[::1]:0".parse().unwrap();
    let (port_tx, port_rx) = std::sync::mpsc::channel();

    let _server_handle = thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let listener = rt.block_on(tokio::net::TcpListener::bind(addr)).unwrap();
        let port = listener.local_addr().unwrap().port();
        port_tx.send(port).unwrap();
        rt.block_on(async {
            Server::builder()
                .add_service(TddyRemoteServer::new(TddyRemoteService::new(handle)))
                .add_service(
                    tddy_service::tonic_terminal::terminal_service_server::TerminalServiceServer::new(terminal_svc),
                )
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
        })
    });

    let port = port_rx.recv().unwrap();

    (presenter_handle, port, shutdown)
}
