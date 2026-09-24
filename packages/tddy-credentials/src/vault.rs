//! The sealed file and the session-scoped handle that opens it.
//!
//! # On-disk format
//!
//! One JSON file **per subject** (the user whose vault it is), mode `0600` from its first byte,
//! replaced by swap-then-rename (`crate::atomic`) so a crash or a full disk can never leave a
//! truncated vault behind — which would parse as an empty one and read to the operator as an
//! ordinary "please sign in again".
//!
//! ```json
//! {
//!   "header": {
//!     "format_version": 2,
//!     "kdf": "argon2id", "kdf_version": 19,
//!     "m_cost_kib": 19456, "t_cost": 2, "p_cost": 1,
//!     "salt": "<hex, 16 bytes>"
//!   },
//!   "wrapped_data_key": { "nonce": "<hex>", "ciphertext": "<hex>" },
//!   "unlock_slots": [ { "id": "<hex>", "nonce": "<hex>", "ciphertext": "<hex>" } ],
//!   "verifier": { "nonce": "<hex>", "ciphertext": "<hex>" },
//!   "records": [ { "id": "<hex>", "nonce": "<hex>", "ciphertext": "<hex>" } ]
//! }
//! ```
//!
//! **Two keys, and the reason there are two.** A random data key seals the records; key-encryption
//! keys only wrap it. Changing how the vault is opened — a new passphrase, another browser slot —
//! then re-wraps 32 bytes instead of re-encrypting every record.
//!
//! **Wrap slots.** `wrapped_data_key` is the **passphrase slot**: the data key under a KEK that
//! Argon2id derives from the user's passphrase with the header's salt and costs, bound to the
//! subject by HKDF-Expand. It is the one way in that needs nobody's browser. Each entry of
//! `unlock_slots` wraps the same data key under a key derived from a random 32-byte [`UnlockKey`]
//! handed to one browser session lineage and **not stored here** — so after a daemon restart,
//! that browser's next session refresh reopens the vault without the passphrase. The file holds
//! only the wraps. At most [`MAX_UNLOCK_SLOTS`] are kept: adding one past the bound evicts the
//! least recently used (a rotation counts as a use).
//!
//! The unlock slots sit beside the header rather than inside it, because the header is the
//! passphrase slot's associated data: a slot added at a refresh, when no passphrase is present,
//! must not invalidate the one wrap only the passphrase can rewrite.
//!
//! **The header is authenticated as associated data** of the passphrase slot, and every parameter
//! in it is checked by name against this build's before anything is derived: a header naming
//! another format, KDF, version or cost is a [`VaultError::FormatMismatch`] — a distinct answer
//! from a failed decrypt, because the remedies differ: one is an upgrade, the other is a wrong
//! passphrase. An edited salt is a key that does not open the vault, [`VaultError::Locked`].
//!
//! **The verifier** is a known plaintext sealed under the data key. The wrapped key already proves
//! the KEK; the verifier proves that the data key a session holds is still the one the file on disk
//! is sealed under, which every read and write re-checks before touching a record.
//!
//! **Each record's identity is its own associated data.** A record is addressed by `id`, a keyed
//! HMAC of its provider and account (length-prefixed, so no pair of strings can collide with
//! another), under a subkey of the data key — so the file names no provider and no account in
//! cleartext. The `id` is the record's associated data, and the identity inside the sealed record
//! must hash back to it, so a sealed record cannot be moved into another account's slot and still
//! open.

mod crypto;
mod format;
mod unlock;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use crate::kdf::{hkdf_expand, keyed_name, to_hex};
use crate::record::{AccountId, CredentialRecord, ProviderId};
use crate::secret::{wipe, SecretBytes, SecretString};
use crypto::{
    check_verifier, open, passphrase_kek, random_bytes, random_key, record_aad, seal,
    unwrap_data_key, wrap_under_passphrase, VERIFIER_AAD, VERIFIER_PLAINTEXT,
};
use format::{
    header_aad, read_vault_file, serialised, set_aside, write_vault_file, Header, SealedRecord,
    VaultFile, SALT_BYTES,
};

