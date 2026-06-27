//! Integration tests for `VncClientState` against a minimal in-process RFB 3.8 server.
//!
//! The fake server generates QR codes — one per frame — encoding `"frame:N"`.
//! Tests decode the QR from the client's RGBA framebuffer to assert the full
//! encode → BGRA wire → BGRA→RGBA conversion → QR decode pipeline is correct.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use tddy_screenshare::client::ScreenSharingClient as _;
use tddy_vnc::vnc_client::VncClientState;

// ── QR helpers ───────────────────────────────────────────────────────────────

/// Render `"frame:N"` as a 256×256 BGRA pixel buffer.
///
/// Black QR modules → [0,0,0,255], white background → [255,255,255,255].
/// VncClientState expects BGRA from the server and converts to RGBA internally.
fn generate_qr_bgra(frame: u64) -> Vec<u8> {
    let content = format!("frame:{}", frame);
    let code = qrcode::QrCode::new(content.as_bytes()).expect("QR encode failed");
    let luma = code
        .render::<image::Luma<u8>>()
        .min_dimensions(256, 256)
        .build();
    let luma = image::imageops::resize(&luma, 256, 256, image::imageops::FilterType::Nearest);
    // BGRA: greyscale in all colour channels (B=G=R=luma, A=255)
    luma.pixels()
        .flat_map(|p| [p.0[0], p.0[0], p.0[0], 255u8])
        .collect()
}

/// Decode a QR code from a 256×256 RGBA framebuffer.
///
/// Uses the R channel as greyscale input (values survive the BGRA→RGBA conversion unchanged
/// for greyscale pixels).
fn decode_qr(rgba: &[u8], width: u32, height: u32) -> Option<String> {
    let w = width as usize;
    let mut img = rqrr::PreparedImage::prepare_from_greyscale(w, height as usize, |x, y| {
        rgba[(y * w + x) * 4] // R channel
    });
    img.detect_grids()
        .into_iter()
        .find_map(|g| g.decode().ok().map(|(_, s)| s))
}

// ── RFB protocol constants ────────────────────────────────────────────────────

/// 16-byte BGRA PixelFormat for RFB ServerInit.
///
/// Layout: bits_per_pixel=32, depth=24, little-endian, true-colour,
/// max=255 per channel, redShift=16, greenShift=8, blueShift=0.
const BGRA_PIXEL_FORMAT: [u8; 16] = [
    32, // bits_per_pixel
    24, // depth
    0,  // big_endian_flag (0 = little-endian)
    1,  // true_colour_flag
    0, 255, // red_max   (u16 BE)
    0, 255, // green_max (u16 BE)
    0, 255, // blue_max  (u16 BE)
    16,  // red_shift   (R is byte 2 in BGRA = bits 16-23)
    8,   // green_shift (G is byte 1      = bits  8-15)
    0,   // blue_shift  (B is byte 0      = bits  0- 7)
    0, 0, 0, // padding
];

// ── Fake RFB server ───────────────────────────────────────────────────────────

#[derive(Default)]
struct ServerState {
    pointer_events: Vec<(u16, u16, u8)>,
    key_events: Vec<(u32, bool)>,
}

struct FakeVncServer {
    port: u16,
    _task: JoinHandle<()>,
    state: Arc<Mutex<ServerState>>,
}

impl FakeVncServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let state: Arc<Mutex<ServerState>> = Arc::new(Mutex::new(ServerState::default()));
        let state_clone = state.clone();
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let s = state_clone.clone();
                tokio::spawn(handle_connection(stream, s));
            }
        });
        Self {
            port,
            _task: task,
            state,
        }
    }

    fn pointer_events(&self) -> Vec<(u16, u16, u8)> {
        self.state.lock().unwrap().pointer_events.clone()
    }

    fn key_events(&self) -> Vec<(u32, bool)> {
        self.state.lock().unwrap().key_events.clone()
    }
}

