//! Where this daemon reads its GitHub → OS-user mapping from (`#agent-tools` 31/32): `users.yaml`,
//! never `daemon.yaml` `users:`.
//!
//! **Supervised** (`./install --systemd`): the root `tddy-supervisor` owns the file and tells the
//! daemon its path in [`USERS_FILE_ENV`] at exec. The daemon reads it, refuses one that is not
//! root-owned or that group or others could write, and never writes it — a mapping change is the
//! supervisor's, and restarts this daemon.
//!
//! **Unsupervised** (desktop, `--user`, dev): the file sits beside the daemon's own config
//! ([`USERS_FILE_NAME`]), and first-login enrolment writes its first row.
//!
//! A missing file is an error naming it, not an empty mapping: the installers and dev scripts
//! render one.
//!
//! [`DaemonConfig::users`](crate::config::DaemonConfig::users) stays the runtime holder every
//! lookup reads; this module only decides where it is filled from.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::live_users::LiveUsers;

/// The variable the supervisor names `users.yaml` in. `tddy_supervisor::spawn_broker::USERS_FILE_ENV`
/// writes it; a test pins the two spellings together.
pub const USERS_FILE_ENV: &str = "TDDY_USERS_FILE";

/// The file's name beside an unsupervised daemon's config.
pub const USERS_FILE_NAME: &str = "users.yaml";

/// Which `users.yaml` this daemon reads, and whether it may write it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsersFileSource {
    /// Named by the supervisor at exec: root-owned, read-only to this daemon.
    HandedBySupervisor(PathBuf),
    /// Beside the daemon's own config file: this daemon's to enrol into.
    BesideConfig(PathBuf),
}

impl UsersFileSource {
    /// The source for a daemon started from `config_path`, given the value of [`USERS_FILE_ENV`]
    /// it inherited (`None` when it was not supervised).
    pub fn resolve(config_path: &Path, handed: Option<&OsStr>) -> UsersFileSource {
        if let Some(handed) = handed {
            if !handed.is_empty() {
                return UsersFileSource::HandedBySupervisor(PathBuf::from(handed));
            }
        }
        // Beside the config file itself, by name: `with_file_name` keeps whatever directory the
        // config path carries (and for a bare file name, that name's own directory) instead of
        // guessing one relative to the working directory.
        UsersFileSource::BesideConfig(config_path.with_file_name(USERS_FILE_NAME))
    }

    /// [`Self::resolve`] over this process's own environment.
    pub fn from_environment(config_path: &Path) -> UsersFileSource {
        Self::resolve(config_path, std::env::var_os(USERS_FILE_ENV).as_deref())
    }

    /// The file.
    pub fn path(&self) -> &Path {
        match self {
            UsersFileSource::HandedBySupervisor(path) | UsersFileSource::BesideConfig(path) => path,
        }
    }
}

/// Read the mapping from `source`. A supervised source is checked root-owned and closed to group
/// and others, and yields a holder that refuses enrolment
/// ([`LiveUsers::owned_by_supervisor`]).
pub fn load_users_file(source: &UsersFileSource) -> anyhow::Result<LiveUsers> {
    use std::os::unix::fs::MetadataExt;

    let path = source.path();
    let contents = std::fs::read_to_string(path).map_err(|error| {
        anyhow::anyhow!("could not read users.yaml at {}: {error}", path.display())
    })?;
    require_root_owned_users_file(source, path)?;
    let mapping = tddy_users_file::UsersMapping::from_yaml(&contents)
        .map_err(|refusal| anyhow::anyhow!("users.yaml at {}: {refusal}", path.display()))?;
    let rows: Vec<crate::config::access::UserMapping> = mapping
        .distinct_rows()
        .into_iter()
        .map(|row| crate::config::access::UserMapping {
            github_user: row.github_user,
            os_user: row.os_user,
        })
        .collect();
    Ok(match source {
        UsersFileSource::HandedBySupervisor(_) => LiveUsers::owned_by_supervisor(rows),
        UsersFileSource::BesideConfig(_) => LiveUsers::new(rows),
    })
}

