//! Integration tests for `RdpClient` against a minimal in-process RDP server.
//!
//! The fake server (`FakeRdpServer`) generates QR-coded frames on demand and records
//! input events via a `RecordingInputHandler`. Tests decode the QR from the client's
//! RGBA framebuffer to assert the full encode → RDP wire → blit → QR-decode pipeline.
//!
//! Async synchronisation uses `tokio::sync::Notify` rather than `sleep` so tests
//! pass as soon as the server receives an event and don't waste time on fixed delays.
//!
//! `RdpServer` uses `Rc` internally (making its futures `!Send`), so `FakeRdpServer`
//! runs on a dedicated OS thread with a single-threaded Tokio runtime and a `LocalSet`.

use std::num::{NonZeroU16, NonZeroUsize};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use ironrdp_server::{
    BitmapUpdate, DesktopSize, DisplayUpdate, KeyboardEvent, MouseEvent, PixelFormat, RdpServer,
    RdpServerDisplay, RdpServerDisplayUpdates, RdpServerInputHandler,
};
use tokio::sync::mpsc;

use tddy_rdp::rdp_client::RdpClient;
use tddy_screenshare::client::ScreenSharingClient as _;

// ── Crypto provider ───────────────────────────────────────────────────────────

/// Install the aws-lc-rs crypto provider before any TLS operation.
///
/// `rustls` 0.23 has no automatic default; the first call installs it and
/// subsequent calls are no-ops (install_default returns Err if already set).
fn ensure_crypto_provider() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let _ = ironrdp_server::tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
            .install_default();
    });
}

// ── QR helpers ────────────────────────────────────────────────────────────────

/// Render `"frame:N"` as a 256×256 RGBA pixel buffer.
fn generate_qr_rgba(frame: u64) -> Vec<u8> {
    let content = format!("frame:{}", frame);
    let code = qrcode::QrCode::new(content.as_bytes()).expect("QR encode failed");
    let luma = code
        .render::<image::Luma<u8>>()
        .min_dimensions(256, 256)
        .build();
    let luma = image::imageops::resize(&luma, 256, 256, image::imageops::FilterType::Nearest);
    luma.pixels()
        .flat_map(|p| [p.0[0], p.0[0], p.0[0], 255u8])
        .collect()
}

/// Decode a QR code from a 256×256 RGBA framebuffer using the R channel as greyscale.
fn decode_qr(rgba: &[u8], width: u32, height: u32) -> Option<String> {
    let w = width as usize;
    let mut img = rqrr::PreparedImage::prepare_from_greyscale(w, height as usize, |x, y| {
        rgba[(y * w + x) * 4] // R channel
    });
    img.detect_grids()
        .into_iter()
        .find_map(|g| g.decode().ok().map(|(_, s)| s))
}

// ── Fake RDP server — display handler ────────────────────────────────────────

struct QrDisplayUpdates {
    frame_rx: mpsc::Receiver<u64>,
}

#[async_trait]
impl RdpServerDisplayUpdates for QrDisplayUpdates {
    async fn next_update(&mut self) -> anyhow::Result<Option<DisplayUpdate>> {
        match self.frame_rx.recv().await {
            Some(n) => {
                let pixels = generate_qr_rgba(n);
                Ok(Some(DisplayUpdate::Bitmap(BitmapUpdate {
                    x: 0,
                    y: 0,
                    width: NonZeroU16::new(256).unwrap(),
                    height: NonZeroU16::new(256).unwrap(),
                    format: PixelFormat::RgbA32,
                    data: Bytes::from(pixels),
                    stride: NonZeroUsize::new(256 * 4).unwrap(),
                })))
            }
            None => Ok(None),
        }
    }
}

struct QrDisplay {
    frame_rx: Option<mpsc::Receiver<u64>>,
}

#[async_trait]
impl RdpServerDisplay for QrDisplay {
    async fn size(&mut self) -> DesktopSize {
        DesktopSize {
            width: 256,
            height: 256,
        }
    }

    async fn updates(&mut self) -> anyhow::Result<Box<dyn RdpServerDisplayUpdates>> {
        let rx = self
            .frame_rx
            .take()
            .expect("updates() called more than once");
        Ok(Box::new(QrDisplayUpdates { frame_rx: rx }))
    }
}

// ── Fake RDP server — input recording ────────────────────────────────────────

/// Simplified mouse event that owns its data and derives `Clone`.
///
/// `ironrdp_server::MouseEvent` doesn't derive `Clone`, so we map to this type
/// immediately on receipt and store it in the shared `ServerState`.
#[derive(Debug, Clone, PartialEq)]
enum RecordedMouse {
    Move { x: u16, y: u16 },
    LeftPressed,
    LeftReleased,
    RightPressed,
    RightReleased,
    Other,
}

