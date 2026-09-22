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
//! type from there cannot appear in these signatures. What it offers that crate instead is
//! [`SessionTokenAuthority`] — "is this token genuine, whoever signed it" — which the OAuth service
//! here verifies through without knowing how a peer's key is found.
//!
//! See `packages/tddy-github/docs/session-token.md` for the format in full.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::pkcs8::EncodePublicKey;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::provider::GitHubUser;

/// The key types this module's signatures are written in, so a dependent can name them without
/// depending on `ed25519-dalek` itself.
pub use ed25519_dalek::{SigningKey as Ed25519SigningKey, VerifyingKey as Ed25519VerifyingKey};

/// Version prefix / first token segment. A `v1` token presented to a `v2` verifier is
/// [`SessionTokenError::UnsupportedVersion`], never a signature failure — the distinction is what
/// makes a rollout diagnosable.
pub const TOKEN_VERSION: &str = "v2";

/// Lifetime of a freshly minted session token. Short by design: the web client refreshes well
/// before expiry (see [`crate::auth_service`]/`RefreshSession`), and a leaked token is only
/// valid for this window.
pub const SESSION_TOKEN_TTL: Duration = Duration::from_secs(5 * 60);

/// Lifetime of a freshly minted refresh token. Long by design and slid forward on every refresh:
/// an actively-used session never has to re-login, while a device untouched for this long does.
/// The refresh token is never sent on normal RPCs — it is used only to mint access tokens.
pub const REFRESH_TOKEN_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// How many bytes of the SPKI digest a [`KeyId`] keeps. 128 bits: far past any collision a fleet
/// of daemons could produce, and short enough to read in a log line.
const KEY_ID_BYTES: usize = 16;

/// Which credential a token is: a short-lived [`TokenKind::Access`] token that authenticates
/// RPCs, or a long-lived [`TokenKind::Refresh`] token that only mints access tokens. Enforcing
/// the kind keeps the two roles strictly separate — an access token cannot mint, and a refresh
/// token cannot authenticate an RPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TokenKind {
    /// Short-lived credential presented on every RPC. The default for a payload with no `kind`
    /// field.
    #[default]
    Access,
    /// Long-lived credential presented only to `RefreshSession` to mint access tokens.
    Refresh,
}

/// Names the key that signed a token: the SHA-256 digest of the key's SPKI DER encoding, base64url
/// without padding, truncated to 16 bytes.
///
/// Derived from the public key rather than assigned, so two daemons cannot collide on one and a
/// daemon cannot change its own id without changing its key. The same derivation is also why a
/// resolved key never goes stale: an id names exactly one key, forever.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct KeyId(String);

impl KeyId {
    /// The id the given public key has, by definition.
    pub fn of(verifying_key: &VerifyingKey) -> Self {
        let digest = Sha256::digest(spki_der(verifying_key));
        Self(URL_SAFE_NO_PAD.encode(&digest[..KEY_ID_BYTES]))
    }

