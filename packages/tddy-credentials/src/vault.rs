//! The sealed file and the session-scoped handle that opens it.
//!
//! # On-disk format
//!
//! One JSON file, mode `0600`, replaced through
//! [`tddy_core::atomic_file::write_atomic_with_mode`] so a crash or a full disk can never leave a
//! truncated vault behind — which would parse as an empty one and read to the operator as an
//! ordinary "please sign in again".
//!
//! ```json
//! {
//!   "header": {
//!     "format_version": 1,
//!     "kdf": "hkdf-sha256",
//!     "kdf_version": 1,
//!     "salt": "<hex, 32 bytes>",
//!     "info": "tddy-credentials/v1/<subject>"
//!   },
//!   "wrapped_data_key": { "nonce": "<hex>", "ciphertext": "<hex>" },
//!   "verifier": { "nonce": "<hex>", "ciphertext": "<hex>" },
//!   "records": [ { "nonce": "<hex>", "ciphertext": "<hex>" } ]
//! }
//! ```
//!
//! **Two keys, and the reason there are two.** The KEK is derived from the user's login credential;
//! it wraps a random data key, and the data key is what seals the records. A credential rotation
//! then re-wraps 32 bytes instead of re-encrypting every record, which is what makes
//! [`SessionVault::rewrap`] cheap enough to run on *every* successful login.
//!
//! **The header is authenticated as associated data**, so its KDF name, version and parameters
//! cannot be edited into a weaker derivation without the open failing. A header that names
//! parameters this build does not produce is a [`VaultError::FormatMismatch`] — a distinct answer
//! from a failed decrypt, because the remedies differ: one is an upgrade, the other is a wrong key.
//!
//! **Each record's identity is its own associated data** (the provider and account, separated by a
//! byte that cannot occur in either), so a sealed record cannot be moved into another account's
//! slot and still open.

use std::path::{Path, PathBuf};

use crate::record::{AccountId, CredentialRecord, ProviderId, VaultEntry};

/// Why a vault operation did not happen.
///
/// `Locked` is deliberately distinct from `Crypto`. A wrong key is the *expected* consequence of a
/// credential rotation and the operator's remedy is to re-link their accounts; a crypto failure on
/// a key that does open the vault is corruption, and the remedy is not the same. Collapsing them
/// would tell the operator to do the wrong thing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VaultError {
    /// The derived key does not unwrap the data key — the login credential is not the one this
    /// vault was sealed under. **Nothing is re-initialised**, and there is no second key.
    #[error(
        "the credential store cannot be opened with this login's key; \
         it was sealed under a different one and must be re-linked"
    )]
    Locked,

    /// The file's header names a format or KDF this build does not produce.
    #[error("the credential store is in {found} format; this build writes {expected}")]
    FormatMismatch { expected: String, found: String },

    /// Reading or replacing the file failed. Names server-side detail — for the log, not the client.
    #[error("{0}")]
    Io(String),

    /// A sealed unit did not authenticate under a key that does open the vault: corruption, or
    /// tampering with a record's label, metadata or secret.
    #[error("a stored credential failed to authenticate; the store has been altered")]
    Crypto,
}

/// The sealed file itself — a path, and the ability to open it into a session.
///
/// Holds no key material. Everything that can read a credential is behind [`SessionVault`], which
/// only exists once a login has produced one.
pub struct CredentialStore;

/// Basename of the vault inside the `auth_storage` directory.
///
/// Public because it is the *replacement* for `github-tokens.json`, and both the daemon that writes
/// it and anything that inspects a deployment's storage need to agree on one name.
pub const VAULT_FILE: &str = "credentials.vault";

impl CredentialStore {
    /// Where the vault lives inside an `auth_storage` directory.
    #[must_use]
    pub fn path_in(auth_storage_dir: impl AsRef<Path>) -> PathBuf {
        auth_storage_dir.as_ref().join(VAULT_FILE)
    }

