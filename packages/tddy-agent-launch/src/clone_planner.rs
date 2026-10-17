//! Deciding how one clone reaches its remote, and holding what it needs until it ends.
//!
//! Runs on the blocking pool, inside the cloner closure: only there is the URL being cloned known
//! (a registered project's stored `git_url`, or a peer's). A failure to plan is remembered and
//! returned to the caller as a precise [`Status`], because the cloner's own error channel is a
//! string that becomes `internal`.

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

use tddy_daemon_kernel::SessionUserResolver;
use tddy_host_service::askpass::{AskpassEndpoint, CloneGrant};
use tddy_host_service::ssh_agent::{AgentSocketResolver, WellKnownAgentSockets};
use tddy_rpc::Status;
use tddy_spawn::clone_env::{CloneSshEnv, OpenSshVersion};

use crate::clone_ssh_env::{clone_ssh_env_for, is_ssh_url, resolve_askpass_path, ClonePlan};
use crate::launch_ports::LaunchSessions;

/// The first line `ssh -V` prints (to stderr), once per process. `None` when `ssh` did not run or
/// printed nothing.
fn host_ssh_banner() -> Option<&'static str> {
    static BANNER: OnceLock<Option<String>> = OnceLock::new();
    BANNER
        .get_or_init(|| {
            let out = Command::new("ssh")
                .arg("-V")
                .output()
                .map_err(|e| log::warn!("clone: could not run `ssh -V`: {e}"))
                .ok()?;
            let text = if out.stderr.is_empty() {
                out.stdout
            } else {
                out.stderr
            };
            let text = String::from_utf8_lossy(&text);
            text.lines().next().map(|l| l.trim().to_string())
        })
        .as_deref()
}

/// What a clone runs with, and the grant that must outlive it.
pub(crate) struct PlannedClone {
    env: BTreeMap<String, String>,
    // Held, never read: dropping it ends the helper's authority, which is when the clone ends.
    _grant: Option<CloneGrant>,
    /// Set when this host's OpenSSH cannot be told to ask: what to tell the operator if it fails.
    unprompted_because: Option<String>,
}

impl PlannedClone {
    fn plain(env: BTreeMap<String, String>) -> Self {
        Self {
            env,
            _grant: None,
            unprompted_because: None,
        }
    }

    /// An SSH clone that must not stop to ask anyone anything: it fails fast instead, and `because`
    /// is what a failure is explained with. A key that needs no passphrase still works.
    fn unprompted(because: String) -> Self {
        let env = CloneSshEnv::new("", "", "").without_prompting();
        Self {
            env: ClonePlan::Ssh(env).environment(),
            _grant: None,
            unprompted_because: Some(because),
        }
    }

    /// The variables to set on the `git clone` child.
    pub(crate) fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    /// `error`, with why no passphrase could be asked for when the clone ran without prompting.
    pub(crate) fn explain_failure(&self, error: String) -> String {
        match &self.unprompted_because {
            Some(why) => format!(
                "{error}; {why}, so a passphrase-protected key must be loaded into the ssh-agent \
                 by hand"
            ),
            None => error,
        }
    }
}

/// Decides each clone's [`PlannedClone`]. Clones share the slot [`Self::take_failure`] reads.
#[derive(Clone)]
pub(crate) struct ClonePlanner {
    facilitator_ssh_command: Option<String>,
    askpass: Option<Arc<AskpassEndpoint>>,
    os_user: String,
    user_resolver: SessionUserResolver,
    session_token: String,
    failure: Arc<Mutex<Option<Status>>>,
    /// Where the OS user's agent is. The host's own lookup unless [`Self::with_host_probes`] says
    /// otherwise.
    agents: Arc<dyn AgentSocketResolver>,
    /// The host's `ssh -V` banner, asked for only once a clone needs it.
    ssh_banner: fn() -> Option<&'static str>,
}

impl ClonePlanner {
    pub(crate) fn new(
        facilitator_ssh_command: Option<String>,
        askpass: Option<Arc<AskpassEndpoint>>,
        os_user: &str,
        user_resolver: SessionUserResolver,
        session_token: &str,
    ) -> Self {
        Self {
            facilitator_ssh_command,
            askpass,
            os_user: os_user.to_string(),
            user_resolver,
            session_token: session_token.to_string(),
            failure: Arc::default(),
            agents: Arc::new(WellKnownAgentSockets),
            ssh_banner: host_ssh_banner,
        }
    }

