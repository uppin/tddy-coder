use std::sync::Arc;

use tddy_rpc::{Request, Status};
use tddy_service::proto::host::*;

use crate::host_tooling::HostToolingProbe;
use crate::host_tooling::{GitIdentity, GithubCliStatus, HostTooling, ProbeOutcome};
use crate::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use crate::service::HostServiceImpl;
use std::sync::Mutex as StdMutex;
use tddy_service::proto::host::HostService;

use tddy_daemon_kernel::SessionUserResolver;

/// The daemon serving the RPC — the one a browser happens to be talking to.
const RELAY_HOST: &str = "workstation-1";
/// Another host in the same common room: the one an operator asks about.
const PROBED_HOST: &str = "server-2";
/// An empty `daemon_instance_id` is the protocol's spelling for "the daemon serving this call".
const THIS_DAEMON: &str = "";

/// The GitHub user a verifiable session token resolves to.
const SIGNED_IN_GITHUB_USER: &str = "octocat";
/// The OS user `SIGNED_IN_GITHUB_USER` maps to in this daemon's `users[]`.
const MAPPED_OS_USER: &str = "ada";
/// A different operator, mapped on the relay so that `octocat` demonstrably is not.
const OTHER_OPERATOR: &str = "monalisa";

const VERIFIABLE_TOKEN: &str = "valid";
const EXPIRED_TOKEN: &str = "expired";

/// A probe that answers with a fixed reading and records the OS user it was run as.
///
/// Deterministic by construction: nothing here consults the machine running the suite, so the
/// answer never depends on which `git` or `gh` happens to be installed on it.
struct RecordingHostToolingProbe {
    answer: HostTooling,
    probed_os_users: StdMutex<Vec<String>>,
}

impl HostToolingProbe for RecordingHostToolingProbe {
    fn probe(&self, os_user: &str) -> HostTooling {
        self.probed_os_users
            .lock()
            .expect("the probe recorder is never held across a panic")
            .push(os_user.to_string());
        self.answer.clone()
    }
}

impl RecordingHostToolingProbe {
    fn answering(answer: HostTooling) -> Arc<Self> {
        Arc::new(Self {
            answer,
            probed_os_users: StdMutex::new(Vec::new()),
        })
    }

    /// Every OS user this probe was run as, in call order.
    fn probed_os_users(&self) -> Vec<String> {
        self.probed_os_users
            .lock()
            .expect("the probe recorder is never held across a panic")
            .clone()
    }
}

/// One host's reading: a configured git identity, and a `gh` authenticated as some login.
fn a_host_committing_as(name: &str, email: &str, gh_login: &str) -> HostTooling {
    HostTooling {
        git: GitIdentity {
            outcome: ProbeOutcome::Ok,
            name_and_email: Some((name.to_string(), email.to_string())),
        },
        github_cli: GithubCliStatus {
            outcome: ProbeOutcome::Ok,
            installed: true,
            authenticated: true,
            login: Some(gh_login.to_string()),
        },
        // These tests are about the git and `gh` halves; no agent is reached, which is the
        // neutral value for the block `#hosts-screen 5/8` added to this struct.
        ssh_agent: crate::ssh_agent::AgentStatus::unreachable(),
        // Likewise for the block `#hosts-screen 7/8` added beside it: no protocol was probed,
        // so this host claims nothing either way about a desktop.
        remote_desktop: Vec::new(),
    }
}

/// The peers this daemon's common room can currently see.
struct FakeEligibleDaemons(Vec<String>);

impl EligibleDaemonSource for FakeEligibleDaemons {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        self.0
            .iter()
            .map(|instance_id| EligibleDaemonInfo {
                instance_id: DaemonInstanceId(instance_id.clone()),
                label: instance_id.clone(),
            })
            .collect()
    }
}

/// A daemon stated by the three things `GetHostTooling` consults: who its `users[]` maps, which
/// peers its common room can see, and what its tooling probe answers.
struct DaemonBuilder {
    mapped_github_user: String,
    mapped_os_user: String,
    peers: Vec<String>,
    probe: Arc<RecordingHostToolingProbe>,
}

/// A daemon named `workstation-1` that maps `octocat` to `ada` and sees no peers.
fn a_daemon_probing_with(probe: Arc<RecordingHostToolingProbe>) -> DaemonBuilder {
    DaemonBuilder {
        mapped_github_user: SIGNED_IN_GITHUB_USER.to_string(),
        mapped_os_user: MAPPED_OS_USER.to_string(),
        peers: Vec::new(),
        probe,
    }
}

impl DaemonBuilder {
    /// Replace the single `users[]` entry — naming someone other than the signed-in operator is
    /// how a test states that this daemon has no OS user for them.
    fn mapping(mut self, github_user: &str, os_user: &str) -> Self {
        self.mapped_github_user = github_user.to_string();
        self.mapped_os_user = os_user.to_string();
        self
    }

    fn seeing_peer(mut self, instance_id: &str) -> Self {
        self.peers.push(instance_id.to_string());
        self
    }

    fn build(self) -> HostServiceImpl {
        let temp = tempfile::tempdir().unwrap();
        let user_resolver: SessionUserResolver = Arc::new(|token| {
            (token == VERIFIABLE_TOKEN).then(|| SIGNED_IN_GITHUB_USER.to_string())
        });
        HostServiceImpl::new(
            a_config_named(RELAY_HOST, &self.mapped_github_user, &self.mapped_os_user),
            temp.path(),
            user_resolver,
        )
        .with_eligible_daemon_source(Arc::new(FakeEligibleDaemons(self.peers)))
        // The room slot is present but unconnected: this daemon is *configured* for a common
        // room, so a forward is attempted and fails on the transport rather than being refused
        // for a missing configuration.
        .with_common_room(Arc::new(tokio::sync::RwLock::new(None)))
        .with_host_tooling(self.probe)
    }
}

