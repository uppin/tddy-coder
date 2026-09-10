//! Reading the private key an operator names — as that operator, and only from their own home.
//!
//! `AddHostKeyRequest.subject` is free text from a browser. Everything else in this flow already
//! runs per user: node 4's probes shell out through [`tddy_daemon_kernel::spawn_as_user::start_output_as_user`] because
//! `git config` and `gh auth status` read `$HOME`, and node 5 resolves an agent socket per user. A
//! read done as the *daemon* diverges from its own stack in the one place it matters most — on a
//! multi-user host it lets a session mapped to `alice` name `/home/bob/.ssh/id_rsa` and have the
//! daemon open it on her behalf.
//!
//! # Two guards, and why both
//!
//! - **Confinement.** The path must be absolute, free of `..`, and inside the mapped user's home.
//!   Purely lexical, deliberately: it touches the filesystem not at all, so the refusal is a
//!   function of the caller's own input and cannot be read as an answer to "does this file exist?".
//! - **Privilege.** The bytes are read with the mapped user's own privileges. This is the guard
//!   that actually holds: a symlink inside `alice`'s home pointing at `bob`'s key resolves for the
//!   kernel as `alice`, who cannot read it — so the lexical check needs no `canonicalize`, which
//!   would itself have to stat a path the caller chose and would leak exactly what it prevents.
//!
//! # One refusal
//!
//! Every way a read can fail collapses into [`KEY_UNREADABLE`]. "No such file" and "not an OpenSSH
//! private key" told apart, and returned to the browser verbatim, make this endpoint a file oracle
//! for any path inside the caller's home. The operator's remedy is the same in both cases: name a
//! key that is there.
//!
//! Feature: `docs/ft/web/hosts-screen-add-key.md`

use ssh_key::{PrivateKey, PublicKey};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

/// What an operator is told when the key they named did not read. **The only such message**: see
/// the module docs for why "absent" and "malformed" must be indistinguishable.
///
/// Names no path, so it cannot be built into a lookup for one, and stays true whichever failure it
/// stands for.
pub const KEY_UNREADABLE: &str =
    "that key could not be read on this host — check the path and that it is an OpenSSH private key";

/// What an operator is told when the key they named is not theirs to read.
///
/// Says nothing about the path beyond the fact that it is outside their home — which is already
/// decided by the path they sent and their own account, and by nothing on disk.
pub const KEY_OUTSIDE_HOME: &str =
    "a key must be a path inside your own home directory on this host";

/// Files belonging to a host OS user, read with that user's own privileges.
///
/// A trait for the same reason [`crate::ssh_agent_add::SshAgentKeyAdder`] is one: impersonating an
/// OS user is the one step of this flow a test cannot perform. Everything built on top of it — the
/// confinement, the parse, the single refusal — is [`read_private_key`], which tests exercise for
/// real.
pub trait HostUserFiles: Send + Sync {
    /// `os_user`'s home directory, from the passwd database.
    fn home_dir(&self, os_user: &str) -> Result<PathBuf, String>;

    /// The bytes at `path`, read as `os_user`.
    fn read_as_user(&self, os_user: &str, path: &Path) -> Result<Vec<u8>, String>;

    /// The **regular files** directly inside `dir`, listed as `os_user`.
    ///
    /// Regular files only, and links are not followed: a directory named `id_rsa` is not a key, and
    /// a link out of this directory names a file that is not necessarily `os_user`'s — neither is
    /// something an operator should be offered as one of their own keys.
    ///
    /// `Err` for a directory that is not there, and for one this user cannot read. The two are told
    /// apart nowhere above this seam — see [`list_key_candidates`].
    fn list_files_as_user(&self, os_user: &str, dir: &Path) -> Result<Vec<PathBuf>, String>;
}

/// The live reader: passwd for the home directory, a child process for the read.
pub struct SpawnedHostUserFiles;

impl HostUserFiles for SpawnedHostUserFiles {
    fn home_dir(&self, os_user: &str) -> Result<PathBuf, String> {
        tddy_daemon_kernel::user_paths::home_dir_for_user(os_user)
            .ok_or_else(|| format!("{os_user} has no home directory on this host"))
    }

    /// Reads through `cat` run as `os_user`, which is how [`tddy_daemon_kernel::spawn_as_user`] drops privileges for
    /// every other per-user read in this daemon.
    ///
    /// A child process rather than `seteuid` around a `read_to_string`: the daemon is
    /// multi-threaded, and a process-wide euid switch would apply to every other request in flight.
    #[cfg(unix)]
    fn read_as_user(&self, os_user: &str, path: &Path) -> Result<Vec<u8>, String> {
        // Resolved before spawning, exactly as node 4's probes resolve `git` and `gh`:
        // `run_capture_as_user` anchors a relative program to the daemon's own toolchain root and
        // never consults `PATH`, so a bare "cat" would try to exec `<daemon-cwd>/cat`.
        let cat = tddy_daemon_kernel::spawn_as_user::find_program_on_spawn_child_path("cat")
            .ok_or_else(|| "this host has no `cat` to read a key with".to_string())?;
        // An OpenSSH private key is ASCII armour, so a lossy decode cannot alter one. A file that
        // is not one fails the parse, which is the same refusal a missing file gets.
        tddy_daemon_kernel::spawn_as_user::run_capture_as_user(
            os_user,
            &cat,
            &[path.display().to_string()],
        )
        .map(String::into_bytes)
        .map_err(|e| e.to_string())
    }

