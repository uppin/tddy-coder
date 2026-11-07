//! Validation of a service's `host_sockets:` against the rest of the supervisor configuration.
//!
//! Kept apart from [`crate::config`] because it needs the whole file at once: who may be given a
//! host socket is `spawn_policy`'s decision, and what a socket's directory may not collide with is
//! every other declared socket.

use std::collections::BTreeSet;
use std::path::Path;

use crate::config::{require_absolute, SupervisorConfig, ROOT_USER};
use crate::error::ConfigError;
use crate::socket::MAX_HANDED_OVER_LISTENERS;

/// The checks on every service's `host_sockets`, which need the whole file: who may be given one
/// comes from `spawn_policy`, and what a path may not collide with is every other socket.
pub(crate) fn validate(config: &SupervisorConfig) -> Result<(), ConfigError> {
    let reject = |message: String| Err(ConfigError { message });
    // Every directory a declared socket lives in. A host socket's directory is handed to its
    // user wholesale, so another socket sharing it would be handed over with it.
    let mut directories: BTreeSet<&Path> = BTreeSet::new();
    directories.extend(config.socket.path.parent());
    for service in &config.services {
        directories.extend(service.socket.iter().filter_map(|s| s.path.parent()));
    }
    let mut paths: BTreeSet<&Path> = BTreeSet::new();
    paths.insert(config.socket.path.as_path());
    paths.extend(
        config
            .services
            .iter()
            .filter_map(|s| s.socket.as_ref())
            .map(|s| s.path.as_path()),
    );

    // TODO(agent-tool-socket-per-user): implement — validate every service's
    // `agent_tool_sockets` as its `host_sockets` (needs `socket`, plain allowed non-root user, one
    // per user, a directory of its own shared with no other declared socket), and count both kinds
    // against `MAX_HANDED_OVER_LISTENERS`.
    for service in &config.services {
        if service.host_sockets.is_empty() {
            continue;
        }
        if service.socket.is_none() {
            return reject(format!(
                    "service `{}` declares host_sockets but no socket: descriptor 3 is always the service's own listener, and the host sockets follow it",
                    service.name
                ));
        }
        // Descriptor 3 plus one per host socket must fit the slots the supervisor reserves.
        if 1 + service.host_sockets.len() > MAX_HANDED_OVER_LISTENERS {
            return reject(format!(
                "service `{}` declares {} host_sockets; at most {} fit",
                service.name,
                service.host_sockets.len(),
                MAX_HANDED_OVER_LISTENERS - 1
            ));
        }
        let mut users = BTreeSet::new();
        for host in &service.host_sockets {
            require_absolute("host socket path", &host.path)?;
            if host.user == ROOT_USER {
                return reject(format!(
                    "service `{}`: a host socket may not be given to `{ROOT_USER}`",
                    service.name
                ));
            }
            if !is_one_safe_segment(&host.user) {
                return reject(format!(
                    "service `{}`: host socket user `{}` is not a plain account name",
                    service.name, host.user
                ));
            }
            if !config
                .spawn_policy
                .allowed_session_users
                .contains(&host.user)
            {
                return reject(format!(
                        "service `{}`: host socket user `{}` is not in spawn_policy.allowed_session_users; the supervisor would never run a session as them",
                        service.name, host.user
                    ));
            }
            if !users.insert(host.user.as_str()) {
                return reject(format!(
                    "service `{}` declares two host sockets for `{}`",
                    service.name, host.user
                ));
            }
            let directory = host.path.parent().filter(|d| d.parent().is_some());
            let Some(directory) = directory else {
                return reject(format!(
                    "host socket path `{}` needs a directory of its own below the root",
                    host.path.display()
                ));
            };
            // Nothing declared may live *inside* the directory being handed over either: it
            // would be locked behind the user's `0700`.
            let encloses_another = paths.iter().any(|other| other.starts_with(directory));
            if encloses_another
                || !paths.insert(host.path.as_path())
                || !directories.insert(directory)
            {
                return reject(format!(
                        "host socket path `{}` shares a path or directory with another declared socket; its directory is handed to `{}`",
                        host.path.display(),
                        host.user
                    ));
            }
        }
    }
    Ok(())
}

