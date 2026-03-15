//! RPC Demo: VirtualTui backend → real terminal frontend.
//!
//! Modes:
//!   - In-process (default): direct channel between VirtualTui and terminal frontend
//!   - LiveKit: VirtualTui served via LiveKit RPC, client connects over WebRTC data channel
//!
//! Usage:
//!   cargo run -p tddy-e2e --example rpc_demo
//!   cargo run -p tddy-e2e --example rpc_demo --features livekit -- \
//!     --livekit-url ws://127.0.0.1:7880 \
//!     --livekit-api-key devkey --livekit-api-secret secret \
//!     --livekit-room demo --livekit-identity client

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};

use tddy_core::backend::{AnyBackend, SharedBackend, StubBackend};
use tddy_core::Presenter;
#[cfg(feature = "livekit")]
use tddy_service::TerminalServiceVirtualTui;
use tddy_service::{start_virtual_tui_session, VirtualTuiSession};
use tddy_tui::raw::{disable_raw_mode, enable_raw_mode_keep_sig};

#[derive(Parser)]
#[command(name = "rpc-demo", about = "VirtualTui RPC demo")]
struct Args {
    /// LiveKit server URL (e.g. ws://127.0.0.1:7880). Enables LiveKit transport.
    #[arg(long)]
    livekit_url: Option<String>,

    /// LiveKit API key (for token generation)
    #[arg(long, default_value = "devkey")]
    livekit_api_key: String,

    /// LiveKit API secret (for token generation)
    #[arg(long, default_value = "secret")]
    livekit_api_secret: String,

    /// LiveKit room name
    #[arg(long, default_value = "rpc-demo")]
    livekit_room: String,

    /// LiveKit client identity
    #[arg(long, default_value = "client")]
    livekit_identity: String,
}

/// Encode a crossterm key event into raw bytes for VirtualTui's key parser.
fn encode_key(key: crossterm::event::KeyEvent) -> Vec<u8> {
    match key.code {
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Up => vec![0x1b, b'[', b'A'],
        KeyCode::Down => vec![0x1b, b'[', b'B'],
        KeyCode::PageUp => vec![0x1b, b'[', b'5', b'~'],
        KeyCode::PageDown => vec![0x1b, b'[', b'6', b'~'],
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            vec![c as u8 & 0x1f]
        }
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            s.as_bytes().to_vec()
        }
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Tab => vec![b'\t'],
        _ => vec![],
    }
}

