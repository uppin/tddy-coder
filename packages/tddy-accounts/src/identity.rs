//! Who a session acts as at a provider — and what a read needs, which is less.
//!
//! A session that pushes to GitHub needs two things: a token to authenticate with, and a name and
//! email to author the commit under. They are not two lookups. Resolving them separately is how a
//! commit ends up authored by one person and pushed by another — a mismatch nobody notices until
//! the repository's history has the wrong name on it — so commits and pushes resolve through
//! **one** entry point, [`acting_identity`], returning **one** [`ActingIdentity`] that carries
//! both.
//!
//! Reads and PR-API calls are not commits. A PR read, a check run, a comment, opening or updating
//! a PR, assigning one to a work item: none of them authors anything, so making each of them prove
//! a commit identity refuses operations that never needed one — every one of them, for an account
//! linked before the provider identifiers existed. Those resolve through [`acting_token`], which
//! answers the account and its token from the record alone and never looks at the metadata. The
//! pairing above is for commits and pushes; the token alone is for everything that only asks
//! GitHub a question or files one.
//!
//! # A missing identity is derived, then kept
//!
//! A record with no [`META_SUBJECT_ID`] or [`META_SUBJECT`] still refuses the *paired*
//! resolution: [`acting_identity`] is pure — it cannot ask GitHub anything — so
//! [`IdentityError::Unusable`] is still what it answers. The commit path above this crate turns
//! that refusal into a question: `GET /user` with the account's own token answers the provider id
//! and login, which are stamped back into the vault record ([`crate::stamped_record`]) so the next
//! resolution — and the next session start — finds them. Only a derivation that fails refuses the
//! commit, as [`IdentityError::DerivationFailed`], naming the account and GitHub's reason.
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
//! # Built from the record's identifiers, never its label
//!
//! [`GitIdentity`] is computed from the record's provider metadata — the immutable
//! [`META_SUBJECT_ID`] and the display [`META_SUBJECT`] `#keyring` 8/9 writes at link time — and
//! never from [`CredentialRecord::label`]. The label is the one mutable field on a record, and
//! renaming an account in the Accounts screen must not change who its past or future commits are
//! authored by.
//!
//! [`META_SUBJECT_ID`]: crate::META_SUBJECT_ID
//! [`META_SUBJECT`]: crate::META_SUBJECT

use crate::{resolve_account, AccountResolution, META_SUBJECT, META_SUBJECT_ID};
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

/// Everything a read resolved: the account and the token to ask with.
///
/// No identity: a read authors nothing, so there is nothing to attribute. A caller that has this
/// can ask GitHub questions as the account; a caller that needs to commit resolves an
/// [`ActingIdentity`] instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActingToken {
    /// Which account answered — the same id `#keyring` 5/9's assignment named.
    pub account: AccountId,
    /// The credential to authenticate with. Never carried onto an RPC response path.
    pub token: String,
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
    /// The record resolved but carries no provider identifiers, and asking the provider who its
    /// token belongs to failed. Returned by the commit path above this crate, never by the pure
    /// [`acting_identity`] — which cannot ask, and answers [`Self::Unusable`] instead.
    DerivationFailed {
        provider: ProviderId,
        account: AccountId,
        /// The metadata key the record lacks — what the provider was asked to supply.
        missing: &'static str,
        /// Why the provider did not answer, in its (or the transport's) own words.
        reason: String,
    },
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAssigned { provider } => write!(
                f,
                "this project has no {provider} account assigned; assign one to the project in \
                 its settings, and nothing in the daemon's environment will stand in for it"
            ),
            Self::UnknownOnThisHost { provider, account } => write!(
                f,
                "this project is assigned the {provider} account {account}, which this host does \
                 not hold yet; link it on this host or wait for the vault to reach it"
            ),
            Self::Ambiguous { provider } => write!(
                f,
                "this project is assigned more than one {provider} account and none is chosen \
                 for it; keep one assignment for {provider} and remove the others"
            ),
            Self::Unusable {
                provider,
                account,
                missing,
            } => write!(
                f,
                "the {provider} account {account} has no `{missing}` recorded, so there is no \
                 identity to commit under; link the account again to refresh it"
            ),
            Self::DerivationFailed {
                provider,
                account,
                missing,
                reason,
            } => write!(
                f,
                "the {provider} account {account} has no `{missing}` recorded, and asking \
                 {provider} who its token belongs to failed ({reason}), so there is no identity \
                 to commit under; link the account again to refresh it"
            ),
        }
    }
}