fn a_config_named(
    instance_id: &str,
    github_user: &str,
    os_user: &str,
) -> tddy_daemon_kernel::config::DaemonConfig {
    let yaml = format!(
        "daemon_instance_id: \"{instance_id}\"\nusers:\n  - github_user: \"{github_user}\"\n    os_user: \"{os_user}\"\n"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    tddy_daemon_kernel::config::DaemonConfig::load(&path).unwrap()
}

/// Ask `service` what `host` has installed, as the holder of `session_token`.
async fn tooling_asked_of(
    service: &HostServiceImpl,
    session_token: &str,
    host: &str,
) -> Result<GetHostToolingResponse, Status> {
    service
        .get_host_tooling(Request::new(GetHostToolingRequest {
            session_token: session_token.to_string(),
            daemon_instance_id: host.to_string(),
        }))
        .await
        .map(|answered| answered.into_inner())
}

/// AC-8 — a token this daemon cannot verify buys nothing, and runs nothing on the host.
#[tokio::test]
async fn host_tooling_rejects_an_invalid_token() {
    // Given a daemon whose probe would answer readily if it were ever asked
    let probe = RecordingHostToolingProbe::answering(a_host_committing_as(
        "Ada Lovelace",
        "ada@example.com",
        "hubot",
    ));
    let service = a_daemon_probing_with(Arc::clone(&probe)).build();

    // When an expired session asks this daemon about itself
    let error = tooling_asked_of(&service, EXPIRED_TOKEN, THIS_DAEMON)
        .await
        .expect_err("an unverifiable session token must be rejected");

    // Then it is refused as unauthenticated, and nothing was run on the host on its behalf
    assert_eq!(error.code, tddy_rpc::Code::Unauthenticated);
    assert_eq!(
        probe.probed_os_users(),
        Vec::<String>::new(),
        "a refused call must not shell out on the host as anyone"
    );
}

/// AC-7 — both facts the probe reads are per-OS-user: `git config --global` reads
/// `$HOME/.gitconfig` and `gh auth status` reads `$HOME/.config/gh/hosts.yml`. Run as the
/// daemon's own user — or as nobody at all — the probe reports a different account's identity
/// under this host's name, which is a wrong answer that reads exactly like a right one and
/// leaves every other test in the suite green.
#[tokio::test]
async fn host_tooling_runs_the_probe_as_the_hosts_os_user() {
    // Given a daemon mapping the signed-in GitHub user `octocat` to the OS user `ada`
    let probe = RecordingHostToolingProbe::answering(a_host_committing_as(
        "Ada Lovelace",
        "ada@example.com",
        "hubot",
    ));
    let service = a_daemon_probing_with(Arc::clone(&probe))
        .mapping(SIGNED_IN_GITHUB_USER, MAPPED_OS_USER)
        .build();

    // When octocat asks this daemon what it has configured
    let tooling = tooling_asked_of(&service, VERIFIABLE_TOKEN, THIS_DAEMON)
        .await
        .expect("a verifiable session asking about the serving daemon must be answered");

    // Then the probe ran exactly once, as the OS user that GitHub user maps to here
    assert_eq!(
        probe.probed_os_users(),
        vec![MAPPED_OS_USER.to_string()],
        "the probe must run as the OS user `{SIGNED_IN_GITHUB_USER}` maps to via users[] — \
             not as the daemon's own user, and not as nobody"
    );
    // And the answer on the wire is that probe's reading, so the identity reported is the one
    // read as `ada` rather than something assembled elsewhere
    assert_eq!(
        tooling
            .git
            .expect("a probed host reports a git identity block")
            .user_name,
        "Ada Lovelace"
    );
}

/// AC-9 — a request naming another host is served by that host, and the routing happens
/// **before** this daemon authenticates the caller locally.
///
/// The operator here has no `users[]` entry on the relay their browser is talking to; their
/// entry lives on `server-2`, the host they are asking about. Authenticating first would refuse
/// them with `permission_denied` from a daemon the question was never about — and that is
/// exactly the ordering `attach_session_agent` and `resolve_stack_base` already establish.
///
/// The refusal that comes back is what proves it: this daemon got as far as forwarding to the
/// peer and failed only on the transport, because the common room in this test holds no
/// connection to carry the call.
#[tokio::test]
async fn host_tooling_for_another_host_is_routed_to_that_peer() {
    // Given a relay that can see `server-2`, and whose users[] maps monalisa — not octocat
    let probe = RecordingHostToolingProbe::answering(a_host_committing_as(
        "Ada Lovelace",
        "ada@example.com",
        "hubot",
    ));
    let service = a_daemon_probing_with(Arc::clone(&probe))
        .mapping(OTHER_OPERATOR, OTHER_OPERATOR)
        .seeing_peer(PROBED_HOST)
        .build();

    // When octocat asks the relay about server-2
    let error = tooling_asked_of(&service, VERIFIABLE_TOKEN, PROBED_HOST)
        .await
        .expect_err("this test's common room can carry no forwarded call");

    // Then the call was routed to the peer rather than judged here: the failure is the forward
    // itself, not `permission_denied` for a user mapping the probed host is the one to hold
    assert_eq!(
        error.code,
        tddy_rpc::Code::FailedPrecondition,
        "an operator unmapped on the relay must not be refused by it; got: {}",
        error.message
    );
    assert!(
        error.message.contains("cannot forward"),
        "the refusal must be about carrying the call to the peer; got: {}",
        error.message
    );
    // And the relay never answered for another host with its own tooling
    assert_eq!(
        probe.probed_os_users(),
        Vec::<String>::new(),
        "a question about server-2 must not be probed locally on workstation-1"
    );
}