/// Start the Presenter + StubBackend + workflow, return the factory for creating VirtualTui sessions.
fn start_backend(
    shutdown: &Arc<AtomicBool>,
) -> Arc<dyn Fn() -> Option<tddy_core::ViewConnection> + Send + Sync> {
    let (event_tx, _) = tokio::sync::broadcast::channel(256);
    let (intent_tx, intent_rx) = std::sync::mpsc::channel();

    let presenter = Presenter::new("stub", "opus")
        .with_broadcast(event_tx)
        .with_intent_sender(intent_tx);
    let output_dir = std::env::temp_dir().join(format!("tddy-rpc-demo-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&output_dir).unwrap();
    let backend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
    let mut presenter = presenter.with_worktree_dir(output_dir.clone());
    presenter.start_workflow(
        backend,
        output_dir,
        None,
        Some("Build auth".to_string()),
        None,
        None,
        false,
        None,
        None,
        None,
    );

    let presenter = Arc::new(Mutex::new(presenter));
    let presenter_for_factory = presenter.clone();
    let factory: Arc<dyn Fn() -> Option<tddy_core::ViewConnection> + Send + Sync> =
        Arc::new(move || {
            presenter_for_factory
                .lock()
                .ok()
                .and_then(|p| p.connect_view())
        });

    let shutdown_p = shutdown.clone();
    let presenter_for_poll = presenter.clone();
    thread::spawn(move || {
        while !shutdown_p.load(Ordering::Relaxed) {
            while let Ok(intent) = intent_rx.try_recv() {
                if let Ok(mut p) = presenter_for_poll.lock() {
                    p.handle_intent(intent);
                }
            }
            if let Ok(mut p) = presenter_for_poll.lock() {
                p.poll_tool_calls();
                p.poll_workflow();
                if p.state().should_quit {
                    shutdown_p.store(true, Ordering::Relaxed);
                    break;
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
    });

    factory
}

/// Input/output handles for the terminal frontend — either in-process channels or LiveKit streams.
struct TerminalIO {
    /// Send raw keyboard bytes to the backend.
    send: Box<dyn FnMut(Vec<u8>) + Send>,
    /// Receive ANSI output bytes from the backend. Returns None when stream ends.
    recv: Box<dyn FnMut() -> Option<Vec<u8>> + Send>,
    /// Shutdown signal for the VirtualTui (in-process only).
    vt_shutdown: Option<Arc<AtomicBool>>,
}

/// In-process mode: direct VirtualTuiSession channels.
fn connect_in_process(
    factory: &(dyn Fn() -> Option<tddy_core::ViewConnection> + Send + Sync),
) -> anyhow::Result<TerminalIO> {
    let session = start_virtual_tui_session(factory)
        .ok_or_else(|| anyhow::anyhow!("connect_view not available"))?;
    let VirtualTuiSession {
        input_tx,
        output_rx,
        shutdown,
    } = session;

    let rt = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
    );

    let rt_send = rt.clone();
    let send = Box::new(move |bytes: Vec<u8>| {
        let _ = rt_send.block_on(input_tx.send(bytes));
    });

    let output_rx = Arc::new(Mutex::new(output_rx));
    let rt_recv = rt;
    let recv = Box::new(move || {
        let mut rx = output_rx.lock().unwrap();
        rt_recv.block_on(async {
            match tokio::time::timeout(Duration::from_millis(50), rx.recv()).await {
                Ok(Some(bytes)) => Some(bytes),
                _ => None,
            }
        })
    });

    Ok(TerminalIO {
        send,
        recv,
        vt_shutdown: Some(shutdown),
    })
}

/// LiveKit mode: server participant serves TerminalService, client connects via RpcClient bidi.
/// Uses a dedicated async runtime. The RpcClient and BidiStreamSender live in a spawned task
/// that bridges between sync send/recv channels and the LiveKit async API.
#[cfg(feature = "livekit")]
fn connect_livekit(
    factory: Arc<dyn Fn() -> Option<tddy_core::ViewConnection> + Send + Sync>,
    args: &Args,
    shutdown: &Arc<AtomicBool>,
) -> anyhow::Result<TerminalIO> {
    use livekit::prelude::*;
    use prost::Message;
    use tddy_livekit::{LiveKitParticipant, RpcClient, TokenGenerator};
    use tddy_service::proto::terminal::{TerminalInput, TerminalOutput};
    use tddy_service::TerminalServiceServer;

    let url = args.livekit_url.as_ref().unwrap().clone();
    let room_name = args.livekit_room.clone();
    let api_key = args.livekit_api_key.clone();
    let api_secret = args.livekit_api_secret.clone();
    let client_identity = args.livekit_identity.clone();
    let server_identity = format!("{}-server", client_identity);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;

    // Generate tokens
    let server_token = TokenGenerator::new(
        api_key.clone(),
        api_secret.clone(),
        room_name.clone(),
        server_identity.clone(),
        Duration::from_secs(3600),
    )
    .generate()
    .map_err(|e| anyhow::anyhow!("server token: {}", e))?;

    let client_token = TokenGenerator::new(
        api_key,
        api_secret,
        room_name,
        client_identity.clone(),
        Duration::from_secs(3600),
    )
    .generate()
    .map_err(|e| anyhow::anyhow!("client token: {}", e))?;

    // Start server participant
    let terminal_service = TerminalServiceVirtualTui::new(factory);
    let shutdown_server = shutdown.clone();
    let url_s = url.clone();
    rt.spawn(async move {
        match LiveKitParticipant::connect(
            &url_s,
            &server_token,
            TerminalServiceServer::new(terminal_service),
            RoomOptions::default(),
        )
        .await
        {
            Ok(server) => {
                eprintln!("[rpc_demo] LiveKit server connected");
                let _ = server.run().await;
            }
            Err(e) => {
                eprintln!("[rpc_demo] LiveKit server connect failed: {}", e);
                shutdown_server.store(true, Ordering::Relaxed);
            }
        }
    });

    // Connect client, wait for server, start bidi — all on the async runtime
    let (client_room, mut client_events) = rt.block_on(async {
        Room::connect(&url, &client_token, RoomOptions::default())
            .await
            .map_err(|e| anyhow::anyhow!("client connect: {}", e))
    })?;
    let rpc_events = client_room.subscribe();

    let target = server_identity.clone();
    rt.block_on(async {
        let target_id: ParticipantIdentity = target.into();
        if client_room.remote_participants().contains_key(&target_id) {
            return Ok(());
        }
        tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(event) = client_events.recv().await {
                if let RoomEvent::ParticipantConnected(p) = event {
                    if p.identity() == target_id {
                        return;
                    }
                }
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("Timed out waiting for server participant"))
    })?;
    eprintln!("[rpc_demo] LiveKit client connected, server visible");

    // Create channels that bridge the sync main loop ↔ async LiveKit bidi stream.
    // The RpcClient + BidiStreamSender live entirely inside a spawned async task.
    let (key_tx, mut key_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let (output_tx, output_rx) = std::sync::mpsc::channel::<Vec<u8>>();

    let shutdown_bidi = shutdown.clone();
    rt.spawn(async move {
        let rpc_client = RpcClient::new(client_room, server_identity, rpc_events);
        let bidi = rpc_client.start_bidi_stream("terminal.TerminalService", "StreamTerminalIO");
        let (mut sender, mut rx) = match bidi {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("[rpc_demo] start_bidi_stream failed: {}", e);
                shutdown_bidi.store(true, Ordering::Relaxed);
                return;
            }
        };

        // Send init
        if let Err(e) = sender
            .send(TerminalInput { data: vec![] }.encode_to_vec(), false)
            .await
        {
            eprintln!("[rpc_demo] send init failed: {}", e);
            return;
        }

        loop {
            if shutdown_bidi.load(Ordering::Relaxed) {
                break;
            }
            tokio::select! {
                // Forward keyboard bytes → LiveKit bidi sender
                Some(bytes) = key_rx.recv() => {
                    let payload = TerminalInput { data: bytes }.encode_to_vec();
                    if let Err(e) = sender.send(payload, false).await {
                        eprintln!("[rpc_demo] livekit send failed: {}", e);
                        break;
                    }
                }
                // Forward LiveKit bidi output → frontend channel
                Some(chunk) = rx.recv() => {
                    match chunk {
                        Ok(bytes) => {
                            if let Ok(output) = TerminalOutput::decode(&bytes[..]) {
                                let _ = output_tx.send(output.data);
                            }
                        }
                        Err(e) => {
                            eprintln!("[rpc_demo] livekit recv error: {}", e);
                            break;
                        }
                    }
                }
                else => break,
            }
        }
    });

    // Wrap the channels as TerminalIO
    let send = Box::new(move |bytes: Vec<u8>| {
        let _ = key_tx.blocking_send(bytes);
    });

    let recv = Box::new(
        move || match output_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(bytes) => Some(bytes),
            Err(_) => None,
        },
    );

    // Keep the runtime alive (it owns the spawned tasks)
    std::mem::forget(rt);

    Ok(TerminalIO {
        send,
        recv,
        vt_shutdown: None,
    })
}

fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .parse_default_env()
        .init();

    let args = Args::parse();
    let shutdown = Arc::new(AtomicBool::new(false));
    let factory = start_backend(&shutdown);

    let use_livekit = args.livekit_url.is_some();
    let mut io = if use_livekit {
        #[cfg(feature = "livekit")]
        {
            eprintln!(
                "[rpc_demo] Connecting via LiveKit: {}",
                args.livekit_url.as_ref().unwrap()
            );
            connect_livekit(factory, &args, &shutdown)?
        }
        #[cfg(not(feature = "livekit"))]
        {
            anyhow::bail!(
                "LiveKit support requires the 'livekit' feature. \
                 Rebuild with: cargo run -p tddy-e2e --features livekit --example rpc_demo -- ..."
            );
        }
    } else {
        eprintln!("[rpc_demo] In-process mode (no --livekit-url)");
        connect_in_process(&*factory)?
    };

    // --- Frontend: real terminal ---
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(std::io::stderr(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
        original_hook(info);
    }));

    enable_raw_mode_keep_sig()?;
    let mut stdout = std::io::stdout();
    execute!(&mut stdout, EnterAlternateScreen)?;

    // Main loop: poll keyboard + drain output
    while !shutdown.load(Ordering::Relaxed) {
        // Drain output → stdout
        while let Some(bytes) = (io.recv)() {
            let _ = stdout.write_all(&bytes);
            let _ = stdout.flush();
        }

        // Poll keyboard
        if event::poll(Duration::from_millis(10)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    break;
                }
                let bytes = encode_key(key);
                if !bytes.is_empty() {
                    (io.send)(bytes);
                }
            }
        }
    }

    // Cleanup
    shutdown.store(true, Ordering::Relaxed);
    if let Some(ref vt_shutdown) = io.vt_shutdown {
        vt_shutdown.store(true, Ordering::Relaxed);
    }

    execute!(std::io::stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()?;

    Ok(())
}
