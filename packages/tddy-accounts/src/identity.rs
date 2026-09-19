//! Who a session acts as at a provider — resolved once, for both halves at the same time.
//!
//! A session that pushes to GitHub needs two things: a token to authenticate with, and a name and
//! email to author the commit under. They are not two lookups. Resolving them separately is how a
//! commit ends up authored by one person and pushed by another — a mismatch nobody notices until
//! the repository's history has the wrong name on it — so this module publishes **one** entry
//! point, [`acting_identity`], returning **one** [`ActingIdentity`] that carries both. There is no
//! `token_for` and no `identity_for`, deliberately.
//!
//! # No environment anywhere in here
//!
//! The identity comes from the project's assignment and this host's vault, and from nowhere else.
//! Not `GITHUB_TOKEN`, not `GH_TOKEN`, not `git config user.email` in the checkout. The rule is
//! absolute rather than a preference, because the failure it prevents is silent: a fallback turns
//! "this project assigns no account, so the push must fail" into "the push succeeded, as whoever
//! the daemon's environment happens to belong to", against a repository that person can write to.
//! [`IdentityError`] is what a caller gets instead, and every one of its refusals is reported.
//!
//! # The identity is derived, never stored
//!
//! [`GitIdentity`] is computed from the record's provider metadata — the immutable
//! [`META_SUBJECT_ID`] and the display [`META_SUBJECT`] `#keyring` 8/9 writes at link time — and
//! never from [`CredentialRecord::label`]. The label is the one mutable field on a record, and
//! renaming an account in the Accounts screen must not change who its past or future commits are
//! authored by.
//!
//! [`META_SUBJECT_ID`]: crate::META_SUBJECT_ID
//! [`META_SUBJECT`]: crate::META_SUBJECT

use tddy_credentials::{AccountId, CredentialRecord, ProviderId};

/// The one provider whose git identity convention this crate knows.
///
/// A constant rather than a literal at each call site: the same string keys the vault, the project
/// assignments and the linking flow, and a typo in one of them resolves to `NotAssigned` — which
/// reads exactly like a project nobody has assigned an account to.
pub const PROVIDER_GITHUB: &str = "github";

/// The name and email a commit made in a session is authored and committed under.
///
/// Both halves at once, and the same pair for author and committer: a session's commit is the
/// work of the account the project assigns, in both roles. The daemon's own snapshot commits are a
/// different thing entirely and keep their own identity — see `session_room::publish_wip_ref`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitIdentity {
    /// The account's handle at the provider — a GitHub login. Never the label a person typed.
    pub name: String,
    /// The account's address at the provider. For GitHub, the `users.noreply.github.com` form
    /// built from the immutable subject id and the login, which is what the provider itself
    /// attributes a commit by.
    pub email: String,
}

/// Everything one resolution produced.
///
/// The token and the identity are fields of one value because they are answers to one question.
/// A caller that has this has both, and a caller that has neither cannot obtain one of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActingIdentity {
    /// Which account answered — the same id `#keyring` 5/9's assignment named.
    pub account: AccountId,
    /// The credential to authenticate with. Never carried onto an RPC response path.
    pub token: String,
    /// Who the commits are by.
    pub git: GitIdentity,
}

/// Why a session has no identity at a provider.
///
/// Four refusals, and they read differently on purpose. "This project assigns no account" and
/// "this project assigns an account this host has not received yet" are the same outcome to a
/// caller that only asks whether it got a token, and they need different words in front of a
/// person: the first is a setting nobody has made, the second is a vault that has not caught up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    /// The project assigns no account at this provider. **Nothing is substituted.**
    NotAssigned { provider: ProviderId },
    /// An account is assigned and this host's vault does not hold it — see
    /// [`AccountResolution::UnknownOnThisHost`](crate::AccountResolution::UnknownOnThisHost).
    UnknownOnThisHost {
        provider: ProviderId,
        account: AccountId,
    },
    /// More than one account is assigned at the one provider. Refused rather than picked.
    Ambiguous { provider: ProviderId },
    /// The record resolved, and it cannot produce a git identity: the provider's own identifiers
    /// are not in its metadata. `missing` names the key that was absent.
    ///
    /// Refused rather than guessed. A commit authored under an invented address is attributed to
    /// nobody, and it is the kind of wrong that is only discovered by reading history.
    Unusable {
        provider: ProviderId,
        account: AccountId,
        missing: &'static str,
    },
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = f;
        todo!("TODO(keyring 9/9): four refusals, each naming what is wrong in its own words")
    }
}

impl std::error::Error for IdentityError {}

/// Resolve who a project acts as at `provider`, from its assignments and what this host holds.
///
/// `assignments` is the project row's whole set across every provider, and `held` is what the
/// session's vault answered — both plain data, so the vault reading and the session gating happen
/// before this call and every outcome is testable without a sealed file on disk. Delegates the
/// choice of account to [`resolve_account`](crate::resolve_account) (`#keyring` 5/9) and adds the
/// half this node owns: turning the chosen record into a usable token **and** identity, or into
/// the refusal that names why there is neither.
pub fn acting_identity(
    assignments: &[(ProviderId, AccountId)],
    provider: &ProviderId,
    held: &[CredentialRecord],
) -> Result<ActingIdentity, IdentityError> {
    let _ = (assignments, provider, held);
    todo!("TODO(keyring 9/9): one resolution, then the record's own token and derived identity")
}
