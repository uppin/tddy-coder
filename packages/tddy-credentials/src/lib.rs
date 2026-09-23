//! Where a credential lives, and what it takes to read one.
//!
//! This crate holds `(provider, account)`-keyed records — a GitHub account's access token today, a
//! Cloudflare API key or a screen-sharing password later — in a single file whose label, metadata
//! **and** secret are one sealed AEAD unit. At rest the file holds ciphertext and a wrapped data
//! key; nothing in it opens it. The key that does is derived at login from the user's own
//! credential, so a daemon with nobody signed in can read nothing.
//!
//! # Two rules this crate inherits, and why they are written down here
//!
//! The trait this replaces (`tddy_github::token_store::GitHubTokenStore`) carried both of them in a
//! doc comment, and deleting that file would delete the only place they are stated.
//!
//! 1. **A secret never reaches an RPC response path.** The session token a daemon hands a browser
//!    travels over a plain-http LAN origin, so a live `repo`-scoped GitHub credential must never be
//!    carried in it or returned to the client. [`SessionVault`]'s reads exist to be *used* by the
//!    daemon, not forwarded.
//! 2. **A failed write fails the login.** A session minted without its credential is a half-login:
//!    the operator appears signed in while every credential-backed read reports itself unavailable,
//!    and re-authenticating — the one remedy — is the one action they have no reason to attempt. So
//!    [`SessionVault::put`] returns `Ok` only once the record is durably retained, and every
//!    failure is reported rather than absorbed. [`VaultError::Locked`] extends the same rule: a
//!    vault that cannot be opened fails the login too, distinctly, rather than as a generic refusal.
//!
//! # No second way in
//!
//! There is no daemon-held master key. A second key that does not need a user would make the daemon
//! able to read credentials with nobody present — exactly the property this crate exists to remove.
//! The cost is accepted explicitly: when the user's credential rotates with no live session,
//! [`VaultError::Locked`] is reported and the accounts are re-linked into a fresh vault. Nothing is
//! re-initialised silently. Every successful login calls [`SessionVault::rewrap`], so a rotation
//! observed while a session can still be established costs nothing.

mod kdf;
pub mod record;
pub mod secret;
pub mod sessions;
pub mod vault;

pub use record::{AccountId, CredentialRecord, ProviderId};
pub use secret::{SecretBytes, SecretString};
pub use sessions::{Reset, Retained, SessionVaults, VaultState, ROTATION_GRACE};
pub use vault::{
    CredentialStore, SessionVault, UnlockKey, VaultError, MAX_UNLOCK_SLOTS, MIN_PASSPHRASE_CHARS,
};
