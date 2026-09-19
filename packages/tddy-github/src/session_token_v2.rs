//! Session tokens signed by the **minting daemon's own Ed25519 key**, verifiable by any daemon
//! that can resolve that key — with no secret shared between them.
//!
//! Token format: `v2.<base64url(json payload)>.<base64url(ed25519 signature)>`, the signature
//! covering the ASCII bytes `v2.<base64url(json payload)>`. The payload carries the GitHub
//! identity, `iat`/`exp`, and a **`kid`** naming the key that signed it. A verifier reads `kid`
//! first, resolves the public key it names, and only then checks the signature — which is what
//! lets a fleet authenticate each other's tokens without every daemon holding one secret.
//!
//! This module owns the format; it does not own **key material or key distribution**. It takes an
//! `ed25519_dalek::SigningKey` and hands back a `VerifyingKey`-shaped question, because
//! `tddy-github` sits *below* the daemon's identity boundary in the crate graph: the crate that
//! generates, persists and publishes the keypair (`tddy-daemon-auth`) depends on this one, so a
//! type from there cannot appear in these signatures.
//!
//! **This module replaces [`crate::session_token`].** The `v1` HMAC format is still compiled and
//! still serves every caller; the cutover — rewiring those callers and deleting `v1` — is the
//! rest of this PR's work, and the two formats never coexist in a release.

use std::fmt;
use std::time::{Duration, SystemTime};

use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::provider::GitHubUser;

// The lifetimes are a property of the session, not of the signature algorithm, so `v2` keeps the
// ones `v1` established rather than restating them.
pub use crate::session_token::{TokenKind, REFRESH_TOKEN_TTL, SESSION_TOKEN_TTL};

/// Version prefix / first token segment. A `v1` token presented to a `v2` verifier is
/// [`SessionTokenError::UnsupportedVersion`], never a signature failure — the distinction is what
/// makes a rollout diagnosable.
pub const TOKEN_VERSION: &str = "v2";

/// Names the key that signed a token: the SHA-256 digest of the key's SPKI DER encoding, base64url
/// without padding, truncated to 16 bytes.
///
/// Derived from the public key rather than assigned, so two daemons cannot collide on one and a
/// daemon cannot change its own id without changing its key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct KeyId(String);

impl KeyId {
    /// The id the given public key has, by definition.
    pub fn of(verifying_key: &VerifyingKey) -> Self {
        // TODO(signing-key): implement
        let _ = verifying_key;
        todo!("KeyId::of")
    }

    /// Read an id back off the wire, rejecting anything that is not the shape [`KeyId::of`] emits.
    pub fn parse(value: &str) -> Result<Self, SessionTokenError> {
        // TODO(signing-key): implement
        let _ = value;
        todo!("KeyId::parse")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for KeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a `v2` token carries. Identical to the `v1` payload but for [`SessionClaims::kid`], which
/// is what makes the token verifiable by a daemon that never held the signer's secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionClaims {
    /// The key that signed this token — see [`KeyId`].
    pub kid: KeyId,
    pub id: u64,
    pub login: String,
    pub avatar_url: String,
    pub name: String,
    pub iat: u64,
    pub exp: u64,
    #[serde(default)]
    pub kind: TokenKind,
}

/// Why a token was rejected.
///
/// [`SessionTokenError::UnknownKeyId`] is the one case a *retry* can fix: the token is
/// well-formed and names a daemon whose key this one has not learned yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionTokenError {
    /// The signature does not verify under the key the token names.
    InvalidSignature,
    /// Not three segments, or a segment that is not the base64url/JSON it must be.
    Malformed,
    /// Well-formed and correctly signed, but `exp` has passed.
    Expired,
    /// A token of a format this verifier does not implement — a `v1` token, most of all.
    UnsupportedVersion,
    /// Well-formed, but no public key is known for the `kid` it names.
    UnknownKeyId(KeyId),
}

impl fmt::Display for SessionTokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSignature => f.write_str("session token signature is not valid"),
            Self::Malformed => f.write_str("session token is malformed"),
            Self::Expired => f.write_str("session token has expired"),
            Self::UnsupportedVersion => f.write_str("session token is not a v2 token"),
            Self::UnknownKeyId(kid) => write!(f, "no public key is known for key id {kid}"),
        }
    }
}

impl std::error::Error for SessionTokenError {}

/// Mints `v2` tokens with one daemon's key.
///
/// Holds the private key, so exactly one of these exists per daemon and it is constructed from
/// whatever owns the key material — see `tddy_daemon_auth::DaemonSigningKey`.
pub struct SessionTokenSigner {
    // TODO(signing-key): implement
    _signing_key: SigningKey,
    _key_id: KeyId,
}

impl SessionTokenSigner {
    /// Take ownership of a daemon's signing key. `key_id` must be [`KeyId::of`] the key's public
    /// half; passing any other id mints tokens no one can verify.
    pub fn new(signing_key: SigningKey, key_id: KeyId) -> Self {
        // TODO(signing-key): implement
        let _ = (&signing_key, &key_id);
        todo!("SessionTokenSigner::new")
    }

