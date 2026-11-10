//! `users.yaml`: the GitHub login → OS user mapping, and the one validation both of its readers
//! apply (`#agent-tools` 33/33, the sweep).
//!
//! Two processes read the file: the root `tddy-supervisor`, which uses only the `os_user` column
//! (which OS users sessions may run as, which per-user daemon sockets exist), and the daemon, which
//! uses the logins. Both parse through [`UsersMapping::from_yaml`], so neither accepts a mapping the
//! other refuses: an `os_user` that cannot name a socket, more OS users than the handed-over
//! descriptors hold, or a row with no login.
//!
//! **What this crate is not.** It reads no file and checks no ownership: the two readers trust
//! different owners (the supervisor accepts root or its own uid and checks every directory above the
//! file; a supervised daemon accepts root alone, for the file only), so each keeps its own trust
//! check and hands this crate the text. It names no coding agent and no daemon type, because the
//! supervisor links it.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// The most distinct OS users one mapping may name: each costs one handed-over descriptor
/// (`user.<os user>`), and of the supervisor's sixteen slots descriptor 3 is the daemon's own
/// listener.
pub const MAX_MAPPED_OS_USERS: usize = 15;

/// One row of `users.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappedUser {
    pub github_user: String,
    pub os_user: String,
}

/// Why a mapping was refused. The `Display` text never names the file's path; each reader prefixes
/// its own (`users.yaml: …`, `users.yaml at <path>: …`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsersFileRefusal {
    /// Not a YAML list of `{github_user, os_user}` rows; the parser's own message.
    Malformed(String),
    /// A row with an empty `github_user`.
    NoGithubUser,
    /// A row mapping a login to `root`: the supervisor never runs a session as root.
    MapsToRoot,
    /// An `os_user` that is not one plain path and descriptor-name segment.
    UnsafeOsUser(String),
    /// One login mapped to two different OS users.
    TwoOsUsers {
        github_user: String,
        first: String,
        second: String,
    },
    /// More than [`MAX_MAPPED_OS_USERS`] distinct OS users.
    TooManyOsUsers(usize),
}

impl std::fmt::Display for UsersFileRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsersFileRefusal::Malformed(error) => write!(f, "{error}"),
            UsersFileRefusal::NoGithubUser => write!(f, "a row names no github_user"),
            UsersFileRefusal::MapsToRoot => write!(
                f,
                "a mapping to `root` is refused; the supervisor never runs a session as root"
            ),
            UsersFileRefusal::UnsafeOsUser(os_user) => write!(
                f,
                "os_user `{os_user}` cannot name a socket: it is not one plain path and \
                 descriptor-name segment"
            ),
            UsersFileRefusal::TwoOsUsers {
                github_user,
                first,
                second,
            } => write!(
                f,
                "github_user `{github_user}` is mapped to two OS users (`{first}` and \
                 `{second}`); which one its sessions run as is ambiguous"
            ),
            UsersFileRefusal::TooManyOsUsers(count) => write!(
                f,
                "users.yaml names {count} distinct OS users; at most {MAX_MAPPED_OS_USERS} \
                 fit the handed-over descriptors"
            ),
        }
    }
}

impl std::error::Error for UsersFileRefusal {}

/// A validated mapping, rows in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsersMapping {
    rows: Vec<MappedUser>,
}

impl UsersMapping {
    /// Parse and validate `users.yaml` text. Blank text is an empty mapping.
    pub fn from_yaml(yaml: &str) -> Result<UsersMapping, UsersFileRefusal> {
        let rows: Vec<MappedUser> = if yaml.trim().is_empty() {
            Vec::new()
        } else {
            serde_yaml::from_str(yaml)
                .map_err(|error| UsersFileRefusal::Malformed(error.to_string()))?
        };
        let mut by_login: std::collections::BTreeMap<&str, &str> =
            std::collections::BTreeMap::new();
        for row in &rows {
            if row.github_user.is_empty() {
                return Err(UsersFileRefusal::NoGithubUser);
            }
            if row.os_user == "root" {
                return Err(UsersFileRefusal::MapsToRoot);
            }
            if !is_one_safe_segment(&row.os_user) {
                return Err(UsersFileRefusal::UnsafeOsUser(row.os_user.clone()));
            }
            match by_login.entry(row.github_user.as_str()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(row.os_user.as_str());
                }
                std::collections::btree_map::Entry::Occupied(entry) => {
                    if *entry.get() != row.os_user {
                        return Err(UsersFileRefusal::TwoOsUsers {
                            github_user: row.github_user.clone(),
                            first: (*entry.get()).to_string(),
                            second: row.os_user.clone(),
                        });
                    }
                }
            }
        }
        let mapping = UsersMapping { rows };
        let distinct = mapping.os_users().len();
        if distinct > MAX_MAPPED_OS_USERS {
            return Err(UsersFileRefusal::TooManyOsUsers(distinct));
        }
        Ok(mapping)
    }

    /// Every row, in file order, an identical repeated row included.
    pub fn rows(&self) -> &[MappedUser] {
        &self.rows
    }

    /// The rows with each identical repeat dropped, first occurrence kept — what the daemon holds.
    pub fn distinct_rows(&self) -> Vec<MappedUser> {
        let mut seen: std::collections::BTreeSet<(String, String)> =
            std::collections::BTreeSet::new();
        self.rows
            .iter()
            .filter(|row| seen.insert((row.github_user.clone(), row.os_user.clone())))
            .cloned()
            .collect()
    }

    /// The distinct OS users the mapping names, sorted.
    pub fn os_users(&self) -> BTreeSet<String> {
        self.rows.iter().map(|row| row.os_user.clone()).collect()
    }
}