    #[cfg(not(unix))]
    fn read_as_user(&self, _os_user: &str, _path: &Path) -> Result<Vec<u8>, String> {
        Err("reading a file as another user is only supported on Unix".to_string())
    }

    /// Lists through `find` run as `os_user`, for the same reason [`Self::read_as_user`] reads
    /// through `cat`: a child process, because a process-wide euid switch in this multi-threaded
    /// daemon would apply to every other request in flight.
    ///
    /// `-type f` is doing security work, not tidiness. `find` does not follow symlinks unless told
    /// to, so a link is `-type l` and never `-type f` — which is what keeps a link planted in one
    /// user's `~/.ssh` from putting another user's key on their list under a name that looks like
    /// their own.
    #[cfg(unix)]
    fn list_files_as_user(&self, os_user: &str, dir: &Path) -> Result<Vec<PathBuf>, String> {
        let find = tddy_daemon_kernel::spawn_as_user::find_program_on_spawn_child_path("find")
            .ok_or_else(|| "this host has no `find` to list a key directory with".to_string())?;
        let listed = tddy_daemon_kernel::spawn_as_user::run_capture_as_user(
            os_user,
            &find,
            &[
                dir.display().to_string(),
                "-maxdepth".to_string(),
                "1".to_string(),
                "-type".to_string(),
                "f".to_string(),
                "-print".to_string(),
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(listed
            .lines()
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .collect())
    }

    #[cfg(not(unix))]
    fn list_files_as_user(&self, _os_user: &str, _dir: &Path) -> Result<Vec<PathBuf>, String> {
        Err("listing a directory as another user is only supported on Unix".to_string())
    }
}

/// The OpenSSH private key `os_user` named at `subject`, still locked if it is passphrase-protected.
///
/// Read as `os_user` and refused unless `subject` is inside their home. Every failure — outside the
/// home aside — is [`KEY_UNREADABLE`]; see the module docs.
pub fn read_private_key(
    files: &dyn HostUserFiles,
    os_user: &str,
    subject: &str,
) -> Result<PrivateKey, String> {
    let home = files.home_dir(os_user).map_err(|reason| {
        // Logged rather than returned: an operator whose account has no home on this host cannot
        // act on that, and the caller learns only that the key did not read.
        log::warn!(
            target: "tddy_daemon::host_private_key",
            "cannot resolve a home directory to confine a key read to: {reason}"
        );
        KEY_UNREADABLE.to_string()
    })?;
    let path = confined_to_home(&home, subject)?;
    let openssh = files.read_as_user(os_user, &path).map_err(|reason| {
        log::debug!(
            target: "tddy_daemon::host_private_key",
            "reading {} as {os_user} failed: {reason}",
            path.display()
        );
        KEY_UNREADABLE.to_string()
    })?;
    PrivateKey::from_openssh(&openssh).map_err(|reason| {
        log::debug!(
            target: "tddy_daemon::host_private_key",
            "{} is not an OpenSSH private key: {reason}",
            path.display()
        );
        KEY_UNREADABLE.to_string()
    })
}

/// The name of the directory `ssh-keygen` writes a key into, and the only one a listing reads.
///
/// Confinement permits a key anywhere inside the user's own home; a *listing* looks in one place,
/// because a walk of a home directory is a walk of the operator's documents. A key kept elsewhere
/// is still addable — it is typed rather than picked.
const SSH_DIR: &str = ".ssh";

/// What `ssh-keygen` appends to a key's file name to name its public half.
const PUB_SUFFIX: &str = ".pub";

/// One private key an operator could pick from a list, described entirely from its public half.
///
/// Carries no key material and cannot: every field here comes out of `<path>.pub`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCandidate {
    /// The absolute path of the **private** key — what an add is then asked for.
    pub path: PathBuf,
    /// e.g. `ssh-ed25519`, from the public half.
    pub key_type: String,
    /// The `SHA256:` fingerprint of the public half, in the form `ssh-add -l` prints.
    pub fingerprint: String,
}

/// The private keys `os_user` could load into their agent, out of their own `~/.ssh`.
///
/// The counterpart of [`read_private_key`]: what this offers, that accepts. Enumerated as
/// `os_user`, inside the home the passwd database gives them, and ordered by path.
///
/// # A candidate is a key whose public half sits beside it
///
/// `~/.ssh/id_ed25519` is offered because `~/.ssh/id_ed25519.pub` is there and parses as an OpenSSH
/// public key. That single rule does all of the filtering this surface needs — `known_hosts`,
/// `authorized_keys`, `config` and a stray note have no public half, and a `.pub` file is not
/// itself a candidate — and it means **no private key is ever opened to build this list**: the type
/// and the fingerprint an operator reads are derived from the public bytes alone.
///
/// A key with no `.pub` beside it is therefore not offered. That is why the path an operator can
/// type stays: the listing is a convenience over `~/.ssh`, not the boundary of what may be added.
///
/// # No answer about what exists
///
/// Returns a list, never a failure — the same collapse [`KEY_UNREADABLE`] performs, for the same
/// reason. A user with no `~/.ssh`, one whose `~/.ssh` this host cannot read, and one with an empty
/// `~/.ssh` are indistinguishable to the caller: an authenticated session must not be able to use
/// this as a probe for what is on the host.
pub fn list_key_candidates(files: &dyn HostUserFiles, os_user: &str) -> Vec<KeyCandidate> {
    let Ok(home) = files.home_dir(os_user).inspect_err(|reason| {
        // Logged rather than surfaced, exactly as `read_private_key` logs it: an operator whose
        // account has no home on this host is offered nothing, and is told nothing either.
        log::warn!(
            target: "tddy_daemon::host_private_key",
            "cannot resolve a home directory to list keys in: {reason}"
        );
    }) else {
        return Vec::new();
    };
    let ssh_dir = home.join(SSH_DIR);
    let listed = match files.list_files_as_user(os_user, &ssh_dir) {
        Ok(listed) => listed,
        Err(reason) => {
            // The whole reason this function returns a `Vec` and not a `Result`: a directory that
            // is not there and one this host cannot get into leave by the same door.
            log::debug!(
                target: "tddy_daemon::host_private_key",
                "listing {} as {os_user} failed: {reason}",
                ssh_dir.display()
            );
            return Vec::new();
        }
    };
    // Membership and order out of one structure: "is `<path>.pub` in this directory?" is the whole
    // filter, and a sorted set answers it in the order an operator then reads the list in.
    let in_the_directory: BTreeSet<PathBuf> = listed.into_iter().collect();
    in_the_directory
        .iter()
        // A `.pub` describes a candidate; it is not one. Skipped before anything is read, so the
        // public half of a key is opened once rather than twice.
        .filter(|path| !is_public_half(path))
        // The half that cannot bend: a path this offers is then sent back as
        // `AddHostKeyRequest.subject`, so it is offered only if the add's own confinement accepts
        // it. Enforced rather than assumed — the two checks are the same function.
        .filter(|path| confined_to_home(&home, &path.display().to_string()).is_ok())
        .filter(|path| in_the_directory.contains(&public_half_of(path)))
        .filter_map(|path| described_by_its_public_half(files, os_user, path))
        .collect()
}

/// One candidate, read entirely out of `<path>.pub`.
///
/// The private file at `path` is never opened: its name is all a listing takes from it, and the two
/// fields an operator reads come from the public bytes beside it. A `.pub` that does not parse as
/// an OpenSSH public key describes nothing, so the key beside it is not offered.
fn described_by_its_public_half(
    files: &dyn HostUserFiles,
    os_user: &str,
    path: &Path,
) -> Option<KeyCandidate> {
    let public_half = public_half_of(path);
    let openssh = files
        .read_as_user(os_user, &public_half)
        .inspect_err(|reason| {
            log::debug!(
                target: "tddy_daemon::host_private_key",
                "reading {} as {os_user} failed: {reason}",
                public_half.display()
            );
        })
        .ok()?;
    // An OpenSSH public key is a single line of ASCII, so a lossy decode cannot alter one, and a
    // file that is not one fails the parse below.
    let public = PublicKey::from_openssh(&String::from_utf8_lossy(&openssh))
        .inspect_err(|reason| {
            log::debug!(
                target: "tddy_daemon::host_private_key",
                "{} is not an OpenSSH public key: {reason}",
                public_half.display()
            );
        })
        .ok()?;
    Some(KeyCandidate {
        path: path.to_path_buf(),
        key_type: public.algorithm().as_str().to_string(),
        fingerprint: public.fingerprint(ssh_key::HashAlg::Sha256).to_string(),
    })
}

/// `<path>.pub`, the way `ssh-keygen` names a public half: appended to the whole file name rather
/// than replacing an extension, so `id_rsa` pairs with `id_rsa.pub`.
fn public_half_of(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(PUB_SUFFIX);
    path.with_file_name(name)
}

/// Whether `path` is itself the public half of some key.
fn is_public_half(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().ends_with(PUB_SUFFIX))
}

