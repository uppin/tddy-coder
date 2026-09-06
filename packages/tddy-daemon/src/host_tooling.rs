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
pub fn classify_gh_auth_status(exit_code: Option<i32>, output: &str) -> GithubCliStatus {
    // No exit code at all means the binary never ran, which is the one thing `gh` not being
    // installed looks like from here.
    if exit_code.is_none() {
        return GithubCliStatus {
            outcome: ProbeOutcome::Ok,
            installed: false,
            authenticated: false,
            login: None,
        };
    }

    // Look for a login *before* looking for a logged-out phrase. `gh auth status` reports every
    // host it knows, so output can carry both, and the honest reading of "logged in to one host,
    // out of another" is authenticated — the reverse would send an operator to re-authenticate a
    // host that already is.
    if let Some(login) = parse_gh_login(output) {
        return GithubCliStatus {
            outcome: ProbeOutcome::Ok,
            installed: true,
            authenticated: true,
            login: Some(login.to_string()),
        };
    }

    // Covers both wordings gh uses: "You are not logged into any GitHub hosts" and the per-host
    // "Not logged in to github.com" — the second phrase contains the first.
    if output.to_ascii_lowercase().contains("not logged in") {
        return GithubCliStatus {
            outcome: ProbeOutcome::Ok,
            installed: true,
            authenticated: false,
            login: None,
        };
    }

    // Deliberately the default. Anything this parse does not recognise is a probe that failed,
    // never a negative finding — see the module docs.
    GithubCliStatus {
        outcome: ProbeOutcome::Failed(format!(
            "gh auth status produced output this daemon does not recognise: {}",
            summarise(output)
        )),
        installed: true,
        authenticated: false,
        login: None,
    }
}

/// The login out of `✓ Logged in to github.com account octocat (keyring)`.
///
/// Both anchors have to be present on one line: "Logged in to" alone appears in `gh`'s own
/// suggestion text, and matching it there would invent a login out of a word that follows.
fn parse_gh_login(output: &str) -> Option<&str> {
    output.lines().find_map(|line| {
        let after_host = line.split_once("Logged in to ")?.1;
        let after_account = after_host.split_once(" account ")?.1;
        after_account.split_whitespace().next()
    })
}

/// A one-line excerpt of unrecognised output, for an operator reading a failure reason.
fn summarise(output: &str) -> String {
    const MAX: usize = 200;
    let trimmed = output.trim();
    match trimmed.char_indices().nth(MAX) {
        Some((cut, _)) => format!("{}…", &trimmed[..cut]),
        None => trimmed.to_string(),
    }
}

/// Parse `git config --get user.name` / `user.email` output into an identity.
///
/// A half-configured host — one half set, the other empty — reports `None`, the same as one with
/// neither. An identity is the pair: git refuses to commit without both, so a host missing either
/// has no identity its commits would carry, and rendering the half it does have beside a blank
/// would state a fact that is not true. Which half is missing is a detail of the fix, not of the
/// finding.
pub fn parse_git_identity(name_output: &str, email_output: &str) -> GitIdentity {
    let name = name_output.trim();
    let email = email_output.trim();
    GitIdentity {
        outcome: ProbeOutcome::Ok,
        name_and_email: (!name.is_empty() && !email.is_empty())
            .then(|| (name.to_string(), email.to_string())),
    }
}

/// The live probe: runs `git` and `gh` as the host's OS user.
pub struct SubprocessHostToolingProbe;

impl HostToolingProbe for SubprocessHostToolingProbe {
    #[cfg(unix)]
    fn probe(&self, os_user: &str) -> HostTooling {
        // One deadline for the whole probe, with every command started before any is collected.
        // They are independent reads of the same account, so running them in series would make the
        // Hosts screen wait for the sum of them and let a slow `gh` — it can reach the network —
        // decide how long the git answer takes.
        let deadline = std::time::Instant::now() + PROBE_TIMEOUT;
        let name = start_probe(os_user, "git", &["config", "--get", "user.name"]);
        let email = start_probe(os_user, "git", &["config", "--get", "user.email"]);
        let github_cli = start_probe(os_user, "gh", &["auth", "status"]);

        HostTooling {
            git: git_identity_of(
                git_config_value(name.collect(deadline), "user.name"),
                git_config_value(email.collect(deadline), "user.email"),
            ),
            github_cli: github_cli_of(github_cli.collect(deadline)),
        }
    }

    /// `run_capture_as_user` is Unix-only, so there is nothing to run here. That is a property of
    /// the platform, not a failure of this host's tooling, and it says so rather than surfacing an
    /// internal error an operator would try to fix.
    #[cfg(not(unix))]
    fn probe(&self, _os_user: &str) -> HostTooling {
        HostTooling {
            git: GitIdentity {
                outcome: ProbeOutcome::Unsupported,
                name_and_email: None,
            },
            github_cli: GithubCliStatus {
                outcome: ProbeOutcome::Unsupported,
                installed: false,
                authenticated: false,
                login: None,
            },
        }
    }
}