pub use unlock::{UnlockKey, MAX_UNLOCK_SLOTS};

/// The shortest passphrase a vault is created or reset under, in characters.
///
/// The passphrase is the one secret that opens the vault after a restart with no browser key, and
/// Argon2id only slows guessing down; it cannot make a four-letter word safe.
pub const MIN_PASSPHRASE_CHARS: usize = 8;

/// The longest passphrase a vault is created, reset or unlocked with, in characters.
///
/// Far beyond any passphrase a person types, and small enough that nobody can make the daemon
/// hash a megabyte for every guess.
pub const MAX_PASSPHRASE_CHARS: usize = 1024;

/// How many of one subject's old vaults a reset keeps set aside beside the live one.
///
/// A reset **never deletes**: past this many, it is refused, and the operator removes an old
/// vault on the daemon's disk by hand before resetting again. Without a cap, every reset — each
/// one needing a fresh sign-in — would leave one more file behind for good.
pub const MAX_SET_ASIDE_VAULTS: usize = 5;

/// Why a vault operation did not happen.
///
/// `Locked` is deliberately distinct from `Crypto`. A wrong key is an ordinary event — a mistyped
/// passphrase, an unlock key rotated since — and the remedy is to try again or reset; a crypto
/// failure on a key that does open the vault is corruption, and the remedy is not the same.
/// Collapsing them would tell the operator to do the wrong thing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VaultError {
    /// The key presented does not unwrap the data key — a wrong passphrase, or an unlock key whose
    /// slot has been rotated or removed. **Nothing is re-initialised**, and there is no second key.
    #[error("the credential vault is locked: that key does not open it")]
    Locked,

    /// The file's header names a format or KDF this build does not produce.
    #[error("the credential store is in {found} format; this build writes {expected}")]
    FormatMismatch { expected: String, found: String },

    /// There is no vault to open — the user has not chosen a passphrase yet.
    #[error("the credential vault has not been created yet; choose a passphrase to create it")]
    Uninitialized,

    /// A vault already exists where one was to be created. Replacing it is a reset.
    #[error("a credential vault already exists; unlock it with its passphrase, or reset it")]
    AlreadyInitialized,

    /// A first passphrase or a reset was asked for with no credential from a fresh sign-in
    /// waiting for the vault. Only a login proves possession of the account a vault is for, so a
    /// session token alone can neither create a vault nor replace one.
    #[error(
        "choosing a credential vault passphrase needs a fresh sign-in on this daemon; sign in \
         again, then choose it"
    )]
    NoFreshLogin,

    /// A reset was asked for while the vault is open on this daemon — nothing was forgotten.
    #[error("the credential vault is open on this daemon; it is not reset while it is open")]
    AlreadyOpen,

    /// A reset would set aside one vault more than [`MAX_SET_ASIDE_VAULTS`]. Nothing is deleted
    /// to make room: an old vault is removed from the daemon's disk by hand first.
    #[error(
        "{kept} earlier credential vaults are already set aside on this daemon; remove one before \
         resetting again"
    )]
    TooManySetAside { kept: usize },

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
/// only exists once a passphrase or an unlock key has opened the file.
pub struct CredentialStore;

/// Domain separation for the subkey that names records.
const RECORD_ID_INFO: &[u8] = b"tddy-credentials/v2/record-id";

impl CredentialStore {
    /// Where `subject`'s vault lives inside an `auth_storage` directory.
    ///
    /// One file per subject: a vault is bound to one user and opens for nobody else, so a shared
    /// file would lock out every user after the first. The subject is
    /// hex-encoded into the basename, which keeps any login safe as a filename — including on a
    /// case-insensitive filesystem, where two logins differing only in case must not collide.
    #[must_use]
    pub fn path_in(auth_storage_dir: impl AsRef<Path>, subject: &str) -> PathBuf {
        auth_storage_dir
            .as_ref()
            .join(format!("credentials-{}.vault", to_hex(subject.as_bytes())))
    }

