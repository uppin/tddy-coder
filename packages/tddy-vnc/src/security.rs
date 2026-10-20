//! RFB security negotiation: which sign-in methods a VNC server offers, and which one to run.
//!
//! The server's security-types message is parsed with unknown type numbers preserved (vnc-rs
//! rejects the whole list when it meets one), so a server offering Apple Remote Desktop next to
//! VNC Authentication still yields a usable list. [`choose_sign_in`] picks the method the
//! available [`Credentials`] can complete, and [`negotiate`] runs it.
//!
//! # Hand-off to vnc-rs
//!
//! vnc-rs has no entry point after the SecurityResult: its connector always runs the version and
//! security handshake itself. [`negotiate`] therefore returns a [`SecuredStream`] that *replays*
//! the handshake bytes vnc-rs expects to read and swallows the bytes it writes in answer, so
//! vnc-rs sees a server that has already authenticated and carries on from ClientInit.
//!
//! - VNC Authentication and None are left to vnc-rs (it owns the DES step, and no DES crate is
//!   a dependency): the stream replays the negotiated version and the single chosen type, and
//!   vnc-rs runs the rest of the exchange against the real server.
//! - VeNCrypt and Apple Remote Desktop are run here; the stream then replays RFB 3.3 with
//!   security type None, which makes vnc-rs read nothing further before ClientInit.
//!
//! # TLSPlain trust
//!
//! VeNCrypt TLSPlain accepts the server's certificate without verifying it. rustls has no
//! anonymous Diffie-Hellman cipher suites, which TigerVNC/GnuTLS servers use for TLSPlain, so
//! this works with servers presenting a certificate (verified or not) and not with servers that
//! offer anonymous-DH only. The TLS session still verifies that the handshake is signed by the
//! presented certificate's key.

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use aes::cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};
use md5::{Digest, Md5};
use num_bigint::BigUint;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{ring, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tddy_screenshare::Credentials;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};

const VENCRYPT_PLAIN: u32 = 256;
const VENCRYPT_TLS_PLAIN: u32 = 259;

const TYPE_NONE: u8 = 1;
const TYPE_VNC_AUTH: u8 = 2;
const TYPE_VENCRYPT: u8 = 19;
const TYPE_APPLE_REMOTE_DESKTOP: u8 = 30;

/// Longest failure reason the client reads from a server.
const MAX_REASON_LEN: usize = 64 * 1024;
/// Largest Diffie-Hellman key size (bytes) accepted from an Apple Remote Desktop server.
const MAX_ARD_KEY_LEN: usize = 512;
/// Size of each of the username and password fields of the ARD credentials block.
const ARD_FIELD_LEN: usize = 64;
const RFB_VERSION_LEN: usize = 12;

/// One entry of the server's security-types list (RFB 3.8 §7.1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VncSecurityType {
    /// Type 1 — no authentication.
    None,
    /// Type 2 — VNC Authentication (DES challenge with a password).
    VncAuth,
    /// Type 19 — VeNCrypt, which carries its own subtype list.
    VeNCrypt,
    /// Type 30 — Apple Remote Desktop (Diffie-Hellman, then username and password).
    AppleRemoteDesktop,
    /// Any other type number, kept so an error can name it.
    Unknown(u8),
}

impl VncSecurityType {
    fn from_number(number: u8) -> Self {
        match number {
            TYPE_NONE => Self::None,
            TYPE_VNC_AUTH => Self::VncAuth,
            TYPE_VENCRYPT => Self::VeNCrypt,
            TYPE_APPLE_REMOTE_DESKTOP => Self::AppleRemoteDesktop,
            other => Self::Unknown(other),
        }
    }

    /// The type number as it appears on the wire.
    pub fn number(self) -> u8 {
        match self {
            Self::None => TYPE_NONE,
            Self::VncAuth => TYPE_VNC_AUTH,
            Self::VeNCrypt => TYPE_VENCRYPT,
            Self::AppleRemoteDesktop => TYPE_APPLE_REMOTE_DESKTOP,
            Self::Unknown(number) => number,
        }
    }
}