/// Whether `name` can ride in a descriptor name and a path segment unchanged.
pub fn is_one_safe_segment(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

#[cfg(test)]
mod tests {
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

    fn the_refusal_of(yaml: &str) -> UsersFileRefusal {
        UsersMapping::from_yaml(yaml).expect_err("the mapping must be refused")
    }

    #[test]
    fn blank_text_is_an_empty_mapping() {
        // Given / When
        let mapping = UsersMapping::from_yaml("  \n").expect("blank text loads");

        // Then
        assert_eq!(mapping, UsersMapping::default());
    }

    #[test]
    fn reads_each_row_in_order_and_the_distinct_os_users_it_names() {
        // Given two logins mapped to alice and one to bob
        let yaml = "- {github_user: a1, os_user: alice}\n\
                    - {github_user: a2, os_user: alice}\n\
                    - {github_user: b1, os_user: bob}\n";

        // When
        let mapping = UsersMapping::from_yaml(yaml).expect("a valid mapping loads");

        // Then
        assert_eq!(
            (
                mapping
                    .rows()
                    .iter()
                    .map(|row| row.github_user.as_str())
                    .collect::<Vec<_>>(),
                mapping.os_users().into_iter().collect::<Vec<_>>()
            ),
            (
                vec!["a1", "a2", "b1"],
                vec!["alice".to_string(), "bob".to_string()]
            )
        );
    }

    #[test]
    fn an_identical_repeated_row_is_one_distinct_row() {
        // Given the same row twice
        let yaml = "- {github_user: a1, os_user: alice}\n\
                    - {github_user: a1, os_user: alice}\n";

        // When
        let mapping = UsersMapping::from_yaml(yaml).expect("a repeated identical row loads");

        // Then
        assert_eq!(
            (mapping.rows().len(), mapping.distinct_rows().len()),
            (2, 1)
        );
    }

    #[test]
    fn refuses_text_that_is_not_a_list_of_rows() {
        assert!(matches!(
            the_refusal_of("- {github_user: a1, os_user: alice, shell: zsh}\n"),
            UsersFileRefusal::Malformed(_)
        ));
    }

    #[test]
    fn refuses_a_row_with_no_github_user() {
        assert_eq!(
            the_refusal_of("- {github_user: '', os_user: alice}\n"),
            UsersFileRefusal::NoGithubUser
        );
    }

    #[test]
    fn refuses_a_mapping_to_root() {
        assert_eq!(
            the_refusal_of("- {github_user: a1, os_user: root}\n"),
            UsersFileRefusal::MapsToRoot
        );
    }

    #[test]
    fn refuses_an_os_user_that_cannot_name_a_socket() {
        assert_eq!(
            the_refusal_of("- {github_user: a1, os_user: '../a:b'}\n"),
            UsersFileRefusal::UnsafeOsUser("../a:b".to_string())
        );
    }

    #[test]
    fn refuses_one_login_mapped_to_two_os_users() {
        assert_eq!(
            the_refusal_of(
                "- {github_user: a, os_user: alice}\n\
                 - {github_user: a, os_user: bob}\n"
            ),
            UsersFileRefusal::TwoOsUsers {
                github_user: "a".to_string(),
                first: "alice".to_string(),
                second: "bob".to_string(),
            }
        );
    }

    #[test]
    fn admits_fifteen_distinct_os_users_and_refuses_sixteen() {
        // Given
        let fifteen: Vec<String> = (0..15).map(|n| format!("u{n}")).collect();
        let sixteen: Vec<String> = (0..16).map(|n| format!("u{n}")).collect();
        let fifteen: Vec<&str> = fifteen.iter().map(String::as_str).collect();
        let sixteen: Vec<&str> = sixteen.iter().map(String::as_str).collect();

        // When
        let admitted = UsersMapping::from_yaml(&a_mapping_of(&fifteen));
        let refused = UsersMapping::from_yaml(&a_mapping_of(&sixteen));

        // Then
        assert_eq!(
            (admitted.map(|m| m.os_users().len()), refused),
            (
                Ok(MAX_MAPPED_OS_USERS),
                Err(UsersFileRefusal::TooManyOsUsers(16))
            )
        );
    }

    #[test]
    fn a_refusal_reads_as_the_supervisor_words_it_today() {
        // Given / When
        let words = [
            UsersFileRefusal::NoGithubUser.to_string(),
            UsersFileRefusal::MapsToRoot.to_string(),
            UsersFileRefusal::UnsafeOsUser("../a:b".to_string()).to_string(),
            UsersFileRefusal::TooManyOsUsers(16).to_string(),
        ];

        // Then — the reasons, without any file prefix
        assert_eq!(
            words,
            [
                "a row names no github_user".to_string(),
                "a mapping to `root` is refused; the supervisor never runs a session as root"
                    .to_string(),
                "os_user `../a:b` cannot name a socket: it is not one plain path and \
                 descriptor-name segment"
                    .to_string(),
                "users.yaml names 16 distinct OS users; at most 15 fit the handed-over descriptors"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn a_safe_segment_is_plain_ascii_and_not_hidden() {
        assert_eq!(
            ["alice", "a.b-c_d", "", ".hidden", "a/b", "a:b"].map(is_one_safe_segment),
            [true, true, false, false, false, false]
        );
    }
}