    /// Create `subject`'s vault at `path`, sealed under a key derived from `passphrase`.
    ///
    /// Only an **absent** file is created: one that exists is [`VaultError::AlreadyInitialized`],
    /// and is left exactly as it was. Replacing a vault is [`Self::reset`], which never deletes.
    pub fn create(
        path: &Path,
        passphrase: &SecretString,
        subject: &str,
    ) -> Result<SessionVault, VaultError> {
        // Held across the existence check and the write, so two creates racing for one path
        // cannot both mint a data key and leave the loser sealing records the file never opens.
        let _serialised = serialised();
        if read_vault_file(path)?.is_some() {
            return Err(VaultError::AlreadyInitialized);
        }
        create_file(path, passphrase, subject)
    }

    /// Open `subject`'s vault at `path` with the passphrase it was created under.
    ///
    /// A passphrase that does not open it is [`VaultError::Locked`], and nothing is changed. An
    /// absent file is [`VaultError::Uninitialized`] — opening never creates.
    pub fn open_with_passphrase(
        path: &Path,
        passphrase: &SecretString,
        subject: &str,
    ) -> Result<SessionVault, VaultError> {
        let file = read_vault_file(path)?.ok_or(VaultError::Uninitialized)?;
        let kek = passphrase_kek(&file.header, passphrase, subject)?;
        let data_key = unwrap_data_key(
            &kek,
            &file.wrapped_data_key.nonce,
            &file.wrapped_data_key.ciphertext,
            &header_aad(&file.header)?,
        )?;
        // The data key authenticated under the KEK, so a verifier that fails under it is
        // corruption.
        check_verifier(&file.verifier, &data_key)?;
        Ok(session(path, subject, data_key))
    }

    /// Set the vault at `path` aside and create a fresh one under `new_passphrase` — the
    /// forgotten-passphrase path.
    ///
    /// The old file is **renamed, never deleted**, to `credentials-<hex subject>.locked-<unix
    /// seconds>.vault` beside it, and that path is returned; `None` when there was no file to set
    /// aside. Its records are unreadable without the old passphrase, but they are not destroyed.
    pub fn reset(
        path: &Path,
        new_passphrase: &SecretString,
        subject: &str,
    ) -> Result<(SessionVault, Option<PathBuf>), VaultError> {
        // One critical section for the rename and the create: nothing of this process can write
        // into the old file after it is set aside, or into the gap before the fresh one exists.
        // The old file is not parsed first — a reset is also the way out of a vault this build
        // cannot read.
        let _serialised = serialised();
        let aside = path.exists().then(|| set_aside(path)).transpose()?;
        let fresh = create_file(path, new_passphrase, subject)?;
        Ok((fresh, aside))
    }
}

/// An opened vault, scoped to one user session.
///
/// Zeroizes its key material on drop. There is no way to obtain one without the user's passphrase
/// or an unlock key a lineage of theirs holds — the daemon holds neither at rest.
///
/// Every operation re-reads the file rather than caching its records, so two handles over one
/// vault — two sessions of the same user — never act on each other's stale view.
pub struct SessionVault {
    path: PathBuf,
    subject: String,
    data_key: SecretBytes,
    record_id_key: SecretBytes,
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
    pub fn put(&self, record: CredentialRecord) -> Result<(), VaultError> {
        let _serialised = serialised();
        let mut file = self.load()?;
        let sealed = self.seal_record(&record)?;
        file.records.retain(|existing| existing.id != sealed.id);
        file.records.push(sealed);
        write_vault_file(&self.path, &file)
    }

