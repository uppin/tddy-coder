//! What a host has installed and configured — as opposed to how busy it is.
//!
//! The first *capability probe* in tddy. [`crate::host_stats`] reports load; this reports the two
//! facts that decide whether work on a host will actually succeed: the git identity its commits
//! would carry, and whether the GitHub CLI there is authenticated.
//!
//! Both are **per-OS-user**: `git config --global` reads `$HOME/.gitconfig` and `gh auth status`
//! reads `$HOME/.config/gh/hosts.yml`. A probe run as the daemon's own user would report a
//! different machine's answer, so both go through [`tddy_daemon_kernel::spawn_as_user::start_output_as_user`].
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

mod desktop_probes;
mod readings;
pub(crate) mod subprocess;

use std::sync::Arc;
use std::time::Duration;

use crate::host_tooling::desktop_probes::desktop_readings_of;
use crate::host_tooling::desktop_probes::start_desktop_probes;
use crate::host_tooling::desktop_probes::warn_if_a_desktop_probe_failed;
#[cfg(unix)]
use crate::host_tooling::readings::agent_status_of;
#[cfg(unix)]
use crate::host_tooling::readings::git_config_value;
#[cfg(unix)]
use crate::host_tooling::readings::git_identity_of;
#[cfg(unix)]
use crate::host_tooling::readings::github_cli_of;
#[cfg(unix)]
use crate::host_tooling::readings::not_on_path;
#[cfg(unix)]
use crate::host_tooling::readings::start_agent_probe;
#[cfg(unix)]
use crate::host_tooling::readings::warn_if_failed;
#[cfg(unix)]
use crate::host_tooling::subprocess::start_probe;
use crate::remote_desktop_probe::{DesktopProtocol, DesktopReachability, RemoteDesktopProbe};

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
    /// Added by `#hosts-screen 5/8`. `HostToolingProbe` owns the git and `gh` halves; this block is
    /// filled from [`crate::ssh_agent`].
    pub ssh_agent: crate::ssh_agent::AgentStatus,
    /// Added by `#hosts-screen 7/8`. One reading per probed protocol, filled from
    /// [`crate::remote_desktop_probe`].
    pub remote_desktop: Vec<crate::remote_desktop_probe::DesktopReachability>,
}

/// Probes a host for what it has installed and configured.
///
/// A trait so the RPC layer can be handed a deterministic double — the same shape as
/// [`crate::host_stats::HostStats`] and `with_host_stats`. Tests must never depend on what happens
/// to be installed on the machine running them.
pub trait HostToolingProbe: Send + Sync {
    /// Read both facts as `os_user`, bounded by [`PROBE_TIMEOUT`].
    ///
    /// `github_hostname` names the GitHub host `gh` is asked about (a Local gh project's); `None`
    /// asks about every host `gh` is logged in to.
    fn probe(&self, os_user: &str, github_hostname: Option<&str>) -> HostTooling;
}

