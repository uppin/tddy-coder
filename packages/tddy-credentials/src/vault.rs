//! The sealed file and the session-scoped handle that opens it.
//!
//! # On-disk format
//!
//! One JSON file **per subject** (the user whose vault it is), mode `0600`, replaced through
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
//!   "unlock_slots": [ { "id": "<hex>", "nonce": "<hex>", "ciphertext": "<hex>" } ],
//!   "verifier": { "nonce": "<hex>", "ciphertext": "<hex>" },
//!   "records": [ { "id": "<hex>", "nonce": "<hex>", "ciphertext": "<hex>" } ]
//! }
//! ```
//!
//! **Two keys, and the reason there are two.** The KEK is derived from the user's login credential
//! (`HKDF-SHA256(salt, ikm, info)`); it wraps a random data key, and the data key is what seals the
//! records. A credential rotation then re-wraps 32 bytes instead of re-encrypting every record,
//! which is what makes [`SessionVault::rewrap`] cheap enough to run on *every* successful login.
//!
//! **Wrap slots.** The data key is wrapped more than once. `wrapped_data_key` is the **login
//! slot**, under the KEK the login credential derives; it is re-wrapped on every login. Each entry
//! of `unlock_slots` wraps the same data key under a key derived from a random 32-byte
//! [`UnlockKey`] that was handed to one browser session lineage and is **not stored here** — so
//! after a daemon restart, that browser's next session refresh can reopen the vault without a new
//! login. The file holds only the wrap; the key that opens it is in the browser; neither alone
//! opens anything. At most [`MAX_UNLOCK_SLOTS`] are kept: adding one past the bound evicts the
//! least recently used (a rotation counts as a use).
//!
//! The unlock slots sit beside the header rather than inside it, because the header is the login
//! slot's associated data: a slot added at a refresh, when no login credential is present, must
//! not invalidate the one wrap only a login can rewrite.
//!
//! **The header is authenticated as associated data** of the wrapped data key, so its KDF name,
//! version and parameters cannot be edited into a weaker derivation without the open failing. A
//! header that names parameters this build does not produce is a [`VaultError::FormatMismatch`] —
//! a distinct answer from a failed decrypt, because the remedies differ: one is an upgrade, the
//! other is a wrong key.
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

use std::path::{Path, PathBuf};

use subtle::ConstantTimeEq;

use crate::kdf::{hkdf_expand, keyed_name, to_hex};
use crate::record::{AccountId, CredentialRecord, ProviderId};
use crate::secret::{wipe, SecretBytes};
use crypto::{
    check_verifier, derive_kek, open, random_32, record_aad, seal, unwrap_data_key, wrap_data_key,
    VERIFIER_AAD, VERIFIER_PLAINTEXT,
};
use format::{
    header_aad, info_for, read_vault_file, serialised, write_vault_file, Header, SealedRecord,
    VaultFile, FORMAT_VERSION, KDF, KDF_VERSION,
};

pub use unlock::{UnlockKey, MAX_UNLOCK_SLOTS};

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

/// Domain separation for the subkey that names records.
const RECORD_ID_INFO: &[u8] = b"tddy-credentials/v1/record-id";

impl CredentialStore {
    /// Where `subject`'s vault lives inside an `auth_storage` directory.
    ///
    /// One file per subject: a vault is sealed under one user's login credential and opens for
    /// nobody else, so a shared file would lock out every user after the first. The subject is
    /// hex-encoded into the basename, which keeps any login safe as a filename — including on a
    /// case-insensitive filesystem, where two logins differing only in case must not collide.
    #[must_use]
    pub fn path_in(auth_storage_dir: impl AsRef<Path>, subject: &str) -> PathBuf {
        auth_storage_dir
            .as_ref()
            .join(format!("credentials-{}.vault", to_hex(subject.as_bytes())))
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
        path: &Path,
        ikm: &[u8],
        subject: &str,
    ) -> Result<SessionVault, VaultError> {
        // Held across the existence check and the first write, so two logins racing to create one
        // vault cannot both mint a data key and leave the loser's session sealing records the file
        // no longer opens.
        let _serialised = serialised();
        match read_vault_file(path)? {
            Some(file) => open_file(path, &file, ikm, subject),
            None => create(path, ikm, subject),
        }
    }

    /// Open the vault at `path` if one exists; `Ok(None)` when there is none. Never creates one.
    ///
    /// For a login that must not leave a vault behind — a stub login, whose credential is
    /// synthetic — but must still be refused when a vault it cannot open is already there.
    pub fn open_existing(
        path: &Path,
        ikm: &[u8],
        subject: &str,
    ) -> Result<Option<SessionVault>, VaultError> {
        match read_vault_file(path)? {
            Some(file) => open_file(path, &file, ikm, subject).map(Some),
            None => Ok(None),
        }
    }
}

