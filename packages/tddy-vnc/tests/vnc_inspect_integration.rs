//! Inspect mode for `VncClientState`: which sign-in methods a VNC server advertises, read without
//! signing in (`#host-desktops` 8/8).
//!
//! The server is an in-process RFB 3.8 server that advertises a configurable security-type list
//! (and, when it offers VeNCrypt, a configurable subtype list) and records what the client then
//! selected. It serves no sign-in at all: inspect has to have everything it needs before one would
//! begin.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pretty_assertions::assert_eq;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use tddy_screenshare::{AdvertisedMethod, ScreenSharingClient};
use tddy_vnc::vnc_client::VncClientState;

// ── RFB security type numbers ─────────────────────────────────────────────────

const NO_AUTHENTICATION: u8 = 1;
const VNC_AUTH: u8 = 2;
const TIGHT: u8 = 16;
const VENCRYPT: u8 = 19;
const APPLE_REMOTE_DESKTOP: u8 = 30;
const VENCRYPT_PLAIN: u32 = 256;
const VENCRYPT_TLS_PLAIN: u32 = 259;

/// How long an inspection may take before the test calls it a hang.
const AN_INSPECTION_FINISHES_WITHIN: Duration = Duration::from_secs(5);

// ── The advertising server ────────────────────────────────────────────────────

/// What the client did after the server advertised its security types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TheClient {
    /// It disconnected without selecting a security type.
    SelectedNothing,
    /// It selected this security type.
    Selected(u8),
}

struct AdvertisingServerBuilder {
    security_types: Vec<u8>,
    vencrypt_subtypes: Vec<u32>,
}

fn a_vnc_server_offering(security_types: &[u8]) -> AdvertisingServerBuilder {
    AdvertisingServerBuilder {
        security_types: security_types.to_vec(),
        vencrypt_subtypes: vec![],
    }
}

impl AdvertisingServerBuilder {
    fn with_vencrypt_subtypes(mut self, subtypes: &[u32]) -> Self {
        self.vencrypt_subtypes = subtypes.to_vec();
        self
    }

    async fn start(self) -> AdvertisingServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let selections = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&selections);
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let outcome =
                    advertise_to(stream, &self.security_types, &self.vencrypt_subtypes).await;
                recorded.lock().unwrap().push(outcome);
            }
        });
        AdvertisingServer {
            port,
            selections,
            _task: task,
        }
    }
}

struct AdvertisingServer {
    port: u16,
    selections: Arc<Mutex<Vec<TheClient>>>,
    _task: JoinHandle<()>,
}

impl AdvertisingServer {
    /// Inspect this server through the `ScreenSharingClient` surface, failing the test on a hang.
    async fn inspected(&self) -> Vec<AdvertisedMethod> {
        tokio::time::timeout(
            AN_INSPECTION_FINISHES_WITHIN,
            <VncClientState as ScreenSharingClient>::inspect("127.0.0.1", self.port),
        )
        .await
        .expect("inspecting the VNC server did not finish within the timeout")
        .expect("inspecting the VNC server succeeds")
    }

    /// What the one client that connected did after the advertisement.
    async fn what_the_client_did(&self) -> TheClient {
        tokio::time::timeout(AN_INSPECTION_FINISHES_WITHIN, async {
            loop {
                if let Some(outcome) = self.selections.lock().unwrap().first().copied() {
                    return outcome;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the client neither selected a security type nor disconnected")
    }
}

/// RFB 3.8 version exchange and the security-types message; then whatever the client selects.
/// VeNCrypt is answered with its version exchange and subtype list, and the connection is closed
/// once the client has read it — no subtype sign-in is ever served.
async fn advertise_to(
    mut stream: TcpStream,
    security_types: &[u8],
    vencrypt_subtypes: &[u32],
) -> TheClient {
    if stream.write_all(b"RFB 003.008\n").await.is_err() {
        return TheClient::SelectedNothing;
    }
    let mut client_version = [0u8; 12];
    if stream.read_exact(&mut client_version).await.is_err() {
        return TheClient::SelectedNothing;
    }
    let mut offer = vec![security_types.len() as u8];
    offer.extend_from_slice(security_types);
    if stream.write_all(&offer).await.is_err() {
        return TheClient::SelectedNothing;
    }
    let Ok(selected) = stream.read_u8().await else {
        return TheClient::SelectedNothing;
    };
    if selected == VENCRYPT {
        let _ = offer_vencrypt_subtypes(&mut stream, vencrypt_subtypes).await;
    }
    TheClient::Selected(selected)
}

async fn offer_vencrypt_subtypes(stream: &mut TcpStream, subtypes: &[u32]) -> std::io::Result<()> {
    stream.write_all(&[0, 2]).await?;
    let mut client_version = [0u8; 2];
    stream.read_exact(&mut client_version).await?;
    stream.write_all(&[0]).await?;
    let mut list = vec![subtypes.len() as u8];
    for subtype in subtypes {
        list.extend_from_slice(&subtype.to_be_bytes());
    }
    stream.write_all(&list).await?;
    // Whatever the client says next is a sign-in, which inspect must never start.
    let mut rest = Vec::new();
    stream.read_to_end(&mut rest).await?;
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn inspect_reports_the_security_types_a_vnc_server_advertises() {
    // Given
    let server = a_vnc_server_offering(&[VNC_AUTH, APPLE_REMOTE_DESKTOP, TIGHT])
        .start()
        .await;

    // When
    let methods = server.inspected().await;

    // Then — in the server's order, with the type tddy cannot sign in with kept by number
    assert_eq!(
        methods,
        vec![
            AdvertisedMethod::VncPassword,
            AdvertisedMethod::AppleRemoteDesktop,
            AdvertisedMethod::UnsupportedVnc {
                security_type: TIGHT
            },
        ]
    );
}

#[tokio::test]
async fn inspect_reports_no_authentication_for_a_server_that_asks_for_none() {
    // Given
    let server = a_vnc_server_offering(&[NO_AUTHENTICATION]).start().await;

    // When
    let methods = server.inspected().await;

    // Then
    assert_eq!(methods, vec![AdvertisedMethod::None]);
}

#[tokio::test]
async fn inspect_reports_the_vencrypt_subtypes_a_vnc_server_offers() {
    // Given
    let server = a_vnc_server_offering(&[VENCRYPT])
        .with_vencrypt_subtypes(&[VENCRYPT_PLAIN, VENCRYPT_TLS_PLAIN])
        .start()
        .await;

    // When
    let methods = server.inspected().await;

    // Then — VeNCrypt itself is not a method an operator can choose; its subtypes are
    assert_eq!(
        methods,
        vec![
            AdvertisedMethod::VencryptPlain,
            AdvertisedMethod::VencryptTlsPlain,
        ]
    );
}

#[tokio::test]
async fn inspect_selects_no_sign_in_method() {
    // Given
    let server = a_vnc_server_offering(&[VNC_AUTH, APPLE_REMOTE_DESKTOP])
        .start()
        .await;

    // When
    server.inspected().await;

    // Then — selecting one would start a sign-in nobody supplied a credential for
    assert_eq!(
        server.what_the_client_did().await,
        TheClient::SelectedNothing
    );
}
