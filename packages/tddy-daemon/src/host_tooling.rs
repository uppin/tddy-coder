//! What a host has installed and configured — as opposed to how busy it is.
//!
//! The first *capability probe* in tddy. [`crate::host_stats`] reports load; this reports the two
//! facts that decide whether work on a host will actually succeed: the git identity its commits
//! would carry, and whether the GitHub CLI there is authenticated.
//!
//! Both are **per-OS-user**: `git config --global` reads `$HOME/.gitconfig` and `gh auth status`
//! reads `$HOME/.config/gh/hosts.yml`. A probe run as the daemon's own user would report a
//! different machine's answer, so both go through [`crate::spawner::start_output_as_user`].
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
    /// The platform cannot run it at all — `start_output_as_user` is Unix-only.
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
///
/// `exit_code` is `None` for the one case where there was no `gh` to run at all — the caller
/// looked for it on the host's `PATH` and found nothing. That is positive evidence of absence, and
/// the only thing this function is allowed to read as "not installed".
pub fn classify_gh_auth_status(exit_code: Option<i32>, output: &str) -> GithubCliStatus {
    // Nothing ran, because the caller found no `gh` to run.
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

/// Parse `git config --global --get user.name` / `user.email` output into an identity.
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
        // Resolve both programs to absolute paths *before* spawning anything.
        // `start_output_as_user` deliberately anchors a relative program to the daemon's own
        // toolchain root and never consults `PATH` (see `spawner::resolve_tool_path`), so a bare
        // "git" would exec `<daemon-cwd>/git` and every host would report a probe failure.
        let git_program = crate::spawner::find_program_on_spawn_child_path("git");
        let gh_program = crate::spawner::find_program_on_spawn_child_path("gh");

        // One deadline for the whole probe, with every command started before any is collected.
        // They are independent reads of the same account, so running them in series would make the
        // Hosts screen wait for the sum of them and let a slow `gh` — it can reach the network —
        // decide how long the git answer takes.
        let deadline = std::time::Instant::now() + PROBE_TIMEOUT;
        let name = git_program
            .as_deref()
            .map(|git| start_probe(os_user, git, &["config", "--global", "--get", "user.name"]));
        let email = git_program
            .as_deref()
            .map(|git| start_probe(os_user, git, &["config", "--global", "--get", "user.email"]));
        let github_cli = gh_program
            .as_deref()
            .map(|gh| start_probe(os_user, gh, &["auth", "status"]));

        let tooling = HostTooling {
            git: match (name, email) {
                (Some(name), Some(email)) => git_identity_of(
                    git_config_value(name.collect(deadline), "user.name"),
                    git_config_value(email.collect(deadline), "user.email"),
                ),
                // A `git` that is not on `PATH` is a probe that could not run — never "no
                // identity configured". The host's `~/.gitconfig` may well hold one; nothing here
                // read it. Reporting the absent tool as a negative finding would send an operator
                // to set an identity that is already set.
                _ => GitIdentity {
                    outcome: ProbeOutcome::Failed(not_on_path("git")),
                    name_and_email: None,
                },
            },
            github_cli: match github_cli {
                Some(started) => github_cli_of(started.collect(deadline)),
                // `gh` absent from that same `PATH` *is* the finding "not installed" — and it is
                // the only evidence this code accepts for it, because it is the only one that
                // distinguishes an absent binary from a spawn that failed for another reason.
                None => classify_gh_auth_status(None, ""),
            },
        };

        warn_if_failed("git identity", os_user, &tooling.git.outcome);
        warn_if_failed("gh", os_user, &tooling.github_cli.outcome);
        tooling
    }

    /// `start_output_as_user` is Unix-only, so there is nothing to run here. That is a property of
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

/// Why a probe could not run when its program is absent from the `PATH` its child would get.
#[cfg(unix)]
fn not_on_path(program: &str) -> String {
    format!("`{program}` is not on the PATH this daemon gives a spawned child")
}

