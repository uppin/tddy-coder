//! Validation of a service's `serves_users` against the rest of the supervisor configuration.
//!
//! Kept apart from [`crate::config`] because it needs the whole file at once: a service that
//! serves users needs the top-level `users:` block (where `users.yaml` and its sockets live) and
//! its own listener at descriptor 3, after which the per-user sockets follow.

use crate::config::SupervisorConfig;
use crate::error::ConfigError;

/// The checks on every service declaring `serves_users`, which need the whole file.
pub(crate) fn validate(config: &SupervisorConfig) -> Result<(), ConfigError> {
    for service in &config.services {
        if !service.serves_users {
            continue;
        }
        if config.users.is_none() {
            return Err(ConfigError {
                message: format!(
                    "service `{}` declares serves_users but supervisor.yaml has no `users:` \
                     block naming the users.yaml it should serve",
                    service.name
                ),
            });
        }
        if service.socket.is_none() {
            return Err(ConfigError {
                message: format!(
                    "service `{}` declares serves_users but no socket: descriptor 3 is always \
                     the service's own listener, and the per-user sockets follow it",
                    service.name
                ),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config::SupervisorConfig;

    fn rejection_of(yaml: &str) -> Option<String> {
        SupervisorConfig::from_yaml(yaml).err().map(|e| e.message)
    }

    fn is_rejected_because(yaml: &str, reason: &str) -> Result<(), String> {
        match rejection_of(yaml) {
            Some(message) if message.contains(reason) => Ok(()),
            Some(message) => Err(format!("rejected, but not for `{reason}`: {message}")),
            None => Err(format!("accepted; expected a rejection for `{reason}`")),
        }
    }

    #[test]
    fn rejects_a_service_that_serves_users_without_a_users_block() {
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    user: tddy
    serves_users: true
    socket:
      path: /run/tddy-daemon.sock
";
        assert_eq!(is_rejected_because(yaml, "users"), Ok(()));
    }

    #[test]
    fn rejects_a_service_that_serves_users_without_its_own_socket() {
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
users:
  file: /etc/tddy/users.yaml
services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    user: tddy
    serves_users: true
";
        assert_eq!(is_rejected_because(yaml, "no socket"), Ok(()));
    }

    #[test]
    fn accepts_a_service_that_serves_users_with_a_users_block_and_a_socket() {
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
users:
  file: /etc/tddy/users.yaml
services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    user: tddy
    serves_users: true
    socket:
      path: /run/tddy-daemon.sock
";
        assert_eq!(
            SupervisorConfig::from_yaml(yaml).map(|c| c.services[0].serves_users),
            Ok(true)
        );
    }
}