/// `subject` as a path inside `home`, or [`KEY_OUTSIDE_HOME`].
///
/// Lexical only — no `canonicalize`, no `metadata`, nothing that touches the filesystem. See the
/// module docs: the read itself runs as the owner, so this check is about intent and blast radius,
/// and a check that had to stat the caller's path would answer the question it exists to refuse.
fn confined_to_home(home: &Path, subject: &str) -> Result<PathBuf, String> {
    let path = Path::new(subject.trim());
    // `..` is what turns "inside your home" into any path on the host, and a relative path is
    // resolved against whatever directory a reader happens to run in — neither is a key's address.
    let addressed_plainly = path
        .components()
        .all(|part| matches!(part, Component::RootDir | Component::Normal(_)));
    if !path.is_absolute() || !addressed_plainly || !path.starts_with(home) {
        return Err(KEY_OUTSIDE_HOME.to_string());
    }
    Ok(path.to_path_buf())
}

/// Files under a home directory of the test's choosing, read with the test process's own
/// privileges.
///
/// Stands in for the OS seam and nothing else: the confinement, the parse and the single refusal
/// are the real ones. `pub(crate)` because the handler tests in
/// `tddy_daemon::connection_service` exercise the same flow end to end and must not grow a second,
/// divergent double of their own.
#[cfg(test)]
pub(crate) struct UserFilesUnder {
    home: PathBuf,
    /// Homes belonging to particular OS users, for the tests that ask whether one user's request
    /// can reach another user's files. Everybody else gets [`Self::home`].
    homes: Vec<(String, PathBuf)>,
    /// Every OS user a read was attempted as, so a test can state *whose* privileges were used —
    /// the whole point of this seam.
    read_as: std::sync::Mutex<Vec<String>>,
    /// The same, for a directory listing: it runs as the owner or it runs as the daemon, and only
    /// the recording can tell a test which.
    listed_as: std::sync::Mutex<Vec<String>>,
    /// Every path this reader was asked to open, so a test can state that building a *listing*
    /// never opened a private key.
    paths_read: std::sync::Mutex<Vec<PathBuf>>,
    /// Every directory a listing was asked for, so a test can state that building a list of keys
    /// did not walk the operator's home directory.
    dirs_listed: std::sync::Mutex<Vec<PathBuf>>,
    /// Directories this host cannot read as the user who owns them — the one failure a test cannot
    /// stage with real permissions without depending on who the test process happens to be.
    unreadable: Vec<PathBuf>,
}

