//! The record model: what is stored, and what identifies it.
//!
//! `(provider, account)` rather than `login → token` is the whole point of the replacement. A
//! second GitHub account, a Cloudflare API key and a screen-sharing password differ in their
//! provider and their account, not in their storage, so one store holds all of them and a new
//! provider costs a `ProviderId` rather than a new file format.

use serde::{Deserialize, Serialize};

/// Which service a credential authenticates against — `github`, `cloudflare`, `screen-sharing`.
///
/// A newtype rather than an enum: a provider is data, and adding one must not be a breaking change
/// to every `match` in the tree. Values are lowercase kebab-case by convention.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProviderId(String);

impl ProviderId {
    /// Name a provider.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which account *at* that provider — a GitHub login, a Cloudflare account id.
///
/// Distinct from the identity of the user who owns the vault: one operator holds several accounts,
/// which is the requirement this model exists to serve.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AccountId(String);

impl AccountId {
    /// Name an account at a provider.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One stored credential.
///
/// `label` and `metadata` are **inside** the AEAD along with `secret`, which is the limit being
/// fixed relative to `tddy_screen_sharing::screen_sharing_vault`: there, a target's label and host
/// sit in cleartext beside the sealed password, so anyone who can read the file learns what the
/// operator has access to even without the secret. Here, tampering with either fails the open.
///
/// `metadata` carries whatever a provider needs that is not the secret — a refresh token's expiry,
/// the scopes granted, the avatar URL a UI shows. It is a map rather than typed fields so a new
/// provider does not change this struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialRecord {
    pub provider: ProviderId,
    pub account: AccountId,
    /// What a human calls this account in the Accounts screen.
    pub label: String,
    /// The credential itself. Never returned to an RPC response path.
    pub secret: String,
    /// Provider-specific detail that is not the secret.
    pub metadata: std::collections::BTreeMap<String, String>,
    /// When this record was last written, as seconds since the Unix epoch.
    ///
    /// The **reconciliation clock**: when two daemons hold different records for one
    /// `(provider, account)`, the later `updated_at` wins. [`version`] does not decide that — see
    /// its own note for the division of labour.
    ///
    /// [`version`]: CredentialRecord::version
    pub updated_at: u64,
    /// How many times this record has been written, starting at [`FIRST_VERSION`].
    ///
    /// **Not the tie-breaker.** `updated_at` decides which of two records survives, because two
    /// daemons editing independently both reach version 2 and neither is "further along". What the
    /// version is for is the *other* direction: a [`Tombstone`] carries the version it deletes, so
    /// a peer still holding version 3 of a record deleted at version 3 knows the deletion is about
    /// the record it has, and not an echo of one it already replaced.
    pub version: u64,
}

/// The version a record carries the first time it is written.
pub const FIRST_VERSION: u64 = 1;

/// The mark a deletion leaves behind.
///
/// A deletion that simply removed the record would not survive one sync: the peer that still holds
/// it would send it back and the person's removal would silently undo itself. So a removal writes
/// *this* — the same `(provider, account)` key, the version it deletes, and when — and the entry
/// stays in the vault carrying no secret, no label and no metadata.
///
/// It is deliberately not a `CredentialRecord` with an empty secret. An empty secret is a record
/// that authenticates against nothing, which is a different and much worse thing to hand a caller
/// than an entry that says the account is gone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tombstone {
    pub provider: ProviderId,
    pub account: AccountId,
    /// The version this deletion removes.
    pub version: u64,
    /// When the deletion happened, as seconds since the Unix epoch. Reconciled against a record's
    /// `updated_at` on the same clock, so a later edit *can* legitimately resurrect an account.
    pub deleted_at: u64,
}

/// What one `(provider, account)` slot holds: a credential, or the fact that there is not one.
///
/// The vault stores these rather than bare records, which is what makes a deletion a thing that can
/// be *propagated* instead of an absence that cannot be told from "this peer never had it".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VaultEntry {
    Record(CredentialRecord),
    Tombstone(Tombstone),
}

impl VaultEntry {
    #[must_use]
    pub fn provider(&self) -> &ProviderId {
        match self {
            Self::Record(record) => &record.provider,
            Self::Tombstone(tombstone) => &tombstone.provider,
        }
    }

    #[must_use]
    pub fn account(&self) -> &AccountId {
        match self {
            Self::Record(record) => &record.account,
            Self::Tombstone(tombstone) => &tombstone.account,
        }
    }

    #[must_use]
    pub fn version(&self) -> u64 {
        match self {
            Self::Record(record) => record.version,
            Self::Tombstone(tombstone) => tombstone.version,
        }
    }

    /// When this entry was written, on the one clock both variants share.
    ///
    /// A record's `updated_at` and a tombstone's `deleted_at` are the same measurement of the same
    /// thing — when this daemon last decided what the slot holds — so reconciliation compares them
    /// directly and neither variant is privileged over the other.
    #[must_use]
    pub fn written_at(&self) -> u64 {
        match self {
            Self::Record(record) => record.updated_at,
            Self::Tombstone(tombstone) => tombstone.deleted_at,
        }
    }

    /// The record here, or `None` when the slot holds a deletion.
    #[must_use]
    pub fn record(&self) -> Option<&CredentialRecord> {
        match self {
            Self::Record(record) => Some(record),
            Self::Tombstone(_) => None,
        }
    }
}