fn require_root_owned_users_file(source: &UsersFileSource, path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::MetadataExt;

    if let UsersFileSource::HandedBySupervisor(_) = source {
        let metadata = std::fs::metadata(path)
            .map_err(|error| anyhow::anyhow!("users.yaml at {}: {error}", path.display()))?;
        if metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            anyhow::bail!(
                "users.yaml at {} is not root-owned and closed to group and others \
                 (owner uid {}, mode {:o}); a supervised daemon trusts only a mapping root wrote",
                path.display(),
                metadata.uid(),
                metadata.mode() & 0o7777
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::config::access::UserMapping;

    fn is_root() -> bool {
        // SAFETY: reads this process's own effective uid.
        unsafe { libc::geteuid() == 0 }
    }

    struct ADeployment {
        dir: tempfile::TempDir,
    }

    impl ADeployment {
        fn with_daemon_yaml() -> ADeployment {
            let dir = tempfile::tempdir().expect("a temporary directory");
            std::fs::write(dir.path().join("daemon.yaml"), "repos_base_path: repos\n")
                .expect("write daemon.yaml");
            ADeployment { dir }
        }

        fn config_path(&self) -> PathBuf {
            self.dir.path().join("daemon.yaml")
        }

        fn users_yaml(&self) -> PathBuf {
            self.dir.path().join(USERS_FILE_NAME)
        }

        fn mapping(self, rows: &str) -> ADeployment {
            std::fs::write(self.users_yaml(), rows).expect("write users.yaml");
            std::fs::set_permissions(self.users_yaml(), std::fs::Permissions::from_mode(0o644))
                .expect("narrow users.yaml");
            self
        }
    }

    const THE_OPERATOR_AS_OPS: &str = "- {github_user: operator, os_user: ops}\n";

    #[test]
    fn an_unsupervised_daemon_reads_users_yaml_beside_its_config() {
        // Given a desktop deployment whose users.yaml maps its operator
        let deployment = ADeployment::with_daemon_yaml().mapping(THE_OPERATOR_AS_OPS);

        // When the daemon resolves and reads its mapping, not having been handed one
        let source = UsersFileSource::resolve(&deployment.config_path(), None);
        let users = load_users_file(&source).expect("the mapping loads");

        // Then it is the file beside its config
        assert_eq!(
            (source, users.snapshot()),
            (
                UsersFileSource::BesideConfig(deployment.users_yaml()),
                vec![UserMapping {
                    github_user: "operator".to_string(),
                    os_user: "ops".to_string(),
                }]
            )
        );
    }

    #[test]
    fn a_supervised_daemon_reads_the_users_yaml_it_was_handed() {
        // Given the supervisor named /etc/tddy/users.yaml at exec
        let deployment = ADeployment::with_daemon_yaml();
        let handed = Path::new("/etc/tddy/users.yaml");

        // When
        let source = UsersFileSource::resolve(&deployment.config_path(), Some(handed.as_os_str()));

        // Then it reads that file, not one beside its config
        assert_eq!(
            source,
            UsersFileSource::HandedBySupervisor(handed.to_path_buf())
        );
    }

    #[test]
    fn a_supervised_daemon_refuses_a_users_yaml_root_does_not_own() {
        if is_root() {
            // Every file this test can create is root's; the refusal is unobservable as root.
            return;
        }
        // Given a users.yaml owned by the account running the test, handed over as the mapping
        let deployment = ADeployment::with_daemon_yaml().mapping(THE_OPERATOR_AS_OPS);

        // When the daemon reads it as supervised
        let refusal = load_users_file(&UsersFileSource::HandedBySupervisor(
            deployment.users_yaml(),
        ))
        .expect_err("a mapping root does not own must not be trusted");

        // Then the refusal names the file and why
        let refusal = format!("{refusal:#}");
        assert!(
            refusal.contains("users.yaml") && refusal.contains("root"),
            "got {refusal}"
        );
    }

    #[test]
    fn a_missing_users_yaml_fails_loading_naming_it() {
        // Given a deployment with no users.yaml
        let deployment = ADeployment::with_daemon_yaml();

        // When
        let refusal = load_users_file(&UsersFileSource::BesideConfig(deployment.users_yaml()))
            .expect_err("a missing mapping is not an empty one");

        // Then
        assert!(
            format!("{refusal:#}").contains("users.yaml"),
            "got {refusal:#}"
        );
    }

    // `#agent-tools` 33/33 (the sweep): the daemon validates users.yaml through `tddy-users-file`,
    // the same checks the supervisor makes, so a mapping one reader refuses the other refuses too.

    fn the_refusal_of_a_desktop_mapping(rows: &str) -> String {
        let deployment = ADeployment::with_daemon_yaml().mapping(rows);
        let refusal = load_users_file(&UsersFileSource::BesideConfig(deployment.users_yaml()))
            .expect_err("the mapping must be refused");
        format!("{refusal:#}")
    }

    #[test]
    fn a_daemon_refuses_a_users_yaml_whose_os_user_cannot_name_a_socket() {
        // Given / When
        let refusal =
            the_refusal_of_a_desktop_mapping("- {github_user: operator, os_user: '../a:b'}\n");

        // Then
        assert!(
            refusal.contains("users.yaml") && refusal.contains("../a:b"),
            "got {refusal}"
        );
    }

    #[test]
    fn a_daemon_refuses_a_users_yaml_naming_more_os_users_than_the_supervisor_can_hand_over() {
        // Given sixteen distinct OS users
        let rows: String = (0..16)
            .map(|n| format!("- {{github_user: login{n}, os_user: u{n}}}\n"))
            .collect();

        // When
        let refusal = the_refusal_of_a_desktop_mapping(&rows);

        // Then
        assert!(
            refusal.contains("users.yaml") && refusal.contains("15"),
            "got {refusal}"
        );
    }

    #[test]
    fn a_daemon_refuses_a_users_yaml_row_that_names_no_github_user() {
        // Given / When
        let refusal = the_refusal_of_a_desktop_mapping("- {github_user: '', os_user: ops}\n");

        // Then
        assert!(
            refusal.contains("users.yaml") && refusal.contains("github_user"),
            "got {refusal}"
        );
    }
}