    /// The record held for this provider and account, or `None` when none is.
    ///
    /// `None` is "no credential", which a caller reports as *unavailable* rather than as an empty
    /// result. A record that exists and will not authenticate is [`VaultError::Crypto`], never
    /// `None` — silently treating corruption as absence is the fallback this store must not have.
    pub fn get(
        &self,
        provider: &ProviderId,
        account: &AccountId,
    ) -> Result<Option<CredentialRecord>, VaultError> {
        let file = self.load()?;
        let id = self.record_id(provider, account);
        file.records
            .iter()
            .find(|sealed| sealed.id == id)
            .map(|sealed| self.open_record(sealed))
            .transpose()
    }

    /// Every record, or every record of one provider, ordered by provider then account.
    ///
    /// Returns whole records, secrets included, because the only caller is the daemon acting for
    /// the signed-in user. What an Accounts screen may see is decided where the RPC is built
    /// (`#keyring` 4/9), not here.
    pub fn list(&self, provider: Option<&ProviderId>) -> Result<Vec<CredentialRecord>, VaultError> {
        let file = self.load()?;
        let mut records = file
            .records
            .iter()
            .map(|sealed| self.open_record(sealed))
            .collect::<Result<Vec<_>, _>>()?;
        records.retain(|record| provider.is_none_or(|wanted| &record.provider == wanted));
        records.sort_by(|a, b| (&a.provider, &a.account).cmp(&(&b.provider, &b.account)));
        Ok(records)
    }

    /// Forget the record for this provider and account. Removing one that is not held is `Ok`.
    pub fn remove(&self, provider: &ProviderId, account: &AccountId) -> Result<(), VaultError> {
        let _serialised = serialised();
        let mut file = self.load()?;
        let id = self.record_id(provider, account);
        let before = file.records.len();
        file.records.retain(|sealed| sealed.id != id);
        if file.records.len() == before {
            return Ok(());
        }
        write_vault_file(&self.path, &file)
    }

    /// Re-read the file and prove this session's data key still opens it.
    ///
    /// A file replaced under a live session — by anything but this crate's own writes, which never
    /// change the data key — is one this session's key no longer opens: [`VaultError::Locked`].
    fn load(&self) -> Result<VaultFile, VaultError> {
        let file = read_vault_file(&self.path)?.ok_or_else(|| {
            VaultError::Io(format!(
                "the credential store at {} is gone",
                self.path.display()
            ))
        })?;
        check_verifier(&file.verifier, &self.data_key).map_err(|_| VaultError::Locked)?;
        Ok(file)
    }

    /// Whether this handle no longer describes the file at its path: the file is gone, or was
    /// replaced by one its data key does not open (a reset, from here or from another process).
    ///
    /// Any other failure to read is a real one, not staleness — it is reported where it happens.
    pub(crate) fn is_stale(&self) -> bool {
        !self.path.exists() || matches!(self.load(), Err(VaultError::Locked))
    }

    fn record_id(&self, provider: &ProviderId, account: &AccountId) -> String {
        let provider = provider.as_str().as_bytes();
        let account = account.as_str().as_bytes();
        keyed_name(
            &self.record_id_key,
            &[
                &(provider.len() as u64).to_be_bytes(),
                provider,
                &(account.len() as u64).to_be_bytes(),
                account,
            ],
        )
    }

    fn seal_record(&self, record: &CredentialRecord) -> Result<SealedRecord, VaultError> {
        let id = self.record_id(&record.provider, &record.account);
        let mut plaintext =
            serde_json::to_vec(&RecordToSeal::from(record)).map_err(|_| VaultError::Crypto)?;
        let sealed = seal(&self.data_key, &plaintext, &record_aad(&id));
        wipe(&mut plaintext);
        let sealed = sealed?;
        Ok(SealedRecord {
            id,
            nonce: sealed.nonce,
            ciphertext: sealed.ciphertext,
        })
    }