    /// The id this signer stamps into every token it mints.
    pub fn key_id(&self) -> &KeyId {
        // TODO(signing-key): implement
        todo!("SessionTokenSigner::key_id")
    }

    /// The public half, for publishing to a key directory.
    pub fn verifying_key(&self) -> VerifyingKey {
        // TODO(signing-key): implement
        todo!("SessionTokenSigner::verifying_key")
    }

    pub fn mint_access(&self, user: &GitHubUser) -> String {
        // TODO(signing-key): implement
        let _ = user;
        todo!("SessionTokenSigner::mint_access")
    }

    pub fn mint_refresh(&self, user: &GitHubUser) -> String {
        // TODO(signing-key): implement
        let _ = user;
        todo!("SessionTokenSigner::mint_refresh")
    }

    pub fn mint(&self, user: &GitHubUser, ttl: Duration) -> String {
        // TODO(signing-key): implement
        let _ = (user, ttl);
        todo!("SessionTokenSigner::mint")
    }

    pub fn mint_with_issued_at(
        &self,
        user: &GitHubUser,
        issued_at: SystemTime,
        ttl: Duration,
    ) -> String {
        // TODO(signing-key): implement
        let _ = (user, issued_at, ttl);
        todo!("SessionTokenSigner::mint_with_issued_at")
    }

    pub fn mint_kind_with_issued_at(
        &self,
        user: &GitHubUser,
        kind: TokenKind,
        issued_at: SystemTime,
        ttl: Duration,
    ) -> String {
        // TODO(signing-key): implement
        let _ = (user, kind, issued_at, ttl);
        todo!("SessionTokenSigner::mint_kind_with_issued_at")
    }
}

/// Verifies `v2` tokens against a key the caller has already resolved.
///
/// Stateless and keyless on purpose: verification is two steps — read the `kid`, then check the
/// signature under the key that id names — and the step *between* them is a lookup this crate
/// cannot perform. Splitting them is what keeps the key directory out of `tddy-github`.
pub struct SessionTokenVerifier;

impl SessionTokenVerifier {
    /// The key id a token names, read **without** verifying anything.
    ///
    /// The result is untrusted — it says which key to fetch, nothing more. A token is only ever
    /// trusted after [`SessionTokenVerifier::verify`] returns for that key.
    pub fn key_id_of(token: &str) -> Result<KeyId, SessionTokenError> {
        // TODO(signing-key): implement
        let _ = token;
        todo!("SessionTokenVerifier::key_id_of")
    }

