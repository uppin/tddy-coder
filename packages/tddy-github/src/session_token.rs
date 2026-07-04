//! Stateless, HMAC-signed session tokens verifiable by any daemon that shares the
//! signing secret (`livekit.api_secret`). See `docs/ft/daemon/session-auth.md`.
//!
//! Token format: `v1.<base64url(json payload)>.<base64url(hmac-sha256 tag)>` where the payload
//! carries the GitHub identity plus `iat`/`exp`. Any holder of the secret can verify the
//! signature and expiry without a shared session store — this is what makes a single login
//! work across every daemon in a LiveKit deployment.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::provider::GitHubUser;

type HmacSha256 = Hmac<Sha256>;

/// Version prefix / first token segment. Bumping this invalidates all previously minted tokens.
const TOKEN_VERSION: &str = "v1";
/// HMAC-SHA256 output size in bytes.
const TAG_LEN: usize = 32;

/// Lifetime of a freshly minted session token. Short by design: the web client refreshes well
/// before expiry (see [`crate::auth_service`]/`RefreshSession`), and a leaked token is only
/// valid for this window.
pub const SESSION_TOKEN_TTL: Duration = Duration::from_secs(5 * 60);

/// Lifetime of a freshly minted refresh token. Long by design and slid forward on every refresh:
/// an actively-used session never has to re-login, while a device untouched for this long does.
/// The refresh token is never sent on normal RPCs — it is used only to mint access tokens.
pub const REFRESH_TOKEN_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Which credential a token is: a short-lived [`TokenKind::Access`] token that authenticates
/// RPCs, or a long-lived [`TokenKind::Refresh`] token that only mints access tokens. Enforcing
/// the kind keeps the two roles strictly separate — an access token cannot mint, and a refresh
/// token cannot authenticate an RPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TokenKind {
    /// Short-lived credential presented on every RPC. The default for a payload with no `kind`
    /// field, so tokens minted before the kind claim existed still verify as access tokens.
    #[default]
    Access,
    /// Long-lived credential presented only to `RefreshSession` to mint access tokens.
    Refresh,
}

/// The verified contents of a session token — the GitHub identity, the token kind, plus
/// issue/expiry times (Unix seconds).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionClaims {
    pub id: u64,
    pub login: String,
    pub avatar_url: String,
    pub name: String,
    pub iat: u64,
    pub exp: u64,
    /// Access vs. refresh. Absent in legacy payloads, which default to [`TokenKind::Access`].
    #[serde(default)]
    pub kind: TokenKind,
}

/// Why a session token failed verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionTokenError {
    /// The HMAC tag did not match — the token was forged or signed with a different secret.
    InvalidSignature,
    /// The token was not well-formed (missing prefix, bad base64, undecodable payload).
    Malformed,
    /// The signature was valid but `exp` is in the past.
    Expired,
}

impl std::fmt::Display for SessionTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSignature => write!(f, "session token: invalid signature"),
            Self::Malformed => write!(f, "session token: malformed"),
            Self::Expired => write!(f, "session token: expired"),
        }
    }
}

impl std::error::Error for SessionTokenError {}

/// Mints and verifies [`SessionClaims`] as HMAC-SHA256-signed opaque tokens.
///
/// Constructed from the shared secret (`livekit.api_secret`); daemons that hold the same secret
/// verify each other's tokens.
#[derive(Clone)]
pub struct SessionTokenSigner {
    secret: Vec<u8>,
}

impl SessionTokenSigner {
    pub fn new(secret: &[u8]) -> Self {
        Self {
            secret: secret.to_vec(),
        }
    }

    /// Mint an access token for `user` valid for [`SESSION_TOKEN_TTL`] from now.
    pub fn mint_access(&self, user: &GitHubUser) -> String {
        self.mint_kind_with_issued_at(
            user,
            TokenKind::Access,
            SystemTime::now(),
            SESSION_TOKEN_TTL,
        )
    }

    /// Mint a refresh token for `user` valid for [`REFRESH_TOKEN_TTL`] from now.
    pub fn mint_refresh(&self, user: &GitHubUser) -> String {
        self.mint_kind_with_issued_at(
            user,
            TokenKind::Refresh,
            SystemTime::now(),
            REFRESH_TOKEN_TTL,
        )
    }

    /// Mint an access token for `user` valid for `ttl` from now.
    pub fn mint(&self, user: &GitHubUser, ttl: Duration) -> String {
        self.mint_kind_with_issued_at(user, TokenKind::Access, SystemTime::now(), ttl)
    }