/// Parse the server's security-types message: a count byte followed by that many type numbers.
///
/// Unknown type numbers are kept as [`VncSecurityType::Unknown`] rather than failing the list.
pub fn parse_security_types(bytes: &[u8]) -> Result<Vec<VncSecurityType>, SecurityError> {
    let (&count, types) = bytes
        .split_first()
        .ok_or_else(|| SecurityError::Protocol("the security-types message is empty".to_owned()))?;
    if count == 0 {
        return Err(SecurityError::Protocol(
            "the server offered no security types".to_owned(),
        ));
    }
    if types.len() != count as usize {
        return Err(SecurityError::Protocol(format!(
            "the security-types message announces {count} types but carries {}",
            types.len()
        )));
    }
    Ok(types
        .iter()
        .map(|&number| VncSecurityType::from_number(number))
        .collect())
}

/// The sign-in method the client runs against the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VncSignIn {
    /// No authentication.
    None,
    /// VNC Authentication with the password.
    VncAuth,
    /// VeNCrypt subtype Plain (256): username and password in the clear.
    VeNCryptPlain,
    /// VeNCrypt subtype TLSPlain (259): username and password inside a TLS session.
    VeNCryptTlsPlain,
    /// Apple Remote Desktop (type 30).
    AppleRemoteDesktop,
}

/// Choose the sign-in method to run, given what the server offers and what the caller supplied.
///
/// `vencrypt_subtypes` is the VeNCrypt subtype list (empty when VeNCrypt is not offered or its
/// subtypes are not known yet). A method needing a credential the caller did not supply is never
/// chosen. Preference: VeNCrypt (TLSPlain over Plain), Apple Remote Desktop, VNC Authentication,
/// then None.
pub fn choose_sign_in(
    offered: &[VncSecurityType],
    vencrypt_subtypes: &[u32],
    credentials: &Credentials,
) -> Result<VncSignIn, SecurityError> {
    let has_username = credentials.username.is_some();
    let has_password = credentials.password.is_some();
    let missing_username_or_password = match (has_username, has_password) {
        (false, _) => Some("username"),
        (true, false) => Some("password"),
        (true, true) => None,
    };
    let mut missing = None;

    if offered.contains(&VncSecurityType::VeNCrypt) {
        match missing_username_or_password {
            Some(credential) => {
                missing.get_or_insert(credential);
            }
            None if vencrypt_subtypes.contains(&VENCRYPT_TLS_PLAIN) => {
                return Ok(VncSignIn::VeNCryptTlsPlain)
            }
            None if vencrypt_subtypes.contains(&VENCRYPT_PLAIN) => {
                return Ok(VncSignIn::VeNCryptPlain)
            }
            None => {}
        }
    }
    if offered.contains(&VncSecurityType::AppleRemoteDesktop) {
        match missing_username_or_password {
            Some(credential) => {
                missing.get_or_insert(credential);
            }
            None => return Ok(VncSignIn::AppleRemoteDesktop),
        }
    }
    if offered.contains(&VncSecurityType::VncAuth) {
        match has_password {
            true => return Ok(VncSignIn::VncAuth),
            false => {
                missing.get_or_insert("password");
            }
        }
    }
    if offered.contains(&VncSecurityType::None) {
        return Ok(VncSignIn::None);
    }

    match missing {
        Some(credential) => Err(SecurityError::MissingCredential(credential)),
        None => Err(SecurityError::NothingSupported {
            offered: offered.iter().map(|t| t.number()).collect(),
        }),
    }
}

