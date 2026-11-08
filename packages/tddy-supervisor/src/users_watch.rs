//! Watching `users.yaml` (`#agent-tools` 31/32): a saved edit reaches the supervisor without a
//! restart of its own.
//!
//! Each change is re-read through [`UsersFile::load`] — the same trust and shape checks as at
//! start-up. A valid file becomes the current mapping and is reported to [`UsersWatchSink::changed`],
//! which rebuilds the per-user listeners and restarts `tddy-daemon`
//! ([`crate::supervisor::Supervisor::apply_users_change`]). An invalid one is reported to
//! [`UsersWatchSink::refused`] and logged, and the mapping in force stays as it was: a typo must
//! never take every user's socket away.
//!
//! Built on the `notify` crate (approved 2026-11-07). The watch is on the file's directory, so an
//! editor's write-to-temporary-then-rename is seen as the change it is.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::error::ConfigError;
use crate::users_file::UsersFile;

/// Where a watch reports what it read.
pub trait UsersWatchSink: Send + Sync + 'static {
    /// `users.yaml` changed and the new mapping is valid; it is now the current one.
    fn changed(&self, next: &UsersFile);
    /// `users.yaml` changed but was refused; the current mapping is unchanged.
    fn refused(&self, why: &ConfigError);
}

/// A running watch on one `users.yaml`. Dropping it stops the watch.
pub struct UsersWatch {
    path: PathBuf,
    current: Arc<Mutex<UsersFile>>,
}

impl UsersWatch {
    /// Start watching `path`, with `initial` — the mapping loaded at start-up — as the current one.
    pub fn spawn(
        path: &Path,
        initial: UsersFile,
        sink: Arc<dyn UsersWatchSink>,
    ) -> anyhow::Result<UsersWatch> {
        // TODO(user-mapping-in-supervisor): implement
        let _ = sink;
        Ok(UsersWatch {
            path: path.to_path_buf(),
            current: Arc::new(Mutex::new(initial)),
        })
    }

    /// The file being watched.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The mapping in force: the last valid one read.
    pub fn current(&self) -> UsersFile {
        self.current
            .lock()
            .expect("users watch lock poisoned")
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    /// How long a saved edit may take to be reported: file events are delivered asynchronously.
    const A_CHANGE_IS_SEEN_WITHIN: Duration = Duration::from_secs(10);

    #[derive(Debug)]
    enum Seen {
        Changed(UsersFile),
        Refused(String),
    }

    struct Recording(Mutex<mpsc::Sender<Seen>>);

    impl UsersWatchSink for Recording {
        fn changed(&self, next: &UsersFile) {
            let _ = self.0.lock().unwrap().send(Seen::Changed(next.clone()));
        }
        fn refused(&self, why: &ConfigError) {
            let _ = self
                .0
                .lock()
                .unwrap()
                .send(Seen::Refused(why.message.clone()));
        }
    }

    struct AWatchedFile {
        _directory: tempfile::TempDir,
        path: PathBuf,
        watch: UsersWatch,
        seen: mpsc::Receiver<Seen>,
    }

    impl AWatchedFile {
        fn mapping(yaml: &str) -> AWatchedFile {
            let directory = tempfile::TempDir::new().expect("create a directory");
            let path = directory.path().join("users.yaml");
            the_file_now_says(&path, yaml);
            let initial = UsersFile::from_yaml(yaml).expect("the initial mapping is valid");
            let (sender, seen) = mpsc::channel();
            let watch = UsersWatch::spawn(&path, initial, Arc::new(Recording(Mutex::new(sender))))
                .expect("start the watch");
            AWatchedFile {
                _directory: directory,
                path,
                watch,
                seen,
            }
        }

        fn is_saved_as(&self, yaml: &str) -> &Self {
            the_file_now_says(&self.path, yaml);
            self
        }

        fn next_report(&self) -> Seen {
            self.seen
                .recv_timeout(A_CHANGE_IS_SEEN_WITHIN)
                .expect("the watch reported nothing for a saved edit")
        }
    }

    fn the_file_now_says(path: &Path, yaml: &str) {
        std::fs::write(path, yaml).expect("write users.yaml");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))
            .expect("narrow users.yaml");
    }

    const ALICE: &str = "- {github_user: a, os_user: alice}\n";
    const ALICE_AND_CAROL: &str =
        "- {github_user: a, os_user: alice}\n- {github_user: c, os_user: carol}\n";
    const A_MAPPING_TO_ROOT: &str = "- {github_user: a, os_user: root}\n";

    #[test]
    fn a_saved_edit_becomes_the_current_mapping_and_is_reported() {
        // Given a watch on a mapping of alice
        let file = AWatchedFile::mapping(ALICE);

        // When carol is added
        let report = file.is_saved_as(ALICE_AND_CAROL).next_report();

        // Then the new mapping is reported and is the one in force
        let expected = UsersFile::from_yaml(ALICE_AND_CAROL).unwrap();
        assert!(
            matches!(&report, Seen::Changed(next) if *next == expected),
            "got {report:?}"
        );
        assert_eq!(file.watch.current(), expected);
    }

    #[test]
    fn an_invalid_users_yaml_edit_keeps_the_running_mapping() {
        // Given a watch on a mapping of alice
        let file = AWatchedFile::mapping(ALICE);

        // When the file is saved mapping a login to root
        let report = file.is_saved_as(A_MAPPING_TO_ROOT).next_report();

        // Then the edit is refused, naming why, and alice's mapping stays in force
        assert!(
            matches!(&report, Seen::Refused(why) if why.contains("root")),
            "got {report:?}"
        );
        assert_eq!(file.watch.current(), UsersFile::from_yaml(ALICE).unwrap());
    }
}
