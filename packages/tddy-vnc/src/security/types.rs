use crate::security::TYPE_APPLE_REMOTE_DESKTOP;
use crate::security::TYPE_NONE;
use crate::security::TYPE_VENCRYPT;
use crate::security::TYPE_VNC_AUTH;
use crate::security::VENCRYPT_PLAIN;
use crate::security::VENCRYPT_TLS_PLAIN;
use std::time::Duration;
use tddy_screenshare::Credentials;

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

impl VncSignIn {
    pub(super) fn is_vencrypt(self) -> bool {
        matches!(self, Self::VeNCryptPlain | Self::VeNCryptTlsPlain)
    }
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
    let missing_account_credential = first_missing_for_account(credentials);

    if offered.contains(&VncSecurityType::VeNCrypt) && missing_account_credential.is_none() {
        if vencrypt_subtypes.contains(&VENCRYPT_TLS_PLAIN) {
            return Ok(VncSignIn::VeNCryptTlsPlain);
        }
        if vencrypt_subtypes.contains(&VENCRYPT_PLAIN) {
            return Ok(VncSignIn::VeNCryptPlain);
        }
    }
    if offered.contains(&VncSecurityType::AppleRemoteDesktop)
        && missing_account_credential.is_none()
    {
        return Ok(VncSignIn::AppleRemoteDesktop);
    }
    if offered.contains(&VncSecurityType::VncAuth) && credentials.password.is_some() {
        return Ok(VncSignIn::VncAuth);
    }
    if offered.contains(&VncSecurityType::None) {
        return Ok(VncSignIn::None);
    }

    // Nothing was chosen: name the first credential a method the server offers was missing.
    let needs_account = offered.contains(&VncSecurityType::VeNCrypt)
        || offered.contains(&VncSecurityType::AppleRemoteDesktop);
    let missing = missing_account_credential
        .filter(|_| needs_account)
        .or_else(|| (offered.contains(&VncSecurityType::VncAuth)).then_some("password"));
    match missing {
        Some(credential) => Err(SecurityError::MissingCredential(credential)),
        None => Err(SecurityError::NothingSupported {
            offered: offered.iter().map(|t| t.number()).collect(),
        }),
    }
}

/// The first credential the username-and-password methods need that `credentials` lacks.
pub(super) fn first_missing_for_account(credentials: &Credentials) -> Option<&'static str> {
    if credentials.username.is_none() {
        Some("username")
    } else if credentials.password.is_none() {
        Some("password")
    } else {
        None
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
    /// The sign-in did not finish in time (a server that stops answering).
    #[error("VNC sign-in did not finish within {0:?}")]
    TimedOut(Duration),
}
