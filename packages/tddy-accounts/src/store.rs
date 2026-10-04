//! The port this service reads the credential store through.
//!
//! Expressed over `tddy-credentials`' **data** types rather than over its `SessionVault`, for two
//! reasons. A `SessionVault` can only be obtained by sealing or opening a real file, so a test of
//! this crate's grouping would be a test of that crate's cryptography. And resolving a
//! `session_token` to the vault it unlocks is the daemon's job, not this crate's — the port is the
//! seam where that resolution happens.

use tddy_credentials::{AccountId, CredentialRecord, ProviderId};

/// Why the store could not answer.
///
/// The five outcomes a listing must be able to tell apart are an open-and-empty store
/// (`Ok(vec![])`) and the four refusals other than [`AccountsError::NotFound`], which only a rename
/// can meet. Collapsing any pair of them would present a recoverable, explainable state as a
/// normal one — see the `#keyring` 4/9 PRD on why `vault_locked` and `vault_uninitialized` are
/// fields and not errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountsError {
    /// The token names no live session, so there is no subject and no vault to open. A refusal,
    /// never an empty listing.
    NoSuchSession,
    /// A vault exists and is not unlocked on this daemon — it restarted, or nothing has opened the
    /// vault since. Nothing is lost: its passphrase opens it, and so does a session refresh that
    /// presents an unlock key.
    Locked,
    /// No vault exists for this subject yet. Choosing a passphrase creates one; until then there is
    /// nothing to list, and that is not the same as an empty vault.
    Uninitialized,
    /// The vault is open and holds no record for this provider and account, so there is nothing to
    /// rename. The caller's view is stale — the account was removed, or was never linked — and that
    /// is the caller's to hear, not a failure of the store.
    NotFound {
        provider: ProviderId,
        account: AccountId,
    },
    /// I/O, corruption, or anything else that is neither of the above. Carries the reason **meant
    /// for the person** — which is not always the underlying error's text: an I/O failure names
    /// server-side paths, so it arrives here as a fixed, path-free sentence and its full detail is
    /// logged on the daemon instead. Never empty; a swallowed reason is what turns a one-line fix
    /// into an afternoon.
    Unavailable(String),
}

/// Read and curate credential records on behalf of one session.
///
/// Every method takes the caller's `session_token` rather than a pre-opened handle: the token is what
/// names the subject whose vault is meant, so there is nothing to look up until it is resolved.
pub trait AccountStore: Send + Sync {
    /// Every record the session's vault holds, in the store's own `(provider, account)` order.
    fn list(&self, session_token: &str) -> Result<Vec<CredentialRecord>, AccountsError>;

    /// Rename one record. Returns the record as it now stands, with `account` unchanged — a rename
    /// that moved the identity would silently break `#keyring` 5/9's project assignments. A record
    /// that is not there is [`AccountsError::NotFound`].
    fn set_label(
        &self,
        session_token: &str,
        provider: &ProviderId,
        account: &AccountId,
        label: &str,
    ) -> Result<CredentialRecord, AccountsError>;

    /// Forget one record. Removing one that is not there is not an error — the caller wanted it
    /// gone, and it is.
    fn remove(
        &self,
        session_token: &str,
        provider: &ProviderId,
        account: &AccountId,
    ) -> Result<(), AccountsError>;
}