/// Why a VNC sign-in could not be completed.
#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    /// The server refused the sign-in (SecurityResult failed), with its reason.
    #[error("the VNC server refused the sign-in: {reason}")]
    Refused { reason: String },
    /// The server offered no method this client can run; the offered type numbers are kept.
    #[error(
        "the VNC server offers no supported sign-in method (offered security types: {offered:?})"
    )]
    NothingSupported { offered: Vec<u8> },
    /// Every method the server offers needs a credential that was not supplied.
    #[error("the VNC server's sign-in methods need a {0}")]
    MissingCredential(&'static str),
    /// The connection failed mid-exchange.
    #[error("VNC sign-in connection failed: {0}")]
    Io(#[from] std::io::Error),
    /// The server broke the protocol.
    #[error("VNC sign-in protocol error: {0}")]
    Protocol(String),
}

// ── Transport ─────────────────────────────────────────────────────────────────

/// A byte stream the sign-in layer can run over.
pub trait Transport: AsyncRead + AsyncWrite + Unpin + Send + Sync {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send + Sync> Transport for T {}

/// A stream positioned so that vnc-rs, running its own handshake over it, ends up at ClientInit.
///
/// Reads first return the `replay` bytes, then the underlying stream; the first `swallow` bytes
/// written are dropped instead of reaching the server. See the module docs.
pub struct SecuredStream {
    inner: Box<dyn Transport>,
    replay: Vec<u8>,
    replayed: usize,
    swallow: usize,
}

impl SecuredStream {
    fn new(inner: Box<dyn Transport>, replay: Vec<u8>, swallow: usize) -> Self {
        Self {
            inner,
            replay,
            replayed: 0,
            swallow,
        }
    }

    /// A stream whose sign-in is complete: vnc-rs is told RFB 3.3 with security type None, which
    /// it handles without reading or writing anything but the version it echoes.
    fn authenticated(inner: Box<dyn Transport>) -> Self {
        let mut replay = RfbVersion::V33.bytes().to_vec();
        replay.extend_from_slice(&u32::from(TYPE_NONE).to_be_bytes());
        Self::new(inner, replay, RFB_VERSION_LEN)
    }
}

impl AsyncRead for SecuredStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.replayed < this.replay.len() {
            let pending = &this.replay[this.replayed..];
            let n = pending.len().min(buf.remaining());
            buf.put_slice(&pending[..n]);
            this.replayed += n;
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for SecuredStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.swallow > 0 {
            let n = this.swallow.min(buf.len());
            this.swallow -= n;
            return Poll::Ready(Ok(n));
        }
        Pin::new(&mut this.inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

// ── Negotiation ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RfbVersion {
    V33,
    V37,
    V38,
}

impl RfbVersion {
    fn bytes(self) -> &'static [u8; RFB_VERSION_LEN] {
        match self {
            Self::V33 => b"RFB 003.003\n",
            Self::V37 => b"RFB 003.007\n",
            Self::V38 => b"RFB 003.008\n",
        }
    }

    /// The highest version this client speaks that the server's announced version allows.
    fn parse_server_version(bytes: &[u8; RFB_VERSION_LEN]) -> Result<Self, SecurityError> {
        let announced = std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| text.strip_prefix("RFB 003."))
            .and_then(|rest| rest.strip_suffix('\n'))
            .and_then(|minor| minor.parse::<u32>().ok())
            .ok_or_else(|| {
                SecurityError::Protocol(format!(
                    "the server did not announce an RFB 3.x version: {:?}",
                    String::from_utf8_lossy(bytes)
                ))
            })?;
        // Apple's servers announce 3.889 and mean 3.8.
        Ok(match announced {
            8.. => Self::V38,
            7 => Self::V37,
            _ => Self::V33,
        })
    }
}

/// Run the version handshake and the sign-in the credentials allow, returning a stream vnc-rs can
/// take from ClientInit. `server_name` is the host the TLS session (TLSPlain) is addressed to.
pub async fn negotiate<S>(
    mut stream: S,
    credentials: &Credentials,
    server_name: &str,
) -> Result<SecuredStream, SecurityError>
where
    S: Transport + 'static,
{
    let mut announced = [0u8; RFB_VERSION_LEN];
    stream.read_exact(&mut announced).await?;
    let version = RfbVersion::parse_server_version(&announced)?;
    stream.write_all(version.bytes()).await?;

    let offered = read_security_types(&mut stream, version).await?;
    log::debug!("VNC server offers security types {offered:?}");

    let vencrypt_subtypes = if version != RfbVersion::V33
        && offered.contains(&VncSecurityType::VeNCrypt)
        && credentials.username.is_some()
        && credentials.password.is_some()
    {
        stream.write_all(&[TYPE_VENCRYPT]).await?;
        read_vencrypt_subtypes(&mut stream).await?
    } else {
        Vec::new()
    };

    let sign_in = choose_sign_in(&offered, &vencrypt_subtypes, credentials)?;
    log::debug!("VNC sign-in method: {sign_in:?}");
    match sign_in {
        VncSignIn::None => Ok(delegate_to_vnc_rs(stream, version, TYPE_NONE)),
        VncSignIn::VncAuth => Ok(delegate_to_vnc_rs(stream, version, TYPE_VNC_AUTH)),
        VncSignIn::AppleRemoteDesktop => {
            require_selectable_types(version)?;
            stream.write_all(&[TYPE_APPLE_REMOTE_DESKTOP]).await?;
            apple_remote_desktop(&mut stream, credentials).await?;
            finish(stream, version).await
        }
        VncSignIn::VeNCryptPlain => {
            stream.write_all(&VENCRYPT_PLAIN.to_be_bytes()).await?;
            plain(&mut stream, credentials).await?;
            finish(stream, version).await
        }
        VncSignIn::VeNCryptTlsPlain => {
            stream.write_all(&VENCRYPT_TLS_PLAIN.to_be_bytes()).await?;
            let mut accepted = [0u8; 1];
            stream.read_exact(&mut accepted).await?;
            if accepted[0] == 0 {
                return Err(SecurityError::Protocol(
                    "the server did not accept VeNCrypt TLSPlain".to_owned(),
                ));
            }
            let mut tls = start_tls(stream, server_name).await?;
            plain(&mut tls, credentials).await?;
            finish(tls, version).await
        }
    }
}

fn require_selectable_types(version: RfbVersion) -> Result<(), SecurityError> {
    match version {
        RfbVersion::V33 => Err(SecurityError::Protocol(
            "this sign-in method needs RFB 3.7 or later".to_owned(),
        )),
        RfbVersion::V37 | RfbVersion::V38 => Ok(()),
    }
}

/// Leave the rest of VNC Authentication / None to vnc-rs: replay the version the server and we
/// agreed on and the one security type, and drop vnc-rs's echo of the version.
fn delegate_to_vnc_rs<S: Transport + 'static>(
    stream: S,
    version: RfbVersion,
    security_type: u8,
) -> SecuredStream {
    let mut replay = version.bytes().to_vec();
    match version {
        RfbVersion::V33 => replay.extend_from_slice(&u32::from(security_type).to_be_bytes()),
        RfbVersion::V37 | RfbVersion::V38 => replay.extend_from_slice(&[1, security_type]),
    }
    SecuredStream::new(Box::new(stream), replay, RFB_VERSION_LEN)
}

