//! Integration tests for `RdpClient` signing in to desktops that require Network Level
//! Authentication (CredSSP over TLS, the `HYBRID` security protocol) and to desktops that
//! offer only TLS.
//!
//! The desktop is a real in-process `ironrdp-server`: configured with `with_hybrid` it runs
//! the CredSSP acceptor (NTLM through `sspi`) against the account it is given, so the client
//! is exercised against IronRDP's own server side rather than a mock of it.
//!
//! `RdpServer` uses `Rc` internally (its futures are `!Send`), so each desktop runs on a
//! dedicated OS thread with a current-thread Tokio runtime and a `LocalSet`.

use std::num::{NonZeroU16, NonZeroUsize};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use ironrdp_server::{
    BitmapUpdate, DesktopSize, DisplayUpdate, PixelFormat, RdpServer, RdpServerDisplay,
    RdpServerDisplayUpdates,
};
use pretty_assertions::assert_eq;
use tokio::sync::mpsc;

use tddy_rdp::rdp_client::RdpClient;
use tddy_screenshare::client::{Credentials, ScreenSharingClient as _};

/// How long a sign-in may take before the test calls it a hang.
///
/// A loopback NTLM exchange completes in well under a second; a client that is stuck
/// waiting on a desktop that refused it must fail the test here rather than block forever.
const A_SIGN_IN_FINISHES_WITHIN: Duration = Duration::from_secs(10);

/// How long a pushed frame may take to reach the client's framebuffer.
const A_FRAME_ARRIVES_WITHIN: Duration = Duration::from_secs(5);

// ── The account on the desktop ────────────────────────────────────────────────

const USERNAME: &str = "alice";
const DOMAIN: &str = "CORP";
const PASSWORD: &str = "correct horse battery staple";

/// The credentials of the account the test desktop knows.
fn alices_credentials() -> Credentials {
    Credentials {
        username: Some(USERNAME.into()),
        domain: Some(DOMAIN.into()),
        password: Some(PASSWORD.into()),
    }
}

/// Alice's username and domain with a password the desktop does not know.
fn alices_credentials_with_a_wrong_password() -> Credentials {
    Credentials {
        password: Some("not the password".into()),
        ..alices_credentials()
    }
}

fn the_desktop_account() -> ironrdp_server::Credentials {
    ironrdp_server::Credentials {
        username: USERNAME.into(),
        password: PASSWORD.into(),
        domain: Some(DOMAIN.into()),
    }
}

// ── QR-coded frames ───────────────────────────────────────────────────────────

/// Render `"frame:N"` as a 256×256 RGBA pixel buffer.
fn generate_qr_rgba(frame: u64) -> Vec<u8> {
    let content = format!("frame:{frame}");
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

/// Decode a QR code from an RGBA framebuffer using the R channel as greyscale.
fn decode_qr(rgba: &[u8], width: u32, height: u32) -> Option<String> {
    let w = width as usize;
    let mut img = rqrr::PreparedImage::prepare_from_greyscale(w, height as usize, |x, y| {
        rgba[(y * w + x) * 4]
    });
    img.detect_grids()
        .into_iter()
        .find_map(|g| g.decode().ok().map(|(_, s)| s))
}

struct QrDisplayUpdates {
    frame_rx: mpsc::Receiver<u64>,
}

#[async_trait]
impl RdpServerDisplayUpdates for QrDisplayUpdates {
    async fn next_update(&mut self) -> anyhow::Result<Option<DisplayUpdate>> {
        match self.frame_rx.recv().await {
            Some(n) => Ok(Some(DisplayUpdate::Bitmap(BitmapUpdate {
                x: 0,
                y: 0,
                width: NonZeroU16::new(256).unwrap(),
                height: NonZeroU16::new(256).unwrap(),
                format: PixelFormat::RgbA32,
                data: Bytes::from(generate_qr_rgba(n)),
                stride: NonZeroUsize::new(256 * 4).unwrap(),
            }))),
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

// ── TLS identity ──────────────────────────────────────────────────────────────

/// Install the aws-lc-rs crypto provider before any TLS operation (idempotent).
fn ensure_crypto_provider() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let _ = ironrdp_server::tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
            .install_default();
    });
}

/// A self-signed TLS identity: the acceptor, and the raw public key CredSSP binds to.
///
/// CredSSP proves the client reached the same endpoint it authenticates to by echoing the
/// server's TLS public key, so `with_hybrid` needs the key the certificate carries.
struct TlsIdentity {
    acceptor: ironrdp_server::tokio_rustls::TlsAcceptor,
    public_key: Vec<u8>,
}

fn a_self_signed_tls_identity() -> TlsIdentity {
    use ironrdp_server::tokio_rustls::rustls;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};

    ensure_crypto_provider();

    let cert = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into(), "localhost".into()])
        .expect("rcgen cert generation failed");
    let public_key = cert.key_pair.public_key_raw().to_vec();
    let cert_chain = vec![CertificateDer::from(cert.cert.der().to_vec())];
    let key =
        PrivateKeyDer::try_from(cert.key_pair.serialize_der()).expect("valid private key DER");

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)
        .expect("valid TLS config");

    TlsIdentity {
        acceptor: ironrdp_server::tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(config)),
        public_key,
    }
}