#[cfg(test)]
impl UserFilesUnder {
    pub(crate) fn home(home: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            homes: Vec::new(),
            read_as: std::sync::Mutex::new(Vec::new()),
            listed_as: std::sync::Mutex::new(Vec::new()),
            paths_read: std::sync::Mutex::new(Vec::new()),
            dirs_listed: std::sync::Mutex::new(Vec::new()),
            unreadable: Vec::new(),
        }
    }

    /// A reader for which `dir` cannot be read at all, the way a `~/.ssh` whose permissions this
    /// host cannot get past cannot be read.
    pub(crate) fn and_a_directory_it_cannot_read(mut self, dir: impl Into<PathBuf>) -> Self {
        self.unreadable.push(dir.into());
        self
    }

    /// A reader that also knows where `os_user` lives — a second account on the same host.
    pub(crate) fn and_the_home_of(mut self, os_user: &str, home: impl Into<PathBuf>) -> Self {
        self.homes.push((os_user.to_string(), home.into()));
        self
    }

    /// The OS users this reader was asked to read as, in order.
    pub(crate) fn users_read_as(&self) -> Vec<String> {
        self.read_as
            .lock()
            .expect("the recording lock is only held to push a user")
            .clone()
    }

    /// The OS users this reader was asked to list a directory as, in order.
    pub(crate) fn users_listed_as(&self) -> Vec<String> {
        self.listed_as
            .lock()
            .expect("the recording lock is only held to push a user")
            .clone()
    }

    /// The paths this reader was asked to open, in order.
    pub(crate) fn paths_read(&self) -> Vec<PathBuf> {
        self.paths_read
            .lock()
            .expect("the recording lock is only held to push a path")
            .clone()
    }

    /// The directories this reader was asked to list, in order.
    pub(crate) fn dirs_listed(&self) -> Vec<PathBuf> {
        self.dirs_listed
            .lock()
            .expect("the recording lock is only held to push a path")
            .clone()
    }
}

#[cfg(test)]
impl HostUserFiles for UserFilesUnder {
    fn home_dir(&self, os_user: &str) -> Result<PathBuf, String> {
        Ok(self
            .homes
            .iter()
            .find(|(user, _)| user == os_user)
            .map(|(_, home)| home.clone())
            .unwrap_or_else(|| self.home.clone()))
    }

    fn read_as_user(&self, os_user: &str, path: &Path) -> Result<Vec<u8>, String> {
        self.read_as
            .lock()
            .expect("the recording lock is only held to push a user")
            .push(os_user.to_string());
        self.paths_read
            .lock()
            .expect("the recording lock is only held to push a path")
            .push(path.to_path_buf());
        std::fs::read(path).map_err(|e| e.to_string())
    }