async fn read_security_types<S>(
    stream: &mut S,
    version: RfbVersion,
) -> Result<Vec<VncSecurityType>, SecurityError>
where
    S: AsyncRead + Unpin,
{
    match version {
        RfbVersion::V33 => {
            let security_type = stream.read_u32().await?;
            if security_type == 0 {
                return Err(refused(read_reason(stream).await?));
            }
            let number = u8::try_from(security_type).map_err(|_| {
                SecurityError::Protocol(format!("invalid security type {security_type}"))
            })?;
            parse_security_types(&[1, number])
        }
        RfbVersion::V37 | RfbVersion::V38 => {
            let count = stream.read_u8().await?;
            if count == 0 {
                return Err(refused(read_reason(stream).await?));
            }
            let mut message = vec![count];
            message.resize(1 + count as usize, 0);
            stream.read_exact(&mut message[1..]).await?;
            parse_security_types(&message)
        }
    }
}

fn refused(reason: String) -> SecurityError {
    SecurityError::Refused { reason }
}

async fn read_reason<S: AsyncRead + Unpin>(stream: &mut S) -> Result<String, SecurityError> {
    let len = stream.read_u32().await? as usize;
    if len > MAX_REASON_LEN {
        return Err(SecurityError::Protocol(format!(
            "the server's failure reason is {len} bytes long"
        )));
    }
    let mut reason = vec![0u8; len];
    stream.read_exact(&mut reason).await?;
    Ok(String::from_utf8_lossy(&reason).into_owned())
}

/// Read the SecurityResult; on failure return the server's reason (RFB 3.8 sends one).
async fn finish<S>(mut stream: S, version: RfbVersion) -> Result<SecuredStream, SecurityError>
where
    S: Transport + 'static,
{
    if stream.read_u32().await? != 0 {
        return Err(match version {
            RfbVersion::V38 => refused(read_reason(&mut stream).await?),
            RfbVersion::V33 | RfbVersion::V37 => {
                refused("the server gave no reason (RFB before 3.8 sends none)".to_owned())
            }
        });
    }
    Ok(SecuredStream::authenticated(Box::new(stream)))
}