    /// Mint an access token whose issue time is `issued_at` (expiry = `issued_at + ttl`). The clock
    /// seam lets tests produce already-expired tokens deterministically without sleeping.
    pub fn mint_with_issued_at(
        &self,
        user: &GitHubUser,
        issued_at: SystemTime,
        ttl: Duration,
    ) -> String {
        self.mint_kind_with_issued_at(user, TokenKind::Access, issued_at, ttl)
    }

    /// Mint a token of `kind` whose issue time is `issued_at` (expiry = `issued_at + ttl`). The
    /// general seam behind the access/refresh helpers; the clock argument lets tests mint
    /// already-expired tokens of either kind without sleeping.
    pub fn mint_kind_with_issued_at(
        &self,
        user: &GitHubUser,
        kind: TokenKind,
        issued_at: SystemTime,
        ttl: Duration,
    ) -> String {
        let iat = unix_seconds(issued_at);
        let claims = SessionClaims {
            id: user.id,
            login: user.login.clone(),
            avatar_url: user.avatar_url.clone(),
            name: user.name.clone(),
            iat,
            exp: iat.saturating_add(ttl.as_secs()),
            kind,
        };
        // Serialization of a plain struct of owned primitives cannot fail.
        let payload_json = serde_json::to_vec(&claims).expect("SessionClaims serializes");
        let payload_b64 = URL_SAFE_NO_PAD.encode(&payload_json);
        let signing_input = format!("{TOKEN_VERSION}.{payload_b64}");
        let tag = self.sign(signing_input.as_bytes());
        format!("{signing_input}.{}", URL_SAFE_NO_PAD.encode(tag))
    }

    /// Verify a token's signature and expiry, returning its claims.
    pub fn verify(&self, token: &str) -> Result<SessionClaims, SessionTokenError> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 || parts[0] != TOKEN_VERSION {
            return Err(SessionTokenError::Malformed);
        }
        let payload_bytes = URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|_| SessionTokenError::Malformed)?;
        let claims: SessionClaims =
            serde_json::from_slice(&payload_bytes).map_err(|_| SessionTokenError::Malformed)?;
        let tag = URL_SAFE_NO_PAD
            .decode(parts[2])
            .map_err(|_| SessionTokenError::Malformed)?;
        if tag.len() != TAG_LEN {
            return Err(SessionTokenError::Malformed);
        }

        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let expected = self.sign(signing_input.as_bytes());
        if !bool::from(expected.ct_eq(&tag)) {
            return Err(SessionTokenError::InvalidSignature);
        }

        if unix_seconds(SystemTime::now()) > claims.exp {
            return Err(SessionTokenError::Expired);
        }
        Ok(claims)
    }

    fn sign(&self, message: &[u8]) -> [u8; TAG_LEN] {
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .expect("HMAC-SHA256 accepts arbitrary key lengths");
        mac.update(message);
        let mut tag = [0u8; TAG_LEN];
        tag.copy_from_slice(&mac.finalize().into_bytes());
        tag
    }
}

