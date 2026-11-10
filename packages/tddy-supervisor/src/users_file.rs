//! `users.yaml`: the one place a GitHub login is mapped to the OS user its sessions run as
//! (`#agent-tools` 31/32).
//!
//! On a supervised host the root supervisor reads it, because it decides which OS users the
//! supervisor may run sessions as and which per-user daemon sockets exist — identity and
//! authorization, nothing specific to any coding agent. The daemon reads the same file for the
//! GitHub logins; the supervisor uses only the `os_user` column and hands the daemon the file's
//! path at exec (see [`crate::spawn_broker::USERS_FILE_ENV`]).
//!
//! **Trust.** Granting an OS user to a login is a privilege input of the root broker, so the file
//! is checked exactly like `supervisor.yaml`: owned by root (or by the supervisor's own uid, which
//! is root on a real host), writable by nobody else, in directories nobody else can write.
//!
//! **Budget.** Each distinct OS user costs one handed-over descriptor (`user.<os user>`), and
//! descriptor 3 is the daemon's own listener, so at most [`MAX_MAPPED_OS_USERS`] fit.
//!
//! Feature doc: `docs/ft/supervisor/tddy-supervisor.md`.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;
use crate::socket::MAX_HANDED_OVER_LISTENERS;

/// The file's conventional name, beside `supervisor.yaml` (`/etc/tddy/users.yaml`).
pub const USERS_FILE_NAME: &str = "users.yaml";

/// The most distinct OS users one mapping may name: every handed-over slot but descriptor 3.
pub const MAX_MAPPED_OS_USERS: usize = MAX_HANDED_OVER_LISTENERS - 1;

/// One row of `users.yaml`: the same shape `daemon.yaml` `users:` had.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappedUser {
    pub github_user: String,
    pub os_user: String,
}

/// Whether `name` can ride in a descriptor name and a path segment unchanged.
fn is_one_safe_segment(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// A validated `users.yaml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsersFile {
    rows: Vec<MappedUser>,
}

impl UsersFile {
    /// Read and validate `users.yaml` at `path`, refusing a file (or a directory above it) that
    /// anybody but the supervisor's own identity could write — the check
    /// [`crate::config::SupervisorConfig::load`] makes.
    pub fn load(path: &Path) -> Result<UsersFile, ConfigError> {
        let file = std::fs::File::open(path).map_err(|error| ConfigError {
            message: format!("could not read {}: {error}", path.display()),
        })?;
        let metadata = file.metadata().map_err(|error| ConfigError {
            message: format!("could not inspect {}: {error}", path.display()),
        })?;
        crate::config::require_trustworthy_config_file(path, &metadata)?;
        let resolved = std::fs::canonicalize(path).map_err(|error| ConfigError {
            message: format!("could not resolve {}: {error}", path.display()),
        })?;
        let mut ancestor = resolved.parent();
        while let Some(directory) = ancestor {
            let metadata = std::fs::metadata(directory).map_err(|error| ConfigError {
                message: format!("could not inspect {}: {error}", directory.display()),
            })?;
            crate::config::require_trustworthy_config_directory(directory, &metadata)?;
            ancestor = directory.parent();
        }
        let mut yaml = String::new();
        {
            use std::io::Read;
            let mut file = file;
            file.read_to_string(&mut yaml)
                .map_err(|error| ConfigError {
                    message: format!("could not read {}: {error}", path.display()),
                })?;
        }
        UsersFile::from_yaml(&yaml)
    }