    /// Regular files only and links left alone, the way `find -maxdepth 1 -type f` leaves them:
    /// [`std::fs::DirEntry::file_type`] reports a symlink as a symlink rather than as what it
    /// points at, so this fake and the live lister agree about what a directory contains.
    fn list_files_as_user(&self, os_user: &str, dir: &Path) -> Result<Vec<PathBuf>, String> {
        self.listed_as
            .lock()
            .expect("the recording lock is only held to push a user")
            .push(os_user.to_string());
        self.dirs_listed
            .lock()
            .expect("the recording lock is only held to push a path")
            .push(dir.to_path_buf());
        if self.unreadable.iter().any(|denied| denied == dir) {
            return Err(format!("{} cannot be read", dir.display()));
        }
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().map_err(|e| e.to_string())?.is_file() {
                files.push(entry.path());
            }
        }
        files.sort();
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALICE: &str = "alice";

    /// A second account on the same host, for the questions that only exist because a host has more
    /// than one operator on it.
    const BOB: &str = "bob";

    /// A host where `alice` has a home directory, and whatever a test puts inside it.
    struct AliceOnThisHost {
        files: UserFilesUnder,
        home: PathBuf,
        _storage: tempfile::TempDir,
    }

    /// A host where `alice` has a home directory with an empty `~/.ssh` in it.
    fn a_host_where_alice_lives() -> AliceOnThisHost {
        let storage = tempfile::tempdir().expect("a temp directory for this host");
        let home = storage.path().join("home").join(ALICE);
        std::fs::create_dir_all(home.join(SSH_DIR)).expect("alice has a ~/.ssh");
        AliceOnThisHost {
            files: UserFilesUnder::home(&home),
            home,
            _storage: storage,
        }
    }

    /// The same host, with a private key of hers in it — no public half beside it, which is all the
    /// read tests need and is itself a case the listing tests state.
    fn a_host_where_alice_has_a_key() -> AliceOnThisHost {
        let host = a_host_where_alice_lives();
        write_a_private_key_at(&host.her_key());
        host
    }

    impl AliceOnThisHost {
        fn read(&self, subject: &Path) -> Result<PrivateKey, String> {
            read_private_key(&self.files, ALICE, &subject.display().to_string())
        }

        fn her_key(&self) -> PathBuf {
            self.ssh_dir().join("id_ed25519")
        }

        fn ssh_dir(&self) -> PathBuf {
            self.home.join(SSH_DIR)
        }

        /// A keypair of hers in `~/.ssh`, both halves, the way `ssh-keygen` leaves them.
        fn also_has_the_keypair(&self, name: &str) -> PublicHalf {
            let path = self.ssh_dir().join(name);
            let key = write_a_private_key_at(&path);
            write_the_public_half_beside(&key, &path)
        }

        /// A file of hers in `~/.ssh` that is not a key — `known_hosts` and friends.
        fn also_has_the_file(&self, name: &str, contents: &str) {
            std::fs::write(self.ssh_dir().join(name), contents).expect("the file is written");
        }

        /// The keys an operator would be offered to pick from.
        fn candidates(&self) -> Vec<KeyCandidate> {
            list_key_candidates(&self.files, ALICE)
        }

        fn paths_offered(&self) -> Vec<PathBuf> {
            self.candidates()
                .into_iter()
                .map(|candidate| candidate.path)
                .collect()
        }

        /// `bob`'s home on the same host, with a keypair of his in it.
        fn where_bob_also_has_a_keypair(mut self) -> Self {
            let his_home = self.home.parent().expect("a /home").join(BOB);
            std::fs::create_dir_all(his_home.join(SSH_DIR)).expect("bob has a ~/.ssh");
            let his_key = his_home.join(SSH_DIR).join("id_ed25519");
            let key = write_a_private_key_at(&his_key);
            write_the_public_half_beside(&key, &his_key);
            self.files = self.files.and_the_home_of(BOB, &his_home);
            self
        }

        /// `bob`'s key, which is his and not hers.
        fn bobs_key(&self) -> PathBuf {
            self.home
                .parent()
                .expect("a /home")
                .join(BOB)
                .join(SSH_DIR)
                .join("id_ed25519")
        }

        /// The same host, where her `~/.ssh` cannot be read at all.
        fn where_her_ssh_dir_cannot_be_read(mut self) -> Self {
            let denied = self.ssh_dir();
            self.files = self.files.and_a_directory_it_cannot_read(denied);
            self
        }

        /// The same host, where she has no `~/.ssh` at all.
        fn where_she_has_no_ssh_dir(self) -> Self {
            std::fs::remove_dir_all(self.ssh_dir()).expect("her ~/.ssh is taken away");
            self
        }
    }

    /// What `<path>.pub` says about the key at `path` — the two fields a listing derives from it,
    /// and the only two an operator reads.
    struct PublicHalf {
        key_type: String,
        fingerprint: String,
    }