/// Simplified keyboard event that owns its data and derives `Clone`.
#[derive(Debug, Clone, PartialEq)]
enum RecordedKey {
    Pressed { code: u8, extended: bool },
    Released { code: u8, extended: bool },
    Other,
}

#[derive(Default)]
struct ServerState {
    mouse_events: Vec<RecordedMouse>,
    key_events: Vec<RecordedKey>,
}

struct RecordingInputHandler {
    state: Arc<Mutex<ServerState>>,
    /// Fires on every input event so tests can wait without sleeping.
    event_notify: Arc<tokio::sync::Notify>,
}

impl RdpServerInputHandler for RecordingInputHandler {
    fn keyboard(&mut self, event: KeyboardEvent) {
        let recorded = match event {
            KeyboardEvent::Pressed { code, extended } => RecordedKey::Pressed { code, extended },
            KeyboardEvent::Released { code, extended } => RecordedKey::Released { code, extended },
            _ => RecordedKey::Other,
        };
        self.state.lock().unwrap().key_events.push(recorded);
        self.event_notify.notify_one();
    }

    fn mouse(&mut self, event: MouseEvent) {
        let recorded = match event {
            MouseEvent::Move { x, y } => RecordedMouse::Move { x, y },
            MouseEvent::LeftPressed => RecordedMouse::LeftPressed,
            MouseEvent::LeftReleased => RecordedMouse::LeftReleased,
            MouseEvent::RightPressed => RecordedMouse::RightPressed,
            MouseEvent::RightReleased => RecordedMouse::RightReleased,
            _ => RecordedMouse::Other,
        };
        self.state.lock().unwrap().mouse_events.push(recorded);
        self.event_notify.notify_one();
    }
}

// ── Fake RDP server harness ───────────────────────────────────────────────────

fn make_tls_acceptor() -> ironrdp_server::tokio_rustls::TlsAcceptor {
    use ironrdp_server::tokio_rustls::rustls;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};

    ensure_crypto_provider();

    let cert = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into(), "localhost".into()])
        .expect("rcgen cert generation failed");
    let cert_der = cert.cert.der().to_vec();
    let key_der = cert.key_pair.serialize_der();

    let cert_chain = vec![CertificateDer::from(cert_der)];
    let key = PrivateKeyDer::try_from(key_der).expect("valid private key DER");

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)
        .expect("valid TLS config");

    ironrdp_server::tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(config))
}

/// In-process RDP server running on a dedicated OS thread.
///
/// `RdpServer` uses `Rc` internally (its `run_connection` future is `!Send`),
/// so it cannot be `tokio::spawn`ed on the default multi-thread runtime.
/// We run it on a dedicated OS thread with a current-thread Tokio runtime and
/// `LocalSet` instead.
struct FakeRdpServer {
    port: u16,
    _thread: std::thread::JoinHandle<()>,
    frame_tx: mpsc::Sender<u64>,
    state: Arc<Mutex<ServerState>>,
    event_notify: Arc<tokio::sync::Notify>,
}

impl FakeRdpServer {
    async fn start() -> Self {
        // Bind a std socket so we can capture the port before moving it to the new thread.
        let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let port = std_listener.local_addr().unwrap().port();

        let acceptor = make_tls_acceptor();
        let (frame_tx, frame_rx) = mpsc::channel::<u64>(32);
        let state: Arc<Mutex<ServerState>> = Arc::new(Mutex::new(ServerState::default()));
        let event_notify: Arc<tokio::sync::Notify> = Arc::new(tokio::sync::Notify::new());

        let state_for_thread = state.clone();
        let notify_for_thread = event_notify.clone();
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

        let thread = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, async move {
                let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
                let display = QrDisplay {
                    frame_rx: Some(frame_rx),
                };
                let input_handler = RecordingInputHandler {
                    state: state_for_thread,
                    event_notify: notify_for_thread,
                };
                let mut server = RdpServer::builder()
                    .with_addr(addr)
                    .with_tls(acceptor)
                    .with_input_handler(input_handler)
                    .with_display_handler(display)
                    .build();

                while let Ok((stream, _)) = listener.accept().await {
                    let _ = server.run_connection(stream).await;
                }
            });
        });

        // Give the server thread a moment to start accepting before clients connect.
        tokio::time::sleep(Duration::from_millis(10)).await;

        Self {
            port,
            _thread: thread,
            frame_tx,
            state,
            event_notify,
        }
    }

    /// Push a QR-coded frame (encoding `"frame:{n}"`) to the connected client.
    fn send_frame(&self, n: u64) {
        let _ = self.frame_tx.try_send(n);
    }

    /// Block until the server records any input event, or `timeout` elapses.
    ///
    /// `tokio::sync::Notify::notify_one()` stores a permit if there are no
    /// current waiters, so events that fire before `await_event` is called are
    /// not lost.
    async fn await_event(&self, timeout: Duration) -> bool {
        tokio::time::timeout(timeout, self.event_notify.notified())
            .await
            .is_ok()
    }

    fn mouse_events(&self) -> Vec<RecordedMouse> {
        self.state.lock().unwrap().mouse_events.clone()
    }

    fn key_events(&self) -> Vec<RecordedKey> {
        self.state.lock().unwrap().key_events.clone()
    }
}