/// Record a probe that could not answer.
///
/// `Failed` is the one outcome whose cause lives on this side of the wire — a missing tool, a
/// refused spawn, a timeout — and without a line in the daemon log the only trace of it is a cell
/// on a screen nobody may be looking at.
#[cfg(unix)]
fn warn_if_failed(subject: &str, os_user: &str, outcome: &ProbeOutcome) {
    if let ProbeOutcome::Failed(reason) = outcome {
        log::warn!(
            target: "tddy_daemon::host_tooling",
            "the {subject} probe for OS user `{os_user}` reported nothing: {reason}"
        );
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

/// The value one `git config --global --get <key>` reported.
///
/// `--global` is load-bearing. Without it git reads system + global + **repo-local** config for the
/// process's cwd, and the probe chdirs into the target user's home — which is itself a
/// work tree whenever that user keeps their dotfiles in git. A repo-local `user.name` there would
/// shadow the global one, and the row would report an identity that commits made anywhere else on
/// the host would not carry.
///
/// An unset key is an empty value, not an error: `git config --get` exits 1 for it, and treating
/// that as a failure would hide the very state this probe exists to report. Any other exit code is
/// a failure, because git only reaches those for a malformed config or an unreadable file — and a
/// `git` that cannot be started is one too, since a host whose git is missing has no identity we
/// can claim to have read.
#[cfg(unix)]
fn git_config_value(probed: Result<ProbeOutput, String>, key: &str) -> Result<String, String> {
    let output = probed?;
    match output.status.code() {
        Some(0) => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
        Some(1) => Ok(String::new()),
        Some(code) => Err(format!(
            "git config --global --get {key} exited with {code}: {}",
            summarise(&String::from_utf8_lossy(&output.stderr))
        )),
        None => Err(format!(
            "git config --global --get {key} was killed by a signal"
        )),
    }
}

/// The state `gh auth status` reported, or why it could not be asked.
#[cfg(unix)]
fn github_cli_of(probed: Result<ProbeOutput, String>) -> GithubCliStatus {
    let failed = |reason: String| GithubCliStatus {
        outcome: ProbeOutcome::Failed(reason),
        installed: false,
        authenticated: false,
        login: None,
    };
    // Every `Err` here is a failure, never a finding — a spawn that never started, and a child this
    // probe killed for overrunning its deadline, alike. `NotFound` in particular: by this point `gh`
    // has already been *found* on the host's PATH, and a spawn also reports `NotFound` for a working
    // directory that does not exist (the probe chdirs into the target user's home) and for an exec
    // that could not be set up. Reading any of those as "gh is not installed" would state a fact
    // this code has evidence against. Absence is decided by the PATH lookup in `probe`, which is the
    // only thing that can see it.
    let output = match probed {
        Ok(output) => output,
        Err(reason) => return failed(reason),
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

/// What one probe command produced: how it exited, and everything it wrote.
#[cfg(unix)]
struct ProbeOutput {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// The bytes a probe command wrote, once both of its pipes reached EOF.
#[cfg(unix)]
struct Written {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// One probe command, started as `os_user`, with its output being read on a thread of its own.
#[cfg(unix)]
struct StartedProbe {
    /// The command line, for a failure reason an operator can act on.
    command: String,
    /// `Err` when the command never started: there is nothing to wait for, and nothing to kill.
    running: Result<RunningProbe, String>,
}

/// A probe command that is running right now, and the two handles onto it.
#[cfg(unix)]
struct RunningProbe {
    /// The live child, held by the **parent** rather than by the reader thread. The parent is the
    /// side that owns the deadline, so it has to be the side that can end the child — and it ends
    /// it through the `Child` itself, never through a pid saved off to one side, because a pid
    /// outlives the process it named and the next signal sent to it lands on whatever the kernel
    /// has since reused it for.
    child: std::process::Child,
    /// Everything the child wrote. One message, sent once both pipes are at EOF.
    written: std::sync::mpsc::Receiver<Written>,
}

/// Start one probe command as `os_user` without waiting for it.
///
/// `program` is an **absolute** path — see `probe`: `start_output_as_user` would resolve a relative
/// one against the daemon's own cwd rather than searching `PATH`.
///
/// Starting is not waiting: this returns as soon as the child exists, so `probe` can start all
/// three commands before collecting any of them and spend one deadline on the lot.
#[cfg(unix)]
fn start_probe(os_user: &str, program: &std::path::Path, args: &[&str]) -> StartedProbe {
    let owned_args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let command = format!("{} {}", program.display(), args.join(" "));
    StartedProbe {
        running: start_running(os_user, program, &owned_args)
            .map_err(|e| format!("could not run {command}: {e}")),
        command,
    }
}

/// Spawn the child and put a reader thread on its pipes.
#[cfg(unix)]
fn start_running(
    os_user: &str,
    program: &std::path::Path,
    args: &[String],
) -> anyhow::Result<RunningProbe> {
    let spawned = crate::spawner::start_output_as_user(os_user, program, args)?;
    Ok(RunningProbe {
        written: read_both_pipes(spawned.stdout, spawned.stderr),
        child: spawned.child,
    })
}

/// Read a child's two pipes to EOF on a thread, and report the bytes on a channel.
///
/// The thread owns **only** the pipes, never the `Child`. That is what makes the parent's kill safe
/// and complete: killing the child closes both pipes, both reads hit EOF, and this thread ends by
/// itself. A thread that instead waited on the child — which is what `Command::output` does inside
/// `run_output_as_user` — is the thread that used to be left parked forever on a timeout.
///
/// Both pipes are read concurrently, for the reason `Command::output` reads them concurrently: a
/// child that fills one pipe's buffer while this thread blocks on the other never drains, and would
/// hang until the deadline killed it — turning an answer that was ready into a timeout.
#[cfg(unix)]
fn read_both_pipes(
    stdout: std::process::ChildStdout,
    stderr: std::process::ChildStderr,
) -> std::sync::mpsc::Receiver<Written> {
    let (tx, written) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let draining_stderr = std::thread::spawn(move || read_to_end(stderr));
        let stdout = read_to_end(stdout);
        // `read_to_end` has nothing to panic on, so `Err` here is unreachable; an empty stderr in
        // its place classifies as unrecognised output, which is a probe failure — the same answer
        // this code gives for every other thing it could not read.
        let stderr = draining_stderr.join().unwrap_or_default();
        let _ = tx.send(Written { stdout, stderr });
    });
    written
}

/// Everything a pipe holds, up to EOF or the first read error.
///
/// A read that fails mid-way keeps what arrived before it rather than discarding it: the classifier
/// reads what the child wrote, and output it cannot recognise is already a probe failure.
#[cfg(unix)]
fn read_to_end(mut pipe: impl std::io::Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = pipe.read_to_end(&mut bytes);
    bytes
}

#[cfg(unix)]
impl StartedProbe {
    /// Wait for this command until `deadline`, then **end it**.
    ///
    /// The bound is on the child, not merely on this wait. A probe that only stopped waiting would
    /// leave the process running, its reader thread parked and its two pipe descriptors open, with
    /// no handle left to close any of them — and the Hosts screen probes every host it lists on
    /// every poll, so one unreachable host would leak all three per tick until the daemon ran out
    /// of descriptors.
    ///
    /// A command that ran out of time reports a failure, never a finding: nothing was read, so
    /// there is nothing this probe is entitled to say about the host.
    fn collect(self, deadline: std::time::Instant) -> Result<ProbeOutput, String> {
        let StartedProbe { command, running } = self;
        let running = running?;
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match running.written.recv_timeout(remaining) {
            Ok(written) => running.finish(written),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                running.end();
                Err(format!(
                    "{} did not answer within {}s",
                    command,
                    PROBE_TIMEOUT.as_secs()
                ))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                running.end();
                Err(format!("the {command} probe ended without reporting"))
            }
        }
    }
}

#[cfg(unix)]
impl RunningProbe {
    /// The exit status of a child that has already closed both pipes.
    ///
    /// This wait needs no bound of its own — a child at EOF on both streams is on its way out — but
    /// it is not optional: a child nobody waits for stays in the process table as a zombie.
    fn finish(mut self, written: Written) -> Result<ProbeOutput, String> {
        let status = self
            .child
            .wait()
            .map_err(|e| format!("could not read how the probe exited: {e}"))?;
        Ok(ProbeOutput {
            status,
            stdout: written.stdout,
            stderr: written.stderr,
        })
    }

    /// End a child that overran the deadline, and reap it.
    ///
    /// Both halves matter. Without the kill the process runs on; without the wait it lingers as a
    /// zombie, which is a process-table entry leaked just as surely.
    fn end(mut self) {
        if let Err(e) = self.child.kill() {
            log::warn!(
                target: "tddy_daemon::host_tooling",
                "could not end an overrunning probe: {e}"
            );
        }
        // Reaped whether or not the kill succeeded: `kill` reports an error for a child that has
        // already exited, and that child is exactly the one still waiting to be reaped.
        if let Err(e) = self.child.wait() {
            log::warn!(
                target: "tddy_daemon::host_tooling",
                "could not reap an overrunning probe: {e}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The OS user this test process already runs as — the one account it can start a child as
    /// without privilege, and the same one `run_capture_as_user`'s tests use.
    #[cfg(unix)]
    fn the_current_os_user() -> String {
        std::env::var("USER").expect("USER must be set to run this test")
    }

    /// A real program, found the way `probe` finds `git` and `gh`: on the `PATH` the daemon gives a
    /// spawned child, resolved to the absolute path `start_probe` requires.
    #[cfg(unix)]
    fn a_program_on_the_spawn_child_path(name: &str) -> std::path::PathBuf {
        crate::spawner::find_program_on_spawn_child_path(name)
            .unwrap_or_else(|| panic!("`{name}` must be on the PATH a spawned child is given"))
    }

    /// The pid of a probe that started — read before it is collected, since collecting consumes it.
    #[cfg(unix)]
    fn the_pid_of(started: &StartedProbe) -> u32 {
        started
            .running
            .as_ref()
            .unwrap_or_else(|reason| panic!("the probe must have started: {reason}"))
            .child
            .id()
    }

    /// Whether the kernel still knows `pid` as a process. Signal 0 asks exactly that and delivers
    /// nothing, and it answers for both halves of ending a child: a killed-but-unreaped child is
    /// still a zombie the process table holds, and only a reaped one is gone.
    #[cfg(unix)]
    fn a_process_still_exists(pid: u32) -> bool {
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }

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

    /// A command that outlives the deadline is a probe that failed — and the child it gave up on
    /// has to be **gone**. The Hosts screen probes every host it lists on every poll, so a timeout
    /// that only stopped waiting would leave a process, a thread and two pipe descriptors behind
    /// per tick, and one unreachable host would exhaust the daemon's descriptors.
    #[cfg(unix)]
    #[test]
    #[serial_test::serial]
    fn reports_a_probe_failure_when_the_lookup_exceeds_its_timeout() {
        // Given — a probe command that cannot answer in time, started as this machine's own user
        let sleep = a_program_on_the_spawn_child_path("sleep");
        let started = start_probe(&the_current_os_user(), &sleep, &["3600"]);
        let child_pid = the_pid_of(&started);

        // When — it is collected against a deadline a quarter of a second away
        let deadline = std::time::Instant::now() + Duration::from_millis(250);
        let status = github_cli_of(started.collect(deadline));

        // Then — the probe reports a failure, not the "not installed" finding that an absent
        // binary earns; nothing was read, so there is nothing to report about this host
        assert!(
            matches!(status.outcome, ProbeOutcome::Failed(_)),
            "a command that ran out of time is a failed probe, got {:?}",
            status.outcome
        );
        assert!(
            !status.authenticated,
            "a failed probe claims nothing about authentication"
        );
        // ... and the child it stopped waiting for was killed and reaped, not left running
        assert!(
            !a_process_still_exists(child_pid),
            "the overrunning child must be ended and reaped, not abandoned to run on"
        );
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