    /// Open the vault at `path` under a key derived from `ikm`, creating it if it does not exist.
    ///
    /// `ikm` is the input keying material the login produced — the user's own credential, never a
    /// daemon secret. `subject` identifies whose vault this is and is bound into the HKDF `info`,
    /// so key material derived for one subject cannot open another's file even from the same `ikm`.
    ///
    /// Creating is not a fallback for failing to open: a vault that exists and does not open is
    /// [`VaultError::Locked`], and the file is left exactly as it was. Only an **absent** file is
    /// created.
    pub fn open_or_create(
        _path: &Path,
        _ikm: &[u8],
        _subject: &str,
    ) -> Result<SessionVault, VaultError> {
        todo!("TODO(keyring 3/9): implement — derive the KEK, unwrap or mint the data key, verify")
    }
}

/// An opened vault, scoped to one user session.
///
/// Lives for the life of the session and zeroizes its key material on drop. There is no way to
/// obtain one without the input keying material a login produced, which is what "only decrypted by
/// a valid user session" means in practice.
pub struct SessionVault {
    path: PathBuf,
}

impl SessionVault {
    /// The file this vault is persisted in.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Retain `record`, replacing any previous record for the same provider and account.
    ///
    /// Returns `Ok` only once the record is durably on disk — the caller turns an `Err` into a
    /// failed login (see the crate docs, rule 2).
    pub fn put(&self, _record: CredentialRecord) -> Result<(), VaultError> {
        todo!("TODO(keyring 3/9): implement — seal under the data key, replace the file atomically")
    }

    /// The record held for this provider and account, or `None` when none is.
    ///
    /// `None` is "no credential", which a caller reports as *unavailable* rather than as an empty
    /// result. A record that exists and will not authenticate is [`VaultError::Crypto`], never
    /// `None` — silently treating corruption as absence is the fallback this store must not have.
    pub fn get(
        &self,
        _provider: &ProviderId,
        _account: &AccountId,
    ) -> Result<Option<CredentialRecord>, VaultError> {
        todo!("TODO(keyring 3/9): implement — open the addressed record under its own identity")
    }

    /// Every record, or every record of one provider.
    ///
    /// Returns whole records, secrets included, because the only caller is the daemon acting for
    /// the signed-in user. What an Accounts screen may see is decided where the RPC is built
    /// (`#keyring` 4/9), not here.
    pub fn list(
        &self,
        _provider: Option<&ProviderId>,
    ) -> Result<Vec<CredentialRecord>, VaultError> {
        todo!("TODO(keyring 3/9): implement — open every record, filtered by provider")
    }

    /// Forget the record for this provider and account. Removing one that is not held is `Ok`.
    ///
    /// The slot is not emptied — it is replaced by a [`VaultEntry::Tombstone`] carrying the version
    /// that was removed. An emptied slot is indistinguishable from one this daemon never held, so a
    /// peer that still has the record would send it straight back and the person's removal would
    /// undo itself at the next sync (`#keyring` 6/9).
    pub fn remove(&self, _provider: &ProviderId, _account: &AccountId) -> Result<(), VaultError> {
        todo!("TODO(keyring 3/9): implement — write the tombstone, replace the file atomically")
    }

    /// Every slot, deletions included, in the store's own `(provider, account)` order.
    ///
    /// Distinct from [`list`](Self::list), and the distinction is the whole reason this exists:
    /// `list` answers *"what credentials do I have"* and a caller acting on the person's behalf
    /// must never see a deletion there. Reconciliation asks the other question — *"what has this
    /// vault decided about each slot"* — and a deletion is one of the answers.
    pub fn entries(&self) -> Result<Vec<VaultEntry>, VaultError> {
        todo!("TODO(keyring 6/9): implement — open every slot, tombstones included")
    }

    /// Re-wrap the data key under a key derived from `ikm`, with a fresh salt.
    ///
    /// Called on **every** successful login. The records are untouched — only the 32-byte wrapped
    /// key and the header's salt change — which is what makes a credential rotation survivable as
    /// long as one login still succeeds under the old credential.
    pub fn rewrap(&self, _ikm: &[u8]) -> Result<(), VaultError> {
        todo!("TODO(keyring 3/9): implement — fresh salt, re-derive the KEK, re-wrap the data key")
    }
}
