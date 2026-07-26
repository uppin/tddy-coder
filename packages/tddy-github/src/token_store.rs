//! Retention of the GitHub access token a web login granted, keyed by GitHub login.
//!
//! The token is what lets the server act on the operator's own behalf against the GitHub API (read
//! PRs on a private repo; later, repoint and merge). It is deliberately kept **outside** the HMAC
//! session token: that token is handed to the browser over a plain-http LAN origin, so a live
//! `repo`-scoped credential must never travel in it or be returned to the client.

/// A per-login store for GitHub access tokens.
///
/// A failed `put` **fails the login** (see `AuthServiceImpl::exchange_code`, PRD D13): a session
/// minted without its token is a half-login — the operator appears signed in while every
/// GitHub-backed read reports itself unavailable, and re-authenticating, the one remedy, is the one
/// action they have no reason to attempt. So an implementation must return `Ok` only once the token
/// is durably retained, and must report every failure rather than absorbing it.
pub trait GitHubTokenStore: Send + Sync + 'static {
    /// Retain `access_token` for `login`, replacing any previous token for it.
    ///
    /// Returns `Ok` only when the token is durably retained — the caller turns an `Err` into a
    /// failed login. The error message may name server-side detail (paths, OS errors); it is for
    /// the server log, and the caller does not pass it to the client.
    fn put(&self, login: &str, access_token: &str) -> Result<(), String>;

    /// The token retained for `login`, or `None` when none is held.
    fn get(&self, login: &str) -> Option<String>;
}
