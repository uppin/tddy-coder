//! The port this service reads `#keyring` 6/9's per-account sync standing through.
//!
//! Expressed over `tddy-credential-sync`'s own [`tddy_credential_sync::AccountSyncSummary`]
//! rather than this crate re-deriving one: the aggregation across peers — worst status wins — is
//! that crate's `SyncJournal::account_summary`, and duplicating its severity ordering here would
//! be a second place that fact could drift from the first.

use tddy_credential_sync::AccountSyncSummary;
use tddy_credentials::{AccountId, ProviderId};

/// Where one `(provider, account)` record stands across every peer it has been offered to.
///
/// `None` means nothing has ever been attempted for this record — no peer is configured to
/// receive credentials (the common case: no `keyring.group_secret`), or none has been admitted
/// yet. A daemon with no sync engine wired at all simply has no [`SyncStatusSource`] registered,
/// which `AccountsServiceImpl` already treats the same way.
pub trait SyncStatusSource: Send + Sync {
    fn status_for(&self, provider: &ProviderId, account: &AccountId) -> Option<AccountSyncSummary>;
}