/// The arguments `gh auth status` is run with: about `github_hostname` alone when one is named (a
/// project's GitHub host), about every host `gh` is logged in to otherwise.
pub fn gh_auth_status_args(github_hostname: Option<&str>) -> Vec<String> {
    let mut args = vec!["auth".to_string(), "status".to_string()];
    if let Some(hostname) = github_hostname {
        args.push("--hostname".to_string());
        args.push(hostname.to_string());
    }
    args
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
pub(crate) fn summarise(output: &str) -> String {
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

/// The protocols the desktop block reports on, each checked on its own default port.
///
/// Both are reported for every host, including the ones nothing answered for. "No desktop on
/// :5900" is a finding, and a block that listed only the protocols that answered could not be told
/// apart from one where nobody looked.
const PROBED_PROTOCOLS: [DesktopProtocol; 2] = [DesktopProtocol::Vnc, DesktopProtocol::Rdp];

/// The live probe: runs `git` and `gh` as the host's OS user, and checks this host's desktops.
///
/// The desktop half is held as a trait object rather than called directly, for the reason the RPC
/// layer holds [`HostToolingProbe`] as one: what a connect finds depends on what happens to be
/// listening on the machine running the suite, and a test may not depend on that.
pub struct SubprocessHostToolingProbe {
    remote_desktop: Arc<dyn RemoteDesktopProbe>,
    /// Where X11 displays are listed from and how x11vnc resolves; `None` lists no X11 channels.
    x11_channels: Option<tddy_daemon_kernel::config::ScreenSharingConfig>,
}

impl Default for SubprocessHostToolingProbe {
    /// The default configuration's bridge paths — what a daemon with no `screen_sharing:` block
    /// resolves. A daemon that has its configuration in hand uses [`Self::for_config`] instead.
    fn default() -> Self {
        Self::for_config(&tddy_daemon_kernel::config::DaemonConfig::default())
    }
}

impl SubprocessHostToolingProbe {
    /// Probe this host's desktops against the bridge binaries `config` would actually spawn, so an
    /// operator's explicit `screen_sharing` paths are the ones checked for existence.
    #[must_use]
    pub fn for_config(config: &tddy_daemon_kernel::config::DaemonConfig) -> Self {
        Self {
            remote_desktop: Arc::new(
                crate::remote_desktop_probe::TcpRemoteDesktopProbe::for_config(config),
            ),
            x11_channels: None,
        }
        .listing_x11_displays_for(config)
    }

    /// Fill the desktop block from `remote_desktop` instead of connecting for real.
    #[must_use]
    pub fn probing_desktops_with(remote_desktop: Arc<dyn RemoteDesktopProbe>) -> Self {
        Self {
            remote_desktop,
            x11_channels: None,
        }
    }

    /// Also report one channel per X11 display in `config`'s `screen_sharing.x11_socket_dir`
    /// ([`crate::x11_displays`]).
    #[must_use]
    pub fn listing_x11_displays_for(
        mut self,
        config: &tddy_daemon_kernel::config::DaemonConfig,
    ) -> Self {
        self.x11_channels = Some(config.screen_sharing.clone().unwrap_or_default());
        self
    }

    /// The endpoint readings, with the X11 channels appended when this probe lists them.
    fn with_x11_channels(&self, endpoints: Vec<DesktopReachability>) -> Vec<DesktopReachability> {
        match &self.x11_channels {
            None => endpoints,
            Some(screen_sharing) => crate::x11_displays::with_x11_channels(
                endpoints,
                screen_sharing,
                &self.remote_desktop,
            ),
        }
    }
}

impl HostToolingProbe for SubprocessHostToolingProbe {
    #[cfg(unix)]
    fn probe(&self, os_user: &str, github_hostname: Option<&str>) -> HostTooling {
        // Resolve both programs to absolute paths *before* spawning anything.
        // `start_output_as_user` deliberately anchors a relative program to the daemon's own
        // toolchain root and never consults `PATH` (see `spawner::resolve_tool_path`), so a bare
        // "git" would exec `<daemon-cwd>/git` and every host would report a probe failure.
        let git_program =
            tddy_daemon_kernel::spawn_as_user::find_program_on_spawn_child_path("git");
        let gh_program = tddy_daemon_kernel::spawn_as_user::find_program_on_spawn_child_path("gh");

        // One deadline for the whole probe, with every command started before any is collected.
        // They are independent reads of the same account, so running them in series would make the
        // Hosts screen wait for the sum of them and let a slow `gh` — it can reach the network —
        // decide how long the git answer takes.
        let deadline = std::time::Instant::now() + PROBE_TIMEOUT;
        // Started before the others for the same reason they are started together: the agent probe
        // carries its own, shorter bound, and running it last would add that bound to whatever `gh`
        // took.
        let ssh_agent = start_agent_probe(os_user);
        // Started with the others for the same reason again: each connect carries its own
        // `CONNECT_TIMEOUT`, and a block collected by connecting to one port and then the next
        // would put both of those bounds after whatever `gh` took — on a screen that probes every
        // host it lists, on every poll.
        let remote_desktop = start_desktop_probes(&self.remote_desktop);
        let name = git_program
            .as_deref()
            .map(|git| start_probe(os_user, git, &["config", "--global", "--get", "user.name"]));
        let email = git_program
            .as_deref()
            .map(|git| start_probe(os_user, git, &["config", "--global", "--get", "user.email"]));
        let gh_args = gh_auth_status_args(github_hostname);
        let github_cli = gh_program.as_deref().map(|gh| {
            let gh_args: Vec<&str> = gh_args.iter().map(String::as_str).collect();
            start_probe(os_user, gh, &gh_args)
        });

        // Collected before the two commands for the reason it was started before them: a struct
        // literal evaluates its fields in order, and a probe still in flight when it is collected is
        // judged on whatever is left of the shared deadline — so collecting it after `gh`, which can
        // reach the network, would fail it for time `gh` had spent.
        let ssh_agent = agent_status_of(ssh_agent, deadline);
        // Collected here for the same reason, and not as the last field of the literal: two bounded
        // TCP connects judged on whatever `git` and `gh` left of the deadline would report a host as
        // unprobeable for time they spent, which is the one thing this block must never claim.
        let remote_desktop = self.with_x11_channels(desktop_readings_of(remote_desktop, deadline));

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
            ssh_agent,
            remote_desktop,
        };

        warn_if_failed("git identity", os_user, &tooling.git.outcome);
        warn_if_failed("gh", os_user, &tooling.github_cli.outcome);
        warn_if_failed("ssh-agent", os_user, &tooling.ssh_agent.outcome);
        warn_if_a_desktop_probe_failed(&tooling.remote_desktop);
        tooling
    }

    /// `start_output_as_user` is Unix-only, so there is nothing to run here. That is a property of
    /// the platform, not a failure of this host's tooling, and it says so rather than surfacing an
    /// internal error an operator would try to fix.
    #[cfg(not(unix))]
    fn probe(&self, _os_user: &str, _github_hostname: Option<&str>) -> HostTooling {
        // The desktop block is the one thing here that is *not* unsupported off Unix, so it is
        // probed rather than declared away. Nothing it does needs `start_output_as_user`: it is a
        // TCP connect to this machine's own loopback plus an existence check on a path, and both
        // answer on every platform. Reporting `Unsupported` for a probe that would have answered
        // would be as much of a fabricated fact as reporting a finding it never made.
        let remote_desktop = start_desktop_probes(&self.remote_desktop);
        let deadline = std::time::Instant::now() + PROBE_TIMEOUT;
        let tooling = HostTooling {
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
            ssh_agent: crate::ssh_agent::AgentStatus::unsupported(),
            remote_desktop: self.with_x11_channels(desktop_readings_of(remote_desktop, deadline)),
        };

        warn_if_a_desktop_probe_failed(&tooling.remote_desktop);
        tooling
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::host_tooling::readings::github_cli_of;
    #[cfg(unix)]
    use crate::host_tooling::subprocess::start_probe;
    #[cfg(unix)]
    use crate::host_tooling::subprocess::StartedProbe;

    #[test]
    fn the_gh_probe_for_a_named_host_asks_gh_about_that_host() {
        // Given a Local gh project on an Enterprise host
        // When the probe's gh arguments are built for it
        let args = gh_auth_status_args(Some("github.example.com"));

        // Then gh is asked about that host only
        assert_eq!(args, ["auth", "status", "--hostname", "github.example.com"]);
    }

    #[test]
    fn the_gh_probe_with_no_host_named_asks_gh_about_every_host() {
        // Given the Hosts screen, which names no GitHub host
        // When the probe's gh arguments are built
        let args = gh_auth_status_args(None);

        // Then gh is asked about every host it is logged in to, as before
        assert_eq!(args, ["auth", "status"]);
    }

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
        tddy_daemon_kernel::spawn_as_user::find_program_on_spawn_child_path(name)
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

    /// A desktop probe that answers from a script instead of connecting to anything.
    ///
    /// Deterministic by construction: a real connect answers about whatever happens to be listening
    /// on the machine running the suite, and a real bridge check about whatever happens to be
    /// installed beside the test binary. Neither is something a test may assert on.
    #[cfg(unix)]
    struct ScriptedDesktopProbe {
        readings: Vec<DesktopReachability>,
    }

    #[cfg(unix)]
    impl RemoteDesktopProbe for ScriptedDesktopProbe {
        fn probe(&self, protocol: DesktopProtocol, port: u16) -> DesktopReachability {
            self.readings
                .iter()
                .find(|reading| reading.protocol == protocol)
                .cloned()
                .unwrap_or_else(|| panic!("nothing scripted for {protocol:?} on port {port}"))
        }
    }

    /// A probe that answers `readings` for the protocols it is asked about.
    #[cfg(unix)]
    fn a_desktop_probe_reporting(
        readings: Vec<DesktopReachability>,
    ) -> Arc<dyn RemoteDesktopProbe> {
        Arc::new(ScriptedDesktopProbe { readings })
    }

    /// A host serving a desktop this daemon has no bridge binary for.
    #[cfg(unix)]
    fn serving_with_no_bridge(protocol: DesktopProtocol) -> DesktopReachability {
        DesktopReachability {
            outcome: ProbeOutcome::Ok,
            channel: crate::remote_desktop_probe::DesktopChannel::Endpoint,
            protocol,
            can_bridge: false,
            desktop_reachable: true,
            port: protocol.default_port(),
        }
    }

    /// A host this daemon could bridge, with nothing serving a desktop on it.
    #[cfg(unix)]
    fn bridgeable_with_no_desktop(protocol: DesktopProtocol) -> DesktopReachability {
        DesktopReachability {
            outcome: ProbeOutcome::Ok,
            channel: crate::remote_desktop_probe::DesktopChannel::Endpoint,
            protocol,
            can_bridge: true,
            desktop_reachable: false,
            port: protocol.default_port(),
        }
    }

    /// The reading the block carries for `protocol`.
    #[cfg(unix)]
    fn the_reading_for(tooling: &HostTooling, protocol: DesktopProtocol) -> DesktopReachability {
        tooling
            .remote_desktop
            .iter()
            .find(|reading| reading.protocol == protocol)
            .cloned()
            .unwrap_or_else(|| panic!("the block reports nothing for {protocol:?}"))
    }

    /// Assert that a block was answered by a probe that actually ran here.
    ///
    /// [`ProbeOutcome::Unsupported`] is the one answer a Unix host can never honestly give for these
    /// three: it means the platform cannot run the probe at all, and this one can. It is what the
    /// `#[cfg(not(unix))]` arm reports, which is exactly the arm the desktop block also had to be
    /// added to — so it is the shape a careless edit leaves behind.
    #[cfg(unix)]
    fn assert_was_probed_on_this_host(subject: &str, outcome: &ProbeOutcome) {
        assert_ne!(
            *outcome,
            ProbeOutcome::Unsupported,
            "the {subject} probe runs on this platform, so it must never report it as unsupported"
        );
    }

    /// AC-4 and AC-7, the pair this whole node exists for. "tddy cannot bridge here" and "nothing is
    /// serving a desktop here" have unrelated fixes — install the bridge, versus start a server — so
    /// the block reports them as two facts. A host with each one and not the other is what proves
    /// they never collapsed into one.
    #[cfg(unix)]
    #[test]
    fn host_tooling_reports_bridge_availability_separately_from_reachability() {
        // Given — VNC: a desktop is up, and this daemon has no bridge for it.
        //         RDP: the bridge is there, and nothing is serving.
        let probe =
            SubprocessHostToolingProbe::probing_desktops_with(a_desktop_probe_reporting(vec![
                serving_with_no_bridge(DesktopProtocol::Vnc),
                bridgeable_with_no_desktop(DesktopProtocol::Rdp),
            ]));

        // When
        let tooling = probe.probe(&the_current_os_user(), None);

        // Then — each protocol reports both facts, and the two disagree in opposite directions
        let vnc = the_reading_for(&tooling, DesktopProtocol::Vnc);
        assert_eq!(vnc.outcome, ProbeOutcome::Ok);
        assert!(vnc.desktop_reachable, "a desktop is serving over VNC");
        assert!(
            !vnc.can_bridge,
            "and this daemon still cannot bridge it — the fix is to install the bridge, not to \
             start a server"
        );
        assert_eq!(vnc.port, DesktopProtocol::Vnc.default_port());

        let rdp = the_reading_for(&tooling, DesktopProtocol::Rdp);
        assert_eq!(rdp.outcome, ProbeOutcome::Ok);
        assert!(rdp.can_bridge, "this daemon has the RDP bridge");
        assert!(
            !rdp.desktop_reachable,
            "and nothing is serving over RDP — the fix is to start a server, not to install a \
             bridge"
        );
        assert_eq!(rdp.port, DesktopProtocol::Rdp.default_port());
    }

    /// The desktop block arrives **beside** nodes 4's and 5's, not instead of them.
    ///
    /// Adding it meant editing both arms of `probe`, including the one that answers
    /// `Unsupported` for everything because the platform cannot spawn as another user. What each of
    /// the three blocks *finds* depends on the machine running the suite, so what this pins is the
    /// part that does not: on a Unix host all three are probes that can run, and every one of them
    /// still reports an answer only a probe that ran can give.
    #[cfg(unix)]
    #[test]
    fn host_tooling_still_reports_git_gh_and_the_ssh_agent_alongside_the_desktop_block() {
        // Given
        let probe =
            SubprocessHostToolingProbe::probing_desktops_with(a_desktop_probe_reporting(vec![
                serving_with_no_bridge(DesktopProtocol::Vnc),
                bridgeable_with_no_desktop(DesktopProtocol::Rdp),
            ]));

        // When
        let tooling = probe.probe(&the_current_os_user(), None);

        // Then — the desktop block reports one reading per probed protocol ...
        assert_eq!(
            tooling
                .remote_desktop
                .iter()
                .map(|reading| reading.protocol)
                .collect::<Vec<_>>(),
            vec![DesktopProtocol::Vnc, DesktopProtocol::Rdp],
        );
        // ... and the three blocks beside it were still answered by probes that ran here
        assert_was_probed_on_this_host("git identity", &tooling.git.outcome);
        assert_was_probed_on_this_host("gh", &tooling.github_cli.outcome);
        assert_was_probed_on_this_host("ssh-agent", &tooling.ssh_agent.outcome);
    }

    /// An endpoint probe that answers by port: a VNC server is listening on exactly `serving`, and
    /// on nothing else. The bridge is always there, so only reachability varies.
    #[cfg(unix)]
    struct VncServedOnPorts {
        serving: Vec<u16>,
    }

    #[cfg(unix)]
    impl RemoteDesktopProbe for VncServedOnPorts {
        fn probe(&self, protocol: DesktopProtocol, port: u16) -> DesktopReachability {
            DesktopReachability {
                outcome: ProbeOutcome::Ok,
                channel: crate::remote_desktop_probe::DesktopChannel::Endpoint,
                protocol,
                can_bridge: true,
                desktop_reachable: protocol == DesktopProtocol::Vnc && self.serving.contains(&port),
                port,
            }
        }
    }

    #[cfg(unix)]
    fn a_vnc_server_listening_on(ports: &[u16]) -> Arc<dyn RemoteDesktopProbe> {
        Arc::new(VncServedOnPorts {
            serving: ports.to_vec(),
        })
    }

    /// A daemon whose `screen_sharing:` block lists displays from `socket_dir` and resolves
    /// x11vnc to `x11vnc` — a path that may or may not exist, as the test arranges.
    #[cfg(unix)]
    fn a_config_listing_x11_displays_in(
        socket_dir: &std::path::Path,
        x11vnc: &std::path::Path,
    ) -> tddy_daemon_kernel::config::DaemonConfig {
        tddy_daemon_kernel::config::DaemonConfig {
            screen_sharing: Some(tddy_daemon_kernel::config::ScreenSharingConfig {
                x11_socket_dir: socket_dir.display().to_string(),
                x11vnc_binary_path: x11vnc.display().to_string(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// The X11 channel readings in a block, in order.
    #[cfg(unix)]
    fn the_x11_channels_in(tooling: &HostTooling) -> Vec<DesktopReachability> {
        tooling
            .remote_desktop
            .iter()
            .filter(|reading| {
                matches!(
                    reading.channel,
                    crate::remote_desktop_probe::DesktopChannel::X11Display(_)
                )
            })
            .cloned()
            .collect()
    }

    /// Display `:0` is already served by the VNC server on 5900, which the endpoint reading
    /// reports. Listing it again as an X11 channel would offer the operator one desktop twice.
    #[cfg(unix)]
    #[test]
    fn does_not_list_a_display_already_served_on_its_vnc_port_twice() {
        // Given
        let sockets = crate::test_util::an_x11_socket_dir()
            .with_a_running_display(0)
            .with_a_running_display(1);
        let probe =
            SubprocessHostToolingProbe::probing_desktops_with(a_vnc_server_listening_on(&[5900]))
                .listing_x11_displays_for(&a_config_listing_x11_displays_in(
                    sockets.path(),
                    std::path::Path::new("/nonexistent/x11vnc"),
                ));

        // When
        let tooling = probe.probe(&the_current_os_user(), None);

        // Then
        assert_eq!(
            the_x11_channels_in(&tooling)
                .iter()
                .map(|reading| reading.channel)
                .collect::<Vec<_>>(),
            vec![crate::remote_desktop_probe::DesktopChannel::X11Display(1)],
        );
    }

    /// A display is there and the VNC bridge is there, but nothing can serve the display to the
    /// bridge without x11vnc. The channel is still listed — the operator learns what to install —
    /// and reports that it cannot be bridged.
    #[cfg(unix)]
    #[test]
    fn reports_x11_channels_as_unbridgeable_without_x11vnc() {
        // Given
        let sockets = crate::test_util::an_x11_socket_dir().with_a_running_display(1);
        let missing_x11vnc = sockets.path().join("x11vnc-is-not-installed");
        let probe =
            SubprocessHostToolingProbe::probing_desktops_with(a_vnc_server_listening_on(&[]))
                .listing_x11_displays_for(&a_config_listing_x11_displays_in(
                    sockets.path(),
                    &missing_x11vnc,
                ));

        // When
        let tooling = probe.probe(&the_current_os_user(), None);

        // Then
        let channels: Vec<_> = the_x11_channels_in(&tooling)
            .into_iter()
            .map(|reading| {
                (
                    reading.channel,
                    reading.desktop_reachable,
                    reading.can_bridge,
                )
            })
            .collect();
        assert_eq!(
            channels,
            vec![(
                crate::remote_desktop_probe::DesktopChannel::X11Display(1),
                true,
                false
            )],
            "(channel, desktop_reachable, can_bridge): the display answered but has no x11vnc"
        );
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
