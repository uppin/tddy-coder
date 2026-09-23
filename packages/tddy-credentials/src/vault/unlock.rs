//! Unlock slots: the data key wrapped once per browser session lineage, under a key that lineage
//! holds and the file does not.

use std::path::Path;

use super::crypto::{
    check_verifier, random_16, random_32, seal, unlock_aad, unlock_kek, unwrap_data_key,
};
use super::format::{read_vault_file, serialised, write_vault_file, UnlockSlot};
use super::{session, CredentialStore, SessionVault, VaultError};
use crate::kdf::{from_hex, to_hex};
use crate::secret::{wipe, SecretBytes};

/// How many browser session lineages may hold an unlock slot on one vault at once.
///
/// Sixteen is several browsers and devices per user with room for ones abandoned without a
/// logout; the least recently used is evicted past it, and that browser's next refresh after a
/// restart simply cannot reopen the vault — the same outcome as a lineage that never had a slot.
pub const MAX_UNLOCK_SLOTS: usize = 16;

impl CredentialStore {
    /// Open the vault at `path` through the unlock slot `unlock` names, without a login.
    ///
    /// For a session refresh after a daemon restart: the browser presents the [`UnlockKey`] it was
    /// handed. A slot that is gone (logged out, evicted) or a key that does not open it is
    /// [`VaultError::Locked`], and nothing is changed. So is an absent file — a refresh never
    /// creates a vault.
    pub fn open_with_unlock_key(
        path: &Path,
        unlock: &UnlockKey,
    ) -> Result<SessionVault, VaultError> {
        let file = read_vault_file(path)?.ok_or(VaultError::Locked)?;
        let slot = file
            .unlock_slots
            .iter()
            .find(|slot| slot.id == unlock.slot_id)
            .ok_or(VaultError::Locked)?;
        let kek = unlock_kek(&unlock.key, &slot.id, &unlock.subject);
        let data_key = unwrap_data_key(
            &kek,
            &slot.nonce,
            &slot.ciphertext,
            &unlock_aad(&slot.id, &unlock.subject),
        )?;
        check_verifier(&file.verifier, &data_key)?;
        Ok(session(path, &unlock.subject, data_key))
    }
}

/// The key to one unlock slot: what a browser session lineage holds so that its refresh can reopen
/// the vault after a daemon restart without a new login.
///
/// **A wrap key, not a stored credential.** Alone it opens nothing — it is useful only together
/// with the vault file, which never leaves the daemon's disk — and it is rotated on every refresh
/// that presents it. The trade-off is stated in `docs/credential-store.md`: it crosses the same
/// plain-http LAN origin the session token does, and a copy of it is worth nothing without the
/// daemon's `auth_storage`.
///
/// Carries the subject and slot id beside the key, so a logout can find and remove its slot from
/// the key alone, whether or not its access token is still valid.
pub struct UnlockKey {
    subject: String,
    slot_id: String,
    key: SecretBytes,
}

impl UnlockKey {
    /// Whose vault this opens.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// Which unlock slot of that vault this opens.
    #[must_use]
    pub fn slot_id(&self) -> &str {
        &self.slot_id
    }

    /// The opaque form a client stores and sends back: `<hex subject>.<slot id>.<hex key>`.
    #[must_use]
    pub fn to_wire(&self) -> String {
        format!(
            "{}.{}.{}",
            to_hex(self.subject.as_bytes()),
            self.slot_id,
            to_hex(self.key.expose())
        )
    }

    /// Parse [`Self::to_wire`]'s form; `None` for anything else, including an empty string.
    #[must_use]
    pub fn from_wire(wire: &str) -> Option<Self> {
        let mut parts = wire.split('.');
        let (subject, slot_id, key) = (parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some() || slot_id.is_empty() || from_hex(slot_id).is_none() {
            return None;
        }
        let subject = String::from_utf8(from_hex(subject)?).ok()?;
        let mut key = from_hex(key)?;
        let bytes = <[u8; 32]>::try_from(key.as_slice()).ok();
        wipe(&mut key);
        let mut bytes = bytes?;
        let key = SecretBytes::new(bytes);
        wipe(&mut bytes);
        Some(Self {
            subject,
            slot_id: slot_id.to_string(),
            key,
        })
    }
}

impl std::fmt::Debug for UnlockKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnlockKey")
            .field("subject", &self.subject)
            .field("slot_id", &self.slot_id)
            .field("key", &self.key)
            .finish()
    }
}

impl SessionVault {
    /// Wrap the data key in a fresh unlock slot and hand back the only key that opens it.
    ///
    /// The returned key is for one browser session lineage to hold and present at its refresh; it
    /// is not written anywhere by this crate. Past [`MAX_UNLOCK_SLOTS`], the least recently used
    /// slot is evicted.
    pub fn add_unlock_slot(&self) -> Result<UnlockKey, VaultError> {
        let _serialised = serialised();
        let mut file = self.load()?;
        let (slot, unlock) = self.new_unlock_slot(to_hex(&random_16()))?;
        file.unlock_slots.push(slot);
        let excess = file.unlock_slots.len().saturating_sub(MAX_UNLOCK_SLOTS);
        file.unlock_slots.drain(..excess);
        write_vault_file(&self.path, &file)?;
        Ok(unlock)
    }

    /// Replace the wrap in slot `slot_id` under a new key, and hand that key back; the previous one
    /// opens nothing afterwards. The slot becomes the most recently used.
    ///
    /// A slot that is gone is [`VaultError::Locked`] — the lineage it belonged to has no way back
    /// in until its next login.
    pub fn rotate_unlock_slot(&self, slot_id: &str) -> Result<UnlockKey, VaultError> {
        let _serialised = serialised();
        let mut file = self.load()?;
        let at = file
            .unlock_slots
            .iter()
            .position(|slot| slot.id == slot_id)
            .ok_or(VaultError::Locked)?;
        file.unlock_slots.remove(at);
        let (slot, unlock) = self.new_unlock_slot(slot_id.to_string())?;
        file.unlock_slots.push(slot);
        write_vault_file(&self.path, &file)?;
        Ok(unlock)
    }

    /// Remove slot `slot_id`. Removing one that is not there is `Ok`.
    pub fn remove_unlock_slot(&self, slot_id: &str) -> Result<(), VaultError> {
        let _serialised = serialised();
        let mut file = self.load()?;
        let before = file.unlock_slots.len();
        file.unlock_slots.retain(|slot| slot.id != slot_id);
        if file.unlock_slots.len() == before {
            return Ok(());
        }
        write_vault_file(&self.path, &file)
    }

    /// The ids of the unlock slots this vault holds, least recently used first.
    pub fn unlock_slot_ids(&self) -> Result<Vec<String>, VaultError> {
        Ok(self
            .load()?
            .unlock_slots
            .into_iter()
            .map(|slot| slot.id)
            .collect())
    }

    fn new_unlock_slot(&self, id: String) -> Result<(UnlockSlot, UnlockKey), VaultError> {
        let mut fresh = random_32();
        let key = SecretBytes::new(fresh);
        wipe(&mut fresh);
        let kek = unlock_kek(&key, &id, &self.subject);
        let sealed = seal(
            &kek,
            self.data_key.expose(),
            &unlock_aad(&id, &self.subject),
        )?;
        Ok((
            UnlockSlot {
                id: id.clone(),
                nonce: sealed.nonce,
                ciphertext: sealed.ciphertext,
            },
            UnlockKey {
                subject: self.subject.clone(),
                slot_id: id,
                key,
            },
        ))
    }
}