    /// A freshly generated, unencrypted ed25519 key in the format `ssh-keygen` writes.
    fn write_a_private_key_at(path: &Path) -> PrivateKey {
        let key = PrivateKey::random(&mut rand::thread_rng(), ssh_key::Algorithm::Ed25519)
            .expect("an ed25519 private key");
        let openssh = key
            .to_openssh(ssh_key::LineEnding::LF)
            .expect("in the format ssh-keygen writes");
        std::fs::write(path, openssh.as_bytes()).expect("the key file is written");
        key
    }

    /// The `.pub` file `ssh-keygen` writes beside a private key, in the one-line format it writes.
    fn write_the_public_half_beside(key: &PrivateKey, path: &Path) -> PublicHalf {
        let public = key.public_key();
        let line = public
            .to_openssh()
            .expect("in the one-line format ssh-keygen writes a .pub in");
        std::fs::write(with_pub_suffix(path), format!("{line}\n"))
            .expect("the public half is written beside it");
        PublicHalf {
            key_type: public.algorithm().as_str().to_string(),
            fingerprint: public.fingerprint(ssh_key::HashAlg::Sha256).to_string(),
        }
    }

    /// `<path>.pub`, the way `ssh-keygen` names a public half: the suffix is appended to the whole
    /// file name rather than replacing an extension.
    fn with_pub_suffix(path: &Path) -> PathBuf {
        let mut name = path.file_name().expect("a key file name").to_os_string();
        name.push(".pub");
        path.with_file_name(name)
    }

    /// The ordinary case, and the guard that keeps every refusal below from passing for a reader
    /// that refuses everything.
    #[test]
    fn reads_a_key_from_inside_the_users_own_home() {
        // Given a host where alice has a key in her home directory
        let host = a_host_where_alice_has_a_key();

        // When she names it
        let read = host.read(&host.her_key());

        // Then
        assert!(read.is_ok(), "alice's own key did not read: {read:?}");
    }

    /// The key is read as its owner, not as the daemon. Read as the daemon, a path the caller chose
    /// is opened with whatever the daemon can reach — which on a supervised host is everything.
    #[test]
    fn reads_the_key_with_the_privileges_of_the_user_it_belongs_to() {
        // Given a host where alice has a key in her home directory
        let host = a_host_where_alice_has_a_key();

        // When she names it
        host.read(&host.her_key()).expect("alice's own key");

        // Then
        assert_eq!(
            host.files.users_read_as(),
            vec![ALICE.to_string()],
            "the read did not happen as the operator's own OS user"
        );
    }

    /// The path is free text from a browser. Unconfined, a session mapped to one user names
    /// another's `~/.ssh/id_rsa`.
    #[test]
    fn refuses_a_key_outside_the_users_home() {
        // Given a host where alice has a home directory, and a key that is not in it
        let host = a_host_where_alice_has_a_key();
        let somebody_elses = host.home.parent().expect("a /home").join("bob");
        std::fs::create_dir_all(&somebody_elses).expect("bob's home");
        let their_key = somebody_elses.join("id_ed25519");
        write_a_private_key_at(&their_key);

        // When alice names it
        let refused = host.read(&their_key);

        // Then
        assert_eq!(refused.err(), Some(KEY_OUTSIDE_HOME.to_string()));
    }

    /// Confinement by prefix alone is defeated by a single `..`, so a path that climbs is refused
    /// whatever it climbs to.
    #[test]
    fn refuses_a_path_that_climbs_out_of_the_home_with_dot_dot() {
        // Given a host where alice has a home directory
        let host = a_host_where_alice_has_a_key();

        // When she names a path that starts inside it and climbs out
        let refused = host.read(&host.home.join("..").join("bob").join("id_ed25519"));

        // Then
        assert_eq!(refused.err(), Some(KEY_OUTSIDE_HOME.to_string()));
    }

    /// A relative path is resolved against whatever directory the reader happens to run in, which
    /// is not a home directory and is not the caller's to choose.
    #[test]
    fn refuses_a_relative_path() {
        // Given a host where alice has a home directory
        let host = a_host_where_alice_has_a_key();

        // When she names a key relative to no stated directory
        let refused = host.read(Path::new(".ssh/id_ed25519"));

        // Then
        assert_eq!(refused.err(), Some(KEY_OUTSIDE_HOME.to_string()));
    }

    /// **The file oracle.** Told apart, these two refusals answer "is there a file at this path?"
    /// for anywhere inside the caller's home — and the answer is returned straight to the browser.
    #[test]
    fn refuses_an_absent_key_and_a_malformed_one_in_the_same_words() {
        // Given a path in alice's home with nothing at it
        let host = a_host_where_alice_has_a_key();
        let named = host.home.join(".ssh").join("maybe-a-key");

        // When she names it while nothing is there, and again once something that is not a key is
        let while_absent = host.read(&named).err();
        std::fs::write(&named, "ssh-ed25519 AAAA... not a private key\n")
            .expect("something that is not a private key");
        let while_malformed = host.read(&named).err();

        // Then
        assert_eq!(
            while_malformed, while_absent,
            "the refusal says whether the file exists"
        );
        assert_eq!(while_absent, Some(KEY_UNREADABLE.to_string()));
    }

