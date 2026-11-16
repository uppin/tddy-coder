//! Where a session's agent runs when that is not the daemon's own host: a **target**, a machine the
//! serving daemon reaches through a remote shell, with no tddy software on it.
//!
//! Stored as `target` in `.session.yaml` ([`crate::SessionMetadata`]) and listed as
//! `SessionEntry.target`; a start names it as `StartSessionRequest.target`. The daemon creates the
//! worktree there, starts the agent there, and serves its terminal as it serves a local one.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-ssh-agent-launch.md.

use serde::{Deserialize, Serialize};

/// The machine a session's agent runs on, by the transport that reaches it. `.session.yaml` spells
/// it as a map naming its `kind`:
///
/// ```yaml
/// target:
///   kind: ssh
///   config_host: buildbox
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionTarget {
    /// An OpenSSH host of the serving daemon's user.
    Ssh {
        /// An explicit `Host` alias from the serving daemon's `~/.ssh/config`.
        config_host: String,
    },
    /// A workspace of one of the session owner's Coder accounts, reached through the `coder` CLI
    /// on the serving daemon. The record names the account and never holds its token: a resume
    /// reads it from the owner's vault again.
    ///
    /// ```yaml
    /// target:
    ///   kind: coder
    ///   account_id: coder-3f1c9a52@coder.example.com
    ///   owner: ada
    ///   workspace: devbox
    /// ```
    Coder {
        /// The Coder account, as the accounts listing names it.
        account_id: String,
        /// The login name of the user the workspace belongs to.
        owner: String,
        /// The workspace's name, unique among its owner's.
        workspace: String,
        /// The workspace's agent to reach, for a workspace with more than one. Absent leaves the
        /// choice to the CLI, and is never written.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<String>,
    },
}

impl SessionTarget {
    /// The ssh target `config_host`.
    pub fn ssh(config_host: impl Into<String>) -> Self {
        Self::Ssh {
            config_host: config_host.into(),
        }
    }

    /// `owner`'s workspace `workspace` of the Coder account `account_id`, on whichever agent the
    /// workspace has.
    pub fn coder(
        account_id: impl Into<String>,
        owner: impl Into<String>,
        workspace: impl Into<String>,
    ) -> Self {
        Self::Coder {
            account_id: account_id.into(),
            owner: owner.into(),
            workspace: workspace.into(),
            agent: None,
        }
    }

    /// This target, on its workspace's agent `agent`. An ssh target has no agent and is returned
    /// as it is.
    #[must_use]
    pub fn on_agent(self, agent: impl Into<String>) -> Self {
        match self {
            Self::Coder {
                account_id,
                owner,
                workspace,
                ..
            } => Self::Coder {
                account_id,
                owner,
                workspace,
                agent: Some(agent.into()),
            },
            ssh @ Self::Ssh { .. } => ssh,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn an_ssh_target_is_written_as_a_map_naming_its_kind_and_host() {
        // Given
        let target = SessionTarget::ssh("buildbox");

        // When
        let yaml = serde_yaml::to_string(&target).expect("a target serializes");

        // Then
        assert_eq!(yaml, "kind: ssh\nconfig_host: buildbox\n");
    }

    #[test]
    fn a_coder_target_is_written_as_a_map_naming_its_account_and_workspace() {
        // Given
        let target = SessionTarget::coder("coder-3f1c9a52@coder.example.com", "ada", "devbox");

        // When
        let yaml = serde_yaml::to_string(&target).expect("a target serializes");

        // Then the agent, which was not named, is not written
        assert_eq!(
            yaml,
            "kind: coder\naccount_id: coder-3f1c9a52@coder.example.com\nowner: ada\nworkspace: devbox\n"
        );
    }

    #[test]
    fn a_coder_target_naming_an_agent_reads_back_as_it_was_written() {
        // Given
        let target = SessionTarget::coder("coder-3f1c9a52@coder.example.com", "ada", "devbox")
            .on_agent("main");

        // When
        let yaml = serde_yaml::to_string(&target).expect("a target serializes");
        let read = serde_yaml::from_str::<SessionTarget>(&yaml).expect("the target reads back");

        // Then
        assert_eq!(
            (yaml.ends_with("workspace: devbox\nagent: main\n"), read),
            (true, target)
        );
    }

    #[test]
    fn a_coder_target_holding_a_token_is_refused() {
        // Given a record that carries a credential beside the account's name
        let yaml = "kind: coder\naccount_id: a1\nowner: ada\nworkspace: devbox\ntoken: secret\n";

        // When
        let read = serde_yaml::from_str::<SessionTarget>(yaml);

        // Then
        assert!(
            read.is_err(),
            "a target must not read a field it does not declare: {read:?}"
        );
    }

    #[test]
    fn a_target_of_a_kind_this_daemon_does_not_know_is_refused() {
        // Given a record written by a daemon that knows another transport
        let yaml = "kind: teleport\nconfig_host: buildbox\n";

        // When
        let read = serde_yaml::from_str::<SessionTarget>(yaml);

        // Then
        assert!(
            read.is_err(),
            "an unknown kind must not read as a target: {read:?}"
        );
    }
}