    /// Read an id back off the wire, rejecting anything that is not the shape [`KeyId::of`] emits.
    ///
    /// "The shape" is exact: 16 bytes in canonical unpadded base64url. A non-canonical spelling of
    /// the same bytes is refused rather than normalised, so one key can never be named two ways.
    pub fn parse(value: &str) -> Result<Self, SessionTokenError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(value)
            .map_err(|_| SessionTokenError::Malformed)?;
        if bytes.len() != KEY_ID_BYTES || URL_SAFE_NO_PAD.encode(&bytes) != value {
            return Err(SessionTokenError::Malformed);
        }
        Ok(Self(value.to_string()))
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

impl TryFrom<String> for KeyId {
    type Error = SessionTokenError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<KeyId> for String {
    fn from(key_id: KeyId) -> Self {
        key_id.0
    }
}

/// A public key's SPKI DER encoding — what a daemon publishes, and what a [`KeyId`] digests.
pub fn spki_der(verifying_key: &VerifyingKey) -> Vec<u8> {
    verifying_key
        .to_public_key_der()
        // An Ed25519 key is 32 bytes under one fixed algorithm identifier; there is no input on
        // which the encoder can fail.
        .expect("an Ed25519 public key always encodes as SPKI DER")
        .as_bytes()
        .to_vec()
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

impl SessionClaims {
    /// The GitHub identity these claims assert.
    pub fn user(&self) -> GitHubUser {
        GitHubUser {
            id: self.id,
            login: self.login.clone(),
            avatar_url: self.avatar_url.clone(),
            name: self.name.clone(),
        }
    }
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

/// Decides whether a session token is genuine, whichever daemon signed it.
///
/// The seam between the format (this crate) and key distribution (the daemon's identity crate):
/// [`crate::AuthServiceImpl`] verifies through one of these and never learns how a peer's key was
/// found. `tddy_daemon_auth::DirectorySessionTokenVerifier` is the daemon's implementation.
#[async_trait]
pub trait SessionTokenAuthority: Send + Sync {
    /// The claims of `token` once its signature and expiry have been checked under the key it
    /// names; otherwise the reason it was refused.
    async fn verify(&self, token: &str) -> Result<SessionClaims, SessionTokenError>;
}

/// Mints `v2` tokens with one daemon's key.
///
/// Holds the private key, so it is constructed from whatever owns the key material — see
/// `tddy_daemon_auth::DaemonSigningKey`. Cloning copies the key; each copy zeroes its bytes on drop.
#[derive(Clone)]
pub struct SessionTokenSigner {
    signing_key: SigningKey,
    key_id: KeyId,
}

impl SessionTokenSigner {
    /// Take ownership of a daemon's signing key. `key_id` must be [`KeyId::of`] the key's public
    /// half; passing any other id mints tokens no one can verify.
    ///
    /// # Panics
    ///
    /// When `key_id` is not the key's own id. That is a wiring fault, and a signer built from it
    /// would mint tokens every verifier refuses — failing here names the fault where it was made.
    pub fn new(signing_key: SigningKey, key_id: KeyId) -> Self {
        assert_eq!(
            key_id,
            KeyId::of(&signing_key.verifying_key()),
            "a session-token signer's key id must be the id of its own public key"
        );
        Self {
            signing_key,
            key_id,
        }
    }

    /// The id this signer stamps into every token it mints.
    pub fn key_id(&self) -> &KeyId {
        &self.key_id
    }

    /// The public half, for publishing to a key directory.
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
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
    /// general seam behind the access/refresh helpers.
    pub fn mint_kind_with_issued_at(
        &self,
        user: &GitHubUser,
        kind: TokenKind,
        issued_at: SystemTime,
        ttl: Duration,
    ) -> String {
        let iat = unix_seconds(issued_at);
        let claims = SessionClaims {
            kid: self.key_id.clone(),
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
        let signing_input = format!("{TOKEN_VERSION}.{}", URL_SAFE_NO_PAD.encode(payload_json));
        let signature = self.signing_key.sign(signing_input.as_bytes());
        format!(
            "{signing_input}.{}",
            URL_SAFE_NO_PAD.encode(signature.to_bytes())
        )
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
        Ok(ParsedToken::parse(token)?.claims.kid)
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
        let parsed = ParsedToken::parse(token)?;
        if parsed.claims.kid != KeyId::of(verifying_key) {
            return Err(SessionTokenError::InvalidSignature);
        }
        verifying_key
            .verify_strict(parsed.signing_input.as_bytes(), &parsed.signature)
            .map_err(|_| SessionTokenError::InvalidSignature)?;
        if unix_seconds(now) > parsed.claims.exp {
            return Err(SessionTokenError::Expired);
        }
        Ok(parsed.claims)
    }
}

/// A token taken apart, with nothing about it checked yet.
struct ParsedToken<'a> {
    /// `v2.<payload>` — the bytes the signature covers.
    signing_input: &'a str,
    claims: SessionClaims,
    signature: Signature,
}

impl<'a> ParsedToken<'a> {
    fn parse(token: &'a str) -> Result<Self, SessionTokenError> {
        let (signing_input, signature) =
            token.rsplit_once('.').ok_or(SessionTokenError::Malformed)?;
        let (version, payload) = signing_input
            .split_once('.')
            .ok_or(SessionTokenError::Malformed)?;
        if version != TOKEN_VERSION {
            return Err(if is_a_version_tag(version) {
                SessionTokenError::UnsupportedVersion
            } else {
                SessionTokenError::Malformed
            });
        }
        let payload = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| SessionTokenError::Malformed)?;
        let claims: SessionClaims =
            serde_json::from_slice(&payload).map_err(|_| SessionTokenError::Malformed)?;
        let signature: [u8; Signature::BYTE_SIZE] = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| SessionTokenError::Malformed)?
            .try_into()
            .map_err(|_| SessionTokenError::Malformed)?;
        Ok(Self {
            signing_input,
            claims,
            signature: Signature::from_bytes(&signature),
        })
    }
}

/// `v` followed by digits: the first segment of *some* token format, just not this one.
fn is_a_version_tag(segment: &str) -> bool {
    segment
        .strip_prefix('v')
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

/// Whole seconds since the Unix epoch; times at or before the epoch clamp to 0.
fn unix_seconds(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

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
        let v1 = a_v1_token();

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

    #[test]
    fn verify_rejects_a_token_with_a_tampered_signature() {
        // Given a valid token whose signature has had one bit flipped
        let token = a_signer_for(the_first_key()).mint_access(&an_operator());
        let tampered = with_tampered_signature(&token);

        // When it is verified
        let refusal =
            SessionTokenVerifier::verify(&tampered, &the_first_key().verifying_key(), now());

        // Then the signature check is what refuses it
        assert_eq!(
            refusal.map(|_| ()),
            Err(SessionTokenError::InvalidSignature)
        );
    }

    #[test]
    fn a_minted_token_carries_the_identity_and_lifetime_it_was_minted_with() {
        // Given a signer and the operator it mints for
        let signer = a_signer_for(the_first_key());

        // When a token with a five-minute life is minted and verified
        let token = signer.mint_with_issued_at(&an_operator(), now(), Duration::from_secs(300));
        let claims = SessionTokenVerifier::verify(&token, &the_first_key().verifying_key(), now())
            .expect("a freshly minted token verifies");

        // Then the operator comes back intact, valid for exactly the requested window
        assert_eq!(
            (
                claims.user().login,
                claims.user().id,
                claims.exp - claims.iat
            ),
            (THE_LOGIN.to_string(), an_operator().id, 300)
        );
    }

    #[test]
    fn an_access_token_lives_five_minutes_and_a_refresh_token_seven_days() {
        // Given one signer minting both credentials
        let signer = a_signer_for(the_first_key());

        // When each is minted and verified
        let lifetime = |token: &str| {
            let claims = SessionTokenVerifier::verify(
                token,
                &the_first_key().verifying_key(),
                SystemTime::now(),
            )
            .expect("a freshly minted token verifies");
            claims.exp - claims.iat
        };

        // Then the access token is short-lived and the refresh token outlives it by a week
        assert_eq!(
            (
                lifetime(&signer.mint_access(&an_operator())),
                lifetime(&signer.mint_refresh(&an_operator()))
            ),
            (5 * 60, 7 * 24 * 60 * 60)
        );
    }

    #[test]
    fn a_key_id_read_off_the_wire_must_be_exactly_the_shape_a_key_produces() {
        // Given a genuine key id, and two strings that only resemble one
        let genuine = KeyId::of(&the_first_key().verifying_key());
        let too_short = &genuine.as_str()[..10];
        let not_base64url = "!!!!!!!!!!!!!!!!!!!!!!";

        // When each is parsed
        let parsed = [genuine.as_str(), too_short, not_base64url].map(KeyId::parse);

        // Then only the genuine one is an id — a looser parse would let one key be named two ways
        assert_eq!(
            parsed,
            [
                Ok(genuine.clone()),
                Err(SessionTokenError::Malformed),
                Err(SessionTokenError::Malformed)
            ]
        );
    }

    /// Flip one bit of the *decoded* signature and re-encode it.
    ///
    /// Tampering with a base64url character instead is not a signature change at all when the
    /// character is the last one: a 64-byte signature leaves four must-be-zero bits there, so most
    /// substitutions yield a non-canonical encoding that fails to *decode* — `Malformed`, not
    /// `InvalidSignature` — depending on what the untampered signature happened to end in. Working
    /// on the bytes makes the tampering the only thing that differs, on every run.
    fn with_tampered_signature(token: &str) -> String {
        let mut signature = URL_SAFE_NO_PAD
            .decode(signature_segment(token))
            .expect("a minted token's signature is base64url");
        signature[0] ^= 0x01;
        format!(
            "{}.{}",
            payload_segment(token),
            URL_SAFE_NO_PAD.encode(signature)
        )
    }

    fn the_kind_of(token: &str) -> TokenKind {
        SessionTokenVerifier::verify(token, &the_first_key().verifying_key(), now())
            .expect("a freshly minted token verifies")
            .kind
    }

    /// A token in the retired `v1` shape — `v1.<payload>.<32-byte HMAC tag>` — as a browser that
    /// signed in before the cutover still holds one.
    fn a_v1_token() -> String {
        let payload = URL_SAFE_NO_PAD.encode(
            r#"{"id":1,"login":"operator","avatar_url":"","name":"operator","iat":0,"exp":9999999999,"kind":"access"}"#,
        );
        let tag = URL_SAFE_NO_PAD.encode([0x5au8; 32]);
        format!("v1.{payload}.{tag}")
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

    /// The clock tokens are verified against. The real one, because `mint_access` and
    /// `mint_refresh` stamp the real one — a fixed instant would drift past their expiry.
    fn now() -> SystemTime {
        SystemTime::now()
    }
}