    // -- what an operator can pick from ---------------------------------------------------------

    /// The ordinary case, and the guard that keeps every "not offered" below from passing for a
    /// listing that offers nothing at all.
    #[test]
    fn offers_a_key_whose_public_half_sits_beside_it() {
        // Given a keypair of alice's in her ~/.ssh
        let host = a_host_where_alice_lives();
        host.also_has_the_keypair("id_ed25519");

        // When she is offered her keys
        let offered = host.paths_offered();

        // Then
        assert_eq!(offered, vec![host.ssh_dir().join("id_ed25519")]);
    }

    /// The two things an operator reads before picking. Both come out of the public half — which is
    /// what makes it safe to show them at all.
    #[test]
    fn describes_each_key_by_the_type_and_fingerprint_in_its_public_half() {
        // Given a keypair of alice's in her ~/.ssh
        let host = a_host_where_alice_lives();
        let public_half = host.also_has_the_keypair("id_ed25519");

        // When she is offered her keys
        let offered = host.candidates();

        // Then
        assert_eq!(
            offered,
            vec![KeyCandidate {
                path: host.ssh_dir().join("id_ed25519"),
                key_type: public_half.key_type,
                fingerprint: public_half.fingerprint,
            }]
        );
    }

    /// The same guard [`reads_the_key_with_the_privileges_of_the_user_it_belongs_to`] puts on the
    /// read. Listed as the daemon, the directory is enumerated with whatever the daemon can reach —
    /// which on a supervised host is every home on the machine.
    #[test]
    fn lists_the_keys_with_the_privileges_of_the_user_they_belong_to() {
        // Given a keypair of alice's in her ~/.ssh
        let host = a_host_where_alice_lives();
        host.also_has_the_keypair("id_ed25519");

        // When she is offered her keys
        host.candidates();

        // Then
        assert_eq!(
            host.files.users_listed_as(),
            vec![ALICE.to_string()],
            "the directory was not listed as the operator's own OS user"
        );
        assert_eq!(
            host.files.users_read_as(),
            vec![ALICE.to_string()],
            "the public half was not read as the operator's own OS user"
        );
    }

    /// **A list of keys must not be a use of them.** Building it opens public halves and nothing
    /// else: a private key that is read to be described is a private key in the daemon's memory for
    /// a call that was only ever asked what exists.
    #[test]
    fn never_opens_a_private_key_to_build_the_list() {
        // Given two keypairs of alice's in her ~/.ssh
        let host = a_host_where_alice_lives();
        host.also_has_the_keypair("id_ed25519");
        host.also_has_the_keypair("id_rsa");

        // When she is offered her keys
        let offered = host.paths_offered();

        // Then
        assert_eq!(offered.len(), 2, "both of her keys should be offered");
        let read = host.files.paths_read();
        assert!(
            !read.is_empty(),
            "nothing was read at all, so this test cannot say what was not read"
        );
        assert_eq!(
            read.iter()
                .filter(|path| path.extension() != Some(std::ffi::OsStr::new("pub")))
                .collect::<Vec<_>>(),
            Vec::<&PathBuf>::new(),
            "listing keys opened something other than a public half: {read:?}"
        );
    }

    /// One directory, named up front. A listing that walked the home directory to find keys would
    /// read the operator's documents to answer a question about `~/.ssh`.
    #[test]
    fn reads_only_the_users_own_ssh_directory() {
        // Given a keypair of alice's in her ~/.ssh
        let host = a_host_where_alice_lives();
        host.also_has_the_keypair("id_ed25519");

        // When she is offered her keys
        host.candidates();

        // Then
        assert_eq!(host.files.dirs_listed(), vec![host.ssh_dir()]);
    }

    /// The listing's counterpart of [`refuses_a_key_outside_the_users_home`]: what a session is
    /// offered is its own operator's, on a host where more than one person has keys.
    #[test]
    fn does_not_offer_another_users_key() {
        // Given a host where bob also has a keypair, and alice has one of her own
        let host = a_host_where_alice_lives().where_bob_also_has_a_keypair();
        host.also_has_the_keypair("id_ed25519");

        // When alice is offered her keys
        let offered = host.paths_offered();

        // Then
        assert!(
            !offered.contains(&host.bobs_key()),
            "another operator's key was offered to alice: {offered:?}"
        );
        assert_eq!(offered, vec![host.ssh_dir().join("id_ed25519")]);
    }

    /// A key kept outside `~/.ssh` is still addable by path — it is simply not one of the choices,
    /// because finding it would mean reading the whole home directory.
    #[test]
    fn does_not_offer_a_keypair_kept_outside_the_ssh_directory() {
        // Given a keypair of alice's in her home directory rather than her ~/.ssh
        let host = a_host_where_alice_lives();
        let elsewhere = host.home.join("id_ed25519");
        let key = write_a_private_key_at(&elsewhere);
        write_the_public_half_beside(&key, &elsewhere);

        // When she is offered her keys
        let offered = host.paths_offered();

        // Then
        assert_eq!(offered, Vec::<PathBuf>::new());
    }

