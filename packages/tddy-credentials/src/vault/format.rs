//! The file itself: its serialised shape, the header check that runs before any decrypt, and the
//! one way it is read and replaced.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use super::{VaultError, MAX_SET_ASIDE_VAULTS};

/// The one format version this build reads and writes. Version 1 derived its key from a login
/// token and was never deployed outside tests, so it is not migrated — it is refused by name.
pub(super) const FORMAT_VERSION: u32 = 2;
/// The one passphrase derivation this build uses.
pub(super) const KDF: &str = "argon2id";
/// Argon2 v1.3 (`0x13`), the version RFC 9106 specifies.
pub(super) const KDF_VERSION: u32 = 0x13;
/// Argon2id's memory cost, in KiB: 19 MiB.
///
/// With [`T_COST`] and [`P_COST`], the first of OWASP's recommended Argon2id configurations
/// (Password Storage Cheat Sheet) and `argon2`'s own default. It is paid once per passphrase
/// unlock, create or reset — never per request — so tens of milliseconds on the daemon's host is
/// the right order of cost; the same guess offline costs the attacker the same memory.
pub(super) const M_COST_KIB: u32 = 19_456;
/// Argon2id's time cost: passes over the memory.
pub(super) const T_COST: u32 = 2;
/// Argon2id's parallelism: lanes.
pub(super) const P_COST: u32 = 1;
/// Bytes of random salt per vault — RFC 9106 recommends 16.
pub(super) const SALT_BYTES: usize = 16;

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

/// Everything a later build needs to reproduce the passphrase derivation, and nothing it could be
/// weakened through: every parameter is checked against this build's before anything is derived,
/// and the whole header is the passphrase slot's associated data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Header {
    pub(super) format_version: u32,
    pub(super) kdf: String,
    pub(super) kdf_version: u32,
    pub(super) m_cost_kib: u32,
    pub(super) t_cost: u32,
    pub(super) p_cost: u32,
    pub(super) salt: String,
}

impl Header {
    /// A header for a new vault: this build's derivation, under a fresh salt.
    pub(super) fn fresh(salt: &[u8]) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            kdf: KDF.to_string(),
            kdf_version: KDF_VERSION,
            m_cost_kib: M_COST_KIB,
            t_cost: T_COST,
            p_cost: P_COST,
            salt: crate::kdf::to_hex(salt),
        }
    }
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
    // Checked by name rather than handed to Argon2: a lowered cost is a weakened derivation this
    // build must not run, and a raised one is a file that could exhaust the daemon's memory.
    let costs = [field("m_cost_kib"), field("t_cost"), field("p_cost")];
    let expected = [M_COST_KIB, T_COST, P_COST];
    if costs
        .iter()
        .zip(expected)
        .any(|(found, expected)| *found != Some(&serde_json::Value::from(expected)))
    {
        let [m, t, p] = costs.map(describe);
        return Err(mismatch(
            format!("{KDF} m={M_COST_KIB},t={T_COST},p={P_COST}"),
            format!("{KDF} m={m},t={t},p={p}"),
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

/// Rename the vault at `path` aside, beside it, as `<stem>.locked-<unix seconds>.vault` — with a
/// `-<n>` suffix when that name is taken — and return where it went. **Never deletes**: whoever
/// still knows the old passphrase can open the file there. With [`MAX_SET_ASIDE_VAULTS`] of this
/// vault's already set aside, it is [`VaultError::TooManySetAside`] and nothing is renamed.
///
/// Called under [`serialised`], so no write of this process lands between the rename and the fresh
/// vault that replaces it.
pub(super) fn set_aside(path: &Path) -> Result<PathBuf, VaultError> {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .ok_or_else(|| VaultError::Io(format!("{} names no vault file", path.display())))?;
    let kept = count_set_aside(path, &stem)?;
    if kept >= MAX_SET_ASIDE_VAULTS {
        return Err(VaultError::TooManySetAside { kept });
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| VaultError::Io(format!("the clock reads before the Unix epoch: {e}")))?
        .as_secs();
    let aside = (1..)
        .map(|n| {
            let suffix = if n == 1 {
                String::new()
            } else {
                format!("-{n}")
            };
            path.with_file_name(format!("{stem}.locked-{now}{suffix}.vault"))
        })
        .find(|candidate| !candidate.exists())
        .expect("some suffix is free");
    std::fs::rename(path, &aside).map_err(|e| {
        VaultError::Io(format!(
            "setting {} aside as {}: {e}",
            path.display(),
            aside.display()
        ))
    })?;
    Ok(aside)
}

/// How many vaults named `<stem>.locked-….vault` sit beside `path`. A subject's hex stem holds no
/// `.`, so no other subject's live or set-aside vault can match another's prefix.
fn count_set_aside(path: &Path, stem: &str) -> Result<usize, VaultError> {
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let prefix = format!("{stem}.locked-");
    let entries = std::fs::read_dir(dir)
        .map_err(|e| VaultError::Io(format!("listing {}: {e}", dir.display())))?;
    let mut kept = 0;
    for entry in entries {
        let entry = entry.map_err(|e| VaultError::Io(format!("listing {}: {e}", dir.display())))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".vault") {
            kept += 1;
        }
    }
    Ok(kept)
}

pub(super) fn write_vault_file(path: &Path, file: &VaultFile) -> Result<(), VaultError> {
    let bytes = serde_json::to_vec_pretty(file)
        .map_err(|e| VaultError::Io(format!("serialising {}: {e}", path.display())))?;
    crate::atomic::write_owner_only(path, &bytes)
        .map_err(|e| VaultError::Io(format!("writing {}: {e}", path.display())))
}
