use super::*;
// Reached through `use super::*` until the proto converters moved to `host_messages`, whose
// facade cannot re-export a name the module merely imports. Bound here instead.
use crate::host_tooling::{
    GitIdentity, GithubCliStatus, HostTooling, HostToolingProbe, ProbeOutcome,
};
use crate::ssh_agent::{AgentKey, AgentStatus};
use std::sync::Mutex;
use tddy_service::proto::connection::GetHostToolingRequest;
use tddy_service::proto::connection::{ProbeOutcome as ProtoProbeOutcome, SshAgentKey};

/// The GitHub login the session token resolves to, and the OS user it maps to on this host.
/// Deliberately different strings: the probes are scoped to the second, and a handler passing
/// the first would answer for an account that does not exist here.
const GITHUB_USER: &str = "ada-gh";
const OS_USER: &str = "ada";

/// A real ed25519 fingerprint, as `ssh-keygen -lf` prints it.
const FINGERPRINT: &str = "SHA256:JfISx02kSjWJevGy/MjUdXCv76HaRM3YkYNvepTyHD8";

/// A tooling probe that answers with a scripted result and remembers who it was asked about.
struct FakeToolingProbe {
    tooling: HostTooling,
    asked_about: Mutex<Vec<String>>,
}

impl HostToolingProbe for FakeToolingProbe {
    fn probe(&self, os_user: &str) -> HostTooling {
        self.asked_about
            .lock()
            .expect("the recording lock is only held to push a name")
            .push(os_user.to_string());
        self.tooling.clone()
    }
}

/// A host with git configured and `gh` authenticated, and no agent reached — the state every
/// test below varies one part of.
fn a_probed_host() -> HostTooling {
    HostTooling {
        git: GitIdentity {
            outcome: ProbeOutcome::Ok,
            name_and_email: Some(("Ada Lovelace".to_string(), "ada@example.com".to_string())),
        },
        github_cli: GithubCliStatus {
            outcome: ProbeOutcome::Ok,
            installed: true,
            authenticated: true,
            login: Some("ada".to_string()),
        },
        ssh_agent: AgentStatus::unreachable(),
        // These tests are about the agent block; no protocol was probed, which is the neutral
        // value for the block `#hosts-screen 7/8` added beside it.
        remote_desktop: Vec::new(),
    }
}

fn an_agent_holding(keys: Vec<AgentKey>) -> AgentStatus {
    AgentStatus::holding(keys)
}

fn a_key(key_type: &str, comment: &str) -> AgentKey {
    AgentKey {
        key_type: key_type.to_string(),
        fingerprint: FINGERPRINT.to_string(),
        comment: comment.to_string(),
    }
}

fn a_config_mapping_ada() -> crate::config::DaemonConfig {
    let yaml = format!("users:\n  - github_user: \"{GITHUB_USER}\"\n    os_user: \"{OS_USER}\"\n");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    crate::config::DaemonConfig::load(&path).unwrap()
}

/// A service whose tooling probe reports `tooling`, plus the probe itself, so a test can ask
/// which OS user it was pointed at.
fn service_probing(tooling: HostTooling) -> (ConnectionServiceImpl, Arc<FakeToolingProbe>) {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().to_path_buf();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let user_resolver: SessionUserResolver =
        Arc::new(|token| (token == "valid").then(|| GITHUB_USER.to_string()));
    let probe = Arc::new(FakeToolingProbe {
        tooling,
        asked_about: Mutex::new(Vec::new()),
    });
    let service = ConnectionServiceImpl::new(
        a_config_mapping_ada(),
        sessions_base_resolver,
        temp.path().to_path_buf(),
        user_resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    )
    .with_host_tooling(Arc::clone(&probe) as Arc<dyn HostToolingProbe>);
    (service, probe)
}

async fn tooling_reported_by(service: &ConnectionServiceImpl) -> GetHostToolingResponse {
    service
        .get_host_tooling(Request::new(GetHostToolingRequest {
            session_token: "valid".to_string(),
            // Empty means "the daemon serving the call", so nothing is routed to a peer.
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("get_host_tooling should succeed for a valid session")
        .into_inner()
}

/// AC-7: the block reports the agent of the **OS user** this host maps the caller to, and every
/// identity it holds arrives whole — type, fingerprint and comment.
#[tokio::test]
async fn host_tooling_reports_the_ssh_agent_block_for_the_hosts_os_user() {
    // Given
    let (service, probe) = service_probing(HostTooling {
        ssh_agent: an_agent_holding(vec![a_key("ssh-ed25519", "ada@workstation")]),
        ..a_probed_host()
    });

    // When
    let reported = tooling_reported_by(&service).await;

    // Then
    assert_eq!(
        *probe.asked_about.lock().unwrap(),
        vec![OS_USER.to_string()],
        "the agent belongs to the host's OS user, not to the caller's GitHub login"
    );
    let agent = reported
        .ssh_agent
        .expect("the response carries an agent block");
    assert_eq!(agent.outcome, ProtoProbeOutcome::Ok as i32);
    assert!(agent.reachable, "an agent answered");
    assert_eq!(
        agent.keys,
        vec![SshAgentKey {
            key_type: "ssh-ed25519".to_string(),
            fingerprint: FINGERPRINT.to_string(),
            comment: "ada@workstation".to_string(),
        }],
        "each held key reaches the wire whole"
    );
}

/// The agent block is an addition, not a replacement: a host whose agent cannot be reached must
/// still report the git identity and `gh` login it does have, or adding this block would have
/// taken away the answers node 4 already gave.
#[tokio::test]
async fn host_tooling_still_reports_git_and_gh_when_the_agent_is_unreachable() {
    // Given
    let (service, _probe) = service_probing(a_probed_host());

    // When
    let reported = tooling_reported_by(&service).await;

    // Then
    let git = reported.git.expect("the response carries a git block");
    assert!(git.configured, "the git identity survives an absent agent");
    assert_eq!(git.user_name, "Ada Lovelace");
    assert_eq!(git.user_email, "ada@example.com");
    let github_cli = reported
        .github_cli
        .expect("the response carries a gh block");
    assert!(github_cli.authenticated);
    assert_eq!(github_cli.login, "ada");
    let agent = reported
        .ssh_agent
        .expect("the response carries an agent block");
    assert!(!agent.reachable, "no agent answered");
    assert_eq!(
        agent.outcome,
        ProtoProbeOutcome::Ok as i32,
        "finding no agent is a successful probe with a negative finding, not a failure"
    );
    assert!(agent.keys.is_empty());
}
