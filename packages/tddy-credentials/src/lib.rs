//! Where a credential lives, and what it takes to read one.
//!
//! This crate holds `(provider, account)`-keyed records — a GitHub account's access token today, a
//! Cloudflare API key or a screen-sharing password later — in a single file whose label, metadata
//! **and** secret are one sealed AEAD unit. At rest the file holds ciphertext and wrapped keys;
//! nothing in it opens it. What does is the user's **vault passphrase** (Argon2id), or an unlock
//! key one of their browser session lineages holds — so a daemon with nobody present can read
//! nothing.
//!
//! The key is not derived from a login. A GitHub OAuth App mints a new access token at every code
//! or device exchange, so a key derived from one would lock the vault at the first fresh login
//! after a restart. A login's token is a *record* here, not key material.
//!
//! # Two rules this crate inherits, and why they are written down here
//!
//! The trait this replaces (`tddy_github::token_store::GitHubTokenStore`) carried both of them in a
//! doc comment, and deleting that file would delete the only place they are stated.
//!
//! 1. **A secret never reaches an RPC response path.** The session token a daemon hands a browser
//!    travels over a plain-http LAN origin, so a live `repo`-scoped GitHub credential must never be
//!    carried in it or returned to the client. [`CredentialRecord`] is not serialisable and its
//!    secret is a [`SecretString`] that prints redacted, so this is a property of the types.
//! 2. **A login whose credential cannot be retained is reported, never silent.** A session minted
//!    without its credential is a half-login: the operator appears signed in while every
//!    credential-backed read reports itself unavailable. So a login over a closed vault says so
//!    ([`VaultState`]) and holds the credential until the vault opens, and a failed write —
//!    [`SessionVault::put`] returns `Ok` only once the record is durably on disk — fails the login.
//!
//! # No second way in
//!
//! There is no daemon-held master key. A key that needed no user would let the daemon read
//! credentials with nobody present — exactly the property this crate exists to remove. The cost is
//! accepted explicitly: a forgotten passphrase is a reset, which sets the old vault aside (never
//! deletes it) and starts an empty one whose credentials must be linked again.

mod atomic;
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
