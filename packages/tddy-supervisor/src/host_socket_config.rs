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