    /// Plan against other answers to "where is the agent" and "which OpenSSH is this".
    #[cfg(test)]
    fn with_host_probes(
        mut self,
        agents: Arc<dyn AgentSocketResolver>,
        ssh_banner: fn() -> Option<&'static str>,
    ) -> Self {
        self.agents = agents;
        self.ssh_banner = ssh_banner;
        self
    }

    /// A planner for the clones `launch` runs on behalf of the session `session_token` names.
    pub(crate) fn for_launch(
        launch: &LaunchSessions,
        facilitator_ssh_command: Option<String>,
        os_user: &str,
        session_token: &str,
    ) -> Self {
        Self::new(
            facilitator_ssh_command,
            launch.askpass.clone(),
            os_user,
            launch.user_resolver.clone(),
            session_token,
        )
    }

    /// The reason a clone was not started, when planning refused one.
    pub(crate) fn take_failure(&self) -> Option<Status> {
        self.failure
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    /// Plan the clone of `git_url`, or say why none may start. The reason is also kept for
    /// [`Self::take_failure`].
    pub(crate) fn plan(&self, git_url: &str) -> Result<PlannedClone, String> {
        self.decide(git_url).map_err(|status| {
            let message = status.message.clone();
            *self.failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(status);
            message
        })
    }

    fn decide(&self, git_url: &str) -> Result<PlannedClone, Status> {
        if let Some(ssh_command) = &self.facilitator_ssh_command {
            let facilitator = ClonePlan::Facilitator {
                ssh_command: ssh_command.clone(),
            };
            return Ok(PlannedClone::plain(facilitator.environment()));
        }
        // An HTTPS (or local) remote asks nobody for a key. A host that never installed an askpass
        // endpoint (no daemon runtime) has nobody to ask either: its clone runs as it always did.
        let Some(askpass) = self.askpass.as_ref().filter(|_| is_ssh_url(git_url)) else {
            return Ok(PlannedClone::plain(BTreeMap::new()));
        };

        // `ssh -V` is not run for a clone that needs no key; for one that does, an OpenSSH that
        // cannot be told to ask would hang on the first passphrase, so it is run without asking.
        let banner = (self.ssh_banner)();
        let version = banner.and_then(OpenSshVersion::parse);
        if !version.is_some_and(|v| v.supports_forced_askpass()) {
            let known = banner.unwrap_or("its version could not be determined");
            log::warn!(
                "clone: this host's OpenSSH ({known}) cannot be told to ask for a key passphrase \
                 (needs >= 8.4); cloning {git_url} without prompting"
            );
            return Ok(PlannedClone::unprompted(format!(
                "this host's OpenSSH ({known}) cannot ask for a key passphrase (needs >= 8.4)"
            )));
        }

        let operator = (self.user_resolver)(&self.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let grant = askpass.register_clone(&operator);
        // Without an agent there is nowhere for ssh to keep an unlocked key, so no passphrase can be
        // asked for — but a key that needs none still works, and refusing the clone outright would
        // break every host that clones with a passphrase-less deploy key and runs no agent.
        let ssh = match clone_ssh_env_for(
            self.agents.as_ref(),
            &self.os_user,
            &resolve_askpass_path(),
            &grant,
        ) {
            Ok(ssh) => ssh,
            Err(no_agent) => {
                log::warn!("clone: {no_agent}; cloning {git_url} without prompting");
                return Ok(PlannedClone::unprompted(no_agent.to_string()));
            }
        };
        Ok(PlannedClone {
            env: ClonePlan::Ssh(ssh).environment(),
            _grant: Some(grant),
            unprompted_because: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tddy_daemon_kernel::SessionUserResolver;
    use tddy_host_service::askpass::AskpassEndpoint;
    use tddy_host_service::host_keypair::FileHostKeypair;
    use tddy_host_service::host_prompts::{HostPromptRegistry, InMemoryHostPromptRegistry};
    use tddy_host_service::ssh_agent::AgentSocketResolver;

    use super::*;

    const SSH_REMOTE: &str = "git@github.com:uppin/tddy-coder.git";

    struct NoAgent;

    impl AgentSocketResolver for NoAgent {
        fn socket_for(&self, _os_user: &str) -> Result<Option<PathBuf>, String> {
            Ok(None)
        }
    }

    struct AgentAt(&'static str);

    impl AgentSocketResolver for AgentAt {
        fn socket_for(&self, _os_user: &str) -> Result<Option<PathBuf>, String> {
            Ok(Some(PathBuf::from(self.0)))
        }
    }

    struct LookupThatFails;

    impl AgentSocketResolver for LookupThatFails {
        fn socket_for(&self, _os_user: &str) -> Result<Option<PathBuf>, String> {
            Err("passwd database did not answer".to_string())
        }
    }

    fn a_modern_openssh() -> Option<&'static str> {
        Some("OpenSSH_9.9p1, OpenSSL 3.0.13")
    }

    fn an_openssh_that_cannot_ask() -> Option<&'static str> {
        Some("OpenSSH_8.3p1 Ubuntu-1ubuntu0.2, OpenSSL 1.1.1f  31 Mar 2020")
    }

    fn the_operator_is_ada() -> SessionUserResolver {
        Arc::new(|_| Some("ada".to_string()))
    }

    async fn a_planner_over(
        agents: impl AgentSocketResolver + 'static,
    ) -> (ClonePlanner, tempfile::TempDir) {
        let keys = tempfile::tempdir().expect("a directory for the host keypair");
        let endpoint = AskpassEndpoint::start(
            Arc::new(InMemoryHostPromptRegistry::new()) as Arc<dyn HostPromptRegistry>,
            Arc::new(FileHostKeypair::new(keys.path())),
        )
        .await
        .expect("the endpoint binds a loopback port");
        let planner = ClonePlanner::new(
            None,
            Some(Arc::new(endpoint)),
            "mantas",
            the_operator_is_ada(),
            "token",
        )
        .with_host_probes(Arc::new(agents), a_modern_openssh);
        (planner, keys)
    }

    #[tokio::test]
    async fn a_host_with_no_agent_still_clones_an_ssh_remote_without_prompting() {
        // Given an OS user with no ssh-agent, cloning over ssh
        let (planner, _keys) = a_planner_over(NoAgent).await;

        // When the clone is planned
        let planned = planner.plan(SSH_REMOTE).expect("the clone is not refused");

        // Then it runs fail-fast with nothing to ask, so a key without a passphrase keeps working
        assert!(planned.env()["GIT_SSH_COMMAND"].contains("-o BatchMode=yes"));
        assert!(!planned.env().contains_key("SSH_ASKPASS"));
        assert!(planner.take_failure().is_none());
    }

    #[tokio::test]
    async fn the_failure_of_a_clone_with_no_agent_says_so() {
        let (planner, _keys) = a_planner_over(NoAgent).await;
        let planned = planner.plan(SSH_REMOTE).expect("planned");

        let said = planned.explain_failure("Permission denied (publickey)".to_string());

        assert!(said.starts_with("Permission denied (publickey)"), "{said}");
        assert!(
            said.contains("no ssh-agent is reachable for mantas"),
            "{said}"
        );
    }

    #[tokio::test]
    async fn a_lookup_that_could_not_be_made_is_not_reported_as_a_missing_agent() {
        let (planner, _keys) = a_planner_over(LookupThatFails).await;
        let planned = planner.plan(SSH_REMOTE).expect("planned");

        let said = planned.explain_failure("Permission denied (publickey)".to_string());

        assert!(said.contains("could not look"), "{said}");
        assert!(!said.contains("no ssh-agent is reachable"), "{said}");
    }

    #[tokio::test]
    async fn a_host_with_an_agent_clones_with_the_passphrase_machinery() {
        let (planner, _keys) = a_planner_over(AgentAt("/run/user/1000/ssh-agent.socket")).await;

        let planned = planner.plan(SSH_REMOTE).expect("planned");

        assert_eq!(
            planned.env()["SSH_AUTH_SOCK"],
            "/run/user/1000/ssh-agent.socket"
        );
        assert_eq!(planned.env()["SSH_ASKPASS_REQUIRE"], "force");
    }

    #[tokio::test]
    async fn an_openssh_older_than_8_4_clones_without_prompting_and_says_so() {
        // Given a host whose OpenSSH cannot be told to ask without a display, though an agent exists
        let (planner, _keys) = a_planner_over(AgentAt("/run/user/1000/ssh-agent.socket")).await;
        let planner = planner.with_host_probes(
            Arc::new(AgentAt("/run/user/1000/ssh-agent.socket")),
            an_openssh_that_cannot_ask,
        );

        // When the clone is planned
        let planned = planner.plan(SSH_REMOTE).expect("the clone is not refused");

        // Then it runs fail-fast with no helper, and a failure names the version and the 8.4 floor
        assert!(planned.env()["GIT_SSH_COMMAND"].contains("-o BatchMode=yes"));
        assert!(!planned.env().contains_key("SSH_ASKPASS"));
        let said = planned.explain_failure("Permission denied (publickey)".to_string());
        assert!(said.contains("OpenSSH_8.3p1"), "{said}");
        assert!(said.contains("needs >= 8.4"), "{said}");
    }
}
