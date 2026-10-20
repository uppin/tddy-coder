use crate::security::types::first_missing_for_account;
use crate::security::types::SecurityError;
use crate::security::VENCRYPT_PLAIN_HEADER_LEN;
use crate::security::VENCRYPT_VERSION;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{ring, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::sync::Arc;
use tddy_screenshare::Credentials;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// VeNCrypt 0.2 version exchange, then the server's subtype list.
pub(super) async fn read_vencrypt_subtypes<S>(stream: &mut S) -> Result<Vec<u32>, SecurityError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut server_version = [0u8; 2];
    stream.read_exact(&mut server_version).await?;
    if server_version < VENCRYPT_VERSION {
        return Err(SecurityError::Protocol(format!(
            "the server speaks VeNCrypt {}.{}, this client needs {}.{}",
            server_version[0], server_version[1], VENCRYPT_VERSION[0], VENCRYPT_VERSION[1]
        )));
    }
    stream.write_all(&VENCRYPT_VERSION).await?;
    if stream.read_u8().await? != 0 {
        return Err(SecurityError::Protocol(format!(
            "the server did not accept VeNCrypt {}.{}",
            VENCRYPT_VERSION[0], VENCRYPT_VERSION[1]
        )));
    }
    let count = stream.read_u8().await?;
    let mut subtypes = Vec::with_capacity(count as usize);
    for _ in 0..count {
        subtypes.push(stream.read_u32().await?);
    }
    Ok(subtypes)
}

/// VeNCrypt Plain: username length, password length, username, password.
pub(super) async fn send_vencrypt_plain_credentials<S>(
    stream: &mut S,
    credentials: &Credentials,
) -> Result<(), SecurityError>
where
    S: AsyncWrite + Unpin,
{
    let (username, password) = username_and_password(credentials)?;
    let mut message =
        Vec::with_capacity(VENCRYPT_PLAIN_HEADER_LEN + username.len() + password.len());
    message.extend_from_slice(&(username.len() as u32).to_be_bytes());
    message.extend_from_slice(&(password.len() as u32).to_be_bytes());
    message.extend_from_slice(username);
    message.extend_from_slice(password);
    stream.write_all(&message).await?;
    stream.flush().await?;
    Ok(())
}

/// The username and password as bytes, or the credential that is missing.
pub(super) fn username_and_password(
    credentials: &Credentials,
) -> Result<(&[u8], &[u8]), SecurityError> {
    match (&credentials.username, &credentials.password) {
        (Some(username), Some(password)) => Ok((username.as_bytes(), password.as_bytes())),
        _ => Err(SecurityError::MissingCredential(
            first_missing_for_account(credentials).unwrap_or("username"),
        )),
    }
}

pub(super) async fn start_tls<S>(
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
pub(super) struct UnverifiedServerCertificate {
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