impl std::error::Error for IdentityError {}

/// The commit identity the provider's own identifiers make: the login as the name, and the
/// `users.noreply.github.com` address the provider attributes a commit by as the email.
///
/// Both the stored resolution ([`acting_identity`]) and a freshly derived answer build through
/// here, so a stamped record and a derivation cannot disagree on the address.
#[must_use]
pub fn git_identity_of(subject_id: &str, login: &str) -> GitIdentity {
    GitIdentity {
        name: login.to_string(),
        email: format!("{subject_id}+{login}@users.noreply.github.com"),
    }
}

/// The assigned record both resolutions start from: [`resolve_account`]'s choice, and the record
/// in `held` it names.
fn resolved_record<'held>(
    assignments: &[(ProviderId, AccountId)],
    provider: &ProviderId,
    held: &'held [CredentialRecord],
) -> Result<(AccountId, &'held CredentialRecord), IdentityError> {
    let account = match resolve_account(assignments, provider, held) {
        AccountResolution::Assigned(account) => account,
        AccountResolution::NotAssigned => {
            return Err(IdentityError::NotAssigned {
                provider: provider.clone(),
            })
        }
        AccountResolution::UnknownOnThisHost(account) => {
            return Err(IdentityError::UnknownOnThisHost {
                provider: provider.clone(),
                account,
            })
        }
        AccountResolution::Ambiguous(provider) => {
            return Err(IdentityError::Ambiguous { provider })
        }
    };
    let record = held
        .iter()
        .find(|r| &r.provider == provider && r.account == account)
        .expect("`resolve_account` answers Assigned only for a record in `held`");
    Ok((account, record))
}

/// Resolve what a read asks `provider` with: the account `assignments` name and its token, from
/// what this host holds.
///
/// Reads and PR-API calls — PR status, checks, comments, opening or updating a PR, assigning one
/// to a work item — authenticate as the account and author nothing, so this never looks at the
/// record's provider metadata: a record linked before `subject_id` existed still yields its token.
/// It answers the resolver's own refusals ([`IdentityError::NotAssigned`],
/// [`IdentityError::UnknownOnThisHost`], [`IdentityError::Ambiguous`]) and never
/// [`IdentityError::Unusable`] — there is no identity here to be unusable.
pub fn acting_token(
    assignments: &[(ProviderId, AccountId)],
    provider: &ProviderId,
    held: &[CredentialRecord],
) -> Result<ActingToken, IdentityError> {
    let (account, record) = resolved_record(assignments, provider, held)?;
    Ok(ActingToken {
        account,
        token: record.secret.expose().to_string(),
    })
}

/// Resolve who a project acts as at `provider`, from its assignments and what this host holds.
///
/// `assignments` is the project row's whole set across every provider, and `held` is what the
/// session's vault answered — both plain data, so the vault reading and the session gating happen
/// before this call and every outcome is testable without a sealed file on disk. Delegates the
/// choice of account to [`resolve_account`](crate::resolve_account) (`#keyring` 5/9) and adds the
/// half this node owns: turning the chosen record into a usable token **and** identity, or into
/// the refusal that names why there is neither.
///
/// The paired resolution, for commits and pushes. A record with no provider identifiers is
/// [`IdentityError::Unusable`] here — this function cannot ask the provider anything — and the
/// commit path above derives and persists the missing half instead of stopping at it. Reads take
/// [`acting_token`].
pub fn acting_identity(
    assignments: &[(ProviderId, AccountId)],
    provider: &ProviderId,
    held: &[CredentialRecord],
) -> Result<ActingIdentity, IdentityError> {
    let (account, record) = resolved_record(assignments, provider, held)?;
    let metadata = |key: &'static str| {
        record
            .metadata
            .get(key)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| IdentityError::Unusable {
                provider: provider.clone(),
                account: account.clone(),
                missing: key,
            })
    };
    let subject_id = metadata(META_SUBJECT_ID)?;
    let login = metadata(META_SUBJECT)?;
    Ok(ActingIdentity {
        account,
        token: record.secret.expose().to_string(),
        git: git_identity_of(subject_id, login),
    })
}