    /// What else lives in a `~/.ssh`. None of it is a private key, and the public half of a key is
    /// the description of a candidate rather than a candidate itself.
    #[test]
    fn does_not_offer_ssh_configuration_or_a_public_half_on_its_own() {
        // Given a ~/.ssh with one keypair in it and the usual furniture around it
        let host = a_host_where_alice_lives();
        host.also_has_the_keypair("id_ed25519");
        host.also_has_the_file("known_hosts", "github.com ssh-ed25519 AAAA...\n");
        host.also_has_the_file("authorized_keys", "ssh-ed25519 AAAA... someone\n");
        host.also_has_the_file("config", "Host github.com\n  User git\n");
        host.also_has_the_file("orphan.pub", "ssh-ed25519 AAAA... orphan\n");

        // When she is offered her keys
        let offered = host.paths_offered();

        // Then
        assert_eq!(offered, vec![host.ssh_dir().join("id_ed25519")]);
    }

    /// A candidate is a *file* that is a private key. A directory with a plausible name and a
    /// public half beside it is neither, and a listing that inferred candidates from `.pub` files
    /// alone would offer it.
    #[test]
    fn does_not_offer_a_directory_that_is_named_like_a_key() {
        // Given a directory in her ~/.ssh named like a key, with a public half beside it
        let host = a_host_where_alice_lives();
        std::fs::create_dir(host.ssh_dir().join("id_rsa")).expect("a directory named like a key");
        host.also_has_the_file("id_rsa.pub", "ssh-ed25519 AAAA... not a file\n");

        // When she is offered her keys
        let offered = host.paths_offered();

        // Then
        assert_eq!(offered, Vec::<PathBuf>::new());
    }

    /// The rule that does the filtering has a cost: a key whose `.pub` was never kept is invisible
    /// here. Stated as a test because it is the reason the typed path must stay.
    #[test]
    fn does_not_offer_a_private_key_with_no_public_half_beside_it() {
        // Given a private key of alice's with no .pub beside it
        let host = a_host_where_alice_has_a_key();

        // When she is offered her keys
        let offered = host.paths_offered();

        // Then
        assert_eq!(offered, Vec::<PathBuf>::new());
    }

    /// The list an operator reads is the same list twice, so a key does not move under the cursor
    /// between one call and the next.
    #[test]
    fn offers_the_keys_in_a_stable_order() {
        // Given three keypairs of alice's, created in an order that is not their sorted order
        let host = a_host_where_alice_lives();
        host.also_has_the_keypair("id_rsa");
        host.also_has_the_keypair("deploy_key");
        host.also_has_the_keypair("id_ed25519");

        // When she is offered her keys
        let offered = host.paths_offered();

        // Then
        assert_eq!(
            offered,
            vec![
                host.ssh_dir().join("deploy_key"),
                host.ssh_dir().join("id_ed25519"),
                host.ssh_dir().join("id_rsa"),
            ]
        );
        assert_eq!(offered, host.paths_offered(), "the order is not stable");
    }

    /// **The directory oracle.** Told apart, these two answer "does this user have a `~/.ssh`, and
    /// can this host get into it?" — a question an authenticated session has no business being able
    /// to ask, and the same collapse [`KEY_UNREADABLE`] performs for a read.
    #[test]
    fn offers_nothing_for_an_absent_ssh_directory_and_nothing_for_an_unreadable_one() {
        // Given a host where alice has no ~/.ssh, and one where hers cannot be read
        let absent = a_host_where_alice_lives().where_she_has_no_ssh_dir();
        let unreadable = a_host_where_alice_lives().where_her_ssh_dir_cannot_be_read();
        unreadable.also_has_the_keypair("id_ed25519");

        // When she is offered her keys on each
        let while_absent = absent.candidates();
        let while_unreadable = unreadable.candidates();

        // Then
        assert_eq!(
            while_unreadable, while_absent,
            "the listing says whether the directory is there"
        );
        assert_eq!(while_absent, Vec::new());
    }

    /// **The two halves must agree.** A listing that offered a path the add then refuses is a
    /// picker whose choices do not work, and the confinement is the half that cannot bend.
    #[test]
    fn offers_only_paths_that_an_add_of_the_same_key_accepts() {
        // Given keypairs of alice's in her ~/.ssh
        let host = a_host_where_alice_lives();
        host.also_has_the_keypair("id_ed25519");
        host.also_has_the_keypair("deploy_key");

        // When each offered key is then named to the read an add performs
        let offered = host.candidates();
        assert_eq!(offered.len(), 2, "both of her keys should be offered");

        // Then
        for candidate in offered {
            let read = host.read(&candidate.path);
            assert!(
                read.is_ok(),
                "{} was offered but the add refused it: {read:?}",
                candidate.path.display()
            );
        }
    }
}
