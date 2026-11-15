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
}

impl SessionTarget {
    /// The ssh target `config_host`.
    pub fn ssh(config_host: impl Into<String>) -> Self {
        Self::Ssh {
            config_host: config_host.into(),
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
