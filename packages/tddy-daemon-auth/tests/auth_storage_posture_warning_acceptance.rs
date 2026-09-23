//! A daemon says so, once, when the directory holding its signing key is more open than `0700`.
//!
//! The `auth_storage` directory holds the daemon's private signing key and every retained GitHub
//! token. Its mode governs who can list it, not who can read the `0600` files inside, so a looser
//! mode is warned about rather than refused — and not repaired, because an operator's `chmod` is
//! theirs to make. What must not happen is a key directory drifting open with nothing ever saying
//! so, or a warning repeated until nobody reads it.
//!
//! Driven through `build_auth_entries_with`, the startup path that owns the warning, and observed
//! through the `log` records it emits — this test binary installs a recorder of its own, and each
//! test counts only the records naming its own directory, so tests running side by side cannot see
//! each other's warnings.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, Mutex, Once, OnceLock};

use log::{Level, LevelFilter, Log, Metadata, Record};
use tddy_daemon_auth::{
    build_auth_entries_with, DaemonSigningKey, SessionTokens, StandaloneKeyDirectory,
    AUTH_LOG_TARGET, SIGNING_KEY_FILE,
};
use tddy_daemon_kernel::config::DaemonConfig;

#[test]
fn warns_exactly_once_at_startup_when_auth_storage_is_looser_than_owner_only() {
    // Given an auth_storage directory an operator left group- and world-listable
    let auth_storage = an_auth_storage_with_mode(0o755);

    // When the daemon builds its auth services
    starts_a_daemon_on(auth_storage.path());

    // Then exactly one warning names that directory and the mode to change
    let warnings = warnings_naming(auth_storage.path());
    assert_eq!(
        warnings.len(),
        1,
        "expected one startup warning about {}, got {warnings:?}",
        auth_storage.path().display()
    );
    assert!(
        warnings[0].contains("755"),
        "the warning must name the mode the operator has to change, got: {}",
        warnings[0]
    );
}

#[test]
fn says_nothing_when_auth_storage_is_owner_only() {
    // Given an owner-only auth_storage directory
    let auth_storage = an_auth_storage_with_mode(0o700);

    // When the daemon builds its auth services
    starts_a_daemon_on(auth_storage.path());

    // Then no warning names it — and the recorder was live, so silence is an observation
    assert!(recorder_is_live(), "the log recorder never saw a record");
    assert_eq!(warnings_naming(auth_storage.path()), Vec::<String>::new());
}

fn an_auth_storage_with_mode(mode: u32) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(mode))
        .expect("the directory's mode is set");
    dir
}

/// Build the daemon's auth entries the way startup does, for a config whose `auth_storage` is
/// `auth_storage`.
fn starts_a_daemon_on(auth_storage: &Path) {
    install_recorder();
    let config = a_config_with_auth_storage(auth_storage);
    let key = DaemonSigningKey::load_or_generate(&auth_storage.join(SIGNING_KEY_FILE))
        .expect("the daemon generates its keypair");
    let tokens = SessionTokens::new(&key, Arc::new(StandaloneKeyDirectory));
    build_auth_entries_with(&config, "127.0.0.1", 0, &tokens)
        .expect("the daemon's auth services build");
}

fn a_config_with_auth_storage(auth_storage: &Path) -> DaemonConfig {
    let yaml = format!(
        "users:\n  - github_user: \"u\"\n    os_user: \"u\"\ngithub:\n  stub: true\n\
         auth_storage: \"{}\"\n",
        auth_storage.display()
    );
    let config_dir = tempfile::tempdir().expect("a directory for the config file");
    let path = config_dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).expect("the config is written");
    DaemonConfig::load(&path).expect("the config fixture loads")
}

/// Every warning under the auth log target that names `dir`.
fn warnings_naming(dir: &Path) -> Vec<String> {
    let dir = dir.display().to_string();
    recorded()
        .lock()
        .expect("the recording is only held to read or append")
        .iter()
        .filter(|record| {
            record.level == Level::Warn
                && record.target == AUTH_LOG_TARGET
                && record.message.contains(&dir)
        })
        .map(|record| record.message.clone())
        .collect()
}

fn recorder_is_live() -> bool {
    !recorded()
        .lock()
        .expect("the recording is only held to read or append")
        .is_empty()
}

struct Recorded {
    level: Level,
    target: String,
    message: String,
}

fn recorded() -> &'static Mutex<Vec<Recorded>> {
    static RECORDED: OnceLock<Mutex<Vec<Recorded>>> = OnceLock::new();
    RECORDED.get_or_init(|| Mutex::new(Vec::new()))
}

/// Records every log record this process emits. `log` takes one logger per process, so it is
/// installed once for the binary; each test filters by its own directory.
struct Recorder;

impl Log for Recorder {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        recorded()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(Recorded {
                level: record.level(),
                target: record.target().to_string(),
                message: record.args().to_string(),
            });
    }

    fn flush(&self) {}
}

fn install_recorder() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        log::set_boxed_logger(Box::new(Recorder))
            .map(|()| log::set_max_level(LevelFilter::Trace))
            .expect("no other logger is installed in this test binary");
    });
}
