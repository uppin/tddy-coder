//! `accounts.AccountsService` — what a person can see and curate in a daemon's credential store.
//!
//! Sits above the vault `tddy-credentials` seals and below the `/accounts` screen. Three methods
//! **show and curate** what already exists; two more add another account, and they are here rather
//! than beside the login flow for the reason [`linking`] exists: completing a login *is* becoming
//! that person, so the flow that adds a credential must not be the flow that establishes identity.
//!
//! One provider's authorization dance is implemented behind [`AccountLinker`]; each further
//! provider is its own work, because each has its own failure modes.
//!
//! **No response this crate produces carries a secret.** `accounts.proto` enforces that by message
//! shape — there is no secret field to fill — and [`AccountSummary::has_secret`] is the one bit a
//! screen needs to tell a linked account from a stale row.
//!
//! [`AccountSummary::has_secret`]: tddy_service::proto::accounts::AccountSummary::has_secret
//! [`linking`]: crate::linking

mod identity;
mod linking;
mod resolver;
mod service;
mod store;

pub use identity::{acting_identity, ActingIdentity, GitIdentity, IdentityError, PROVIDER_GITHUB};
pub use linking::{
    record_for_link, removal_allowed, AccountLinker, LinkChallenge, LinkError, LinkProgress,
    LinkedAccountStore, LinkedIdentity, RemovalRefusal, META_SUBJECT, META_SUBJECT_ID,
};
pub use resolver::{resolve_account, AccountResolution};
pub use service::{build_accounts_entry, AccountsServiceImpl};
pub use store::{AccountStore, AccountsError};
