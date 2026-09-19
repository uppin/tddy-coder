//! Which account a project uses at a provider — and what it means when none does.
//!
//! Four answers, kept distinct on purpose. Collapsing any two of them is how a commit ends up
//! authored by the wrong person: "no assignment" and "assigned to an account this host has never
//! seen" look identical from a call site that only asks *"do I have a token?"*, and they need
//! different words in front of a person.

use tddy_credentials::{AccountId, CredentialRecord, ProviderId};

/// What resolving a project's account at one provider produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountResolution {
    /// Exactly one account is assigned for the provider, and this host holds it.
    Assigned(AccountId),
    /// The project assigns no account at this provider.
    ///
    /// **This resolves to nothing.** Not the caller's own login, not the only account the vault
    /// happens to hold — the operation that wanted a credential fails, and says why. A resolver
    /// that helpfully picked a candidate here would push commits under an identity nobody chose.
    NotAssigned,
    /// An account is assigned, but this host's vault has no record of it.
    ///
    /// Reachable because an [`AccountId`] is minted by the daemon that linked it, so an assignment
    /// forwarded to a peer names something that peer will not hold until the vault is propagated
    /// (`#keyring` 6/9). Distinct from [`Self::NotAssigned`]: a person's own choice is present,
    /// just not usable here yet.
    UnknownOnThisHost(AccountId),
    /// More than one account is assigned for the one provider.
    ///
    /// `SetProjectAccounts` refuses to store this, so it is reachable only from a row written by
    /// something that bypassed the RPC. It is an answer rather than a panic because a registry file
    /// is editable by hand.
    Ambiguous(ProviderId),
}

/// Resolve `provider`'s account for a project, given the project's assignments and the records this
/// host's vault holds.
///
/// `assignments` is the project row's whole set, across every provider; `held` is what the vault
/// answered for the session doing the work. Both are plain data, so this is a pure function — the
/// vault reading and the session gating happen before it, which is what lets every one of the four
/// answers be tested without a sealed file on disk.
#[must_use]
pub fn resolve_account(
    assignments: &[(ProviderId, AccountId)],
    provider: &ProviderId,
    held: &[CredentialRecord],
) -> AccountResolution {
    let _ = (assignments, provider, held);
    todo!("(#keyring 5/9): one assignment for the provider, matched against what this host holds")
}
