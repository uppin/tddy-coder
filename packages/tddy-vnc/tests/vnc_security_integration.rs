//! Sign-in tests for `VncClientState` against an in-process RFB 3.8 server whose security-type
//! list is configurable.
//!
//! The fake server performs the server side of each exchange with known credentials, then sends
//! ServerInit and a solid-colour framebuffer update — so "signs in" means a frame streamed, not
//! only a SecurityResult. Each exchange is verified on the server side independently of the
//! client's code: VNC Authentication against a DES known-answer vector, VeNCrypt Plain and
//! TLSPlain by comparing the received username and password, Apple Remote Desktop by computing
//! the Diffie-Hellman secret from the server's own private key and decrypting the credentials.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};
use md5::{Digest, Md5};
use num_bigint::BigUint;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;

use tddy_screenshare::{AuthenticationRefused, Credentials, ScreenSharingClient};
use tddy_vnc::vnc_client::VncClientState;

// ── RFB security type numbers ─────────────────────────────────────────────────

const VNC_AUTH: u8 = 2;
const VENCRYPT: u8 = 19;
const APPLE_REMOTE_DESKTOP: u8 = 30;
const VENCRYPT_PLAIN: u32 = 256;
const VENCRYPT_TLS_PLAIN: u32 = 259;
const X509_PLAIN: u32 = 262;

// ── RFB message sizes ─────────────────────────────────────────────────────────

const RFB_VERSION_LEN: usize = 12;
const VENCRYPT_CHOICE_LEN: usize = 4;

/// Client-to-server message types, and the bytes each carries after its type byte.
const MSG_SET_PIXEL_FORMAT: u8 = 0;
const SET_PIXEL_FORMAT_BODY_LEN: usize = 19;
const MSG_SET_ENCODINGS: u8 = 2;
const ENCODING_LEN: usize = 4;
const MSG_FRAMEBUFFER_UPDATE_REQUEST: u8 = 3;
const FRAMEBUFFER_UPDATE_REQUEST_BODY_LEN: usize = 9;
const MSG_KEY_EVENT: u8 = 4;
const KEY_EVENT_BODY_LEN: usize = 7;
const MSG_POINTER_EVENT: u8 = 5;
const POINTER_EVENT_BODY_LEN: usize = 5;

// ── The fake server's account ─────────────────────────────────────────────────

const USERNAME: &str = "alice";
const PASSWORD: &str = "secret";

/// The VNC Authentication challenge the fake server always sends.
const VNC_AUTH_CHALLENGE: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

/// DES-ECB of [`VNC_AUTH_CHALLENGE`] under the key for [`PASSWORD`] ("secret", zero-padded to
/// eight bytes, each byte bit-reversed per RFB §7.2.2), computed independently with
/// `openssl enc -des-ecb -K cea6c64ea62e0000 -nopad`.
const VNC_AUTH_RESPONSE_FOR_PASSWORD: [u8; 16] = [
    0xee, 0x22, 0x53, 0x9f, 0x33, 0xa5, 0x98, 0x3e, 0xc1, 0x2f, 0x9c, 0x2e, 0xdb, 0xc9, 0x95, 0xdd,
];

const WRONG_PASSWORD_REASON: &str = "the password is not right for this desktop";

// ── Apple Remote Desktop Diffie-Hellman parameters ────────────────────────────

const ARD_GENERATOR: u16 = 2;
const ARD_KEY_LENGTH: usize = 128;
const ARD_FIELD_LENGTH: usize = 64;
const AES_BLOCK_LENGTH: usize = 16;
const ARD_CREDENTIALS_LENGTH: usize = 2 * ARD_FIELD_LENGTH;

/// The 1024-bit MODP prime of RFC 2409 §6.2 (Oakley Group 2), the size macOS Screen Sharing uses.
const ARD_PRIME_HEX: &str = concat!(
    "FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E088A67CC74",
    "020BBEA63B139B22514A08798E3404DDEF9519B3CD3A431B302B0A6DF25F1437",
    "4FE1356D6D51C245E485B576625E7EC6F44C42E9A637ED6B0BFF5CB6F406B7ED",
    "EE386BFB5A899FA5AE9F24117C4B1FE649286651ECE65381FFFFFFFFFFFFFFFF",
);

