//! RDP client implementing `ScreenSharingClient` via IronRDP.
//!
//! Connection sequence:
//!   TCP → X.224 negotiation (connect_begin) → TLS upgrade → connect_finalize → ActiveStage
//!
//! TLS uses a `NoCertificateVerification` verifier (via ironrdp-tls) so that self-signed
//! test certs work. In production the daemon should validate the server certificate
//! out-of-band (e.g. thumbprint pinning).

use std::net::SocketAddr;

use anyhow::{Context, Result};
use ironrdp_connector::{BitmapConfig, ClientConnector, Config, Credentials, DesktopSize, ServerName};
use ironrdp_graphics::image_processing::PixelFormat;
use ironrdp_pdu::{
    gcc::KeyboardType,
    input::{
        fast_path::{FastPathInputEvent, KeyboardFlags},
        mouse::{MousePdu, PointerFlags},
    },
    rdp::capability_sets::{BitmapCodecs, MajorPlatformType},
};
use ironrdp_session::{image::DecodedImage, ActiveStage, ActiveStageOutput};
use ironrdp_tokio::{connect_begin, connect_finalize, mark_as_upgraded, FramedWrite as _, TokioFramed};
use tokio::net::TcpStream;

/// Dummy `NetworkClient` — no CredSSP is performed in TLS-only mode.
struct NoNetworkClient;

impl ironrdp_tokio::NetworkClient for NoNetworkClient {
    async fn send(
        &mut self,
        _request: &ironrdp_connector::sspi::generator::NetworkRequest,
    ) -> ironrdp_connector::ConnectorResult<Vec<u8>> {
        Ok(vec![])
    }
}

/// Active RDP session.
pub struct RdpClient {
    image: DecodedImage,
    active: ActiveStage,
    framed: TokioFramed<ironrdp_tls::TlsStream<TcpStream>>,
}

#[async_trait::async_trait]
impl tddy_screenshare::ScreenSharingClient for RdpClient {
    async fn connect(host: &str, port: u16, password: Option<&str>) -> Result<Self> {
        // 1. TCP
        let stream = TcpStream::connect(format!("{host}:{port}"))
            .await
            .context("TCP connect")?;
        let mut framed = TokioFramed::new(stream);

        // 2. Connector config
        let config = Config {
            desktop_size: DesktopSize { width: 256, height: 256 },
            desktop_scale_factor: 0,
            credentials: Credentials::UsernamePassword {
                username: "user".into(),
                password: password.unwrap_or("").into(),
            },
            enable_tls: true,
            enable_credssp: false,
            domain: None,
            client_build: 0,
            client_name: "tddy-rdp".into(),
            keyboard_type: KeyboardType::IbmEnhanced,
            keyboard_subtype: 0,
            keyboard_functional_keys_count: 12,
            keyboard_layout: 0x0409,
            ime_file_name: String::new(),
            // Empty codec list so the server uses raw-pixel SetSurfaceBits (CODEC_ID_NONE)
            // instead of QOI/QoiZ codecs that ironrdp-session doesn't support by default.
            bitmap: Some(BitmapConfig {
                lossy_compression: false,
                color_depth: 32,
                codecs: BitmapCodecs(vec![]),
            }),
            dig_product_id: String::new(),
            client_dir: String::new(),
            alternate_shell: String::new(),
            work_dir: String::new(),
            platform: MajorPlatformType::UNSPECIFIED,
            hardware_id: None,
            request_data: None,
            autologon: false,
            enable_audio_playback: false,
            performance_flags: Default::default(),
            license_cache: None,
            timezone_info: Default::default(),
            compression_type: None,
            enable_server_pointer: false,
            pointer_software_rendering: false,
            multitransport_flags: None,
        };

        let client_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
        let mut connector = ClientConnector::new(config, client_addr);

        // 3. X.224 negotiation until TLS upgrade point
        let should_upgrade = connect_begin(&mut framed, &mut connector)
            .await
            .context("connect_begin")?;

        // 4. TLS upgrade
        let (stream, leftover) = framed.into_inner();
        let (tls_stream, server_cert) = ironrdp_tls::upgrade(stream, host)
            .await
            .context("TLS upgrade")?;
        let mut framed = TokioFramed::new_with_leftover(tls_stream, leftover);

        let upgraded = mark_as_upgraded(should_upgrade, &mut connector);

        // 5. Finalize connection
        let server_public_key = ironrdp_tls::extract_tls_server_public_key(&server_cert)
            .unwrap_or(&[])
            .to_vec();

        let server_name = ServerName::new(host);

        let result = connect_finalize(
            upgraded,
            connector,
            &mut framed,
            &mut NoNetworkClient,
            server_name,
            server_public_key,
            None,
        )
        .await
        .context("connect_finalize")?;

        let image = DecodedImage::new(
            PixelFormat::RgbA32,
            result.desktop_size.width,
            result.desktop_size.height,
        );
        let active = ActiveStage::new(result);

        Ok(Self { image, active, framed })
    }

