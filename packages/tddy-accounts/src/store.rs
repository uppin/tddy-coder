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
/// The four outcomes a screen must be able to tell apart are an open-and-empty store (`Ok(vec![])`)
/// and these three. Collapsing any pair of them would present a recoverable, explainable failure as
/// a normal state — see the `#keyring` 4/9 PRD on why `vault_locked` is a field and not an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountsError {
    /// The token names no live session, so there is no key and no vault to open. A refusal, never
    /// an empty listing.
    NoSuchSession,
    /// A vault exists and this session's key does not unwrap it — the login credential changed.
    /// Recoverable by re-linking, which is why the screen must say so rather than show nothing.
    Locked,
    /// I/O, corruption, or anything else that is neither of the above. Carries the reason **meant
    /// for the person** — which is not always the underlying error's text: an I/O failure names
    /// server-side paths, so it arrives here as a fixed, path-free sentence and its full detail is
    /// logged on the daemon instead. Never empty; a swallowed reason is what turns a one-line fix
    /// into an afternoon.
    Unavailable(String),
}

/// Read and curate credential records on behalf of one session.
///
/// Every method takes the caller's `session_token` rather than a pre-opened handle: the vault's key
/// is derived from the session, so there is nothing to open until the token is resolved.
pub trait AccountStore: Send + Sync {
    /// Every record the session's vault holds, in the store's own `(provider, account)` order.
    fn list(&self, session_token: &str) -> Result<Vec<CredentialRecord>, AccountsError>;

    /// Rename one record. Returns the record as it now stands, with `account` unchanged — a rename
    /// that moved the identity would silently break `#keyring` 5/9's project assignments.
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