/// The fake server's fixed Diffie-Hellman private key.
const ARD_SERVER_PRIVATE_KEY: [u8; 32] = [0x5a; 32];

// ── Framebuffer ───────────────────────────────────────────────────────────────

const SCREEN_WIDTH: u16 = 4;
const SCREEN_HEIGHT: u16 = 4;

/// The colour of every pixel the fake server sends, as BGRA on the wire.
const SCREEN_BGRA: [u8; 4] = [0x30, 0x20, 0x10, 0xff];
/// The same colour as the client's RGBA framebuffer holds it.
const SCREEN_RGBA: [u8; 4] = [0x10, 0x20, 0x30, 0xff];

/// 32bpp little-endian true-colour, red at bits 16-23 (BGRA byte order).
const BGRA_PIXEL_FORMAT: [u8; 16] = [32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0];

const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5);
const FRAME_TIMEOUT: Duration = Duration::from_secs(5);

// ── Fake server ───────────────────────────────────────────────────────────────

/// The RFB version the fake server announces.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ServerVersion {
    V33,
    V37,
    V38,
    /// Apple's `RFB 003.889`, which means 3.8.
    Apple889,
}

impl ServerVersion {
    fn announcement(self) -> &'static [u8; RFB_VERSION_LEN] {
        match self {
            Self::V33 => b"RFB 003.003\n",
            Self::V37 => b"RFB 003.007\n",
            Self::V38 => b"RFB 003.008\n",
            Self::Apple889 => b"RFB 003.889\n",
        }
    }

    /// From 3.7 the server offers a list and the client chooses; in 3.3 the server decides.
    fn client_chooses_security_type(self) -> bool {
        self != Self::V33
    }

    /// From 3.8 a failed SecurityResult carries a reason.
    fn explains_refusals(self) -> bool {
        matches!(self, Self::V38 | Self::Apple889)
    }
}

#[derive(Clone)]
struct ServerConfig {
    version: ServerVersion,
    security_types: Vec<u8>,
    vencrypt_subtypes: Vec<u32>,
    tls: TlsAcceptor,
}

#[derive(Default)]
struct ServerRecord {
    client_version: Option<[u8; RFB_VERSION_LEN]>,
    selected_security_type: Option<u8>,
    selected_vencrypt_subtype: Option<u32>,
    /// What the client sent after the VeNCrypt subtype list, once it chose or closed.
    vencrypt_choice_bytes: Option<Vec<u8>>,
}

struct FakeRfbServerBuilder {
    version: ServerVersion,
    security_types: Vec<u8>,
    vencrypt_subtypes: Vec<u32>,
}

fn a_vnc_server() -> FakeRfbServerBuilder {
    FakeRfbServerBuilder {
        version: ServerVersion::V38,
        security_types: vec![VNC_AUTH],
        vencrypt_subtypes: vec![],
    }
}

impl FakeRfbServerBuilder {
    fn with_version(mut self, version: ServerVersion) -> Self {
        self.version = version;
        self
    }

    fn offering(mut self, security_types: &[u8]) -> Self {
        self.security_types = security_types.to_vec();
        self
    }

    fn with_vencrypt_subtypes(mut self, subtypes: &[u32]) -> Self {
        self.vencrypt_subtypes = subtypes.to_vec();
        self
    }

    async fn start(self) -> FakeRfbServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let record = Arc::new(Mutex::new(ServerRecord::default()));
        let config = ServerConfig {
            version: self.version,
            security_types: self.security_types,
            vencrypt_subtypes: self.vencrypt_subtypes,
            tls: a_self_signed_tls_acceptor(),
        };
        let task_record = record.clone();
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(serve_one_client(
                    stream,
                    config.clone(),
                    task_record.clone(),
                ));
            }
        });
        FakeRfbServer {
            port,
            record,
            _task: task,
        }
    }
}

