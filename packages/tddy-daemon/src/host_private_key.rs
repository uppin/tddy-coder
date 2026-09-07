//! Reading the private key an operator names — as that operator, and only from their own home.
//!
//! `AddHostKeyRequest.subject` is free text from a browser. Everything else in this flow already
//! runs per user: node 4's probes shell out through [`crate::spawner::start_output_as_user`] because
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
//! Feature: `docs/ft/web/1-WIP/PRD-2026-09-06-agent-add-key.md`

use ssh_key::PrivateKey;
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
}

/// The live reader: passwd for the home directory, a child process for the read.
pub struct SpawnedHostUserFiles;

impl HostUserFiles for SpawnedHostUserFiles {
    fn home_dir(&self, os_user: &str) -> Result<PathBuf, String> {
        crate::user_sessions_path::home_dir_for_user(os_user)
            .ok_or_else(|| format!("{os_user} has no home directory on this host"))
    }

    /// Reads through `cat` run as `os_user`, which is how [`crate::spawner`] drops privileges for
    /// every other per-user read in this daemon.
    ///
    /// A child process rather than `seteuid` around a `read_to_string`: the daemon is
    /// multi-threaded, and a process-wide euid switch would apply to every other request in flight.
    #[cfg(unix)]
    fn read_as_user(&self, os_user: &str, path: &Path) -> Result<Vec<u8>, String> {
        // Resolved before spawning, exactly as node 4's probes resolve `git` and `gh`:
        // `run_capture_as_user` anchors a relative program to the daemon's own toolchain root and
        // never consults `PATH`, so a bare "cat" would try to exec `<daemon-cwd>/cat`.
        let cat = crate::spawner::find_program_on_spawn_child_path("cat")
            .ok_or_else(|| "this host has no `cat` to read a key with".to_string())?;
        // An OpenSSH private key is ASCII armour, so a lossy decode cannot alter one. A file that
        // is not one fails the parse, which is the same refusal a missing file gets.
        crate::spawner::run_capture_as_user(os_user, &cat, &[path.display().to_string()])
            .map(String::into_bytes)
            .map_err(|e| e.to_string())
    }

    #[cfg(not(unix))]
    fn read_as_user(&self, _os_user: &str, _path: &Path) -> Result<Vec<u8>, String> {
        Err("reading a file as another user is only supported on Unix".to_string())
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
/// [`crate::connection_service`] exercise the same flow end to end and must not grow a second,
/// divergent double of their own.
#[cfg(test)]
pub(crate) struct UserFilesUnder {
    home: PathBuf,
    /// Every OS user a read was attempted as, so a test can state *whose* privileges were used —
    /// the whole point of this seam.
    read_as: std::sync::Mutex<Vec<String>>,
}

#[cfg(test)]
impl UserFilesUnder {
    pub(crate) fn home(home: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            read_as: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// The OS users this reader was asked to read as, in order.
    pub(crate) fn users_read_as(&self) -> Vec<String> {
        self.read_as
            .lock()
            .expect("the recording lock is only held to push a user")
            .clone()
    }
}

#[cfg(test)]
impl HostUserFiles for UserFilesUnder {
    fn home_dir(&self, _os_user: &str) -> Result<PathBuf, String> {
        Ok(self.home.clone())
    }

    fn read_as_user(&self, os_user: &str, path: &Path) -> Result<Vec<u8>, String> {
        self.read_as
            .lock()
            .expect("the recording lock is only held to push a user")
            .push(os_user.to_string());
        std::fs::read(path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALICE: &str = "alice";

    /// A host where `alice` has a home directory, and a key inside it.
    struct AliceOnThisHost {
        files: UserFilesUnder,
        home: PathBuf,
        _storage: tempfile::TempDir,
    }

    fn a_host_where_alice_has_a_key() -> AliceOnThisHost {
        let storage = tempfile::tempdir().expect("a temp directory for this host");
        let home = storage.path().join("home").join(ALICE);
        std::fs::create_dir_all(home.join(".ssh")).expect("alice has a ~/.ssh");
        write_a_private_key_at(&home.join(".ssh").join("id_ed25519"));
        AliceOnThisHost {
            files: UserFilesUnder::home(&home),
            home,
            _storage: storage,
        }
    }

    impl AliceOnThisHost {
        fn read(&self, subject: &Path) -> Result<PrivateKey, String> {
            read_private_key(&self.files, ALICE, &subject.display().to_string())
        }

        fn her_key(&self) -> PathBuf {
            self.home.join(".ssh").join("id_ed25519")
        }
    }

    /// A freshly generated, unencrypted ed25519 key in the format `ssh-keygen` writes.
    fn write_a_private_key_at(path: &Path) {
        let key = PrivateKey::random(&mut rand::thread_rng(), ssh_key::Algorithm::Ed25519)
            .expect("an ed25519 private key");
        let openssh = key
            .to_openssh(ssh_key::LineEnding::LF)
            .expect("in the format ssh-keygen writes");
        std::fs::write(path, openssh.as_bytes()).expect("the key file is written");
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
}