// ── VeNCrypt ──────────────────────────────────────────────────────────────────

/// VeNCrypt 0.2 version exchange, then the server's subtype list.
async fn read_vencrypt_subtypes<S>(stream: &mut S) -> Result<Vec<u32>, SecurityError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut server_version = [0u8; 2];
    stream.read_exact(&mut server_version).await?;
    if server_version < [0, 2] {
        return Err(SecurityError::Protocol(format!(
            "the server speaks VeNCrypt {}.{}, this client needs 0.2",
            server_version[0], server_version[1]
        )));
    }
    stream.write_all(&[0, 2]).await?;
    if stream.read_u8().await? != 0 {
        return Err(SecurityError::Protocol(
            "the server did not accept VeNCrypt 0.2".to_owned(),
        ));
    }
    let count = stream.read_u8().await?;
    let mut subtypes = Vec::with_capacity(count as usize);
    for _ in 0..count {
        subtypes.push(stream.read_u32().await?);
    }
    Ok(subtypes)
}

/// VeNCrypt Plain: username length, password length, username, password.
async fn plain<S>(stream: &mut S, credentials: &Credentials) -> Result<(), SecurityError>
where
    S: AsyncWrite + Unpin,
{
    let username = required(&credentials.username, "username")?.as_bytes();
    let password = required(&credentials.password, "password")?.as_bytes();
    let mut message = Vec::with_capacity(8 + username.len() + password.len());
    message.extend_from_slice(&(username.len() as u32).to_be_bytes());
    message.extend_from_slice(&(password.len() as u32).to_be_bytes());
    message.extend_from_slice(username);
    message.extend_from_slice(password);
    stream.write_all(&message).await?;
    stream.flush().await?;
    Ok(())
}

fn required<'a>(
    credential: &'a Option<String>,
    name: &'static str,
) -> Result<&'a String, SecurityError> {
    credential
        .as_ref()
        .ok_or(SecurityError::MissingCredential(name))
}

async fn start_tls<S>(
    stream: S,
    server_name: &str,
) -> Result<tokio_rustls::client::TlsStream<S>, SecurityError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let provider = Arc::new(ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| SecurityError::Protocol(format!("TLS is unavailable: {e}")))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(UnverifiedServerCertificate { provider }))
        .with_no_client_auth();
    let name = ServerName::try_from(server_name.to_owned()).map_err(|e| {
        SecurityError::Protocol(format!(
            "{server_name:?} is not a valid TLS server name: {e}"
        ))
    })?;
    Ok(tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(name, stream)
        .await?)
}

/// Accepts any server certificate (see the module docs on TLSPlain trust) while still checking
/// that the TLS handshake is signed by the key of the certificate the server presented.
#[derive(Debug)]
struct UnverifiedServerCertificate {
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for UnverifiedServerCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

// ── Apple Remote Desktop ──────────────────────────────────────────────────────

/// Apple Remote Desktop (type 30): read the Diffie-Hellman parameters and the server's public
/// key, answer with the username and password (two 64-byte null-terminated fields) encrypted
/// with AES-128-ECB under the MD5 of the shared secret, followed by the client's public key.
async fn apple_remote_desktop<S>(
    stream: &mut S,
    credentials: &Credentials,
) -> Result<(), SecurityError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let username = required(&credentials.username, "username")?.as_bytes();
    let password = required(&credentials.password, "password")?.as_bytes();
    if username.len() >= ARD_FIELD_LEN || password.len() >= ARD_FIELD_LEN {
        return Err(SecurityError::Protocol(format!(
            "Apple Remote Desktop accepts a username and password of at most {} bytes",
            ARD_FIELD_LEN - 1
        )));
    }