// ── The desktop under test ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum Security {
    /// CredSSP over TLS (`HYBRID` / `HYBRID_EX`): the Windows default with NLA on.
    RequiresNla,
    /// TLS only: the account is checked from the Client Info PDU after the connection.
    OffersOnlyTls,
}

/// Builds a Windows-like desktop that knows Alice's account.
struct DesktopBuilder {
    security: Security,
}

fn a_desktop_that_requires_nla() -> DesktopBuilder {
    DesktopBuilder {
        security: Security::RequiresNla,
    }
}

fn a_desktop_that_offers_only_tls() -> DesktopBuilder {
    DesktopBuilder {
        security: Security::OffersOnlyTls,
    }
}

impl DesktopBuilder {
    async fn start(self) -> RunningDesktop {
        let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let port = std_listener.local_addr().unwrap().port();
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

        let identity = a_self_signed_tls_identity();
        let security = self.security;
        let (frame_tx, frame_rx) = mpsc::channel::<u64>(32);

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
                let builder = RdpServer::builder().with_addr(addr);
                let builder = match security {
                    Security::RequiresNla => {
                        builder.with_hybrid(identity.acceptor, identity.public_key)
                    }
                    Security::OffersOnlyTls => builder.with_tls(identity.acceptor),
                };
                let mut server = builder
                    .with_no_input()
                    .with_display_handler(display)
                    .build();
                server.set_credentials(Some(the_desktop_account()));

                while let Ok((stream, _)) = listener.accept().await {
                    let _ = server.run_connection(stream).await;
                }
            });
        });

        RunningDesktop {
            port,
            frame_tx,
            _thread: thread,
        }
    }
}

struct RunningDesktop {
    port: u16,
    frame_tx: mpsc::Sender<u64>,
    _thread: std::thread::JoinHandle<()>,
}

impl RunningDesktop {
    /// Sign in with `credentials`, failing the test if the attempt neither succeeds nor fails
    /// within [`A_SIGN_IN_FINISHES_WITHIN`].
    async fn sign_in_with(&self, credentials: &Credentials) -> anyhow::Result<RdpClient> {
        tokio::time::timeout(
            A_SIGN_IN_FINISHES_WITHIN,
            RdpClient::connect("127.0.0.1", self.port, credentials),
        )
        .await
        .unwrap_or_else(|_| {
            panic!(
                "signing in did not finish within {A_SIGN_IN_FINISHES_WITHIN:?}: \
                 the client hung instead of connecting or failing"
            )
        })
    }

    /// Push a QR-coded frame (encoding `"frame:{n}"`) to the connected client.
    fn shows_frame(&self, n: u64) {
        self.frame_tx
            .try_send(n)
            .expect("the desktop accepts a frame to show");
    }
}

/// Drive the client until its framebuffer decodes to a QR code.
async fn the_frame_the_client_sees(client: &mut RdpClient) -> String {
    tokio::time::timeout(A_FRAME_ARRIVES_WITHIN, async {
        loop {
            let (w, h) = client.framebuffer_dimensions();
            if let Some(qr) = decode_qr(client.get_rgba_frame(), w, h) {
                return qr;
            }
            let _ = tokio::time::timeout(Duration::from_millis(50), client.poll_events()).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!("the desktop's frame did not reach the client within {A_FRAME_ARRIVES_WITHIN:?}")
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn signs_in_to_a_server_that_requires_nla_with_username_domain_and_password() {
    // Given
    let desktop = a_desktop_that_requires_nla().start().await;

    // When
    let mut client = desktop
        .sign_in_with(&alices_credentials())
        .await
        .expect("signing in to an NLA desktop with the right account succeeds");
    desktop.shows_frame(1);

    // Then
    assert_eq!(the_frame_the_client_sees(&mut client).await, "frame:1");
}

#[tokio::test]
async fn a_wrong_password_on_an_nla_server_fails_to_connect_as_an_authentication_error() {
    // Given
    let desktop = a_desktop_that_requires_nla().start().await;

    // When
    let refusal = desktop
        .sign_in_with(&alices_credentials_with_a_wrong_password())
        .await
        .err()
        .expect("signing in with a wrong password fails");

    // Then
    let message = format!("{refusal:#}");
    assert!(
        message.to_lowercase().contains("authentication"),
        "expected the refusal to name an authentication failure, got: {message}"
    );
}

#[tokio::test]
async fn still_signs_in_to_a_server_that_offers_only_tls() {
    // Given
    let desktop = a_desktop_that_offers_only_tls().start().await;

    // When
    let mut client = desktop
        .sign_in_with(&alices_credentials())
        .await
        .expect("signing in to a TLS-only desktop with the right account succeeds");
    desktop.shows_frame(1);

    // Then
    assert_eq!(the_frame_the_client_sees(&mut client).await, "frame:1");
}
