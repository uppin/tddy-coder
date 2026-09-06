//! What a host has installed and configured — as opposed to how busy it is.
//!
//! The first *capability probe* in tddy. [`crate::host_stats`] reports load; this reports the two
//! facts that decide whether work on a host will actually succeed: the git identity its commits
//! would carry, and whether the GitHub CLI there is authenticated.
//!
//! Both are **per-OS-user**: `git config` reads `$HOME/.gitconfig` and `gh auth status` reads
//! `$HOME/.config/gh/hosts.yml`. A probe run as the daemon's own user would report a different
//! machine's answer, so both go through [`crate::spawner::run_capture_as_user`].
//!
//! # Absence is an answer
//!
//! Every outcome below is distinguishable on the wire, because collapsing any of them into an empty
//! string produces a fabricated fact an operator acts on:
//!
//! - git: configured (name + email) · ran and found none · could not run
//! - `gh`: not installed · installed but logged out · authenticated as `<login>` · could not run
//!
//! In particular, **output we do not recognise classifies as a failure, never as a negative**.
//! Reporting an authenticated host as logged out is the worse error: it sends an operator to fix
//! something that is not broken.

use std::time::Duration;

/// How long a single probe may take before it is abandoned.
///
/// `gh auth status` can reach the network, and an unreachable host must not hold the RPC open — the
/// Hosts screen probes every host it lists.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Whether a probe could run, before asking what it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    Ok,
    /// Could not run, or produced output this code does not understand.
    Failed(String),
    /// The platform cannot run it at all — `run_capture_as_user` is Unix-only.
    Unsupported,
}

/// The git identity commits made on a host would carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitIdentity {
    pub outcome: ProbeOutcome,
    /// `None` with `Ok` means the probe ran and found no identity configured — a real answer.
    pub name_and_email: Option<(String, String)>,
}

/// The state of the GitHub CLI on a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubCliStatus {
    pub outcome: ProbeOutcome,
    pub installed: bool,
    pub authenticated: bool,
    /// The login `gh` is authenticated as — the **host's**, not the tddy session's.
    pub login: Option<String>,
}

/// Everything the tooling probe reports for one host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostTooling {
    pub git: GitIdentity,
    pub github_cli: GithubCliStatus,
}

/// Probes a host for what it has installed and configured.
///
/// A trait so the RPC layer can be handed a deterministic double — the same shape as
/// [`crate::host_stats::HostStats`] and `with_host_stats`. Tests must never depend on what happens
/// to be installed on the machine running them.
pub trait HostToolingProbe: Send + Sync {
    /// Read both facts as `os_user`, bounded by [`PROBE_TIMEOUT`].
    fn probe(&self, os_user: &str) -> HostTooling;
}

/// Classify the output of `gh auth status`.
///
/// Split out from the subprocess call so the classification — the part that is easy to get wrong and
/// expensive to get wrong — is testable without running anything.
///
/// `gh auth status` writes to **stderr**, and its wording is not a stable API. The parse is therefore
/// deliberately narrow, and anything unrecognised is [`ProbeOutcome::Failed`].
pub fn classify_gh_auth_status(_exit_code: Option<i32>, _output: &str) -> GithubCliStatus {
    // TODO(host-identity): implement
    unimplemented!("host-identity: classify_gh_auth_status")
}

/// Parse `git config --get user.name` / `user.email` output into an identity.
pub fn parse_git_identity(_name_output: &str, _email_output: &str) -> GitIdentity {
    // TODO(host-identity): implement
    unimplemented!("host-identity: parse_git_identity")
}

/// The live probe: runs `git` and `gh` as the host's OS user.
pub struct SubprocessHostToolingProbe;

impl HostToolingProbe for SubprocessHostToolingProbe {
    fn probe(&self, _os_user: &str) -> HostTooling {
        // TODO(host-identity): implement
        unimplemented!("host-identity: probe")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A configured host reports both halves of the identity its commits would carry.
    #[test]
    fn reports_the_configured_git_user_name_and_email() {
        let identity = parse_git_identity("Ada Lovelace\n", "ada@example.com\n");

        assert_eq!(identity.outcome, ProbeOutcome::Ok);
        assert_eq!(
            identity.name_and_email,
            Some(("Ada Lovelace".to_string(), "ada@example.com".to_string())),
            "both halves are reported, trimmed of the trailing newline git emits"
        );
    }

    /// "No identity configured" is a real finding, and must not arrive as a pair of empty strings —
    /// a row rendering `""` as a name tells an operator nothing about why their commits are wrong.
    #[test]
    fn reports_that_no_git_identity_is_configured_rather_than_an_empty_name() {
        let identity = parse_git_identity("", "");

        assert_eq!(
            identity.outcome,
            ProbeOutcome::Ok,
            "the probe ran fine; it simply found nothing"
        );
        assert_eq!(
            identity.name_and_email, None,
            "an unconfigured host reports None, not Some((\"\", \"\"))"
        );
    }

    /// `gh` absent from PATH: the command cannot start at all.
    #[test]
    fn reports_that_the_github_cli_is_not_installed() {
        let status = classify_gh_auth_status(None, "");

        assert_eq!(status.outcome, ProbeOutcome::Ok);
        assert!(!status.installed, "gh is not installed");
        assert!(!status.authenticated);
    }

    /// Installed but logged out — `gh auth status` exits non-zero and says so on stderr.
    #[test]
    fn reports_the_github_cli_as_logged_out() {
        let status = classify_gh_auth_status(
            Some(1),
            "You are not logged into any GitHub hosts. To log in, run: gh auth login\n",
        );

        assert_eq!(status.outcome, ProbeOutcome::Ok);
        assert!(status.installed, "the binary ran, so it is installed");
        assert!(!status.authenticated);
        assert_eq!(status.login, None);
    }

    /// Authenticated — the login is what the row shows, labelled as the host's.
    #[test]
    fn reports_the_login_the_github_cli_is_authenticated_as() {
        let status = classify_gh_auth_status(
            Some(0),
            "github.com\n  ✓ Logged in to github.com account octocat (keyring)\n  \
             - Active account: true\n  - Token scopes: 'gist', 'read:org', 'repo'\n",
        );

        assert_eq!(status.outcome, ProbeOutcome::Ok);
        assert!(status.installed);
        assert!(status.authenticated);
        assert_eq!(status.login, Some("octocat".to_string()));
    }

    /// The load-bearing honesty case. `gh auth status` wording is not a stable API, so output this
    /// code does not understand must classify as a **failure**, never as "logged out" — telling an
    /// operator to re-authenticate a host that is already authenticated is the worse error, and it
    /// sends them to fix something that is not broken.
    #[test]
    fn classifies_unrecognised_gh_output_as_a_probe_failure_not_as_logged_out() {
        let status = classify_gh_auth_status(Some(0), "some future format nobody here has seen\n");

        assert!(
            matches!(status.outcome, ProbeOutcome::Failed(_)),
            "unrecognised output is a failure, got {:?}",
            status.outcome
        );
        assert!(
            !status.authenticated,
            "a failed probe claims nothing about authentication"
        );
    }
}
