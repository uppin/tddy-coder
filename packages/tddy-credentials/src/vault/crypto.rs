//! The sealing primitives every part of the vault is built from: one AEAD, the key-encryption
//! keys each wrap slot derives, and the verifier that proves a data key still opens the file.

use chacha20poly1305::aead::{Aead, OsRng, Payload};
use chacha20poly1305::{AeadCore, ChaCha20Poly1305, KeyInit};
use rand::RngCore;
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, Zeroizing};

use super::format::{header_aad, Header, Sealed, FORMAT_VERSION};
use super::VaultError;
use crate::kdf::{from_hex, hkdf_expand, hkdf_sha256, to_hex};
use crate::secret::{SecretBytes, SecretString};

/// What the verifier seals.
pub(super) const VERIFIER_PLAINTEXT: &[u8] = b"tddy-credentials/v2/verifier";
pub(super) const VERIFIER_AAD: &[u8] = b"tddy-credentials/v2/verifier";
/// Prefix of a record's associated data; its `id` follows it.
const RECORD_AAD_PREFIX: &[u8] = b"tddy-credentials/v2/record/";
/// Domain separation for the passphrase slot's key: the subject follows it.
///
/// Every HKDF `info` in this crate carries a label — `passphrase/`, `unlock/`, `record-id` — so no
/// subject or slot id can make one derivation's input read as another's.
const PASSPHRASE_INFO_PREFIX: &str = "tddy-credentials/v2/passphrase/";
/// Domain separation for an unlock slot's key: the slot id and subject follow it.
const UNLOCK_INFO_PREFIX: &str = "tddy-credentials/v2/unlock/";
/// The derivation an unlock slot is made under, named in its associated data.
const UNLOCK_KDF: &str = "hkdf-sha256";
/// ChaCha20-Poly1305's nonce: 96 bits, drawn fresh from the OS for every seal.
const NONCE_BYTES: usize = 12;

pub(super) fn record_aad(id: &str) -> Vec<u8> {
    [RECORD_AAD_PREFIX, id.as_bytes()].concat()
}

/// `N` bytes from the OS's generator.
pub(super) fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes
}

/// A fresh 32-byte key, owned from the moment it exists.
pub(super) fn random_key() -> SecretBytes {
    let mut fresh = random_bytes::<32>();
    let key = SecretBytes::new(fresh);
    fresh.zeroize();
    key
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
        "{FORMAT_VERSION}/{UNLOCK_KDF}/{}",
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
    // A key that does not unwrap the data key is the wrong key — a wrong passphrase, a slot
    // re-wrapped since, or another subject's file — never corruption to be "repaired".
    let unwrapped =
        Zeroizing::new(open(kek, nonce, ciphertext, aad).map_err(|_| VaultError::Locked)?);
    let mut bytes = <[u8; 32]>::try_from(unwrapped.as_slice()).map_err(|_| VaultError::Crypto)?;
    let data_key = SecretBytes::new(bytes);
    bytes.zeroize();
    Ok(data_key)
}

/// The passphrase slot's KEK: Argon2id over the passphrase with the header's salt and costs, then
/// HKDF-Expand with the subject in `info`, so one passphrase never opens another subject's vault.
///
/// The header's costs have already been checked against this build's by name; they are read from
/// the header anyway, so the derivation is exactly the one the file describes.
pub(super) fn passphrase_kek(
    header: &Header,
    passphrase: &SecretString,
    subject: &str,
) -> Result<SecretBytes, VaultError> {
    let salt = from_hex(&header.salt).ok_or(VaultError::Crypto)?;
    let params = argon2::Params::new(header.m_cost_kib, header.t_cost, header.p_cost, Some(32))
        .map_err(|_| VaultError::Crypto)?;
    let argon2 = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut stretched = [0u8; 32];
    let derived = argon2.hash_password_into(passphrase.expose().as_bytes(), &salt, &mut stretched);
    let prk = SecretBytes::new(stretched);
    stretched.zeroize();
    derived.map_err(|_| VaultError::Crypto)?;
    Ok(hkdf_expand(
        &prk,
        format!("{PASSPHRASE_INFO_PREFIX}{subject}").as_bytes(),
    ))
}

/// Wrap `data_key` in the passphrase slot, with the header as associated data.
pub(super) fn wrap_under_passphrase(
    header: &Header,
    passphrase: &SecretString,
    subject: &str,
    data_key: &SecretBytes,
) -> Result<Sealed, VaultError> {
    let kek = passphrase_kek(header, passphrase, subject)?;
    seal(&kek, data_key.expose(), &header_aad(header)?)
}

/// The cipher for one seal or open. It holds its own copy of `key`, which it wipes as it drops —
/// and so does the one-time Poly1305 key it derives per message (`poly1305`'s `zeroize`).
fn cipher(key: &SecretBytes) -> ChaCha20Poly1305 {
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
    if nonce.len() != NONCE_BYTES {
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
