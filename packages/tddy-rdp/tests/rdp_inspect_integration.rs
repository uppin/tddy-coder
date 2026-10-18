//! Inspect mode for `RdpClient`: which sign-in an RDP desktop requires, read from its X.224
//! negotiation response without signing in (`#host-desktops` 8/8).
//!
//! The desktop is a real in-process `ironrdp-server`, configured as the NLA suite configures it:
//! `with_hybrid` supports only CredSSP (`HYBRID` / `HYBRID_EX`), `with_tls` supports only TLS. So
//! what inspect reports is what IronRDP's own server side negotiates, not a mock's idea of it.
//!
//! `RdpServer` uses `Rc` internally (its futures are `!Send`), so each desktop runs on a dedicated
//! OS thread with a current-thread Tokio runtime and a `LocalSet`.

use std::time::Duration;

use async_trait::async_trait;
use ironrdp_server::{DesktopSize, RdpServer, RdpServerDisplay, RdpServerDisplayUpdates};
use pretty_assertions::assert_eq;

use tddy_rdp::rdp_client::RdpClient;
use tddy_screenshare::{AdvertisedMethod, ScreenSharingClient};

/// How long an inspection may take before the test calls it a hang.
const AN_INSPECTION_FINISHES_WITHIN: Duration = Duration::from_secs(10);

// ── A desktop that never shows anything ───────────────────────────────────────

/// Inspect never gets past negotiation, so the desktop has nothing to display.
struct NothingShown;

#[async_trait]
impl RdpServerDisplayUpdates for NothingShown {
    async fn next_update(&mut self) -> anyhow::Result<Option<ironrdp_server::DisplayUpdate>> {
        std::future::pending().await
    }
}

struct BlankDisplay;

#[async_trait]
impl RdpServerDisplay for BlankDisplay {
    async fn size(&mut self) -> DesktopSize {
        DesktopSize {
            width: 64,
            height: 64,
        }
    }

    async fn updates(&mut self) -> anyhow::Result<Box<dyn RdpServerDisplayUpdates>> {
        Ok(Box::new(NothingShown))
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
    /// CredSSP over TLS only (`HYBRID` / `HYBRID_EX`): the Windows default with NLA on.
    RequiresNla,
    /// TLS only.
    OffersOnlyTls,
}

struct RunningDesktop {
    port: u16,
    _thread: std::thread::JoinHandle<()>,
}

fn a_desktop_that_requires_nla() -> RunningDesktop {
    a_desktop_with(Security::RequiresNla)
}

fn a_desktop_that_offers_only_tls() -> RunningDesktop {
    a_desktop_with(Security::OffersOnlyTls)
}

fn a_desktop_with(security: Security) -> RunningDesktop {
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    std_listener.set_nonblocking(true).unwrap();
    let port = std_listener.local_addr().unwrap().port();
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let identity = a_self_signed_tls_identity();

    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async move {
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            let builder = RdpServer::builder().with_addr(addr);
            let builder = match security {
                Security::RequiresNla => {
                    builder.with_hybrid(identity.acceptor, identity.public_key)
                }
                Security::OffersOnlyTls => builder.with_tls(identity.acceptor),
            };
            let mut server = builder
                .with_no_input()
                .with_display_handler(BlankDisplay)
                .build();

            while let Ok((stream, _)) = listener.accept().await {
                let _ = server.run_connection(stream).await;
            }
        });
    });

    RunningDesktop {
        port,
        _thread: thread,
    }
}

impl RunningDesktop {
    /// Inspect this desktop through the `ScreenSharingClient` surface, failing the test on a hang.
    async fn inspected(&self) -> Vec<AdvertisedMethod> {
        tokio::time::timeout(
            AN_INSPECTION_FINISHES_WITHIN,
            RdpClient::inspect("127.0.0.1", self.port),
        )
        .await
        .unwrap_or_else(|_| {
            panic!("inspecting the desktop did not finish within {AN_INSPECTION_FINISHES_WITHIN:?}")
        })
        .expect("inspecting the RDP desktop succeeds")
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn inspect_reports_nla_for_a_server_that_requires_it() {
    // Given
    let desktop = a_desktop_that_requires_nla();

    // When
    let methods = desktop.inspected().await;

    // Then
    assert_eq!(methods, vec![AdvertisedMethod::RdpNla]);
}

#[tokio::test]
async fn inspect_reports_tls_for_a_server_that_offers_only_tls() {
    // Given
    let desktop = a_desktop_that_offers_only_tls();

    // When
    let methods = desktop.inspected().await;

    // Then
    assert_eq!(methods, vec![AdvertisedMethod::RdpTls]);
}
