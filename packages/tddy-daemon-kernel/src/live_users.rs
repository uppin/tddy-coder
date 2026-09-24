//! The `users:` map, held once for the whole running daemon.
//!
//! Every token-gated RPC resolves its caller's OS user through `users:`, and the daemon hands each
//! of its ~20 services a clone of one [`DaemonConfig`](crate::config::DaemonConfig). While `users:`
//! was a plain `Vec` in that config, each clone was a snapshot: a row enrolled at run time would
//! have been seen by whichever copy wrote it and by no other service until a restart. This is the
//! one place the rows live instead. [`DaemonConfig`](crate::config::DaemonConfig) carries it as its
//! `users` field, so **cloning a config shares the rows rather than copying them** — every service
//! built from one loaded config reads the same map, and a row enrolled through any of them is seen
//! by all of them at once.
//!
//! The lookup is unchanged by that: [`LiveUsers::os_user_for_github`] answers `None` for anyone not
//! in the map, with no default arm. The one mutation is [`LiveUsers::enrol_first_login`], which only
//! ever writes the *first* row — see [`crate::first_login_enrolment`].

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard};

use crate::config::UserMapping;
use crate::first_login_enrolment::{enrol_first_login, EnrolmentRefusal};

/// A cheap-to-clone handle on the daemon's `users:` rows. Clones share the rows.
#[derive(Clone, Default)]
pub struct LiveUsers {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    rows: RwLock<Vec<UserMapping>>,
    /// Held across every rewrite of the config file these rows are persisted in.
    ///
    /// Two writers rewrite that file: enrolment, and `daemon_config.DaemonConfigService`'s
    /// `UpdateConfig`, which re-serialises the whole config — `users:` included — from memory.
    /// Unserialised, an update that read the rows before an enrolment and wrote after it would
    /// persist the file without the enrolled row, and the operator would be unknown on the next
    /// start. A separate lock from `rows`, so a lookup never waits on file I/O.
    file_writes: Mutex<()>,
}

impl LiveUsers {
    /// A holder over `rows`, shared by every clone made from it.
    pub fn new(rows: Vec<UserMapping>) -> Self {
        Self {
            inner: Arc::new(Inner {
                rows: RwLock::new(rows),
                file_writes: Mutex::new(()),
            }),
        }
    }

    fn rows(&self) -> RwLockReadGuard<'_, Vec<UserMapping>> {
        self.inner.rows.read().expect("live users lock poisoned")
    }

    /// Serialise a rewrite of the config file these rows are persisted in, for as long as the
    /// guard is held.
    fn lock_file_writes(&self) -> MutexGuard<'_, ()> {
        self.inner
            .file_writes
            .lock()
            .expect("live users file lock poisoned")
    }

    /// The OS user `github_user` is mapped to, or `None` when they are not mapped. No default arm.
    pub fn os_user_for_github(&self, github_user: &str) -> Option<String> {
        self.rows()
            .iter()
            .find(|u| u.github_user == github_user)
            .map(|u| u.os_user.clone())
    }

    /// The row mapping `os_user`, for the local peer-trust path that starts from a uid.
    pub fn mapping_for_os_user(&self, os_user: &str) -> Option<UserMapping> {
        self.rows().iter().find(|u| u.os_user == os_user).cloned()
    }

    /// The GitHub user of the first row, when there is one — the account a desktop enrolled.
    pub fn first_github_user(&self) -> Option<String> {
        self.rows().first().map(|u| u.github_user.clone())
    }

    /// A copy of every row, in order.
    pub fn snapshot(&self) -> Vec<UserMapping> {
        self.rows().clone()
    }

    /// Whether no row exists.
    pub fn is_empty(&self) -> bool {
        self.rows().is_empty()
    }

    /// Enrol `github_user` as this deployment's first and only row, mapped to `os_user`: persist it
    /// to `config_path`, then make it visible to every lookup.
    ///
    /// Serialised against every other enrolment and config rewrite, and the emptiness check is
    /// repeated inside that serialisation — so of two first logins racing, exactly one is enrolled
    /// and the other is refused [`EnrolmentRefusal::AlreadyEnrolled`]. Persisted before it is
    /// applied: when the write fails nothing is applied, so the daemon never admits a login it
    /// could not record.
    pub fn enrol_first_login(
        &self,
        config_path: &Path,
        github_user: &str,
        os_user: &str,
    ) -> Result<UserMapping, EnrolmentRefusal> {
        let _file = self.lock_file_writes();
        if let Some(enrolled) = self.first_github_user() {
            return Err(EnrolmentRefusal::AlreadyEnrolled {
                github_user: enrolled,
            });
        }
        let mapping = enrol_first_login(config_path, github_user, os_user)?;
        self.inner
            .rows
            .write()
            .expect("live users lock poisoned")
            .push(mapping.clone());
        Ok(mapping)
    }

    /// Run `rewrite` — a rewrite of the config file these rows are persisted in — serialised
    /// against enrolment, so neither write can lose the other's.
    pub fn while_rewriting_config_file<R>(&self, rewrite: impl FnOnce() -> R) -> R {
        let _file = self.lock_file_writes();
        rewrite()
    }
}

