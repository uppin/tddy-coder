//! The sealing primitives every part of the vault is built from: one AEAD, the key-encryption
//! keys each wrap slot derives, and the verifier that proves a data key still opens the file.

use chacha20poly1305::aead::{Aead, OsRng, Payload};
use chacha20poly1305::{AeadCore, ChaCha20Poly1305, KeyInit};
use rand::RngCore;
use subtle::ConstantTimeEq;

use super::format::{header_aad, info_for, Header, Sealed, FORMAT_VERSION, KDF, KDF_VERSION};
use super::VaultError;
use crate::kdf::{from_hex, hkdf_sha256, to_hex};
use crate::secret::{wipe, SecretBytes};

/// What the verifier seals.
pub(super) const VERIFIER_PLAINTEXT: &[u8] = b"tddy-credentials/v1/verifier";
pub(super) const VERIFIER_AAD: &[u8] = b"tddy-credentials/v1/verifier";
/// Prefix of a record's associated data; its `id` follows it.
const RECORD_AAD_PREFIX: &[u8] = b"tddy-credentials/v1/record/";
/// Domain separation for an unlock slot's derivation; the slot id and subject follow it.
const UNLOCK_INFO_PREFIX: &str = "tddy-credentials/v1/unlock/";

pub(super) fn record_aad(id: &str) -> Vec<u8> {
    [RECORD_AAD_PREFIX, id.as_bytes()].concat()
}

pub(super) fn random_16() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes
}

pub(super) fn random_32() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes
}

fn unlock_info(slot_id: &str, subject: &str) -> String {
    format!("{UNLOCK_INFO_PREFIX}{slot_id}/{subject}")
}

/// An unlock slot's KEK. The unlock key is already 32 uniformly random bytes, so the HKDF salt is
/// empty; the slot id and subject in `info` bind the key to the one slot of the one vault.
pub(super) fn unlock_kek(unlock_key: &SecretBytes, slot_id: &str, subject: &str) -> SecretBytes {
    hkdf_sha256(
        &[],
        unlock_key.expose(),
        unlock_info(slot_id, subject).as_bytes(),
    )
}

/// An unlock slot's associated data: the derivation it was made under, and which slot it is.
pub(super) fn unlock_aad(slot_id: &str, subject: &str) -> Vec<u8> {
    format!(
        "{FORMAT_VERSION}/{KDF}/{KDF_VERSION}/{}",
        unlock_info(slot_id, subject)
    )
    .into_bytes()
}

/// Unwrap a data key from one wrap slot. A key that does not open it is [`VaultError::Locked`].
pub(super) fn unwrap_data_key(
    kek: &SecretBytes,
    nonce: &str,
    ciphertext: &str,
    aad: &[u8],
) -> Result<SecretBytes, VaultError> {
    // A key that does not unwrap the data key is the wrong key — the expected consequence of a
    // rotated credential, a slot re-wrapped since, or another subject's file — never corruption to
    // be "repaired".
    let mut unwrapped = open(kek, nonce, ciphertext, aad).map_err(|_| VaultError::Locked)?;
    let Ok(mut bytes) = <[u8; 32]>::try_from(unwrapped.as_slice()) else {
        wipe(&mut unwrapped);
        return Err(VaultError::Crypto);
    };
    wipe(&mut unwrapped);
    let data_key = SecretBytes::new(bytes);
    wipe(&mut bytes);
    Ok(data_key)
}

pub(super) fn derive_kek(
    header: &Header,
    ikm: &[u8],
    subject: &str,
) -> Result<SecretBytes, VaultError> {
    let salt = from_hex(&header.salt).ok_or(VaultError::Crypto)?;
    Ok(hkdf_sha256(&salt, ikm, info_for(subject).as_bytes()))
}

pub(super) fn wrap_data_key(
    header: &Header,
    ikm: &[u8],
    subject: &str,
    data_key: &SecretBytes,
) -> Result<Sealed, VaultError> {
    let kek = derive_kek(header, ikm, subject)?;
    seal(&kek, data_key.expose(), &header_aad(header)?)
}

fn cipher(key: &SecretBytes) -> ChaCha20Poly1305 {
    // TODO(keyring): the cipher holds its own copy of the key schedule, which is not wiped when it
    // drops; that needs `chacha20poly1305`'s `zeroize` feature, a CLAUDE.md § ASK decision.
    ChaCha20Poly1305::new(key.expose().into())
}

pub(super) fn seal(key: &SecretBytes, plaintext: &[u8], aad: &[u8]) -> Result<Sealed, VaultError> {
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher(key)
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| VaultError::Crypto)?;
    Ok(Sealed {
        nonce: to_hex(&nonce),
        ciphertext: to_hex(&ciphertext),
    })
}

/// Open one sealed unit. Any failure — malformed hex, a wrong nonce length, a tag that does not
/// authenticate — is [`VaultError::Crypto`]; the caller decides when that means `Locked` instead.
pub(super) fn open(
    key: &SecretBytes,
    nonce: &str,
    ciphertext: &str,
    aad: &[u8],
) -> Result<Vec<u8>, VaultError> {
    let nonce = from_hex(nonce).ok_or(VaultError::Crypto)?;
    let ciphertext = from_hex(ciphertext).ok_or(VaultError::Crypto)?;
    if nonce.len() != 12 {
        return Err(VaultError::Crypto);
    }
    cipher(key)
        .decrypt(
            chacha20poly1305::Nonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad,
            },
        )
        .map_err(|_| VaultError::Crypto)
}

pub(super) fn check_verifier(verifier: &Sealed, data_key: &SecretBytes) -> Result<(), VaultError> {
    let plaintext = open(
        data_key,
        &verifier.nonce,
        &verifier.ciphertext,
        VERIFIER_AAD,
    )?;
    if bool::from(plaintext.as_slice().ct_eq(VERIFIER_PLAINTEXT)) {
        Ok(())
    } else {
        Err(VaultError::Crypto)
    }
}