/// Both halves of the identity, or the first reason one of them could not be read.
///
/// A half that could not be read is not a half that is unset: it makes the whole identity unknown,
/// because the pair that could not be assembled might well have been complete.
#[cfg(unix)]
fn git_identity_of(name: Result<String, String>, email: Result<String, String>) -> GitIdentity {
    match (name, email) {
        (Ok(name), Ok(email)) => parse_git_identity(&name, &email),
        (Err(reason), _) | (Ok(_), Err(reason)) => GitIdentity {
            outcome: ProbeOutcome::Failed(reason),
            name_and_email: None,
        },
    }
}

/// The value one `git config --get <key>` reported.
///
/// An unset key is an empty value, not an error: `git config --get` exits 1 for it, and treating
/// that as a failure would hide the very state this probe exists to report. Any other exit code is
/// a failure, because git only reaches those for a malformed config or an unreadable file — and a
/// `git` that cannot be started is one too, since a host whose git is missing has no identity we
/// can claim to have read.
#[cfg(unix)]
fn git_config_value(
    probed: Result<crate::spawner::CaptureAsUser, String>,
    key: &str,
) -> Result<String, String> {
    let run = probed?;
    let output = run
        .result
        .map_err(|e| format!("could not run {}: {e}", run.resolved_program.display()))?;
    match output.status.code() {
        Some(0) => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
        Some(1) => Ok(String::new()),
        Some(code) => Err(format!(
            "git config --get {key} exited with {code}: {}",
            summarise(&String::from_utf8_lossy(&output.stderr))
        )),
        None => Err(format!("git config --get {key} was killed by a signal")),
    }
}

/// The state `gh auth status` reported, or why it could not be asked.
#[cfg(unix)]
fn github_cli_of(probed: Result<crate::spawner::CaptureAsUser, String>) -> GithubCliStatus {
    let failed = |reason: String| GithubCliStatus {
        outcome: ProbeOutcome::Failed(reason),
        installed: false,
        authenticated: false,
        login: None,
    };
    let run = match probed {
        Ok(run) => run,
        Err(reason) => return failed(reason),
    };
    let output = match run.result {
        Ok(output) => output,
        // The one error that is a finding rather than a failure: no such program means `gh` is not
        // installed for this user. Anything else — a failed privilege drop, a permission error —
        // says nothing about whether gh is there.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return classify_gh_auth_status(None, "");
        }
        Err(e) => {
            return failed(format!(
                "could not run {}: {e}",
                run.resolved_program.display()
            ));
        }
    };
    // A signal-killed child has no exit code, and `None` means something else entirely to
    // `classify_gh_auth_status` — "the binary never ran", i.e. not installed. Keep the two apart.
    let Some(code) = output.status.code() else {
        return failed(format!(
            "gh auth status was killed by a signal: {}",
            output.status
        ));
    };
    // `gh auth status` reports on stderr, and has moved between the two streams across releases, so
    // classify what it wrote wherever it wrote it.
    let mut written = String::from_utf8_lossy(&output.stdout).into_owned();
    written.push_str(&String::from_utf8_lossy(&output.stderr));
    classify_gh_auth_status(Some(code), &written)
}

/// One probe command, running as `os_user` on a thread of its own.
#[cfg(unix)]
struct StartedProbe {
    /// The command line, for a failure reason an operator can act on.
    command: String,
    outcome: std::sync::mpsc::Receiver<anyhow::Result<crate::spawner::CaptureAsUser>>,
}

/// Start one probe command as `os_user` without waiting for it.
#[cfg(unix)]
fn start_probe(os_user: &str, program: &str, args: &[&str]) -> StartedProbe {
    let owned_user = os_user.to_string();
    let owned_program = std::path::PathBuf::from(program);
    let owned_args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let command = format!("{program} {}", args.join(" "));
    let (tx, outcome) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(crate::spawner::run_output_as_user(
            &owned_user,
            &owned_program,
            &owned_args,
        ));
    });
    StartedProbe { command, outcome }
}

#[cfg(unix)]
impl StartedProbe {
    /// Wait for this command until `deadline`, then give up on it.
    ///
    /// The bound is on waiting, not on the child: `spawner` runs a command to completion and hands
    /// back no handle to kill. An overrunning probe is therefore abandoned rather than killed — its
    /// thread finishes in its own time and its result is dropped — which is what the caller needs,
    /// since the Hosts screen probes every host it lists and one unreachable `gh` must not hold the
    /// RPC open.
    fn collect(
        self,
        deadline: std::time::Instant,
    ) -> Result<crate::spawner::CaptureAsUser, String> {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match self.outcome.recv_timeout(remaining) {
            Ok(Ok(run)) => Ok(run),
            Ok(Err(e)) => Err(format!("could not run {}: {e}", self.command)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(format!(
                "{} did not answer within {}s",
                self.command,
                PROBE_TIMEOUT.as_secs()
            )),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(format!(
                "the {} probe ended without reporting",
                self.command
            )),
        }
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
