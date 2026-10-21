use crate::host_tooling::classify_gh_auth_status;
use crate::host_tooling::parse_git_identity;
use crate::host_tooling::subprocess::ProbeOutput;
use crate::host_tooling::summarise;
use crate::host_tooling::GitIdentity;
use crate::host_tooling::GithubCliStatus;
use crate::host_tooling::ProbeOutcome;
use crate::host_tooling::PROBE_TIMEOUT;

/// Start the ssh-agent probe for `os_user` without waiting for it.
///
/// The agent conversation blocks on a socket — `ssh-agent-lib`'s client is a blocking one — so it
/// runs on a thread of its own like the two commands beside it, and like them it is abandoned
/// rather than killed if it overruns. This is the thread that client runs on; it starts none of its
/// own.
#[cfg(unix)]
pub(super) fn start_agent_probe(
    os_user: &str,
) -> std::sync::mpsc::Receiver<crate::ssh_agent::AgentStatus> {
    use crate::ssh_agent::SshAgentProbe;

    let owned_user = os_user.to_string();
    let (tx, outcome) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let probe =
            crate::ssh_agent::WireProtocolAgentProbe::new(crate::ssh_agent::WellKnownAgentSockets);
        let _ = tx.send(probe.identities(&owned_user));
    });
    outcome
}

/// Wait for the agent probe until `deadline`, then give up on it.
///
/// Its own worst case is *twice* [`crate::ssh_agent::AGENT_TIMEOUT`] — one read may begin just
/// inside its deadline and then take a full socket timeout of its own — so it is longer than
/// [`PROBE_TIMEOUT`] and this deadline can be the one that expires first. `probe` collects the agent
/// before the commands beside it, so reaching this deadline means the probe did have the whole of
/// [`PROBE_TIMEOUT`] and never came back — a failure, and never the empty key list an operator would
/// read as "this host has no keys loaded".
#[cfg(unix)]
pub(super) fn agent_status_of(
    started: std::sync::mpsc::Receiver<crate::ssh_agent::AgentStatus>,
    deadline: std::time::Instant,
) -> crate::ssh_agent::AgentStatus {
    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
    match started.recv_timeout(remaining) {
        Ok(status) => status,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            crate::ssh_agent::AgentStatus::failed(format!(
                "the ssh-agent probe did not finish within {}s",
                PROBE_TIMEOUT.as_secs()
            ))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            crate::ssh_agent::AgentStatus::failed("the ssh-agent probe ended without reporting")
        }
    }
}

/// Why a probe could not run when its program is absent from the `PATH` its child would get.
#[cfg(unix)]
pub(super) fn not_on_path(program: &str) -> String {
    format!("`{program}` is not on the PATH this daemon gives a spawned child")
}

/// Record a probe that could not answer.
///
/// `Failed` is the one outcome whose cause lives on this side of the wire — a missing tool, a
/// refused spawn, a timeout — and without a line in the daemon log the only trace of it is a cell
/// on a screen nobody may be looking at.
#[cfg(unix)]
pub(super) fn warn_if_failed(subject: &str, os_user: &str, outcome: &ProbeOutcome) {
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
pub(super) fn git_identity_of(
    name: Result<String, String>,
    email: Result<String, String>,
) -> GitIdentity {
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
pub(super) fn git_config_value(
    probed: Result<ProbeOutput, String>,
    key: &str,
) -> Result<String, String> {
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
pub(super) fn github_cli_of(probed: Result<ProbeOutput, String>) -> GithubCliStatus {
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
