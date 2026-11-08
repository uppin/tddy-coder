//! A desktop deployment learns who its operator is by them signing in once.
//!
//! `users.yaml` (`#agent-tools` 31/32; `daemon.yaml` `users:` before it) is what maps a GitHub login to the OS user its sessions run as, and a login that is
//! not in it is refused `permission_denied` with no default arm — the check that stops an
//! arbitrary GitHub account driving somebody else's machine. A server operator writes that map
//! when they install the daemon.
//!
//! `./install --desktop` has nobody to write it. It renders a config with `users:` unset, so the
//! first login is refused and the application opens on a settings screen it cannot get past.
//! Enrolment closes that gap **without** loosening the check: the first login on a deployment that
//! has never had one is written down, and every login after that meets the unchanged lookup.

use tddy_daemon_kernel::first_login_enrolment::{enrol_first_login, EnrolmentRefusal};
use tddy_daemon_kernel::live_users::LiveUsers;
use tddy_daemon_kernel::users_file::{load_users_file, UsersFileSource, USERS_FILE_NAME};

const THE_OPERATOR: &str = "operator";
const THE_OS_USER: &str = "operator-os";
const SOMEBODY_ELSE: &str = "a-stranger";

/// What the daemon.yaml of every deployment here says, besides any mapping.
const THE_DAEMON_YAML: &str =
    "repos_base_path: \"/tmp/repos\"\ngithub:\n  client_id: \"Iv1.0123456789abcdef\"\n";

/// A desktop deployment on disk (`#agent-tools` 31/32): its daemon.yaml, and a `users.yaml` beside
/// it mapping exactly the given GitHub users.
struct ADesktop {
    dir: tempfile::TempDir,
}

fn a_desktop_enrolling(users: &[(&str, &str)]) -> ADesktop {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(dir.path().join("daemon.yaml"), THE_DAEMON_YAML).expect("write daemon.yaml");
    let rows: String = users
        .iter()
        .map(|(github_user, os_user)| {
            format!("- github_user: \"{github_user}\"\n  os_user: \"{os_user}\"\n")
        })
        .collect();
    let rows = if rows.is_empty() {
        "[]\n".to_string()
    } else {
        rows
    };
    std::fs::write(dir.path().join(USERS_FILE_NAME), rows).expect("write users.yaml");
    ADesktop { dir }
}

impl ADesktop {
    fn users_yaml(&self) -> std::path::PathBuf {
        self.dir.path().join(USERS_FILE_NAME)
    }

    fn daemon_yaml(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("daemon.yaml")).expect("read daemon.yaml")
    }

    /// The mapping as the daemon reads it on its next start.
    fn mapping_on_restart(&self) -> LiveUsers {
        load_users_file(&UsersFileSource::BesideConfig(self.users_yaml()))
            .expect("users.yaml loads")
    }
}

#[test]
fn the_first_login_on_an_unenrolled_deployment_is_written_down() {
    // Given a freshly installed desktop deployment, which maps nobody
    let desktop = a_desktop_enrolling(&[]);

    // When its operator signs in for the first time
    enrol_first_login(&desktop.users_yaml(), THE_OPERATOR, THE_OS_USER)
        .expect("a first login is enrolled");

    // Then the daemon resolves them to the account it runs as, from its next start on
    assert_eq!(
        desktop
            .mapping_on_restart()
            .os_user_for_github(THE_OPERATOR)
            .as_deref(),
        Some(THE_OS_USER)
    );
}

#[test]
fn first_login_enrolment_on_desktop_writes_users_yaml_and_no_daemon_yaml_users_key() {
    // Given an unenrolled desktop
    let desktop = a_desktop_enrolling(&[]);

    // When its first login is enrolled
    enrol_first_login(&desktop.users_yaml(), THE_OPERATOR, THE_OS_USER)
        .expect("a first login is enrolled");

    // Then the row is in users.yaml, and daemon.yaml is exactly as it was
    assert_eq!(
        (
            desktop.mapping_on_restart().first_github_user(),
            desktop.daemon_yaml()
        ),
        (Some(THE_OPERATOR.to_string()), THE_DAEMON_YAML.to_string())
    );
}

#[test]
fn an_enrolled_deployment_refuses_to_enrol_a_second_account() {
    // Given a deployment already enrolled to its operator
    let desktop = a_desktop_enrolling(&[(THE_OPERATOR, THE_OS_USER)]);

    // When a different GitHub account signs in
    let refusal = enrol_first_login(&desktop.users_yaml(), SOMEBODY_ELSE, THE_OS_USER);

    // Then it is refused, and the deployment still maps only the operator
    assert_eq!(
        (
            refusal,
            desktop
                .mapping_on_restart()
                .os_user_for_github(SOMEBODY_ELSE)
        ),
        (
            Err(EnrolmentRefusal::AlreadyEnrolled {
                github_user: THE_OPERATOR.to_string(),
            }),
            None,
        ),
        "signing in is not how a second GitHub account is added"
    );
}

/// The check enrolment exists to work *with* rather than around. Green on arrival in its old
/// form; red now only because the mapping is read from users.yaml.
#[test]
fn an_unmapped_login_still_resolves_to_nobody() {
    // Given an enrolled deployment
    let desktop = a_desktop_enrolling(&[(THE_OPERATOR, THE_OS_USER)]);
    let users = desktop.mapping_on_restart();

    // When the operator and some other GitHub account are looked up
    let resolved = (
        users.os_user_for_github(THE_OPERATOR),
        users.os_user_for_github(SOMEBODY_ELSE),
    );

    // Then only the operator maps — there is no default account a stranger lands on
    assert_eq!(resolved, (Some(THE_OS_USER.to_string()), None));
}