    fn open_record(&self, sealed: &SealedRecord) -> Result<CredentialRecord, VaultError> {
        let mut plaintext = open(
            &self.data_key,
            &sealed.nonce,
            &sealed.ciphertext,
            &record_aad(&sealed.id),
        )?;
        let record = serde_json::from_slice::<OpenedRecord>(&plaintext);
        wipe(&mut plaintext);
        let record = CredentialRecord::from(record.map_err(|_| VaultError::Crypto)?);
        // The identity inside the seal must name the slot it was found in.
        let expected = self.record_id(&record.provider, &record.account);
        if bool::from(expected.as_bytes().ct_eq(sealed.id.as_bytes())) {
            Ok(record)
        } else {
            Err(VaultError::Crypto)
        }
    }
}

/// What is sealed for a record: [`CredentialRecord`]'s fields, borrowed, so serialising one makes no
/// copy of the secret. The record type itself is not `Serialize` — this mirror is the only path from
/// a record to bytes, and it ends inside the AEAD.
#[derive(Serialize)]
struct RecordToSeal<'a> {
    provider: &'a ProviderId,
    account: &'a AccountId,
    label: &'a str,
    secret: &'a str,
    metadata: &'a BTreeMap<String, String>,
    updated_at: u64,
}

impl<'a> From<&'a CredentialRecord> for RecordToSeal<'a> {
    fn from(record: &'a CredentialRecord) -> Self {
        Self {
            provider: &record.provider,
            account: &record.account,
            label: &record.label,
            secret: record.secret.expose(),
            metadata: &record.metadata,
            updated_at: record.updated_at,
        }
    }
}

/// What an opened seal parses into; its secret moves straight into a [`SecretString`].
#[derive(Deserialize)]
struct OpenedRecord {
    provider: ProviderId,
    account: AccountId,
    label: String,
    secret: String,
    metadata: BTreeMap<String, String>,
    updated_at: u64,
}

impl From<OpenedRecord> for CredentialRecord {
    fn from(opened: OpenedRecord) -> Self {
        Self {
            provider: opened.provider,
            account: opened.account,
            label: opened.label,
            secret: SecretString::new(opened.secret),
            metadata: opened.metadata,
            updated_at: opened.updated_at,
        }
    }
}

/// Session keys for a data key: the key itself and the subkey that names records.
fn session(path: &Path, subject: &str, data_key: SecretBytes) -> SessionVault {
    let record_id_key = hkdf_expand(&data_key, RECORD_ID_INFO);
    SessionVault {
        path: path.to_path_buf(),
        subject: subject.to_string(),
        data_key,
        record_id_key,
    }
}