    let generator = stream.read_u16().await?;
    let key_len = stream.read_u16().await? as usize;
    if key_len == 0 || key_len > MAX_ARD_KEY_LEN {
        return Err(SecurityError::Protocol(format!(
            "unusable Apple Remote Desktop key length {key_len}"
        )));
    }
    let mut prime = vec![0u8; key_len];
    stream.read_exact(&mut prime).await?;
    let mut server_public = vec![0u8; key_len];
    stream.read_exact(&mut server_public).await?;
    let prime = BigUint::from_bytes_be(&prime);
    let server_public = BigUint::from_bytes_be(&server_public);
    if prime < BigUint::from(3u8) || server_public <= BigUint::from(1u8) || server_public >= prime {
        return Err(SecurityError::Protocol(
            "the Apple Remote Desktop key parameters are not usable".to_owned(),
        ));
    }

    let random = ring::default_provider().secure_random;
    let mut private_key = vec![0u8; key_len];
    random.fill(&mut private_key).map_err(random_failed)?;
    let private_key = BigUint::from_bytes_be(&private_key);
    let client_public = BigUint::from(generator).modpow(&private_key, &prime);
    let shared_secret = server_public.modpow(&private_key, &prime);
    let aes_key = Md5::digest(be_padded(&shared_secret, key_len));

    let mut block = [0u8; 2 * ARD_FIELD_LEN];
    random.fill(&mut block).map_err(random_failed)?;
    block[..username.len()].copy_from_slice(username);
    block[username.len()..ARD_FIELD_LEN].fill(0);
    block[ARD_FIELD_LEN..ARD_FIELD_LEN + password.len()].copy_from_slice(password);
    block[ARD_FIELD_LEN + password.len()..].fill(0);
    let cipher = aes::Aes128::new(GenericArray::from_slice(&aes_key));
    for chunk in block.chunks_exact_mut(16) {
        cipher.encrypt_block(GenericArray::from_mut_slice(chunk));
    }

    let mut message = block.to_vec();
    message.extend_from_slice(&be_padded(&client_public, key_len));
    stream.write_all(&message).await?;
    stream.flush().await?;
    Ok(())
}

fn random_failed(_: rustls::crypto::GetRandomFailed) -> SecurityError {
    SecurityError::Protocol("the system random number generator failed".to_owned())
}

/// `value` as big-endian bytes, left-padded with zeros to `len`.
fn be_padded(value: &BigUint, len: usize) -> Vec<u8> {
    let bytes = value.to_bytes_be();
    let mut padded = vec![0u8; len.saturating_sub(bytes.len())];
    padded.extend_from_slice(&bytes);
    padded
}

#[cfg(test)]
mod tests {
    use super::*;

    const VENCRYPT_PLAIN: u32 = 256;
    const VENCRYPT_TLS_PLAIN: u32 = 259;

    fn a_password_only() -> Credentials {
        Credentials {
            password: Some("secret".to_owned()),
            ..Credentials::default()
        }
    }

    fn a_username_and_password() -> Credentials {
        Credentials {
            username: Some("alice".to_owned()),
            password: Some("secret".to_owned()),
            ..Credentials::default()
        }
    }

    #[test]
    fn parses_a_list_that_mixes_known_and_unknown_types() {
        // Given
        let message = [5u8, 1, 2, 77, 19, 30];

        // When
        let types = parse_security_types(&message).expect("the list parses");

        // Then
        assert_eq!(
            types,
            vec![
                VncSecurityType::None,
                VncSecurityType::VncAuth,
                VncSecurityType::Unknown(77),
                VncSecurityType::VeNCrypt,
                VncSecurityType::AppleRemoteDesktop,
            ]
        );
    }

    #[test]
    fn a_method_needing_a_username_is_not_chosen_without_one() {
        // Given
        let offered = [VncSecurityType::AppleRemoteDesktop];

        // When
        let choice = choose_sign_in(&offered, &[], &a_password_only());

        // Then
        assert!(
            matches!(choice, Err(SecurityError::MissingCredential("username"))),
            "expected a missing-username error, got {choice:?}"
        );
    }

    #[test]
    fn prefers_tls_plain_over_plain() {
        // Given
        let offered = [VncSecurityType::VeNCrypt];
        let subtypes = [VENCRYPT_PLAIN, VENCRYPT_TLS_PLAIN];

        // When
        let choice = choose_sign_in(&offered, &subtypes, &a_username_and_password())
            .expect("a method is chosen");

        // Then
        assert_eq!(choice, VncSignIn::VeNCryptTlsPlain);
    }
}
