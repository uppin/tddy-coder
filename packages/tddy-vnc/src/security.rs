//! RFB security negotiation: which sign-in methods a VNC server offers, and which one to run.
//!
//! The server's security-types message is parsed with unknown type numbers preserved (vnc-rs
//! rejects the whole list when it meets one), so a server offering Apple Remote Desktop next to
//! VNC Authentication still yields a usable list. [`choose_sign_in`] picks the method the
//! available [`Credentials`] can complete.

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

/// Parse the server's security-types message: a count byte followed by that many type numbers.
///
/// Unknown type numbers are kept as [`VncSecurityType::Unknown`] rather than failing the list.
pub fn parse_security_types(bytes: &[u8]) -> Result<Vec<VncSecurityType>, SecurityError> {
    // TODO(vnc-security): implement
    let _ = bytes;
    todo!()
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
    /// VeNCrypt subtype TLSPlain (259): username and password inside an anonymous TLS session.
    VeNCryptTlsPlain,
    /// Apple Remote Desktop (type 30).
    AppleRemoteDesktop,
}

/// Choose the sign-in method to run, given what the server offers and what the caller supplied.
///
/// `vencrypt_subtypes` is the VeNCrypt subtype list (empty when VeNCrypt is not offered).
/// A method needing a credential the caller did not supply is never chosen.
pub fn choose_sign_in(
    offered: &[VncSecurityType],
    vencrypt_subtypes: &[u32],
    credentials: &Credentials,
) -> Result<VncSignIn, SecurityError> {
    // TODO(vnc-security): implement
    let _ = (offered, vencrypt_subtypes, credentials);
    todo!()
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