    /// Parse and validate `users.yaml` text: a YAML list of [`MappedUser`] rows, no `root`, at most
    /// [`MAX_MAPPED_OS_USERS`] distinct OS users, each OS user one plain path and fd-name segment.
    pub fn from_yaml(yaml: &str) -> Result<UsersFile, ConfigError> {
        let rows: Vec<MappedUser> = if yaml.trim().is_empty() {
            Vec::new()
        } else {
            serde_yaml::from_str(yaml).map_err(|error| ConfigError {
                message: format!("users.yaml: {error}"),
            })?
        };
        let mut by_login: std::collections::BTreeMap<&str, &str> =
            std::collections::BTreeMap::new();
        for row in &rows {
            if row.github_user.is_empty() {
                return Err(ConfigError {
                    message: "users.yaml: a row names no github_user".to_string(),
                });
            }
            if row.os_user == "root" {
                return Err(ConfigError {
                    message: "users.yaml: a mapping to `root` is refused; the supervisor never \
                              runs a session as root"
                        .to_string(),
                });
            }
            if !is_one_safe_segment(&row.os_user) {
                return Err(ConfigError {
                    message: format!(
                        "users.yaml: os_user `{}` cannot name a socket: it is not one plain \
                         path and descriptor-name segment",
                        row.os_user
                    ),
                });
            }
            match by_login.entry(row.github_user.as_str()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(row.os_user.as_str());
                }
                std::collections::btree_map::Entry::Occupied(entry) => {
                    if *entry.get() != row.os_user {
                        return Err(ConfigError {
                            message: format!(
                                "users.yaml: github_user `{}` is mapped to two OS users \
                                 (`{}` and `{}`); which one its sessions run as is ambiguous",
                                row.github_user,
                                entry.get(),
                                row.os_user
                            ),
                        });
                    }
                }
            }
        }
        let users = UsersFile { rows };
        let distinct = users.os_users().len();
        if distinct > MAX_MAPPED_OS_USERS {
            return Err(ConfigError {
                message: format!(
                    "users.yaml names {distinct} distinct OS users; at most \
                     {MAX_MAPPED_OS_USERS} fit the handed-over descriptors"
                ),
            });
        }
        Ok(users)
    }

    /// Every row, in file order.
    pub fn rows(&self) -> &[MappedUser] {
        &self.rows
    }

    /// The distinct OS users the mapping names, sorted — the users sessions may run as and the
    /// users who get a daemon socket.
    pub fn os_users(&self) -> BTreeSet<String> {
        self.rows.iter().map(|row| row.os_user.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn a_mapping_of(os_users: &[&str]) -> String {
        os_users
            .iter()
            .enumerate()
            .map(|(index, os_user)| {
                format!("- {{github_user: login{index}, os_user: {os_user}}}\n")
            })
            .collect()
    }

    fn the_rejection_of(yaml: &str) -> String {
        UsersFile::from_yaml(yaml)
            .expect_err("the mapping must be refused")
            .message
    }

    #[test]
    fn reads_each_row_and_the_distinct_os_users_it_names() {
        // Given two logins mapped to alice and one to bob
        let yaml = "- {github_user: a1, os_user: alice}\n\
                    - {github_user: a2, os_user: alice}\n\
                    - {github_user: b1, os_user: bob}\n";

        // When
        let users = UsersFile::from_yaml(yaml).expect("a valid mapping loads");

        // Then
        assert_eq!(users.rows().len(), 3);
        assert_eq!(
            users.os_users().into_iter().collect::<Vec<_>>(),
            vec!["alice".to_string(), "bob".to_string()]
        );
    }

    #[test]
    fn fifteen_mapped_os_users_fit() {
        // Given
        let names: Vec<String> = (0..MAX_MAPPED_OS_USERS).map(|n| format!("u{n}")).collect();
        let yaml = a_mapping_of(&names.iter().map(String::as_str).collect::<Vec<_>>());

        // When
        let users = UsersFile::from_yaml(&yaml).expect("fifteen users fit");

        // Then
        assert_eq!(MAX_MAPPED_OS_USERS, 15);
        assert_eq!(users.os_users().len(), 15);
    }

    #[test]
    fn a_sixteenth_mapped_user_fails_loading_naming_users_yaml_and_the_limit() {
        // Given one OS user more than the descriptors left after the daemon's own listener
        let names: Vec<String> = (0..=MAX_MAPPED_OS_USERS).map(|n| format!("u{n}")).collect();
        let yaml = a_mapping_of(&names.iter().map(String::as_str).collect::<Vec<_>>());

        // When
        let rejection = the_rejection_of(&yaml);

        // Then
        assert!(
            rejection.contains("users.yaml") && rejection.contains("15"),
            "the refusal must name the file and the limit; got {rejection}"
        );
    }

    #[test]
    fn a_login_mapped_to_two_os_users_is_refused() {
        // Given one GitHub login mapped twice — which OS user its sessions run as is ambiguous
        let rejection = the_rejection_of(
            "- {github_user: a, os_user: alice}\n- {github_user: a, os_user: bob}\n",
        );

        // Then
        assert!(
            rejection.contains("users.yaml") && rejection.contains("`a`"),
            "got {rejection}"
        );
    }

    #[test]
    fn a_mapping_to_root_is_refused() {
        // Given
        let rejection = the_rejection_of("- {github_user: someone, os_user: root}\n");

        // Then
        assert!(rejection.contains("root"), "got {rejection}");
    }

    #[test]
    fn an_os_user_that_cannot_name_a_socket_is_refused() {
        // Given a name that would escape its socket directory and split LISTEN_FDNAMES
        let rejection = the_rejection_of("- {github_user: someone, os_user: \"../a:b\"}\n");

        // Then
        assert!(rejection.contains("../a:b"), "got {rejection}");
    }

    #[test]
    fn a_users_yaml_another_user_could_write_is_refused() {
        // Given a users.yaml anybody on the host may write
        let directory = tempfile::TempDir::new().expect("create a directory");
        let path = directory.path().join(USERS_FILE_NAME);
        std::fs::write(&path, "- {github_user: a, os_user: alice}\n").expect("write users.yaml");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666))
            .expect("widen users.yaml");

        // When
        let rejection = UsersFile::load(&path).expect_err("a world-writable mapping is refused");

        // Then
        assert!(
            rejection.message.contains("writable"),
            "got {}",
            rejection.message
        );
    }

    #[test]
    fn loads_a_users_yaml_only_the_supervisor_can_write() {
        // Given
        let directory = tempfile::TempDir::new().expect("create a directory");
        let path = directory.path().join(USERS_FILE_NAME);
        std::fs::write(&path, "- {github_user: a, os_user: alice}\n").expect("write users.yaml");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("narrow users.yaml");

        // When
        let users = UsersFile::load(&path).expect("a trusted mapping loads");

        // Then
        assert_eq!(
            users.rows(),
            &[MappedUser {
                github_user: "a".to_string(),
                os_user: "alice".to_string()
            }]
        );
    }

    /// `#agent-tools` 33/33: the shared users.yaml budget is the supervisor's own handover budget.
    #[test]
    fn the_shared_mapping_budget_is_every_handover_slot_but_the_daemons_own() {
        assert_eq!(
            (tddy_users_file::MAX_MAPPED_OS_USERS, MAX_MAPPED_OS_USERS),
            (MAX_HANDED_OVER_LISTENERS - 1, MAX_HANDED_OVER_LISTENERS - 1)
        );
    }
}
