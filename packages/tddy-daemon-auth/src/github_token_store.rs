//! File-backed retention of each operator's GitHub access token, rooted at the `auth_storage`
//! config path.
//!
//! The daemon runs from a systemd unit with no `GITHUB_TOKEN` in its environment, so the only
//! credential it can use for an operator's PR reads is the one that operator granted at login. It
//! outlives a daemon restart (the web login does not re-run on restart), so it is persisted — as a
//! single `0600` JSON file of `login -> access_token`.
//!
//! Every write goes through [`tddy_core::atomic_file::write_atomic_with_mode`], which is the one
//! place in the tree that knows how to replace a file without the old contents ever being at risk.
//! This store used to stage and rename by hand, with its own `O_TRUNC` open in the middle of it;
//! `docs/dev/todo/2026-08-16-the-daemon-s-secret-stores-still-truncate-in-place.md` records why a
//! second implementation of that dance is worse than no second implementation, and the mode-aware
//! variant exists precisely because a secret's *first* write must not land at the process umask.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use tddy_core::atomic_file::write_atomic_with_mode;
use tddy_github::token_store::GitHubTokenStore;

/// Basename of the token file inside the `auth_storage` directory.
const TOKENS_FILE: &str = "github-tokens.json";

/// Basename of the file [`FileGitHubTokenStore::probe_writable`] creates and removes.
const PROBE_FILE: &str = "github-tokens.probe";

/// Serialises every `put` in this process.
///
/// `put` is a read-modify-write of one shared file, so two concurrent logins would each read the
/// same map and the second write would drop the first operator's token. The lock is process-wide
/// rather than per-store because the store is constructed per `auth_storage` path and nothing
/// guarantees a single instance owns a path.
static PUT_LOCK: Mutex<()> = Mutex::new(());

/// Owner-only permissions: these are live GitHub credentials.
const OWNER_ONLY_FILE: u32 = 0o600;
const OWNER_ONLY_DIR: u32 = 0o700;

/// A `GitHubTokenStore` persisted under one directory (`auth_storage`).
pub struct FileGitHubTokenStore {
    storage_dir: PathBuf,
    tokens_path: PathBuf,
}

impl FileGitHubTokenStore {
    /// Store tokens in `TOKENS_FILE` under `auth_storage_dir`.
    pub fn new(auth_storage_dir: impl AsRef<Path>) -> Self {
        let storage_dir = auth_storage_dir.as_ref().to_path_buf();
        Self {
            tokens_path: storage_dir.join(TOKENS_FILE),
            storage_dir,
        }
    }

    /// The file the tokens are persisted in.
    #[must_use]
    pub fn tokens_path(&self) -> &Path {
        &self.tokens_path
    }

    /// Create the storage directory and prove a file can actually be written in it, removing the
    /// probe afterwards.
    ///
    /// Called once at daemon startup. An unwritable `auth_storage` fails *every* real login (a
    /// failed retention fails the login, PRD D13), so it has to be surfaced at boot rather than to
    /// the first operator who tries to sign in.
    pub fn probe_writable(&self) -> Result<(), String> {
        ensure_owner_only_dir(&self.storage_dir)?;
        let probe = self.storage_dir.join(PROBE_FILE);
        write_atomic_with_mode(&probe, b"", OWNER_ONLY_FILE)
            .map_err(|e| format!("writing {}: {e}", probe.display()))?;
        std::fs::remove_file(&probe).map_err(|e| format!("removing {}: {e}", probe.display()))
    }

    fn read_all(&self) -> HashMap<String, String> {
        // An absent file is an empty store; an unreadable or malformed one is reported by the caller
        // as "no token for this login", which surfaces as *unavailable* rather than as "no PR".
        match std::fs::read_to_string(&self.tokens_path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
                log::warn!(
                    target: "tddy_daemon::github_token_store",
                    "{} is not readable as a GitHub token map ({e}); treating it as empty",
                    self.tokens_path.display()
                );
                HashMap::new()
            }),
            // No file yet is the ordinary state before the first login — not worth a line.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            // Anything else (a permission error, an I/O fault) makes every retained token invisible
            // while the file itself is sitting right there, so it must not pass silently: it is the
            // only clue an operator diagnosing "PR status unavailable" would get.
            Err(e) => {
                log::warn!(
                    target: "tddy_daemon::github_token_store",
                    "{} could not be read ({e}); treating it as empty",
                    self.tokens_path.display()
                );
                HashMap::new()
            }
        }
    }
}