/// Handle one RFB client connection: handshake → init → event loop.
async fn handle_connection(mut s: TcpStream, state: Arc<Mutex<ServerState>>) {
    // ── Handshake ──
    s.write_all(b"RFB 003.008\n").await.unwrap();
    let mut buf12 = [0u8; 12];
    if s.read_exact(&mut buf12).await.is_err() {
        return;
    }

    // Security: advertise SecurityType::None (type 1)
    s.write_all(&[1u8, 1u8]).await.unwrap();
    let mut sec = [0u8; 1];
    if s.read_exact(&mut sec).await.is_err() {
        return;
    }
    s.write_all(&[0u8, 0, 0, 0]).await.unwrap(); // SecurityResult: OK

    // ── Initialisation ──
    let mut client_init = [0u8; 1];
    if s.read_exact(&mut client_init).await.is_err() {
        return;
    }

    // ServerInit: width + height + PixelFormat + name
    let name = b"FakeVNC";
    let mut init = Vec::with_capacity(4 + 16 + 4 + name.len());
    init.extend_from_slice(&256u16.to_be_bytes()); // width
    init.extend_from_slice(&256u16.to_be_bytes()); // height
    init.extend_from_slice(&BGRA_PIXEL_FORMAT);
    init.extend_from_slice(&(name.len() as u32).to_be_bytes());
    init.extend_from_slice(name);
    if s.write_all(&init).await.is_err() {
        return;
    }

    // ── Event loop ──
    let mut frame_counter = 0u64;
    loop {
        let mut type_byte = [0u8; 1];
        if s.read_exact(&mut type_byte).await.is_err() {
            break;
        }
        match type_byte[0] {
            0 => {
                // SetPixelFormat: 3 padding + 16 PixelFormat
                let mut buf = [0u8; 19];
                if s.read_exact(&mut buf).await.is_err() {
                    break;
                }
            }
            2 => {
                // SetEncodings: 1 padding + 2 num_encodings + 4*num
                let mut hdr = [0u8; 3];
                if s.read_exact(&mut hdr).await.is_err() {
                    break;
                }
                let num = u16::from_be_bytes([hdr[1], hdr[2]]) as usize;
                let mut enc = vec![0u8; num * 4];
                if s.read_exact(&mut enc).await.is_err() {
                    break;
                }
            }
            3 => {
                // FramebufferUpdateRequest: 1 incremental + 2x + 2y + 2w + 2h = 9 bytes
                let mut req = [0u8; 9];
                if s.read_exact(&mut req).await.is_err() {
                    break;
                }
                // Respond with FramebufferUpdate containing one Raw rect
                let pixels = generate_qr_bgra(frame_counter);
                frame_counter += 1;
                let mut msg = Vec::with_capacity(12 + pixels.len());
                msg.push(0u8); // message type: FramebufferUpdate
                msg.push(0u8); // padding
                msg.extend_from_slice(&1u16.to_be_bytes()); // num_rects
                msg.extend_from_slice(&0u16.to_be_bytes()); // x
                msg.extend_from_slice(&0u16.to_be_bytes()); // y
                msg.extend_from_slice(&256u16.to_be_bytes()); // width
                msg.extend_from_slice(&256u16.to_be_bytes()); // height
                msg.extend_from_slice(&0i32.to_be_bytes()); // encoding: Raw
                msg.extend_from_slice(&pixels);
                if s.write_all(&msg).await.is_err() {
                    break;
                }
            }
            4 => {
                // KeyEvent: 1 down_flag + 2 padding + 4 keysym = 7 more bytes
                let mut buf = [0u8; 7];
                if s.read_exact(&mut buf).await.is_err() {
                    break;
                }
                let down = buf[0] != 0;
                let keysym = u32::from_be_bytes([buf[3], buf[4], buf[5], buf[6]]);
                state.lock().unwrap().key_events.push((keysym, down));
            }
            5 => {
                // PointerEvent: 1 button_mask + 2 x + 2 y = 5 more bytes
                let mut buf = [0u8; 5];
                if s.read_exact(&mut buf).await.is_err() {
                    break;
                }
                let btn = buf[0];
                let x = u16::from_be_bytes([buf[1], buf[2]]);
                let y = u16::from_be_bytes([buf[3], buf[4]]);
                state.lock().unwrap().pointer_events.push((x, y, btn));
            }
            _ => break,
        }
    }
}