// ── Test helpers ──────────────────────────────────────────────────────────────

/// Poll until a QR frame distinct from `prev` is decoded from the client framebuffer.
///
/// RDP is server-push: frames arrive only after `FakeRdpServer::send_frame()` is
/// called and the PDU round-trip completes. We poll `poll_events()` in a tight loop
/// with a 5s outer timeout so the failure message names the missing frame rather than
/// just timing out silently.
///
/// 5s timeout: covers normal PDU latency in CI. Single-frame round-trips take < 50ms.
async fn receive_next_qr_frame(client: &mut RdpClient, prev: Option<&str>) -> String {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let (w, h) = client.framebuffer_dimensions();
            if let Some(qr) = decode_qr(client.get_rgba_frame(), w, h) {
                if Some(qr.as_str()) != prev {
                    return qr;
                }
            }
            // Drive the event loop; timeout is the fallback if no PDU arrives yet.
            let _ = tokio::time::timeout(Duration::from_millis(50), client.poll_events()).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "server did not deliver a new QR frame within 5s (prev={:?})",
            prev
        )
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn streams_qr_coded_frames_with_incrementing_content() {
    // Given: a fake RDP server that delivers QR-coded frames on demand
    let server = FakeRdpServer::start().await;
    let mut client = RdpClient::connect("127.0.0.1", server.port, None, None)
        .await
        .expect("RDP connect failed");

    // When/Then: three consecutive frames arrive in order, each QR decoding correctly
    let mut prev: Option<String> = None;
    for expected_frame in 0u64..3 {
        server.send_frame(expected_frame);
        let decoded = receive_next_qr_frame(&mut client, prev.as_deref()).await;
        assert_eq!(
            decoded,
            format!("frame:{}", expected_frame),
            "QR mismatch on frame {}",
            expected_frame
        );
        prev = Some(decoded);
    }
}

#[tokio::test]
async fn framebuffer_dimensions_match_server_desktop_size() {
    // Given: a fake RDP server that advertises 256×256 in QrDisplay::size()
    let server = FakeRdpServer::start().await;

    // When: client completes the connection handshake
    let client = RdpClient::connect("127.0.0.1", server.port, None, None)
        .await
        .expect("RDP connect failed");

    // Then: the client framebuffer reflects the server's negotiated desktop size
    assert_eq!(client.framebuffer_dimensions(), (256, 256));
}

#[tokio::test]
async fn pointer_event_is_received_by_server() {
    // Given: connected client and a server ready to record input
    let server = FakeRdpServer::start().await;
    let mut client = RdpClient::connect("127.0.0.1", server.port, None, None)
        .await
        .expect("RDP connect failed");

    // When: a pointer move event is injected
    client
        .inject_pointer(42, 17, 0)
        .await
        .expect("inject_pointer failed");

    // Then: the server records a move event for the same coordinates
    // 500ms: covers PDU round-trip + server dispatch; no sleep needed because
    // RecordingInputHandler calls Notify::notify_one() on every event.
    let received = server.await_event(Duration::from_millis(500)).await;
    assert!(
        received,
        "server did not receive any input event within 500ms"
    );

    let events = server.mouse_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, RecordedMouse::Move { x: 42, y: 17 })),
        "expected Move{{x:42, y:17}} in recorded events, got {:?}",
        events
    );
}

#[tokio::test]
async fn keyboard_press_is_received_by_server() {
    // Given: connected client and a server ready to record input
    let server = FakeRdpServer::start().await;
    let mut client = RdpClient::connect("127.0.0.1", server.port, None, None)
        .await
        .expect("RDP connect failed");

    // When: key-down for X11 keysym 0x41 ('A') is injected — PS/2 scancode 0x1e
    client
        .inject_key(0x41, true)
        .await
        .expect("inject_key failed");

    // Then: the server records a Pressed event for scancode 0x1e, no extended bit
    let received = server.await_event(Duration::from_millis(500)).await;
    assert!(
        received,
        "server did not receive any input event within 500ms"
    );

    let events = server.key_events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            RecordedKey::Pressed {
                code: 0x1e,
                extended: false
            }
        )),
        "expected Pressed{{code:0x1e, extended:false}} in recorded events, got {:?}",
        events
    );
}

#[tokio::test]
async fn closes_connection_cleanly_on_stop() {
    // Given: connected client
    let server = FakeRdpServer::start().await;
    let mut client = RdpClient::connect("127.0.0.1", server.port, None, None)
        .await
        .expect("RDP connect failed");

    // When/Then: stop() completes without error
    client.stop().await.expect("stop() failed");
    drop(server);
}