/// Create `dir` and its parents, owner-only from the moment they exist — they hold live GitHub
/// credentials.
///
/// The mode is given to the *creation* rather than applied afterwards, for the same reason
/// [`tddy_core::atomic_file`] passes it to `open`: a `create_dir_all` followed by `set_permissions`
/// leaves a window in which the directory is listable at the process umask. A directory that
/// already exists therefore keeps the mode it carries — the daemon creates the store's home, it
/// does not re-impose a mode on one an operator has already set.
fn ensure_owner_only_dir(dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(OWNER_ONLY_DIR)
            .create(dir)
            .map_err(|e| format!("creating {}: {e}", dir.display()))
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))
}

impl GitHubTokenStore for FileGitHubTokenStore {
    fn put(&self, login: &str, access_token: &str) -> Result<(), String> {
        // Held across the whole read-modify-write. A poisoned lock means an earlier `put` panicked;
        // the map it guards lives on disk and is re-read below, so there is no torn state to inherit.
        let _serialised = PUT_LOCK.lock().unwrap_or_else(PoisonError::into_inner);

        let mut tokens = self.read_all();
        tokens.insert(login.to_string(), access_token.to_string());
        let json = serde_json::to_string_pretty(&tokens)
            .map_err(|e| format!("serializing the GitHub token map: {e}"))?;

        ensure_owner_only_dir(&self.storage_dir)?;

        // The whole map is staged beside the real file and renamed into place, owner-only from the
        // moment the staging file exists. Anything that can fail — the allocation, the write, the
        // `fsync` — fails while only the staging file is at risk, so a crash or a full disk can
        // never leave a truncated token file behind. That matters more here than almost anywhere:
        // a truncated file parses as an *empty map*, which would take every operator's token away
        // at once and read to them as an ordinary "please sign in again".
        write_atomic_with_mode(&self.tokens_path, json.as_bytes(), OWNER_ONLY_FILE)
            .map_err(|e| format!("writing {}: {e}", self.tokens_path.display()))
    }

    fn get(&self, login: &str) -> Option<String> {
        self.read_all().get(login).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_the_token_it_retained_for_a_login() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let store = FileGitHubTokenStore::new(dir.path());

        // When
        store.put("operator", "gho_granted").unwrap();

        // Then
        assert_eq!(store.get("operator").as_deref(), Some("gho_granted"));
    }

    #[test]
    fn keeps_each_logins_token_separate() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let store = FileGitHubTokenStore::new(dir.path());
        store.put("alice", "gho_alice").unwrap();

        // When
        store.put("bob", "gho_bob").unwrap();

