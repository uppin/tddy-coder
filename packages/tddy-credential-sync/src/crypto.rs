//! The primitives this crate's two cryptographic surfaces are built from: the HMAC
//! [`GroupSecret::proof_for`](crate::transport::GroupSecret::proof_for) computes, the HKDF
//! [`VaultTransportKey::shared_secret`](crate::transport_key::VaultTransportKey::shared_secret)
//! derives through, and the AEAD a [`WrappedRecords`](crate::transport::WrappedRecords) payload is
//! sealed and opened with.
//!
//! Hand-rolled HMAC/HKDF and the same AEAD choice as `tddy-credentials`, at the same dependency
//! versions, for the same reason: these are already this workspace's primitives, not a new
//! decision made here.

use chacha20poly1305::aead::{Aead, OsRng, Payload};
use chacha20poly1305::{AeadCore, ChaCha20Poly1305, KeyInit as AeadKeyInit};
use hmac::{Hmac, KeyInit as HmacKeyInit, Mac};
use sha2::Sha256;
use zeroize::Zeroize;

use tddy_credentials::SecretBytes;

type HmacSha256 = Hmac<Sha256>;

/// `N` bytes from the OS's generator — the fresh challenge a self-advertisement's group proof
/// answers.
pub(crate) fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
    bytes
}

/// `HMAC-SHA256(key, message)`.
pub(crate) fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    // HMAC accepts a key of any length, so construction cannot fail.
    let mut mac: HmacSha256 =
        HmacKeyInit::new_from_slice(key).expect("HMAC-SHA256 takes a key of any length");
    mac.update(message);
    let mut out = [0u8; 32];
    out.copy_from_slice(&mac.finalize().into_bytes());
    out
}

/// HKDF-Expand to 32 bytes from key material that is already uniformly random — an X25519
/// agreement, turned into a sealing key under a purpose label.
pub(crate) fn hkdf_expand(ikm: &[u8], info: &[u8]) -> SecretBytes {
    let mut out = hmac_sha256(ikm, &[info, &[0x01]].concat());
    let key = SecretBytes::new(out);
    out.zeroize();
    key
}

/// One sealed unit: a fresh nonce and the ciphertext it authenticates.
pub(crate) struct Sealed {
    pub(crate) nonce: [u8; 12],
    pub(crate) ciphertext: Vec<u8>,
}

fn cipher(key: &SecretBytes) -> ChaCha20Poly1305 {
    AeadKeyInit::new(key.expose().into())
}

/// Seal `plaintext` under `key`, with `aad` authenticated but not encrypted.
pub(crate) fn seal(key: &SecretBytes, plaintext: &[u8], aad: &[u8]) -> Sealed {
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher(key)
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .expect("encryption under a freshly generated nonce does not fail");
    Sealed {
        nonce: nonce.into(),
        ciphertext,
    }
}

/// Open a sealed unit. Any failure — a tag that does not authenticate, including under the wrong
/// key or the wrong associated data — is `Err`.
pub(crate) fn open(
    key: &SecretBytes,
    nonce: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, ()> {
    if nonce.len() != 12 {
        return Err(());
    }
    cipher(key)
        .decrypt(
            chacha20poly1305::Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| ())
}
