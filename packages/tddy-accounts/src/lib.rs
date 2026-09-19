//! `accounts.AccountsService` — what a person can see and curate in a daemon's credential store.
//!
//! Sits above the vault `tddy-credentials` seals and below the `/accounts` screen. Three methods,
//! and deliberately no fourth: this crate **shows and curates** what already exists, it never links
//! an account. Each provider's link flow is its own work, because each one is an authorization
//! dance with its own failure modes.
//!
//! **No response this crate produces carries a secret.** `accounts.proto` enforces that by message
//! shape — there is no secret field to fill — and [`AccountSummary::has_secret`] is the one bit a
//! screen needs to tell a linked account from a stale row.
//!
//! [`AccountSummary::has_secret`]: tddy_service::proto::accounts::AccountSummary::has_secret

mod resolver;
mod service;
mod store;

pub use resolver::{resolve_account, AccountResolution};
pub use service::{build_accounts_entry, AccountsServiceImpl};
pub use store::{AccountStore, AccountsError};
