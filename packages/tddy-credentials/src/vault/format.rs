//! The file itself: its serialised shape, the header check that runs before any decrypt, and the
//! one way it is read and replaced.

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use super::VaultError;

/// The one format version this build reads and writes.
pub(super) const FORMAT_VERSION: u32 = 1;
/// The one key derivation this build uses.
pub(super) const KDF: &str = "hkdf-sha256";
pub(super) const KDF_VERSION: u32 = 1;
/// Prefix of the HKDF `info`; the subject follows it.
const INFO_PREFIX: &str = "tddy-credentials/v1/";
/// Owner-only: the file is ciphertext, but a readable one is a copy for an offline attempt.
const OWNER_ONLY_FILE: u32 = 0o600;

/// Serialises every read-modify-write of a vault file in this process.
///
/// Two sessions of the same user are two [`super::SessionVault`]s over one file, so a per-handle
/// lock would let the second writer drop the first writer's record. Process-wide for the reason the
/// token store this replaces gave: nothing guarantees one handle owns a path.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn serialised() -> std::sync::MutexGuard<'static, ()> {
    // A poisoned lock means an earlier write panicked; the state it guards is re-read from disk
    // under the lock, so there is no torn in-memory state to inherit.
    WRITE_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Header {
    pub(super) format_version: u32,
    pub(super) kdf: String,
    pub(super) kdf_version: u32,
    pub(super) salt: String,
    pub(super) info: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Sealed {
    pub(super) nonce: String,
    pub(super) ciphertext: String,
}

/// One browser session lineage's wrap of the data key. The key that opens it is not in the file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct UnlockSlot {
    pub(super) id: String,
    pub(super) nonce: String,
    pub(super) ciphertext: String,
}

/// Field order matters to anyone reading the file: the ciphertext is last in each record, and the
/// records are last in the file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SealedRecord {
    pub(super) id: String,
    pub(super) nonce: String,
    pub(super) ciphertext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct VaultFile {
    pub(super) header: Header,
    pub(super) wrapped_data_key: Sealed,
    pub(super) unlock_slots: Vec<UnlockSlot>,
    pub(super) verifier: Sealed,
    pub(super) records: Vec<SealedRecord>,
}

pub(super) fn info_for(subject: &str) -> String {
    format!("{INFO_PREFIX}{subject}")
}

pub(super) fn header_aad(header: &Header) -> Result<Vec<u8>, VaultError> {
    serde_json::to_vec(header).map_err(|_| VaultError::Crypto)
}

/// Read and parse the vault at `path`; `Ok(None)` when there is no file.
pub(super) fn read_vault_file(path: &Path) -> Result<Option<VaultFile>, VaultError> {
    let raw = match std::fs::read(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(VaultError::Io(format!("reading {}: {e}", path.display()))),
    };
    let document: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|_| VaultError::Crypto)?;
    check_format(&document)?;
    serde_json::from_value(document)
        .map(Some)
        .map_err(|_| VaultError::Crypto)
}

/// Refuse, by name, a header this build does not write — before anything tries to decrypt.
fn check_format(document: &serde_json::Value) -> Result<(), VaultError> {
    let field = |name: &str| document.get("header").and_then(|header| header.get(name));
    let mismatch = |expected: String, found: String| VaultError::FormatMismatch { expected, found };

    let format_version = field("format_version");
    if format_version != Some(&serde_json::Value::from(FORMAT_VERSION)) {
        return Err(mismatch(
            FORMAT_VERSION.to_string(),
            describe(format_version),
        ));
    }
    let kdf = field("kdf");
    if kdf.and_then(serde_json::Value::as_str) != Some(KDF) {
        return Err(mismatch(KDF.to_string(), describe(kdf)));
    }
    let kdf_version = field("kdf_version");
    if kdf_version != Some(&serde_json::Value::from(KDF_VERSION)) {
        return Err(mismatch(
            format!("{KDF} v{KDF_VERSION}"),
            format!("{KDF} v{}", describe(kdf_version)),
        ));
    }
    Ok(())
}

fn describe(value: Option<&serde_json::Value>) -> String {
    match value {
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => "an unversioned".to_string(),
    }
}

pub(super) fn write_vault_file(path: &Path, file: &VaultFile) -> Result<(), VaultError> {
    let bytes = serde_json::to_vec_pretty(file)
        .map_err(|e| VaultError::Io(format!("serialising {}: {e}", path.display())))?;
    tddy_core::atomic_file::write_atomic_with_mode(path, bytes, OWNER_ONLY_FILE)
        .map_err(|e| VaultError::Io(format!("writing {}: {e}", path.display())))
}