/// Write a fresh vault at `path` under `passphrase`. The caller holds [`serialised`] and has
/// established that nothing is there.
fn create_file(
    path: &Path,
    passphrase: &SecretString,
    subject: &str,
) -> Result<SessionVault, VaultError> {
    let data_key = random_key();
    let header = Header::fresh(&random_bytes::<SALT_BYTES>());
    let file = VaultFile {
        wrapped_data_key: wrap_under_passphrase(&header, passphrase, subject, &data_key)?,
        unlock_slots: Vec::new(),
        verifier: seal(&data_key, VERIFIER_PLAINTEXT, VERIFIER_AAD)?,
        header,
        records: Vec::new(),
    };
    write_vault_file(path, &file)?;
    Ok(session(path, subject, data_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    const THE_OPERATOR: &str = "operator";
    const THE_PASSPHRASE: &str = "correct horse battery staple";

    fn the_passphrase() -> SecretString {
        SecretString::new(THE_PASSPHRASE)
    }

    fn a_record(account: &str) -> CredentialRecord {
        CredentialRecord {
            provider: ProviderId::new("github"),
            account: AccountId::new(account),
            label: account.to_string(),
            secret: SecretString::new(format!("gho_{account}")),
            metadata: Default::default(),
            updated_at: 1,
        }
    }

    #[test]
    fn retains_every_record_when_writes_are_concurrent() {
        // Given four sessions of one user writing to one vault at the same moment
        let dir = tempfile::tempdir().unwrap();
        let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
        let key = CredentialStore::create(&path, &the_passphrase(), THE_OPERATOR)
            .unwrap()
            .add_unlock_slot()
            .unwrap();
        let accounts = ["alice", "bob", "carol", "dave"];
        let at_once = std::sync::Barrier::new(accounts.len());

        // When
        std::thread::scope(|scope| {
            for account in accounts {
                let (path, at_once, key) = (&path, &at_once, &key);
                scope.spawn(move || {
                    let vault = CredentialStore::open_with_unlock_key(path, key).unwrap();
                    at_once.wait();
                    vault.put(a_record(account)).unwrap();
                });
            }
        });

        // Then — a lock-free read-modify-write lets the last writer's file drop the others' records
        let vault = CredentialStore::open_with_unlock_key(&path, &key).unwrap();
        assert_eq!(
            vault.list(None),
            Ok(accounts.iter().map(|a| a_record(a)).collect::<Vec<_>>())
        );
    }

    #[cfg(unix)]
    #[test]
    fn writes_the_vault_readable_only_by_its_owner() {
        // Given
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);

        // When
        CredentialStore::create(&path, &the_passphrase(), THE_OPERATOR)
            .unwrap()
            .put(a_record("operator"))
            .unwrap();

        // Then
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    fn a_vault_with_unlock_slots(count: usize) -> (tempfile::TempDir, PathBuf, Vec<UnlockKey>) {
        let dir = tempfile::tempdir().unwrap();
        let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
        let vault = CredentialStore::create(&path, &the_passphrase(), THE_OPERATOR).unwrap();
        vault.put(a_record(THE_OPERATOR)).unwrap();
        let keys = (0..count)
            .map(|_| vault.add_unlock_slot().unwrap())
            .collect();
        (dir, path, keys)
    }

    fn opens_with(path: &Path, key: &UnlockKey) -> Result<Option<CredentialRecord>, VaultError> {
        CredentialStore::open_with_unlock_key(path, key)
            .and_then(|vault| vault.get(&ProviderId::new("github"), &AccountId::new(THE_OPERATOR)))
    }

    #[test]
    fn every_unlock_slot_opens_the_same_records_as_the_passphrase() {
        // Given two browser lineages holding a slot each
        let (_dir, path, keys) = a_vault_with_unlock_slots(2);

        // When each opens the vault on its own
        let opened: Vec<_> = keys.iter().map(|key| opens_with(&path, key)).collect();

        // Then both reach the same record
        assert_eq!(
            opened,
            vec![
                Ok(Some(a_record(THE_OPERATOR))),
                Ok(Some(a_record(THE_OPERATOR)))
            ]
        );
    }

    #[test]
    fn a_removed_unlock_slot_opens_nothing_and_the_others_still_do() {
        // Given two lineages, one of which logs out
        let (_dir, path, keys) = a_vault_with_unlock_slots(2);
        CredentialStore::open_with_unlock_key(&path, &keys[0])
            .unwrap()
            .remove_unlock_slot(keys[0].slot_id())
            .unwrap();

        // When each tries to open the vault
        let opened = (
            opens_with(&path, &keys[0]),
            opens_with(&path, &keys[1]).is_ok(),
        );

        // Then only the one still holding a slot does
        assert_eq!(opened, (Err(VaultError::Locked), true));
    }

    #[test]
    fn an_unlock_key_for_one_slot_does_not_open_another() {
        // Given two slots, and a key whose slot id is swapped for the other's
        let (_dir, path, keys) = a_vault_with_unlock_slots(2);
        let wire = keys[0].to_wire();
        let crossed = UnlockKey::from_wire(&wire.replace(keys[0].slot_id(), keys[1].slot_id()))
            .expect("still well-formed");

        // When it is presented
        let opened = opens_with(&path, &crossed);

        // Then the slot id is bound into the derivation
        assert_eq!(opened, Err(VaultError::Locked));
    }

    #[test]
    fn rotating_a_slot_proves_the_presented_key_rather_than_trusting_its_slot_id() {
        // Given an open vault, and a key naming a real slot but carrying key bytes of its own
        let (_dir, path, keys) = a_vault_with_unlock_slots(1);
        let vault = CredentialStore::open_with_unlock_key(&path, &keys[0]).unwrap();
        let forged = UnlockKey::from_wire(&format!(
            "{}.{}.{}",
            to_hex(THE_OPERATOR.as_bytes()),
            keys[0].slot_id(),
            "00".repeat(32)
        ))
        .expect("well-formed");

        // When the forged key asks for a rotation
        let rotated = vault.rotate_unlock_slot(&forged).map(|key| key.to_wire());

        // Then it is refused, and the genuine key still opens its slot
        assert_eq!(
            (rotated, opens_with(&path, &keys[0]).is_ok()),
            (Err(VaultError::Locked), true)
        );
    }

    #[test]
    fn past_the_bound_the_least_recently_used_unlock_slot_is_evicted() {
        // Given a vault already holding the most slots it keeps
        let (_dir, path, keys) = a_vault_with_unlock_slots(MAX_UNLOCK_SLOTS);

        // When one more lineage signs in
        CredentialStore::open_with_passphrase(&path, &the_passphrase(), THE_OPERATOR)
            .unwrap()
            .add_unlock_slot()
            .unwrap();

        // Then the oldest slot is the one that no longer opens the vault
        assert_eq!(opens_with(&path, &keys[0]), Err(VaultError::Locked));
    }

    #[test]
    fn past_the_bound_the_slot_count_holds_at_the_bound() {
        // Given a vault already holding the most slots it keeps
        let (_dir, path, _keys) = a_vault_with_unlock_slots(MAX_UNLOCK_SLOTS);
        let vault =
            CredentialStore::open_with_passphrase(&path, &the_passphrase(), THE_OPERATOR).unwrap();

        // When one more lineage signs in
        vault.add_unlock_slot().unwrap();

        // Then
        assert_eq!(
            vault.unlock_slot_ids().map(|ids| ids.len()),
            Ok(MAX_UNLOCK_SLOTS)
        );
    }

    #[test]
    fn the_vault_file_never_holds_an_unlock_key() {
        // Given a lineage holding a slot
        let (_dir, path, keys) = a_vault_with_unlock_slots(1);

        // When the file is read
        let on_disk = std::fs::read_to_string(&path).unwrap();

        // Then the key the browser holds is nowhere in it
        let key_hex = keys[0].to_wire().rsplit('.').next().unwrap().to_string();
        assert!(!on_disk.contains(&key_hex));
    }

    #[test]
    fn an_unlock_key_survives_its_wire_form() {
        // Given a key a login handed out
        let (_dir, _path, keys) = a_vault_with_unlock_slots(1);
        let wire = keys[0].to_wire();

        // When it is parsed back from what the browser stored
        let parsed = UnlockKey::from_wire(&wire).map(|key| key.to_wire());

        // Then
        assert_eq!(parsed, Some(wire));
    }

    #[test]
    fn nothing_but_an_unlock_keys_wire_form_parses_as_one() {
        // Given what an empty or truncated `localStorage` entry would hold
        let not_keys = ["", "a.b"];

        // When each is parsed
        let parsed: Vec<bool> = not_keys
            .iter()
            .map(|wire| UnlockKey::from_wire(wire).is_some())
            .collect();

        // Then none is taken for a key
        assert_eq!(parsed, vec![false, false]);
    }

    #[test]
    fn names_each_subjects_vault_apart_even_when_logins_differ_only_in_case() {
        // Given two logins a case-insensitive filesystem would otherwise fold into one name
        let dir = Path::new("/auth");
        let as_that_filesystem_sees_it = |subject| {
            CredentialStore::path_in(dir, subject)
                .to_string_lossy()
                .to_lowercase()
        };

        // When / Then
        assert_ne!(
            as_that_filesystem_sees_it("Operator"),
            as_that_filesystem_sees_it("operator")
        );
    }
}
