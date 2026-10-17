//! The whole clone, assembled from the real parts.
//!
//! Real: `git`, `ssh`, `sshd`, `ssh-agent`, the `tddy-askpass` binary, the daemon's askpass
//! endpoint, the prompt registry, the host keypair and the environment the production code composes
//! for a clone. Scripted: the operator, who answers the way a browser does.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_host_service::askpass::{AskpassEndpoint, CloneGrant};
use tddy_host_service::host_keypair::FileHostKeypair;
use tddy_host_service::host_prompts::{HostPromptRegistry, InMemoryHostPromptRegistry};
use tddy_spawn::clone_env::CloneSshEnv;

use super::operator::{an_operator_answering, ScriptedOperator};
use super::ssh_agent::{a_running_ssh_agent, AgentFixture};
use super::sshd::{git_head, SshdFixture};
use super::{built_binary, current_os_user};

/// The GitHub user whose session started the clone — the one a prompt is raised for.
pub const OPERATOR: &str = "ada";

/// How long a clone may take in a test before the test calls it a hang.
///
/// The thing under test is that a clone *returns*; without this bound a regression to the old
/// behaviour would stall the suite for the daemon's whole five-minute deadline.
pub const A_CLONE_THAT_HANGS: Duration = Duration::from_secs(60);

pub struct CloneRig {
    pub sshd: SshdFixture,
    pub agent: AgentFixture,
    pub operator: ScriptedOperator,
    grant: CloneGrant,
    connect_timeout: Duration,
    _endpoint: AskpassEndpoint,
    _keys: tempfile::TempDir,
    destinations: tempfile::TempDir,
}

/// A rig whose operator will type `answer` when asked for a passphrase.
pub async fn a_clone_rig(sshd: SshdFixture, answer: &'static str) -> CloneRig {
    let keys = tempfile::tempdir().expect("a directory for the host keypair");
    let keypair = Arc::new(FileHostKeypair::new(keys.path()));
    let prompts: Arc<dyn HostPromptRegistry> = Arc::new(InMemoryHostPromptRegistry::new());
    let operator = an_operator_answering(answer, OPERATOR, Arc::clone(&prompts), keypair.clone());
    let endpoint = AskpassEndpoint::start(prompts, keypair)
        .await
        .expect("the askpass endpoint binds a loopback port");
    let grant = endpoint.register_clone(OPERATOR);
    CloneRig {
        sshd,
        agent: a_running_ssh_agent(),
        operator,
        grant,
        connect_timeout: Duration::from_secs(5),
        _endpoint: endpoint,
        _keys: keys,
        destinations: tempfile::tempdir().expect("a directory for the clones"),
    }
}

/// What a clone did.
pub struct CloneRun {
    pub destination: PathBuf,
    pub outcome: Result<(), String>,
}

impl CloneRig {
    pub fn with_a_connect_timeout_of(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Clone the served repository into a fresh directory, as the current OS user, with the
    /// environment the daemon composes for a clone.
    pub async fn clone_the_repository(&self, name: &str) -> CloneRun {
        self.clone_from(&self.sshd.clone_url(), name).await
    }

    /// [`Self::clone_the_repository`] from somewhere other than the fixture server.
    pub async fn clone_from(&self, url: &str, name: &str) -> CloneRun {
        let destination = self.destinations.path().join(name);
        let env = CloneSshEnv::new(
            built_binary("tddy-askpass"),
            &self.grant.url(),
            &self.grant.token(),
        )
        .agent_socket(self.agent.socket())
        .ssh_args(self.sshd.client_ssh_args())
        .connect_timeout(self.connect_timeout)
        .into_env();

        let (url, dest) = (url.to_string(), destination.clone());
        let clone = tokio::task::spawn_blocking(move || {
            let pairs: Vec<(&str, &str)> =
                env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            tddy_spawn::spawner::clone_as_user_with_env(&current_os_user(), &url, &dest, &pairs)
                .map_err(|e| format!("{e:#}"))
        });
        let outcome = tokio::time::timeout(A_CLONE_THAT_HANGS, clone)
            .await
            .expect("the clone returned instead of waiting for a passphrase nobody can type")
            .expect("the clone task did not panic");
        CloneRun {
            destination,
            outcome,
        }
    }
}

impl CloneRun {
    /// The clone's checkout is at `commit`.
    pub fn is_checked_out_at(&self, commit: &str) -> bool {
        self.outcome.is_ok() && git_head(&self.destination) == commit
    }

    /// The error the clone ended with. Panics when it succeeded, naming what it did instead.
    pub fn failure(&self) -> &str {
        match &self.outcome {
            Err(error) => error,
            Ok(()) => panic!(
                "the clone into {} was expected to fail but completed",
                self.destination.display()
            ),
        }
    }

    pub fn destination(&self) -> &Path {
        &self.destination
    }
}