/// Whole seconds since the Unix epoch; times at or before the epoch clamp to 0.
fn unix_seconds(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_github_user() -> GitHubUser {
        GitHubUser {
            id: 42,
            login: "octocat".to_string(),
            avatar_url: "https://github.com/octocat.png".to_string(),
            name: "The Octocat".to_string(),
        }
    }

    /// Flip the token's final character to a different base64url character, corrupting the
    /// signature while leaving the payload segment intact.
    fn with_tampered_signature(token: &str) -> String {
        let mut chars: Vec<char> = token.chars().collect();
        let last = chars.len() - 1;
        chars[last] = if chars[last] == 'A' { 'B' } else { 'A' };
        chars.into_iter().collect()
    }

    #[test]
    fn a_minted_token_round_trips_the_github_identity() {
        // Given a signer and a user
        let signer = SessionTokenSigner::new(b"shared-secret");
        let user = a_github_user();

        // When the token is minted and verified with the same secret
        let token = signer.mint(&user, Duration::from_secs(300));
        let claims = signer
            .verify(&token)
            .expect("token minted with our secret should verify");

        // Then the identity is recovered intact and the validity window is the requested ttl
        assert_eq!(claims.id, 42);
        assert_eq!(claims.login, "octocat");
        assert_eq!(claims.avatar_url, "https://github.com/octocat.png");
        assert_eq!(claims.name, "The Octocat");
        assert_eq!(claims.exp - claims.iat, 300);
    }

    #[test]
    fn verify_rejects_a_token_signed_with_a_different_secret() {
        // Given a token minted with one secret
        let token =
            SessionTokenSigner::new(b"the-real-secret").mint(&a_github_user(), SESSION_TOKEN_TTL);

        // When a signer holding a different secret verifies it
        let result = SessionTokenSigner::new(b"a-different-secret").verify(&token);

        // Then it is rejected as forged
        assert_eq!(result.unwrap_err(), SessionTokenError::InvalidSignature);
    }

    #[test]
    fn verify_rejects_a_token_with_a_tampered_signature() {
        // Given a valid token whose signature byte has been altered
        let signer = SessionTokenSigner::new(b"shared-secret");
        let token = signer.mint(&a_github_user(), SESSION_TOKEN_TTL);
        let tampered = with_tampered_signature(&token);

        // When it is verified
        let result = signer.verify(&tampered);

        // Then the signature check fails
        assert_eq!(result.unwrap_err(), SessionTokenError::InvalidSignature);
    }

    #[test]
    fn verify_rejects_a_token_without_the_version_prefix() {
        let signer = SessionTokenSigner::new(b"shared-secret");
        assert_eq!(
            signer.verify("nope.cGF5bG9hZA.dGFn").unwrap_err(),
            SessionTokenError::Malformed
        );
    }

    #[test]
    fn verify_rejects_a_token_that_is_not_valid_base64() {
        let signer = SessionTokenSigner::new(b"shared-secret");
        assert_eq!(
            signer.verify("v1.!!!not-base64!!!.###").unwrap_err(),
            SessionTokenError::Malformed
        );
    }

    #[test]
    fn verify_rejects_a_structurally_incomplete_token() {
        let signer = SessionTokenSigner::new(b"shared-secret");
        assert_eq!(
            signer.verify("v1.onlyonesegment").unwrap_err(),
            SessionTokenError::Malformed
        );
    }

    #[test]
    fn a_minted_access_token_carries_the_access_kind() {
        // Given a signer
        let signer = SessionTokenSigner::new(b"shared-secret");

        // When an access token is minted and verified
        let token = signer.mint_access(&a_github_user());
        let claims = signer.verify(&token).expect("access token verifies");

        // Then it is tagged as an access-kind token with the short access lifetime
        assert_eq!(claims.kind, TokenKind::Access);
        assert_eq!(claims.exp - claims.iat, SESSION_TOKEN_TTL.as_secs());
    }

    #[test]
    fn a_minted_refresh_token_carries_the_refresh_kind_and_a_seven_day_lifetime() {
        // Given a signer
        let signer = SessionTokenSigner::new(b"shared-secret");

        // When a refresh token is minted and verified
        let token = signer.mint_refresh(&a_github_user());
        let claims = signer.verify(&token).expect("refresh token verifies");

        // Then it is tagged as a refresh-kind token with the 7-day lifetime
        assert_eq!(claims.kind, TokenKind::Refresh);
        assert_eq!(REFRESH_TOKEN_TTL, Duration::from_secs(7 * 24 * 60 * 60));
        assert_eq!(claims.exp - claims.iat, REFRESH_TOKEN_TTL.as_secs());
    }

    #[test]
    fn a_legacy_payload_without_a_kind_field_verifies_as_an_access_token() {
        // Given a token payload minted before the `kind` claim existed (no `kind` field)
        let legacy_json =
            r#"{"id":42,"login":"octocat","avatar_url":"a","name":"n","iat":0,"exp":9999999999}"#;

        // When it is deserialized into the current claims shape
        let claims: SessionClaims =
            serde_json::from_str(legacy_json).expect("legacy payload deserializes");

        // Then it defaults to an access-kind token, so tokens already in browsers keep working
        assert_eq!(claims.kind, TokenKind::Access);
    }

    #[test]
    fn verify_rejects_an_expired_token() {
        // Given a token issued ten minutes ago with a five-minute lifetime
        let signer = SessionTokenSigner::new(b"shared-secret");
        let issued_at = SystemTime::now() - Duration::from_secs(600);
        let token =
            signer.mint_with_issued_at(&a_github_user(), issued_at, Duration::from_secs(300));

        // When it is verified now
        let result = signer.verify(&token);

        // Then it is rejected as expired even though the signature is valid
        assert_eq!(result.unwrap_err(), SessionTokenError::Expired);
    }
}
