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
    pub updated_at: u64,
}