/// A TLS acceptor holding a freshly generated self-signed certificate for `localhost`.
fn a_self_signed_tls_acceptor() -> TlsAcceptor {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])
        .expect("self-signed certificate is generated");
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
        certified.key_pair.serialize_der(),
    ));
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("TLS protocol versions are supported")
    .with_no_client_auth()
    .with_single_cert(vec![certified.cert.der().clone()], key)
    .expect("TLS server config accepts the certificate");
    TlsAcceptor::from(Arc::new(config))
}

struct FakeRfbServer {
    port: u16,
    record: Arc<Mutex<ServerRecord>>,
    _task: JoinHandle<()>,
}

impl FakeRfbServer {
    fn selected_security_type(&self) -> Option<u8> {
        self.record.lock().unwrap().selected_security_type
    }

    fn selected_vencrypt_subtype(&self) -> Option<u32> {
        self.record.lock().unwrap().selected_vencrypt_subtype
    }

    fn client_version(&self) -> Option<[u8; RFB_VERSION_LEN]> {
        self.record.lock().unwrap().client_version
    }

    /// The bytes the client sent after the VeNCrypt subtype list, waiting for it to choose or
    /// close the connection.
    async fn vencrypt_choice_bytes(&self) -> Vec<u8> {
        tokio::time::timeout(SIGN_IN_TIMEOUT, async {
            loop {
                if let Some(bytes) = self.record.lock().unwrap().vencrypt_choice_bytes.clone() {
                    return bytes;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the client neither chose a VeNCrypt subtype nor closed the connection")
    }
}

enum SignInOutcome {
    Accepted,
    Refused(&'static str),
}

/// Whether the account the client presented is the fake server's.
fn is_the_account(username: &[u8], password: &[u8]) -> bool {
    username == USERNAME.as_bytes() && password == PASSWORD.as_bytes()
}

async fn serve_one_client(
    mut stream: TcpStream,
    config: ServerConfig,
    record: Arc<Mutex<ServerRecord>>,
) -> std::io::Result<()> {
    stream.write_all(config.version.announcement()).await?;
    let mut client_version = [0u8; RFB_VERSION_LEN];
    stream.read_exact(&mut client_version).await?;
    record.lock().unwrap().client_version = Some(client_version);

    let selected = offer_security_types(&mut stream, &config).await?;
    record.lock().unwrap().selected_security_type = Some(selected);
    let version = config.version;

    match selected {
        VNC_AUTH => {
            let outcome = serve_vnc_authentication(&mut stream).await?;
            finish_sign_in(stream, version, outcome).await
        }
        APPLE_REMOTE_DESKTOP => {
            let outcome = serve_apple_remote_desktop(&mut stream).await?;
            finish_sign_in(stream, version, outcome).await
        }
        VENCRYPT => {
            let subtype =
                negotiate_vencrypt_subtype(&mut stream, &config.vencrypt_subtypes, &record).await?;
            record.lock().unwrap().selected_vencrypt_subtype = subtype;
            match subtype {
                Some(VENCRYPT_PLAIN) => {
                    let outcome = serve_plain(&mut stream).await?;
                    finish_sign_in(stream, version, outcome).await
                }
                Some(VENCRYPT_TLS_PLAIN) => {
                    stream.write_all(&[1]).await?;
                    let mut tls = config.tls.accept(stream).await?;
                    let outcome = serve_plain(&mut tls).await?;
                    finish_sign_in(tls, version, outcome).await
                }
                _ => stream.shutdown().await,
            }
        }
        _ => stream.shutdown().await,
    }
}

/// Offer the security types the way the announced version does; return the one that is used.
async fn offer_security_types(
    stream: &mut TcpStream,
    config: &ServerConfig,
) -> std::io::Result<u8> {
    if config.version.client_chooses_security_type() {
        let mut offer = vec![config.security_types.len() as u8];
        offer.extend_from_slice(&config.security_types);
        stream.write_all(&offer).await?;
        stream.read_u8().await
    } else {
        let decided = config.security_types[0];
        stream.write_all(&u32::from(decided).to_be_bytes()).await?;
        Ok(decided)
    }
}

/// Send the SecurityResult, then either serve the desktop or close with the reason.
async fn finish_sign_in<S>(
    mut stream: S,
    version: ServerVersion,
    outcome: SignInOutcome,
) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match outcome {
        SignInOutcome::Accepted => {
            stream.write_all(&0u32.to_be_bytes()).await?;
            serve_desktop(stream).await
        }
        SignInOutcome::Refused(reason) => {
            let mut result = 1u32.to_be_bytes().to_vec();
            if version.explains_refusals() {
                result.extend_from_slice(&(reason.len() as u32).to_be_bytes());
                result.extend_from_slice(reason.as_bytes());
            }
            stream.write_all(&result).await?;
            stream.shutdown().await
        }
    }
}

fn accepted_when(account_matches: bool) -> SignInOutcome {
    match account_matches {
        true => SignInOutcome::Accepted,
        false => SignInOutcome::Refused(WRONG_PASSWORD_REASON),
    }
}

async fn serve_vnc_authentication(stream: &mut TcpStream) -> std::io::Result<SignInOutcome> {
    stream.write_all(&VNC_AUTH_CHALLENGE).await?;
    let mut response = [0u8; 16];
    stream.read_exact(&mut response).await?;
    Ok(accepted_when(response == VNC_AUTH_RESPONSE_FOR_PASSWORD))
}

/// VeNCrypt 0.2: version exchange, then the subtype list; returns the subtype the client chose,
/// or `None` when the client closed the connection instead. Whatever the client sent after the
/// list is recorded.
async fn negotiate_vencrypt_subtype(
    stream: &mut TcpStream,
    subtypes: &[u32],
    record: &Mutex<ServerRecord>,
) -> std::io::Result<Option<u32>> {
    stream.write_all(&[0, 2]).await?;
    let mut client_vencrypt_version = [0u8; 2];
    stream.read_exact(&mut client_vencrypt_version).await?;
    let version_accepted = client_vencrypt_version == [0, 2];
    stream.write_all(&[u8::from(!version_accepted)]).await?;

    let mut list = vec![subtypes.len() as u8];
    for subtype in subtypes {
        list.extend_from_slice(&subtype.to_be_bytes());
    }
    stream.write_all(&list).await?;

    let mut choice = Vec::new();
    let mut buf = [0u8; VENCRYPT_CHOICE_LEN];
    while choice.len() < VENCRYPT_CHOICE_LEN {
        let read = stream
            .read(&mut buf[..VENCRYPT_CHOICE_LEN - choice.len()])
            .await?;
        if read == 0 {
            break;
        }
        choice.extend_from_slice(&buf[..read]);
    }
    record.lock().unwrap().vencrypt_choice_bytes = Some(choice.clone());
    Ok(<[u8; VENCRYPT_CHOICE_LEN]>::try_from(choice)
        .ok()
        .map(u32::from_be_bytes))
}

/// VeNCrypt Plain: username length, password length, username, password.
async fn serve_plain<S>(stream: &mut S) -> std::io::Result<SignInOutcome>
where
    S: AsyncRead + Unpin,
{
    let username_len = stream.read_u32().await? as usize;
    let password_len = stream.read_u32().await? as usize;
    let mut username = vec![0u8; username_len];
    stream.read_exact(&mut username).await?;
    let mut password = vec![0u8; password_len];
    stream.read_exact(&mut password).await?;
    Ok(accepted_when(is_the_account(&username, &password)))
}

/// Apple Remote Desktop (type 30): send generator, key length, prime and the server's public
/// key; receive 128 AES-128-ECB-encrypted credential bytes and the client's public key. The AES
/// key is the MD5 of the shared secret, written big-endian in `ARD_KEY_LENGTH` bytes.
async fn serve_apple_remote_desktop(stream: &mut TcpStream) -> std::io::Result<SignInOutcome> {
    let prime = BigUint::parse_bytes(ARD_PRIME_HEX.as_bytes(), 16).expect("prime parses");
    let private_key = BigUint::from_bytes_be(&ARD_SERVER_PRIVATE_KEY);
    let server_public_key = BigUint::from(ARD_GENERATOR).modpow(&private_key, &prime);

    let mut parameters = Vec::new();
    parameters.extend_from_slice(&ARD_GENERATOR.to_be_bytes());
    parameters.extend_from_slice(&(ARD_KEY_LENGTH as u16).to_be_bytes());
    parameters.extend_from_slice(&padded_be(&prime));
    parameters.extend_from_slice(&padded_be(&server_public_key));
    stream.write_all(&parameters).await?;

    let mut encrypted_credentials = [0u8; ARD_CREDENTIALS_LENGTH];
    stream.read_exact(&mut encrypted_credentials).await?;
    let mut client_public_key = vec![0u8; ARD_KEY_LENGTH];
    stream.read_exact(&mut client_public_key).await?;

    let shared_secret = BigUint::from_bytes_be(&client_public_key).modpow(&private_key, &prime);
    let aes_key = Md5::digest(padded_be(&shared_secret));
    let credentials = aes128_ecb_decrypt(&aes_key, &encrypted_credentials);

    Ok(accepted_when(is_the_account(
        null_terminated(&credentials[..ARD_FIELD_LENGTH]),
        null_terminated(&credentials[ARD_FIELD_LENGTH..]),
    )))
}

fn padded_be(value: &BigUint) -> Vec<u8> {
    let bytes = value.to_bytes_be();
    let mut padded = vec![0u8; ARD_KEY_LENGTH - bytes.len()];
    padded.extend_from_slice(&bytes);
    padded
}

fn aes128_ecb_decrypt(
    key: &[u8],
    ciphertext: &[u8; ARD_CREDENTIALS_LENGTH],
) -> [u8; ARD_CREDENTIALS_LENGTH] {
    let cipher = aes::Aes128::new(GenericArray::from_slice(key));
    let mut plaintext = *ciphertext;
    for block in plaintext.chunks_exact_mut(AES_BLOCK_LENGTH) {
        cipher.decrypt_block(GenericArray::from_mut_slice(block));
    }
    plaintext
}

fn null_terminated(field: &[u8]) -> &[u8] {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    &field[..end]
}

/// ClientInit → ServerInit, then answer every FramebufferUpdateRequest with one solid frame.
async fn serve_desktop<S>(mut stream: S) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let _shared = stream.read_u8().await?;

    let name = b"FakeSecureVNC";
    let mut init = Vec::new();
    init.extend_from_slice(&SCREEN_WIDTH.to_be_bytes());
    init.extend_from_slice(&SCREEN_HEIGHT.to_be_bytes());
    init.extend_from_slice(&BGRA_PIXEL_FORMAT);
    init.extend_from_slice(&(name.len() as u32).to_be_bytes());
    init.extend_from_slice(name);
    stream.write_all(&init).await?;

    loop {
        match stream.read_u8().await? {
            MSG_SET_PIXEL_FORMAT => skip(&mut stream, SET_PIXEL_FORMAT_BODY_LEN).await?,
            MSG_SET_ENCODINGS => {
                let _padding = stream.read_u8().await?;
                let count = stream.read_u16().await? as usize;
                skip(&mut stream, count * ENCODING_LEN).await?;
            }
            MSG_FRAMEBUFFER_UPDATE_REQUEST => {
                skip(&mut stream, FRAMEBUFFER_UPDATE_REQUEST_BODY_LEN).await?;
                stream.write_all(&a_solid_framebuffer_update()).await?;
            }
            MSG_KEY_EVENT => skip(&mut stream, KEY_EVENT_BODY_LEN).await?,
            MSG_POINTER_EVENT => skip(&mut stream, POINTER_EVENT_BODY_LEN).await?,
            _ => return Ok(()),
        }
    }
}

async fn skip<S>(stream: &mut S, len: usize) -> std::io::Result<()>
where
    S: AsyncRead + Unpin,
{
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    Ok(())
}

fn a_solid_framebuffer_update() -> Vec<u8> {
    let mut msg = vec![0u8, 0];
    msg.extend_from_slice(&1u16.to_be_bytes());
    msg.extend_from_slice(&0u16.to_be_bytes());
    msg.extend_from_slice(&0u16.to_be_bytes());
    msg.extend_from_slice(&SCREEN_WIDTH.to_be_bytes());
    msg.extend_from_slice(&SCREEN_HEIGHT.to_be_bytes());
    msg.extend_from_slice(&0i32.to_be_bytes());
    for _ in 0..(SCREEN_WIDTH as usize * SCREEN_HEIGHT as usize) {
        msg.extend_from_slice(&SCREEN_BGRA);
    }
    msg
}

// ── Credentials ───────────────────────────────────────────────────────────────

fn a_password_only(password: &str) -> Credentials {
    Credentials {
        password: Some(password.to_owned()),
        ..Credentials::default()
    }
}

fn a_wrong_username_and_password() -> Credentials {
    Credentials {
        username: Some("mallory".to_owned()),
        password: Some("not-the-password".to_owned()),
        ..Credentials::default()
    }
}

fn a_username_and_password() -> Credentials {
    Credentials {
        username: Some(USERNAME.to_owned()),
        password: Some(PASSWORD.to_owned()),
        ..Credentials::default()
    }
}

// ── Client steps ──────────────────────────────────────────────────────────────

/// Connect through the `ScreenSharingClient` surface, failing the test if sign-in hangs.
async fn sign_in(
    server: &FakeRfbServer,
    credentials: &Credentials,
) -> anyhow::Result<VncClientState> {
    tokio::time::timeout(
        SIGN_IN_TIMEOUT,
        <VncClientState as ScreenSharingClient>::connect("127.0.0.1", server.port, credentials),
    )
    .await
    .expect("VNC sign-in did not finish within the timeout")
}

/// Request a full frame and return the colour of the first pixel the server drew.
///
/// The client's framebuffer starts out all zero (transparent black); the first pixel that is
/// anything else is the first drawn one, and it is the caller's to compare.
async fn first_pixel_of_a_frame(client: &mut VncClientState) -> [u8; 4] {
    client
        .request_frame_update(false)
        .await
        .expect("frame update request is sent");
    tokio::time::timeout(FRAME_TIMEOUT, async {
        loop {
            client.poll_events().await.expect("VNC events are readable");
            if let Some(pixel) = client.get_rgba_frame().first_chunk::<4>() {
                if *pixel != [0; 4] {
                    return *pixel;
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("no frame from the VNC server within the timeout")
}

fn error_text(err: &anyhow::Error) -> String {
    format!("{err:#}")
}

/// The error lists exactly `types` as the security types the server offered.
fn assert_error_names_types(err: &anyhow::Error, types: &[u8]) {
    let message = error_text(err);
    let listed = format!("{types:?}");
    assert!(
        message.contains(&listed),
        "error does not list the offered types {listed}: {message}"
    );
}

fn assert_error_carries_the_servers_reason(err: &anyhow::Error) {
    let message = error_text(err);
    assert!(
        message.contains(WRONG_PASSWORD_REASON),
        "error does not carry the server's reason: {message}"
    );
}

// ── Sign-in by method ─────────────────────────────────────────────────────────

#[tokio::test]
async fn signs_in_with_vnc_authentication() {
    // Given
    let server = a_vnc_server().offering(&[VNC_AUTH]).start().await;

    // When
    let mut client = sign_in(&server, &a_password_only(PASSWORD))
        .await
        .expect("VNC Authentication succeeds");

    // Then
    assert_eq!(first_pixel_of_a_frame(&mut client).await, SCREEN_RGBA);
}

#[tokio::test]
async fn signs_in_with_vencrypt_plain() {
    // Given
    let server = a_vnc_server()
        .offering(&[VENCRYPT])
        .with_vencrypt_subtypes(&[VENCRYPT_PLAIN])
        .start()
        .await;

    // When
    let mut client = sign_in(&server, &a_username_and_password())
        .await
        .expect("VeNCrypt Plain sign-in succeeds");

    // Then
    assert_eq!(first_pixel_of_a_frame(&mut client).await, SCREEN_RGBA);
}

#[tokio::test]
async fn signs_in_with_vencrypt_tls_plain() {
    // Given
    let server = a_vnc_server()
        .offering(&[VENCRYPT])
        .with_vencrypt_subtypes(&[VENCRYPT_PLAIN, VENCRYPT_TLS_PLAIN])
        .start()
        .await;

    // When
    let mut client = sign_in(&server, &a_username_and_password())
        .await
        .expect("VeNCrypt TLSPlain sign-in succeeds");

    // Then
    assert_eq!(server.selected_vencrypt_subtype(), Some(VENCRYPT_TLS_PLAIN));
    assert_eq!(first_pixel_of_a_frame(&mut client).await, SCREEN_RGBA);
}

#[tokio::test]
async fn signs_in_with_apple_remote_desktop() {
    // Given
    let server = a_vnc_server()
        .offering(&[APPLE_REMOTE_DESKTOP])
        .start()
        .await;

    // When
    let mut client = sign_in(&server, &a_username_and_password())
        .await
        .expect("Apple Remote Desktop sign-in succeeds");

    // Then
    assert_eq!(first_pixel_of_a_frame(&mut client).await, SCREEN_RGBA);
}

// ── Choice by credentials ─────────────────────────────────────────────────────

#[tokio::test]
async fn chooses_vnc_authentication_when_no_username_is_given_and_ard_is_also_offered() {
    // Given
    let server = a_vnc_server()
        .offering(&[APPLE_REMOTE_DESKTOP, VNC_AUTH])
        .start()
        .await;

    // When
    let mut client = sign_in(&server, &a_password_only(PASSWORD))
        .await
        .expect("VNC Authentication succeeds next to ARD");

    // Then
    assert_eq!(server.selected_security_type(), Some(VNC_AUTH));
    assert_eq!(first_pixel_of_a_frame(&mut client).await, SCREEN_RGBA);
}

#[tokio::test]
async fn chooses_apple_remote_desktop_when_a_username_is_given() {
    // Given
    let server = a_vnc_server()
        .offering(&[VNC_AUTH, APPLE_REMOTE_DESKTOP])
        .start()
        .await;

    // When
    let mut client = sign_in(&server, &a_username_and_password())
        .await
        .expect("Apple Remote Desktop sign-in succeeds next to VNC Authentication");

    // Then
    assert_eq!(server.selected_security_type(), Some(APPLE_REMOTE_DESKTOP));
    assert_eq!(first_pixel_of_a_frame(&mut client).await, SCREEN_RGBA);
}

// ── Failures ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_server_offering_only_unknown_types_is_refused_naming_them() {
    // Given
    let server = a_vnc_server().offering(&[77, 99]).start().await;

    // When
    let err = sign_in(&server, &a_username_and_password())
        .await
        .err()
        .expect("sign-in fails when no offered type is supported");

    // Then
    assert_error_names_types(&err, &[77, 99]);
}

#[tokio::test]
async fn a_vencrypt_server_with_no_supported_subtype_is_refused_without_a_second_type_byte() {
    // Given: VeNCrypt is chosen first, and ARD is offered as a tempting alternative
    let server = a_vnc_server()
        .offering(&[VENCRYPT, APPLE_REMOTE_DESKTOP])
        .with_vencrypt_subtypes(&[X509_PLAIN])
        .start()
        .await;

    // When
    let err = sign_in(&server, &a_username_and_password())
        .await
        .err()
        .expect("sign-in fails when no VeNCrypt subtype is supported");

    // Then
    assert_error_names_types(&err, &[VENCRYPT]);
    assert_eq!(server.selected_security_type(), Some(VENCRYPT));
    assert_eq!(
        server.vencrypt_choice_bytes().await,
        Vec::<u8>::new(),
        "the client wrote bytes into the VeNCrypt phase instead of giving up"
    );
}

#[tokio::test]
async fn a_refused_sign_in_fails_to_connect_with_the_servers_reason() {
    // Given
    let server = a_vnc_server().offering(&[VNC_AUTH]).start().await;

    // When
    let err = sign_in(&server, &a_password_only("not-the-password"))
        .await
        .err()
        .expect("sign-in fails with the wrong password");

    // Then
    assert_error_carries_the_servers_reason(&err);
}

#[tokio::test]
async fn wrong_credentials_for_vencrypt_plain_are_refused_with_the_servers_reason() {
    // Given
    let server = a_vnc_server()
        .offering(&[VENCRYPT])
        .with_vencrypt_subtypes(&[VENCRYPT_PLAIN])
        .start()
        .await;

    // When
    let err = sign_in(&server, &a_wrong_username_and_password())
        .await
        .err()
        .expect("sign-in fails with the wrong account");

    // Then
    assert_error_carries_the_servers_reason(&err);
}

#[tokio::test]
async fn wrong_credentials_for_vencrypt_tls_plain_are_refused_with_the_servers_reason() {
    // Given
    let server = a_vnc_server()
        .offering(&[VENCRYPT])
        .with_vencrypt_subtypes(&[VENCRYPT_TLS_PLAIN])
        .start()
        .await;

    // When
    let err = sign_in(&server, &a_wrong_username_and_password())
        .await
        .err()
        .expect("sign-in fails with the wrong account");

    // Then
    assert_error_carries_the_servers_reason(&err);
}

#[tokio::test]
async fn wrong_credentials_for_apple_remote_desktop_are_refused_with_the_servers_reason() {
    // Given
    let server = a_vnc_server()
        .offering(&[APPLE_REMOTE_DESKTOP])
        .start()
        .await;

    // When
    let err = sign_in(&server, &a_wrong_username_and_password())
        .await
        .err()
        .expect("sign-in fails with the wrong account");

    // Then
    assert_error_carries_the_servers_reason(&err);
}

// ── RFB versions ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn signs_in_with_vnc_authentication_over_rfb_3_3() {
    // Given
    let server = a_vnc_server()
        .with_version(ServerVersion::V33)
        .offering(&[VNC_AUTH])
        .start()
        .await;

    // When
    let mut client = sign_in(&server, &a_password_only(PASSWORD))
        .await
        .expect("VNC Authentication succeeds over RFB 3.3");

    // Then
    assert_eq!(first_pixel_of_a_frame(&mut client).await, SCREEN_RGBA);
}

#[tokio::test]
async fn signs_in_with_vnc_authentication_over_rfb_3_7() {
    // Given
    let server = a_vnc_server()
        .with_version(ServerVersion::V37)
        .offering(&[VNC_AUTH])
        .start()
        .await;

    // When
    let mut client = sign_in(&server, &a_password_only(PASSWORD))
        .await
        .expect("VNC Authentication succeeds over RFB 3.7");

    // Then
    assert_eq!(first_pixel_of_a_frame(&mut client).await, SCREEN_RGBA);
}

#[tokio::test]
async fn apple_remote_desktop_is_refused_over_rfb_3_3_which_cannot_choose_a_security_type() {
    // Given
    let server = a_vnc_server()
        .with_version(ServerVersion::V33)
        .offering(&[APPLE_REMOTE_DESKTOP])
        .start()
        .await;

    // When
    let err = sign_in(&server, &a_username_and_password())
        .await
        .err()
        .expect("Apple Remote Desktop cannot be run over RFB 3.3");

    // Then
    let message = error_text(&err);
    assert!(
        message.contains("needs RFB 3.7 or later"),
        "error does not explain the version: {message}"
    );
}

#[tokio::test]
async fn an_apple_rfb_3_889_announcement_is_answered_as_rfb_3_8() {
    // Given
    let server = a_vnc_server()
        .with_version(ServerVersion::Apple889)
        .offering(&[VNC_AUTH])
        .start()
        .await;

    // When
    let mut client = sign_in(&server, &a_password_only(PASSWORD))
        .await
        .expect("VNC Authentication succeeds against an RFB 3.889 server");

    // Then
    assert_eq!(server.client_version(), Some(*b"RFB 003.008\n"));
    assert_eq!(first_pixel_of_a_frame(&mut client).await, SCREEN_RGBA);
}

#[tokio::test]
async fn a_refused_vnc_password_is_reported_as_an_authentication_failure() {
    // Given
    let server = a_vnc_server().offering(&[VNC_AUTH]).start().await;

    // When
    let err = sign_in(&server, &a_password_only("not-the-password"))
        .await
        .err()
        .expect("sign-in fails with the wrong password");

    // Then — tagged by type, so the bridge reports it without matching on the server's words
    let refusal = err
        .chain()
        .find_map(|cause| cause.downcast_ref::<AuthenticationRefused>())
        .unwrap_or_else(|| {
            panic!(
                "the error chain carries no AuthenticationRefused: {}",
                error_text(&err)
            )
        });
    assert_eq!(refusal.0, WRONG_PASSWORD_REASON);
}
