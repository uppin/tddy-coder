//! A desktop deployment learns who its operator is by them signing in once.
//!
//! `users:` is what maps a GitHub login to the OS user its sessions run as, and a login that is
//! not in it is refused `permission_denied` with no default arm — the check that stops an
//! arbitrary GitHub account driving somebody else's machine. A server operator writes that map
//! when they install the daemon.
//!
//! `./install --desktop` has nobody to write it. It renders a config with `users:` unset, so the
//! first login is refused and the application opens on a settings screen it cannot get past.
//! Enrolment closes that gap **without** loosening the check: the first login on a deployment that
//! has never had one is written down, and every login after that meets the unchanged lookup.

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::first_login_enrolment::{enrol_first_login, EnrolmentRefusal};

const THE_OPERATOR: &str = "operator";
const THE_OS_USER: &str = "operator-os";
const SOMEBODY_ELSE: &str = "a-stranger";

#[test]
fn the_first_login_on_an_unenrolled_deployment_is_written_down() {
    // Given a freshly installed desktop deployment, which maps nobody
    let (path, _dir) = a_config_enrolling(&[]);

    // When its operator signs in for the first time
    enrol_first_login(&path, THE_OPERATOR, THE_OS_USER).expect("a first login is enrolled");

    // Then the daemon resolves them to the account it runs as, from that moment on
    let reloaded = DaemonConfig::load(&path).expect("the rewritten config loads");
    assert_eq!(
        reloaded.os_user_for_github(THE_OPERATOR).as_deref(),
        Some(THE_OS_USER)
    );
}

#[test]
fn an_enrolled_deployment_refuses_to_enrol_a_second_account() {
    // Given a deployment already enrolled to its operator
    let (path, _dir) = a_config_enrolling(&[(THE_OPERATOR, THE_OS_USER)]);

    // When a different GitHub account signs in
    let refusal = enrol_first_login(&path, SOMEBODY_ELSE, THE_OS_USER);

    // Then it is refused, and the deployment still maps only the operator
    let reloaded = DaemonConfig::load(&path).expect("the untouched config loads");
    assert_eq!(
        (refusal, reloaded.os_user_for_github(SOMEBODY_ELSE)),
        (
            Err(EnrolmentRefusal::AlreadyEnrolled {
                github_user: THE_OPERATOR.to_string(),
            }),
            None,
        ),
        "signing in is not how a second GitHub account is added"
    );
}

#[test]
fn enrolment_keeps_everything_else_the_config_says() {
    // Given an unenrolled deployment that is configured in other ways
    let (path, _dir) = a_config_enrolling(&[]);
    let before = DaemonConfig::load(&path).expect("the config loads");

    // When its first login is enrolled
    enrol_first_login(&path, THE_OPERATOR, THE_OS_USER).expect("a first login is enrolled");

    // Then only `users:` changed — enrolment rewrites the file it was read from
    let after = DaemonConfig::load(&path).expect("the rewritten config loads");
    assert_eq!(
        (
            after.github.as_ref().and_then(|g| g.client_id.clone()),
            after.repos_base_path.clone(),
        ),
        (
            before.github.as_ref().and_then(|g| g.client_id.clone()),
            before.repos_base_path.clone(),
        ),
        "enrolment must not lose the rest of the configuration"
    );
}

/// A daemon config file mapping exactly the given GitHub users, and configured besides.
fn a_config_enrolling(users: &[(&str, &str)]) -> (std::path::PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let mapped: String = users
        .iter()
        .map(|(github_user, os_user)| {
            format!("  - github_user: \"{github_user}\"\n    os_user: \"{os_user}\"\n")
        })
        .collect();
    let yaml = format!(
        "repos_base_path: \"/tmp/repos\"\nusers:\n{mapped}\
         github:\n  client_id: \"Iv1.0123456789abcdef\"\n"
    );
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).expect("the config is written");
    (path, dir)
}

/// The check enrolment exists to work *with* rather than around. Green on arrival, and deliberately
/// so: it is the guard that catches enrolment being turned into the default arm it must never be.
#[test]
fn an_unmapped_login_still_resolves_to_nobody() {
    // Given an enrolled deployment
    let (path, _dir) = a_config_enrolling(&[(THE_OPERATOR, THE_OS_USER)]);
    let config = DaemonConfig::load(&path).expect("the config loads");

    // When some other GitHub account is looked up
    let resolved = config.os_user_for_github(SOMEBODY_ELSE);

    // Then it maps to nobody — there is no default account a stranger lands on
    assert_eq!(resolved, None);
}