/// An opened vault, scoped to one user session.
///
/// Lives for the life of the session and zeroizes its key material on drop. There is no way to
/// obtain one without the input keying material a login produced, which is what "only decrypted by
/// a valid user session" means in practice.
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

    /// Re-wrap the data key under a key derived from `ikm`, with a fresh salt.
    ///
    /// Called on **every** successful login. The records are untouched — only the 32-byte wrapped
    /// key and the header's salt change — which is what makes a credential rotation survivable as
    /// long as one login still succeeds under the old credential.
    pub fn rewrap(&self, ikm: &[u8]) -> Result<(), VaultError> {
        let _serialised = serialised();
        let mut file = self.load()?;
        file.header.salt = to_hex(&random_32());
        file.wrapped_data_key = wrap_data_key(&file.header, ikm, &self.subject, &self.data_key)?;
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
        let mut plaintext = serde_json::to_vec(record).map_err(|_| VaultError::Crypto)?;
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
        let record = serde_json::from_slice::<CredentialRecord>(&plaintext);
        wipe(&mut plaintext);
        let record = record.map_err(|_| VaultError::Crypto)?;
        // The identity inside the seal must name the slot it was found in.
        let expected = self.record_id(&record.provider, &record.account);
        if bool::from(expected.as_bytes().ct_eq(sealed.id.as_bytes())) {
            Ok(record)
        } else {
            Err(VaultError::Crypto)
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

fn create(path: &Path, ikm: &[u8], subject: &str) -> Result<SessionVault, VaultError> {
    let mut fresh = random_32();
    let data_key = SecretBytes::new(fresh);
    wipe(&mut fresh);
    let header = Header {
        format_version: FORMAT_VERSION,
        kdf: KDF.to_string(),
        kdf_version: KDF_VERSION,
        salt: to_hex(&random_32()),
        info: info_for(subject),
    };
    let file = VaultFile {
        wrapped_data_key: wrap_data_key(&header, ikm, subject, &data_key)?,
        unlock_slots: Vec::new(),
        verifier: seal(&data_key, VERIFIER_PLAINTEXT, VERIFIER_AAD)?,
        header,
        records: Vec::new(),
    };
    write_vault_file(path, &file)?;
    Ok(session(path, subject, data_key))
}

fn open_file(
    path: &Path,
    file: &VaultFile,
    ikm: &[u8],
    subject: &str,
) -> Result<SessionVault, VaultError> {
    let kek = derive_kek(&file.header, ikm, subject)?;
    let data_key = unwrap_data_key(
        &kek,
        &file.wrapped_data_key.nonce,
        &file.wrapped_data_key.ciphertext,
        &header_aad(&file.header)?,
    )?;
    // The data key authenticated under the KEK, so a verifier that fails under it is corruption.
    check_verifier(&file.verifier, &data_key)?;
    Ok(session(path, subject, data_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    const THE_OPERATOR: &str = "operator";
    const THE_LOGIN_CREDENTIAL: &[u8] = b"gho_the_token_this_login_granted";

    fn a_record(account: &str) -> CredentialRecord {
        CredentialRecord {
            provider: ProviderId::new("github"),
            account: AccountId::new(account),
            label: account.to_string(),
            secret: format!("gho_{account}"),
            metadata: Default::default(),
            updated_at: 1,
        }
    }

    #[test]
    fn retains_every_record_when_writes_are_concurrent() {
        // Given four sessions of one user writing to one vault at the same moment
        let dir = tempfile::tempdir().unwrap();
        let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
        CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR).unwrap();
        let accounts = ["alice", "bob", "carol", "dave"];
        let at_once = std::sync::Barrier::new(accounts.len());

        // When
        std::thread::scope(|scope| {
            for account in accounts {
                let (path, at_once) = (&path, &at_once);
                scope.spawn(move || {
                    let vault =
                        CredentialStore::open_or_create(path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
                            .unwrap();
                    at_once.wait();
                    vault.put(a_record(account)).unwrap();
                });
            }
        });

        // Then — a lock-free read-modify-write lets the last writer's file drop the others' records
        let vault =
            CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR).unwrap();
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
        CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
            .unwrap()
            .put(a_record("operator"))
            .unwrap();

        // Then
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn opening_an_absent_vault_without_creating_leaves_no_file() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);

        // When
        let opened = CredentialStore::open_existing(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
            .map(|vault| vault.is_some());

        // Then
        assert_eq!((opened, path.exists()), (Ok(false), false));
    }

    fn a_vault_with_unlock_slots(count: usize) -> (tempfile::TempDir, PathBuf, Vec<UnlockKey>) {
        let dir = tempfile::tempdir().unwrap();
        let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
        let vault =
            CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR).unwrap();
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
    fn every_unlock_slot_opens_the_same_records_as_the_login() {
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
    fn a_login_rewrap_leaves_every_unlock_slot_working() {
        // Given a lineage holding a slot
        let (_dir, path, keys) = a_vault_with_unlock_slots(1);

        // When the next login rewraps the login slot under a rotated credential
        CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
            .unwrap()
            .rewrap(b"gho_a_rotated_credential")
            .unwrap();

        // Then the browser's slot still opens the vault
        assert_eq!(
            opens_with(&path, &keys[0]),
            Ok(Some(a_record(THE_OPERATOR)))
        );
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
    fn past_the_bound_the_least_recently_used_unlock_slot_is_evicted() {
        // Given a vault already holding the most slots it keeps
        let (_dir, path, keys) = a_vault_with_unlock_slots(MAX_UNLOCK_SLOTS);

        // When one more lineage signs in
        let vault =
            CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR).unwrap();
        let newest = vault.add_unlock_slot().unwrap();

        // Then the oldest is gone, and the count holds at the bound
        let ids = vault.unlock_slot_ids().unwrap();
        assert_eq!(
            (
                ids.len(),
                ids.contains(&keys[0].slot_id().to_string()),
                ids.last()
            ),
            (MAX_UNLOCK_SLOTS, false, Some(&newest.slot_id().to_string()))
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
    fn an_unlock_key_survives_its_wire_form_and_nothing_else_parses_as_one() {
        let (_dir, _path, keys) = a_vault_with_unlock_slots(1);
        let wire = keys[0].to_wire();
        assert_eq!(
            (
                UnlockKey::from_wire(&wire).map(|key| key.to_wire()),
                UnlockKey::from_wire("").is_none(),
                UnlockKey::from_wire("a.b").is_none()
            ),
            (Some(wire), true, true)
        );
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