    fn framebuffer_dimensions(&self) -> (u32, u32) {
        (self.image.width().into(), self.image.height().into())
    }

    fn get_rgba_frame(&self) -> &[u8] {
        self.image.data()
    }

    async fn poll_events(&mut self) -> Result<bool> {
        let (action, frame) = self.framed.read_pdu().await.context("read PDU")?;
        let outputs = self
            .active
            .process(&mut self.image, action, &frame)
            .context("process PDU")?;

        let mut updated = false;
        for output in outputs {
            match output {
                ActiveStageOutput::GraphicsUpdate(_) => updated = true,
                ActiveStageOutput::ResponseFrame(frame) => {
                    self.framed
                        .write_all(&frame)
                        .await
                        .context("write response")?;
                }
                ActiveStageOutput::Terminate(_) => anyhow::bail!("RDP session terminated"),
                _ => {}
            }
        }
        Ok(updated)
    }

    async fn request_frame_update(&mut self, _incremental: bool) -> Result<()> {
        // RDP is server-push; the server sends updates without explicit requests.
        Ok(())
    }

    async fn inject_pointer(&mut self, x: u32, y: u32, button_mask: u32) -> Result<()> {
        let mut flags = PointerFlags::MOVE;
        if button_mask & 1 != 0 {
            flags |= PointerFlags::LEFT_BUTTON | PointerFlags::DOWN;
        }
        if button_mask & 2 != 0 {
            flags |= PointerFlags::RIGHT_BUTTON | PointerFlags::DOWN;
        }
        if button_mask & 4 != 0 {
            flags |= PointerFlags::MIDDLE_BUTTON_OR_WHEEL | PointerFlags::DOWN;
        }
        let event = FastPathInputEvent::MouseEvent(MousePdu {
            flags,
            number_of_wheel_rotation_units: 0,
            x_position: x as u16,
            y_position: y as u16,
        });
        self.send_input_events(&[event]).await
    }

    async fn inject_key(&mut self, keysym: u32, pressed: bool) -> Result<()> {
        let scancode = keysym_to_scancode(keysym);
        let flags = if pressed {
            KeyboardFlags::empty()
        } else {
            KeyboardFlags::RELEASE
        };
        let event = FastPathInputEvent::KeyboardEvent(flags, scancode);
        self.send_input_events(&[event]).await
    }

    async fn stop(&mut self) -> Result<()> {
        if let Ok(outputs) = self.active.graceful_shutdown() {
            for output in outputs {
                if let ActiveStageOutput::ResponseFrame(frame) = output {
                    let _ = self.framed.write_all(&frame).await;
                }
            }
        }
        Ok(())
    }
}

impl RdpClient {
    async fn send_input_events(&mut self, events: &[FastPathInputEvent]) -> Result<()> {
        let outputs = self
            .active
            .process_fastpath_input(&mut self.image, events)
            .context("process_fastpath_input")?;
        for output in outputs {
            if let ActiveStageOutput::ResponseFrame(frame) = output {
                self.framed
                    .write_all(&frame)
                    .await
                    .context("write input frame")?;
            }
        }
        Ok(())
    }
}