// ── Test helpers ──────────────────────────────────────────────────────────────

/// Poll until ServerInit dimensions arrive (first SetResolution event).
async fn wait_for_dimensions(client: &mut VncClientState) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        let _ = client.update().await;
        if client.dimensions().0 > 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("timeout waiting for VNC server dimensions");
}

/// Request the next frame and poll until the QR content is distinct from `prev`.
///
/// After `SetResolution` the framebuffer is pre-allocated with zeros, so we
/// cannot rely on emptiness — we compare decoded QR content instead.
async fn receive_next_qr_frame(client: &mut VncClientState, prev: Option<&str>) -> String {
    client.request_update(false).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        let _ = client.update().await;
        let (w, h) = client.dimensions();
        if let Some(qr) = decode_qr(client.get_framebuffer(), w, h) {
            if Some(qr.as_str()) != prev {
                return qr;
            }
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!(
        "timeout waiting for next distinct QR frame (prev={:?})",
        prev
    );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn frames_stream_with_incrementing_qr_content() {
    // Given: fake VNC server generating one QR code per FramebufferUpdateRequest
    let server = FakeVncServer::start().await;
    let mut client = VncClientState::connect("127.0.0.1", server.port, None)
        .await
        .expect("VNC connect failed");
    wait_for_dimensions(&mut client).await;

    let mut prev: Option<String> = None;
    for expected_frame in 0u64..3 {
        // When: requesting and receiving the next distinct frame
        let decoded = receive_next_qr_frame(&mut client, prev.as_deref()).await;

        // Then: QR content encodes the frame number, proving streaming and BGRA→RGBA conversion
        assert_eq!(
            decoded,
            format!("frame:{}", expected_frame),
            "frame {} QR content mismatch",
            expected_frame
        );
        prev = Some(decoded);
    }
}

#[tokio::test]
async fn framebuffer_dimensions_match_server_init() {
    // Given: fake VNC server advertising 256×256
    let server = FakeVncServer::start().await;
    let mut client = VncClientState::connect("127.0.0.1", server.port, None)
        .await
        .unwrap();

    // When: first poll_events delivers SetResolution
    wait_for_dimensions(&mut client).await;

    // Then: dimensions match what was sent in ServerInit
    assert_eq!(client.dimensions(), (256, 256));
}

#[tokio::test]
async fn inject_pointer_is_recorded_by_server() {
    // Given: connected client
    let server = FakeVncServer::start().await;
    let mut client = VncClientState::connect("127.0.0.1", server.port, None)
        .await
        .unwrap();
    wait_for_dimensions(&mut client).await;

    // When: sending a pointer event
    client.pointer_event(42, 17, 1).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Then: server recorded it with the correct coordinates and button mask
    let events = server.pointer_events();
    assert!(!events.is_empty(), "no pointer events received by server");
    assert_eq!(events[0], (42, 17, 1));
}

#[tokio::test]
async fn inject_key_is_recorded_by_server() {
    // Given: connected client
    let server = FakeVncServer::start().await;
    let mut client = VncClientState::connect("127.0.0.1", server.port, None)
        .await
        .unwrap();
    wait_for_dimensions(&mut client).await;

    // When: sending a key-down event for XK_Return
    client.keyboard_key(0xff0d, true).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Then: server recorded keysym=0xff0d, down=true
    let events = server.key_events();
    assert!(!events.is_empty(), "no key events received by server");
    assert_eq!(events[0], (0xff0d, true));
}

#[tokio::test]
async fn stop_closes_connection_cleanly() {
    // Given: connected client
    let server = FakeVncServer::start().await;
    let mut client = VncClientState::connect("127.0.0.1", server.port, None)
        .await
        .unwrap();
    wait_for_dimensions(&mut client).await;

    // When/Then: stop() returns Ok(()) without error
    client.stop().await.expect("stop() failed");
}