impl From<Vec<UserMapping>> for LiveUsers {
    fn from(rows: Vec<UserMapping>) -> Self {
        Self::new(rows)
    }
}

impl std::fmt::Debug for LiveUsers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.rows().iter()).finish()
    }
}

/// `users:` is written and read as the plain list it always was.
impl serde::Serialize for LiveUsers {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.rows().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for LiveUsers {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<UserMapping>::deserialize(deserializer).map(Self::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THE_OPERATOR: &str = "operator";
    const A_SECOND_OPERATOR: &str = "a-second-operator";
    const THE_OS_USER: &str = "operator-os";

    #[test]
    fn concurrent_first_logins_enrol_exactly_one() {
        // Given an unenrolled deployment, and two different first logins arriving together
        let (users, path, _dir) = an_unenrolled_deployment();
        let barrier = std::sync::Barrier::new(2);

        // When both try to enrol at once
        let outcomes: Vec<Result<UserMapping, EnrolmentRefusal>> = std::thread::scope(|scope| {
            [THE_OPERATOR, A_SECOND_OPERATOR]
                .map(|login| {
                    let (users, path, barrier) = (users.clone(), &path, &barrier);
                    scope.spawn(move || {
                        barrier.wait();
                        users.enrol_first_login(path, login, THE_OS_USER)
                    })
                })
                .into_iter()
                .map(|racer| racer.join().expect("an enrolling thread panicked"))
                .collect()
        });

        // Then one is enrolled, the other refused, and exactly one row exists in memory and on disk
        let enrolled = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
        let persisted = crate::config::DaemonConfig::load(&path)
            .expect("the rewritten config loads")
            .users
            .snapshot()
            .len();
        assert_eq!(
            (enrolled, users.snapshot().len(), persisted),
            (1, 1, 1),
            "two racing first logins must not both enrol; outcomes {outcomes:?}"
        );
    }

    #[test]
    fn a_row_enrolled_through_one_clone_is_seen_by_every_other() {
        // Given two services holding clones of one deployment's users
        let (enrolling_service, path, _dir) = an_unenrolled_deployment();
        let another_service = enrolling_service.clone();

        // When one of them enrols the first login
        enrolling_service
            .enrol_first_login(&path, THE_OPERATOR, THE_OS_USER)
            .expect("a first login is enrolled");

        // Then the other resolves it at once, with no reload
        assert_eq!(
            another_service.os_user_for_github(THE_OPERATOR).as_deref(),
            Some(THE_OS_USER)
        );
    }

    #[test]
    fn a_login_whose_enrolment_cannot_be_persisted_is_not_applied() {
        // Given an unenrolled deployment whose config file has gone
        let (users, path, dir) = an_unenrolled_deployment();
        drop(dir);

        // When a first login tries to enrol
        let refusal = users.enrol_first_login(&path, THE_OPERATOR, THE_OS_USER);

        // Then it is refused as unwritable, and nobody is mapped
        assert_eq!(
            (
                matches!(refusal, Err(EnrolmentRefusal::ConfigNotWritable { .. })),
                users.os_user_for_github(THE_OPERATOR),
            ),
            (true, None),
            "a row that was never recorded must not be admitted; got {refusal:?}"
        );
    }

    /// A deployment with no `users:` row, as held in memory and as written in its config file.
    fn an_unenrolled_deployment() -> (LiveUsers, std::path::PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "repos_base_path: \"/tmp/repos\"\n").expect("the config is written");
        (LiveUsers::default(), path, dir)
    }
}
