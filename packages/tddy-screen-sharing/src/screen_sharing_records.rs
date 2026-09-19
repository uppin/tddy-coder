//! Screen-sharing targets as credential-store records.
//!
//! A desktop's password is a credential like any other, so it is a `CredentialRecord` in the
//! session-gated store (`#keyring` 3/9) rather than an entry in a vault this crate owns. That is
//! the whole claim this node makes: a **second** provider costs a [`ProviderId`] and a mapping,
//! and no provider-specific storage, crypto or sync code.
//!
//! # What lives where in the record
//!
//! | Target field | In the record |
//! |---|---|
//! | `id` | the [`AccountId`] — the record's identity at this provider |
//! | `label` | `label` |
//! | `host`, `port`, `protocol`, `username` | `metadata`, under the keys below |
//! | the password | `secret` |
//!
//! **Every one of those is inside the AEAD**, which is the limit being fixed relative to
//! [`crate::screen_sharing_vault`]: there a target's label and host sit in cleartext beside the
//! sealed password, so anyone who can read the file learns which machines the operator reaches
//! without ever opening a secret. Here, tampering with `host` fails the open.

use tddy_credentials::{AccountId, CredentialRecord, ProviderId};
use tddy_service::proto::screen_sharing::ScreenSharingTarget;

/// The provider every screen-sharing credential is stored under.
///
/// Lowercase kebab-case, matching the convention `ProviderId`'s own docs set. The Accounts screen
/// (`#keyring` 4/9) groups by this string without knowing what it means.
pub const SCREEN_SHARING_PROVIDER: &str = "screen-sharing";

/// Metadata key holding the target's hostname or address.
pub const META_HOST: &str = "host";
/// Metadata key holding the target's TCP port, rendered as decimal.
pub const META_PORT: &str = "port";
/// Metadata key holding the wire protocol, as the proto enum's own name (`VNC`, `RDP`).
///
/// The enum's **name** rather than its number, because a record outlives the build that wrote it
/// and a renumbered enum would silently reinterpret every stored target.
pub const META_PROTOCOL: &str = "protocol";
/// Metadata key holding the login username — required for RDP, optional for VNC.
pub const META_USERNAME: &str = "username";

/// Name the provider screen-sharing credentials are stored under.
#[must_use]
pub fn screen_sharing_provider() -> ProviderId {
    ProviderId::new(SCREEN_SHARING_PROVIDER)
}

/// The account a target occupies at that provider.
///
/// A target's id **is** its account: two desktops are two accounts of the `screen-sharing`
/// provider, exactly as two GitHub logins are two accounts of `github`.
#[must_use]
pub fn account_for(target_id: &str) -> AccountId {
    AccountId::new(target_id)
}

/// Why a target operation did not happen.
///
/// `Locked` is a distinct outcome and not an empty list, for the same reason `#keyring` 4/9 makes
/// `vault_locked` a field: a store that opens and holds nothing and a store this session's key no
/// longer unwraps are different situations, and the second is recoverable by re-linking. Rendering
/// it as "no targets" tells the person to add desktops they already have.
/// Shaped after `#keyring` 4/9's `AccountsError` and deliberately carrying no `thiserror`
/// derive — this crate gains no dependency to describe four outcomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetError {
    /// The token names no live session, so there is no key and nothing to open.
    NoSuchSession,

    /// A store exists and this session's key does not unwrap it.
    Locked,

    /// A record under this provider does not carry the metadata a target needs. Corruption or a
    /// record written by something that is not this mapping — never silently defaulted, because a
    /// target with a defaulted host is a target that connects somewhere unintended.
    Malformed(String),

    /// Anything else the store reported, verbatim. A swallowed reason is what turns a one-line fix
    /// into an afternoon.
    Unavailable(String),
}

/// The record that holds this target, with `password` as its secret.
///
/// `written_at` is the store's reconciliation clock — seconds since the Unix epoch — and is passed
/// in rather than read here so the caller controls the clock in a test.
#[must_use]
pub fn record_for(
    _target: &ScreenSharingTarget,
    _password: &str,
    _written_at: u64,
) -> CredentialRecord {
    todo!("TODO(keyring 7/9): implement — id to account, metadata into the sealed map")
}

/// The target a record describes, or why it is not one.
///
/// Returns a [`TargetError::Malformed`] rather than a defaulted target when the metadata is
/// missing or unparseable: see that variant's note.
pub fn target_from(_record: &CredentialRecord) -> Result<ScreenSharingTarget, TargetError> {
    todo!("TODO(keyring 7/9): implement — metadata back into the target, refusing defaults")
}

/// The port this service reads and writes screen-sharing credentials through.
///
/// Expressed over targets and a `session_token` rather than over `tddy_credentials::SessionVault`,
/// following the shape `#keyring` 4/9's `AccountStore` established and for the same two reasons: a
/// `SessionVault` can only be obtained by sealing or opening a real file, and resolving a
/// `session_token` to the vault it unlocks is the daemon's job, not this crate's.
///
/// There is deliberately **no** `unlock`. The session is the key.
pub trait ScreenSharingTargetStore: Send + Sync {
    /// Every target this session's store holds, in the store's own account order.
    fn list(&self, session_token: &str) -> Result<Vec<ScreenSharingTarget>, TargetError>;

    /// Retain `target` with `password` as its secret, and answer with the target as stored —
    /// including the id the store minted for it.
    fn add(
        &self,
        session_token: &str,
        target: &ScreenSharingTarget,
        password: &str,
    ) -> Result<ScreenSharingTarget, TargetError>;

    /// Forget one target. Removing one that is not held is not an error.
    fn remove(&self, session_token: &str, target_id: &str) -> Result<(), TargetError>;

    /// The password sealed with this target, for handing to a bridge process.
    ///
    /// Separate from [`list`](Self::list) because a listing crosses an RPC boundary and a password
    /// must not: `tddy-credentials` rule 1 — a secret never reaches an RPC response path.
    fn password_for(&self, session_token: &str, target_id: &str) -> Result<String, TargetError>;
}
