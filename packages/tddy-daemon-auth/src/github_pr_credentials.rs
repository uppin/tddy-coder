//! How a PR lookup resolves the calling operator's GitHub credential.
//!
//! Three outcomes, and the distinction between them is what the PR-Stack screen shows:
//!
//! - [`PrLookup::Empty`] — stub / demo authentication (`github.stub: true`). The product is demoed
//!   and tested without real GitHub credentials, so the lookup short-circuits to "no PRs": clean,
//!   successful, indistinguishable from a repository that genuinely has none.
//! - [`PrLookup::Unavailable`] — a real login whose credential cannot be used. Reported with an
//!   operator-facing reason, never as "no PR exists".
//! - [`PrLookup::Perform`] — a real login with a usable token.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md (C3, D7, D8, D12).

/// What a PR lookup should do for one caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrLookup {
    /// Report no PRs, successfully — a demo login has no credential by construction, and that must
    /// look exactly like a repository with no open PRs.
    Empty,
    /// Query GitHub with this access token.
    Perform(String),
    /// Report the PR status as unavailable, carrying an operator-facing reason.
    Unavailable(String),
}

/// Decide how to look up PRs for the caller: `stub_mode` is `github.stub` from `daemon.yaml`, and
/// `stored_token` is the access token retained for the caller's GitHub login at login time.
#[must_use]
pub fn pr_lookup_for_caller(stub_mode: bool, stored_token: Option<&str>) -> PrLookup {
    if stub_mode {
        // Demo authentication never reaches GitHub, whatever happens to be stored from an earlier
        // real login — and it is never an error, so a demo surfaces no error banner.
        return PrLookup::Empty;
    }
    match stored_token {
        Some(token) if !token.trim().is_empty() => PrLookup::Perform(token.to_string()),
        Some(_) => PrLookup::Unavailable(
            "the stored GitHub token for this login is blank — sign in to GitHub again".to_string(),
        ),
        None => PrLookup::Unavailable(
            "no GitHub token is stored for this login — sign in to GitHub again to grant repository \
             access"
                .to_string(),
        ),
    }
}