/// Whether `name` can ride in a descriptor name and a path segment unchanged.
fn is_one_safe_segment(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    //! `agent_tool_sockets:` (`#agent-tools` 26/28) is validated like `host_sockets:`, and both kinds
    //! share the handed-over descriptor slots.

    use crate::config::{HostSocket, SupervisorConfig};
    use crate::socket::MAX_HANDED_OVER_LISTENERS;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    /// A daemon service with its own socket, the given `host_sockets:` and `agent_tool_sockets:`
    /// blocks, and `allowed` as the session users.
    fn a_daemon_declaring(host_sockets: &str, agent_tool_sockets: &str, allowed: &str) -> String {
        format!(
            "\
socket:
  path: /run/tddy-supervisor.sock
services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    user: tddy
    socket:
      path: /run/tddy-daemon.sock
    host_sockets:
{host_sockets}
    agent_tool_sockets:
{agent_tool_sockets}
spawn_policy:
  allowed_session_users: [{allowed}]
"
        )
    }

    /// `      - {user: <user>, path: /run/tddy/<dir>/<file>}`
    fn a_socket_entry(user: &str, dir: &str, file: &str) -> String {
        format!("      - {{user: {user}, path: /run/tddy/{dir}/{file}}}")
    }

    /// Why `yaml` was rejected, or `None` when it was accepted.
    fn rejection_of(yaml: &str) -> Option<String> {
        SupervisorConfig::from_yaml(yaml).err().map(|e| e.message)
    }

    /// Whether `yaml` was rejected for a reason mentioning `reason`.
    fn is_rejected_because(yaml: &str, reason: &str) -> Result<(), String> {
        match rejection_of(yaml) {
            Some(message) if message.contains(reason) => Ok(()),
            Some(message) => Err(format!("rejected, but not for `{reason}`: {message}")),
            None => Err(format!("accepted; expected a rejection for `{reason}`")),
        }
    }

    #[test]
    fn supervisor_config_rejects_a_shared_dir_and_too_many_listeners() {
        // Given alice's agent-tool socket in her host socket's directory
        let shared_dir = a_daemon_declaring(
            &a_socket_entry("alice", "alice", "host.sock"),
            &a_socket_entry("alice", "alice", "agent.sock"),
            "alice",
        );
        // And as many host sockets as fit, plus one agent-tool socket more
        let users: Vec<String> = (0..MAX_HANDED_OVER_LISTENERS - 1)
            .map(|i| format!("user{i}"))
            .collect();
        let host_sockets: Vec<String> = users
            .iter()
            .map(|user| a_socket_entry(user, &format!("{user}-host"), "host.sock"))
            .collect();
        let too_many = a_daemon_declaring(
            &host_sockets.join("\n"),
            &a_socket_entry(&users[0], "user0-tools", "agent.sock"),
            &users.join(", "),
        );

        // Then both are rejected, each for its own reason
        assert_eq!(
            (
                is_rejected_because(&shared_dir, "shares a path or directory"),
                is_rejected_because(&too_many, "fit"),
            ),
            (Ok(()), Ok(()))
        );
    }

    #[test]
    fn rejects_an_agent_tool_socket_for_a_user_the_policy_does_not_allow_sessions_as() {
        // Given carol is not an allowed session user
        let yaml = a_daemon_declaring(
            &a_socket_entry("alice", "alice", "host.sock"),
            &a_socket_entry("carol", "carol-tools", "agent.sock"),
            "alice",
        );

        // Then
        assert_eq!(is_rejected_because(&yaml, "allowed_session_users"), Ok(()));
    }

    #[test]
    fn rejects_an_agent_tool_socket_for_root() {
        // Given
        let yaml = a_daemon_declaring(
            &a_socket_entry("alice", "alice", "host.sock"),
            &a_socket_entry("root", "root-tools", "agent.sock"),
            "alice",
        );

        // Then — refused as an agent-tool socket, before any policy lookup
        assert_eq!(
            is_rejected_because(&yaml, "an agent-tool socket may not be given to `root`"),
            Ok(())
        );
    }

    #[test]
    fn rejects_two_agent_tool_sockets_for_one_user() {
        // Given
        let yaml = a_daemon_declaring(
            &a_socket_entry("alice", "alice", "host.sock"),
            &[
                a_socket_entry("alice", "alice-tools-1", "agent.sock"),
                a_socket_entry("alice", "alice-tools-2", "agent.sock"),
            ]
            .join("\n"),
            "alice",
        );

        // Then
        assert_eq!(is_rejected_because(&yaml, "two agent-tool sockets"), Ok(()));
    }

    #[test]
    fn rejects_agent_tool_sockets_on_a_service_that_has_no_socket_of_its_own() {
        // Given descriptor 3 would then be an agent-tool socket the service takes for its own
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    user: tddy
    agent_tool_sockets:
      - {user: alice, path: /run/tddy/alice-tools/agent.sock}
spawn_policy:
  allowed_session_users: [alice]
";

        // Then
        assert_eq!(is_rejected_because(yaml, "no socket"), Ok(()));
    }

    #[test]
    fn accepts_a_host_and_an_agent_tool_socket_for_one_user_in_directories_of_their_own() {
        // Given — guard: the shape `supervisor.yaml` is meant to take
        let yaml = a_daemon_declaring(
            &a_socket_entry("alice", "run/alice", "host.sock"),
            &a_socket_entry("alice", "run-agent-tools/alice", "agent.sock"),
            "alice",
        );

        // Then
        assert_eq!(
            SupervisorConfig::from_yaml(&yaml)
                .map(|config| config.services[0].agent_tool_sockets.clone())
                .map_err(|e| e.message),
            Ok(vec![HostSocket {
                user: "alice".to_string(),
                path: PathBuf::from("/run/tddy/run-agent-tools/alice/agent.sock"),
            }])
        );
    }

    #[test]
    fn declares_no_agent_tool_sockets_by_default() {
        // Given / When — guard: an existing supervisor.yaml keeps loading
        let config = SupervisorConfig::from_yaml(
            "socket:\n  path: /run/s.sock\nservices:\n  - {name: d, exec_start: /bin/d, user: tddy}\n",
        )
        .expect("parse config");

        // Then
        assert_eq!(config.services[0].agent_tool_sockets, Vec::new());
    }
}