        // Then
        assert_eq!(
            (store.get("alice"), store.get("bob")),
            (Some("gho_alice".to_string()), Some("gho_bob".to_string()))
        );
    }

    #[test]
    fn replaces_a_logins_previous_token_on_a_fresh_login() {
        // Given — the operator signed in again, granting a newly scoped token
        let dir = tempfile::tempdir().unwrap();
        let store = FileGitHubTokenStore::new(dir.path());
        store.put("operator", "gho_old_read_user_only").unwrap();

        // When
        store.put("operator", "gho_new_with_repo_scope").unwrap();

        // Then
        assert_eq!(
            store.get("operator").as_deref(),
            Some("gho_new_with_repo_scope")
        );
    }

    #[test]
    fn holds_no_token_for_a_login_that_never_signed_in() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let store = FileGitHubTokenStore::new(dir.path());

        // When
        let token = store.get("stranger");

        // Then — the caller reports this as *unavailable*, never as "no PR exists"
        assert_eq!(token, None);
    }

    #[test]
    fn creates_the_storage_directory_when_it_does_not_exist_yet() {
        // Given — `auth_storage` points at a directory the daemon has not created yet
        let dir = tempfile::tempdir().unwrap();
        let store = FileGitHubTokenStore::new(dir.path().join("auth"));

        // When
        store.put("operator", "gho_granted").unwrap();

        // Then
        assert_eq!(store.get("operator").as_deref(), Some("gho_granted"));
    }

    #[cfg(unix)]
    #[test]
    fn writes_the_token_file_readable_only_by_its_owner() {
        // Given
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let store = FileGitHubTokenStore::new(dir.path());

        // When
        store.put("operator", "gho_granted").unwrap();

        // Then — these are live GitHub credentials
        let mode = std::fs::metadata(store.tokens_path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn retains_every_token_when_logins_are_concurrent() {
        // Given — four operators whose logins land on one store at the same moment
        let dir = tempfile::tempdir().unwrap();
        let store = FileGitHubTokenStore::new(dir.path());
        let logins = ["alice", "bob", "carol", "dave"];
        let at_once = std::sync::Barrier::new(logins.len());

        // When
        std::thread::scope(|scope| {
            let (shared_store, at_once) = (&store, &at_once);
            for login in logins {
                scope.spawn(move || {
                    at_once.wait();
                    shared_store.put(login, &format!("gho_{login}")).unwrap();
                });
            }
        });

        // Then — a lock-free read-modify-write lets the last writer's map overwrite the others'
        let retained: Vec<Option<String>> = logins.iter().map(|l| store.get(l)).collect();
        assert_eq!(
            retained,
            vec![
                Some("gho_alice".to_string()),
                Some("gho_bob".to_string()),
                Some("gho_carol".to_string()),
                Some("gho_dave".to_string()),
            ]
        );
    }

    #[test]
    fn probing_an_unwritable_storage_path_reports_it_rather_than_accepting_it() {
        // Given — `auth_storage` pointing inside a file, which no directory can be created under
        let dir = tempfile::tempdir().unwrap();
        let occupied = dir.path().join("not-a-directory");
        std::fs::write(&occupied, "").unwrap();
        let store = FileGitHubTokenStore::new(occupied.join("auth"));

        // When
        let probe = store.probe_writable();

        // Then — the daemon refuses to start on this, rather than failing the first real login
        assert!(
            probe.is_err(),
            "an unusable auth_storage path must be reported at boot, got: {probe:?}"
        );
    }

    #[test]
    fn probing_a_usable_storage_path_creates_it_and_leaves_no_probe_file_behind() {
        // Given — `auth_storage` pointing at a directory the daemon has not created yet
        let dir = tempfile::tempdir().unwrap();
        let store = FileGitHubTokenStore::new(dir.path().join("auth"));

        // When
        store.probe_writable().expect("the path should be usable");

        // Then — the storage directory exists and holds nothing but what a real login will write
        assert_eq!(
            std::fs::read_dir(dir.path().join("auth")).unwrap().count(),
            0
        );
    }

    #[test]
    fn reads_back_a_token_retained_by_an_earlier_daemon_process() {
        // Given — a login that happened before a restart
        let dir = tempfile::tempdir().unwrap();
        FileGitHubTokenStore::new(dir.path())
            .put("operator", "gho_granted")
            .unwrap();

        // When — a fresh store over the same auth_storage path
        let after_restart = FileGitHubTokenStore::new(dir.path());

        // Then — the operator does not have to sign in again
        assert_eq!(
            after_restart.get("operator").as_deref(),
            Some("gho_granted")
        );
    }

    /// ⛔ `docs/dev/todo/2026-08-16-the-daemon-s-secret-stores-still-truncate-in-place.md`.
    ///
    /// The store truncates in place, so a crash between truncate and write leaves a **truncated
    /// secret at rest**. This crate exists to be the identity boundary, so shipping it with that
    /// path at its centre would be worse than a slightly larger diff — and hand-rolling an atomic
    /// write beside `tddy_core::atomic_file::write_atomic` would deepen the duplication the entry is
    /// about. It is routed through the existing helper as part of the move.
    ///
    /// A read-only storage directory stands in for the failure: it is what a full filesystem does
    /// to the swap file the replacement is staged in, and it is the same stand-in
    /// `tddy_core::atomic_file`'s own disk-full test uses.
    #[cfg(unix)]
    #[test]
    fn a_failed_write_leaves_the_previous_secret_intact() {
        use std::os::unix::fs::PermissionsExt;

        // Given a store holding a token
        let dir = tempfile::tempdir().unwrap();
        let store = FileGitHubTokenStore::new(dir.path());
        store.put("alice", "the-original-token").unwrap();

        // When a write fails part-way — simulated by writing to a path made unwritable
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let unwritable = std::fs::File::create(dir.path().join(".probe")).is_err();
        let refused = store.put("alice", "a-replacement-that-never-lands");
        let survivor = store.get("alice");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();

        // Then the original is still readable, never a truncated prefix of either — asserted
        // first that the failure was actually injected, because root ignores the permission bits
        // and would otherwise let this test report `ok` while proving nothing.
        assert!(
            unwritable,
            "this test needs a non-root uid to inject the write failure"
        );
        assert!(
            refused.is_err(),
            "a write into a read-only directory must be reported, not swallowed"
        );
        assert_eq!(
            survivor.as_deref(),
            Some("the-original-token"),
            "a partial write must not destroy the value it was replacing"
        );
    }
}