/// X11 keysym → PS/2 Set 1 scancode (make code).
fn keysym_to_scancode(keysym: u32) -> u8 {
    match keysym {
        // Number row
        0x60 => 0x29, // ` ~
        0x31 => 0x02, // 1 !
        0x32 => 0x03, // 2 @
        0x33 => 0x04, // 3 #
        0x34 => 0x05, // 4 $
        0x35 => 0x06, // 5 %
        0x36 => 0x07, // 6 ^
        0x37 => 0x08, // 7 &
        0x38 => 0x09, // 8 *
        0x39 => 0x0a, // 9 (
        0x30 => 0x0b, // 0 )
        0x2d => 0x0c, // - _
        0x3d => 0x0d, // = +
        // QWERTY row 2
        0x71 | 0x51 => 0x10, // q Q
        0x77 | 0x57 => 0x11, // w W
        0x65 | 0x45 => 0x12, // e E
        0x72 | 0x52 => 0x13, // r R
        0x74 | 0x54 => 0x14, // t T
        0x79 | 0x59 => 0x15, // y Y
        0x75 | 0x55 => 0x16, // u U
        0x69 | 0x49 => 0x17, // i I
        0x6f | 0x4f => 0x18, // o O
        0x70 | 0x50 => 0x19, // p P
        0x5b => 0x1a,         // [ {
        0x5d => 0x1b,         // ] }
        // Home row
        0x61 | 0x41 => 0x1e, // a A
        0x73 | 0x53 => 0x1f, // s S
        0x64 | 0x44 => 0x20, // d D
        0x66 | 0x46 => 0x21, // f F
        0x67 | 0x47 => 0x22, // g G
        0x68 | 0x48 => 0x23, // h H
        0x6a | 0x4a => 0x24, // j J
        0x6b | 0x4b => 0x25, // k K
        0x6c | 0x4c => 0x26, // l L
        0x3b => 0x27,         // ; :
        0x27 => 0x28,         // ' "
        0x5c => 0x2b,         // \ |
        // Bottom row
        0x7a | 0x5a => 0x2c, // z Z
        0x78 | 0x58 => 0x2d, // x X
        0x63 | 0x43 => 0x2e, // c C
        0x76 | 0x56 => 0x2f, // v V
        0x62 | 0x42 => 0x30, // b B
        0x6e | 0x4e => 0x31, // n N
        0x6d | 0x4d => 0x32, // m M
        0x2c => 0x33,         // , <
        0x2e => 0x34,         // . >
        0x2f => 0x35,         // / ?
        // Special keys
        0x20 => 0x39,   // Space
        0xff08 => 0x0e, // BackSpace
        0xff09 => 0x0f, // Tab
        0xff0d => 0x1c, // Return
        0xff1b => 0x01, // Escape
        0xffff => 0x53, // Delete
        0xff50 => 0x47, // Home
        0xff57 => 0x4f, // End
        0xff55 => 0x49, // Page_Up
        0xff56 => 0x51, // Page_Down
        0xff51 => 0x4b, // Left
        0xff53 => 0x4d, // Right
        0xff52 => 0x48, // Up
        0xff54 => 0x50, // Down
        // Modifiers
        0xffe1 | 0xffe2 => 0x2a, // Shift_L / Shift_R
        0xffe3 | 0xffe4 => 0x1d, // Control_L / Control_R
        0xffe7 | 0xffe8 => 0x5b, // Meta_L / Meta_R
        0xffe9 | 0xffea => 0x38, // Alt_L / Alt_R
        // F1–F12
        0xffbe => 0x3b, // F1
        0xffbf => 0x3c, // F2
        0xffc0 => 0x3d, // F3
        0xffc1 => 0x3e, // F4
        0xffc2 => 0x3f, // F5
        0xffc3 => 0x40, // F6
        0xffc4 => 0x41, // F7
        0xffc5 => 0x42, // F8
        0xffc6 => 0x43, // F9
        0xffc7 => 0x44, // F10
        0xffc8 => 0x57, // F11
        0xffc9 => 0x58, // F12
        _ => 0x00,
    }
}