    /// Check `token`'s signature under `verifying_key` and its expiry against `now`.
    ///
    /// Rejects a token whose payload `kid` is not [`KeyId::of`] `verifying_key`: a caller that
    /// resolved the wrong key must be told so, not handed claims it did not check.
    pub fn verify(
        token: &str,
        verifying_key: &VerifyingKey,
        now: SystemTime,
    ) -> Result<SessionClaims, SessionTokenError> {
        // TODO(signing-key): implement
        let _ = (token, verifying_key, now);
        todo!("SessionTokenVerifier::verify")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THE_LOGIN: &str = "operator";

    #[test]
    fn a_token_verifies_under_the_public_half_of_the_key_that_signed_it() {
        // Given a daemon's signer
        let signer = a_signer_for(the_first_key());

        // When it mints an access token and that token is verified under its public key
        let token = signer.mint_access(&an_operator());
        let claims = SessionTokenVerifier::verify(&token, &the_first_key().verifying_key(), now());

        // Then the claims come back naming the operator and the key that signed them
        assert_eq!(
            claims.map(|claims| (claims.login, claims.kid)),
            Ok((
                THE_LOGIN.to_string(),
                KeyId::of(&the_first_key().verifying_key())
            ))
        );
    }

    #[test]
    fn verify_rejects_a_token_carrying_another_keys_signature() {
        // Given one daemon's token and another daemon's signature over its own token
        let token = a_signer_for(the_first_key()).mint_access(&an_operator());
        let forgery = a_signer_for(the_second_key()).mint_access(&an_operator());

        // When the second signature is spliced onto the first token's payload
        //
        // Both segments are canonical base64url the encoder itself produced, so this rejection is
        // the signature check and nothing else — unlike editing a character of the encoding, which
        // a decoder may refuse before any key is consulted.
        let spliced = format!(
            "{}.{}",
            payload_segment(&token),
            signature_segment(&forgery)
        );

        // Then it is refused as an invalid signature
        assert_eq!(
            SessionTokenVerifier::verify(&spliced, &the_first_key().verifying_key(), now())
                .map(|_| ()),
            Err(SessionTokenError::InvalidSignature)
        );
    }

    #[test]
    fn verify_rejects_a_v1_token_as_an_unsupported_version() {
        // Given a token in the HMAC format this one replaces
        let v1 = crate::session_token::SessionTokenSigner::new(b"a-fleet-secret")
            .mint_access(&an_operator());

        // When a v2 verifier is given it
        let refusal = SessionTokenVerifier::verify(&v1, &the_first_key().verifying_key(), now());

        // Then it says so by version, not by signature — a rollout must be diagnosable
        assert_eq!(
            refusal.map(|_| ()),
            Err(SessionTokenError::UnsupportedVersion)
        );
    }

    #[test]
    fn verify_rejects_a_token_that_names_a_key_other_than_the_one_supplied() {
        // Given a token one daemon signed
        let token = a_signer_for(the_first_key()).mint_access(&an_operator());

        // When it is verified against a different daemon's public key
        let refusal =
            SessionTokenVerifier::verify(&token, &the_second_key().verifying_key(), now());

        // Then it is refused — a caller that resolved the wrong key is told so, not handed claims
        assert_eq!(
            refusal.map(|_| ()),
            Err(SessionTokenError::InvalidSignature)
        );
    }

    #[test]
    fn verify_rejects_a_token_whose_expiry_has_passed() {
        // Given a token minted an hour before it is presented, with a five-minute life
        let issued_at = now() - Duration::from_secs(3600);
        let token = a_signer_for(the_first_key()).mint_kind_with_issued_at(
            &an_operator(),
            TokenKind::Access,
            issued_at,
            SESSION_TOKEN_TTL,
        );

        // When it is verified
        let refusal = SessionTokenVerifier::verify(&token, &the_first_key().verifying_key(), now());

        // Then it is expired
        assert_eq!(refusal.map(|_| ()), Err(SessionTokenError::Expired));
    }

    #[test]
    fn the_key_id_of_a_token_is_readable_without_verifying_it() {
        // Given a token whose signature has been replaced with another key's
        let token = a_signer_for(the_first_key()).mint_access(&an_operator());
        let forgery = a_signer_for(the_second_key()).mint_access(&an_operator());
        let spliced = format!(
            "{}.{}",
            payload_segment(&token),
            signature_segment(&forgery)
        );

        // When its key id is read
        let named = SessionTokenVerifier::key_id_of(&spliced);

        // Then the id is still legible — it is what tells a verifier which key to fetch, and the
        // token is trusted only once that key verifies it
        assert_eq!(named, Ok(KeyId::of(&the_first_key().verifying_key())));
    }

    #[test]
    fn a_key_id_identifies_one_key_and_distinguishes_it_from_another() {
        // Given two daemons' public keys
        let first = the_first_key().verifying_key();
        let second = the_second_key().verifying_key();

        // Then each key has one id, and no two keys share one
        assert_eq!(KeyId::of(&first), KeyId::of(&first));
        assert_ne!(KeyId::of(&first), KeyId::of(&second));
    }

    #[test]
    fn a_refresh_token_is_distinguishable_from_an_access_token() {
        // Given one signer minting both credentials for one operator
        let signer = a_signer_for(the_first_key());

        // When each is verified
        let access = the_kind_of(&signer.mint_access(&an_operator()));
        let refresh = the_kind_of(&signer.mint_refresh(&an_operator()));

        // Then the two roles stay separate: an access token cannot mint, a refresh token cannot
        // authenticate an RPC, and the kind is what enforces it
        assert_eq!((access, refresh), (TokenKind::Access, TokenKind::Refresh));
    }

    #[test]
    fn a_malformed_token_is_refused_as_malformed() {
        // Given something that is not a token at all
        let refusal =
            SessionTokenVerifier::verify("not-a-token", &the_first_key().verifying_key(), now());

        // Then it is refused for its shape, before any key is consulted
        assert_eq!(refusal.map(|_| ()), Err(SessionTokenError::Malformed));
    }

    fn the_kind_of(token: &str) -> TokenKind {
        SessionTokenVerifier::verify(token, &the_first_key().verifying_key(), now())
            .expect("a freshly minted token verifies")
            .kind
    }

    /// The payload half of a token — `v2.<payload>` — which is what the signature covers.
    fn payload_segment(token: &str) -> String {
        token
            .rsplit_once('.')
            .expect("a token has three segments")
            .0
            .to_string()
    }

    fn signature_segment(token: &str) -> String {
        token
            .rsplit_once('.')
            .expect("a token has three segments")
            .1
            .to_string()
    }

    fn a_signer_for(key: SigningKey) -> SessionTokenSigner {
        let key_id = KeyId::of(&key.verifying_key());
        SessionTokenSigner::new(key, key_id)
    }

    /// Fixed key material, so a failure names a behaviour rather than a seed.
    fn the_first_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn the_second_key() -> SigningKey {
        SigningKey::from_bytes(&[9u8; 32])
    }

    fn an_operator() -> GitHubUser {
        GitHubUser {
            id: 1,
            login: THE_LOGIN.to_string(),
            avatar_url: String::new(),
            name: THE_LOGIN.to_string(),
        }
    }

    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
    }
}
