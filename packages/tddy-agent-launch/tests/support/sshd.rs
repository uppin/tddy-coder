//! A user-mode `sshd` on an ephemeral loopback port, serving one bare repository to one key.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use tddy_testing_commons::wait::eventually_blocking;

/// What the client believes about the server's host key.
#[derive(Clone, Copy)]
pub enum HostTrust {
    /// Accepts the key unseen, so no host-key question is ever asked.
    Trusted,
    /// Has never seen the key and is told to ask — ssh then asks through its askpass helper.
    Unknown,
}

/// How the client's key relates to what the server will accept.
#[derive(Clone, Copy)]
enum KeyKind {
    PassphraseProtected(&'static str),
    Unprotected,
    UnprotectedButNotAuthorized,
}

pub struct SshdBuilder {
    key: KeyKind,
    key_dir_depth: usize,
    trust: HostTrust,
}

pub fn an_sshd() -> SshdBuilder {
    SshdBuilder {
        key: KeyKind::Unprotected,
        key_dir_depth: 0,
        trust: HostTrust::Trusted,
    }
}

impl SshdBuilder {
    pub fn authorizing_a_passphrase_protected_key(mut self, passphrase: &'static str) -> Self {
        self.key = KeyKind::PassphraseProtected(passphrase);
        self
    }

    pub fn rejecting_the_clients_unprotected_key(mut self) -> Self {
        self.key = KeyKind::UnprotectedButNotAuthorized;
        self
    }

    /// Put the client's key under `depth` nested directories, so its path outgrows the 100
    /// characters ssh shows in a passphrase prompt.
    pub fn with_the_key_deep_under_the_home(mut self, depth: usize) -> Self {
        self.key_dir_depth = depth;
        self
    }

    pub fn unknown_to_the_client(mut self) -> Self {
        self.trust = HostTrust::Unknown;
        self
    }

    pub fn started(self) -> SshdFixture {
        SshdFixture::start(self)
    }
}

pub struct SshdFixture {
    dir: tempfile::TempDir,
    port: u16,
    child: Child,
    key: PathBuf,
    trust: HostTrust,
    repo: PathBuf,
}

impl SshdFixture {
    fn start(builder: SshdBuilder) -> Self {
        let dir = tempfile::tempdir().expect("a directory for the sshd fixture");
        let root = dir.path();

        run(Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", ""])
            .arg("-f")
            .arg(root.join("hostkey")));

        let mut key_dir = root.join("client");
        for level in 0..builder.key_dir_depth {
            key_dir = key_dir.join(format!("a-deliberately-long-directory-name-{level}"));
        }
        std::fs::create_dir_all(&key_dir).expect("the client key directory");
        let key = key_dir.join("id_clone");
        let passphrase = match builder.key {
            KeyKind::PassphraseProtected(passphrase) => passphrase,
            KeyKind::Unprotected | KeyKind::UnprotectedButNotAuthorized => "",
        };
        run(Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", passphrase])
            .arg("-f")
            .arg(&key));

        let authorized = root.join("authorized_keys");
        match builder.key {
            KeyKind::UnprotectedButNotAuthorized => {
                // A different key is authorized, so the client's is valid but not welcome here.
                run(Command::new("ssh-keygen")
                    .args(["-q", "-t", "ed25519", "-N", ""])
                    .arg("-f")
                    .arg(root.join("someone-else")));
                std::fs::copy(root.join("someone-else.pub"), &authorized).expect("authorized_keys");
            }
            _ => {
                std::fs::copy(key.with_extension("pub"), &authorized).expect("authorized_keys");
            }
        }

        let repo = seed_a_bare_repository(root);

        let port = a_free_loopback_port();
        std::fs::write(
            root.join("sshd_config"),
            format!(
                "Port {port}\nListenAddress 127.0.0.1\nHostKey {host}\nAuthorizedKeysFile {authorized}\n\
                 PidFile {pid}\nUsePAM no\nStrictModes no\nPasswordAuthentication no\n\
                 KbdInteractiveAuthentication no\nLogLevel ERROR\n",
                host = root.join("hostkey").display(),
                authorized = authorized.display(),
                pid = root.join("sshd.pid").display(),
            ),
        )
        .expect("sshd_config");

        let child = Command::new(sshd_binary())
            .args(["-D", "-e", "-f"])
            .arg(root.join("sshd_config"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sshd starts");
        eventually_blocking(
            "the fixture sshd to accept connections",
            Duration::from_secs(10),
            || {
                std::net::TcpStream::connect(("127.0.0.1", port))
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
        );

        Self {
            dir,
            port,
            child,
            key,
            trust: builder.trust,
            repo,
        }
    }

    pub fn clone_url(&self) -> String {
        format!(
            "ssh://{}@127.0.0.1:{}{}",
            super::current_os_user(),
            self.port,
            self.repo.display()
        )
    }

    /// The ssh arguments that make this server reachable by this key and nothing else — what a
    /// real host would have in the OS user's `~/.ssh/config`, passed on the command line because a
    /// test cannot edit the real home.
    pub fn client_ssh_args(&self) -> Vec<String> {
        let known_hosts = match self.trust {
            HostTrust::Trusted => "/dev/null".to_string(),
            HostTrust::Unknown => {
                let empty = self.dir.path().join("known_hosts");
                std::fs::write(&empty, "").expect("an empty known_hosts");
                empty.display().to_string()
            }
        };
        let strictness = match self.trust {
            HostTrust::Trusted => "no",
            HostTrust::Unknown => "ask",
        };
        [
            "-F".to_string(),
            "/dev/null".to_string(),
            "-i".to_string(),
            self.key.display().to_string(),
            "-o".to_string(),
            "IdentitiesOnly=yes".to_string(),
            "-o".to_string(),
            format!("StrictHostKeyChecking={strictness}"),
            "-o".to_string(),
            format!("UserKnownHostsFile={known_hosts}"),
        ]
        .into_iter()
        .collect()
    }

    /// `SHA256:…`, in the form `ssh-add -l` prints.
    pub fn key_fingerprint(&self) -> String {
        let out = Command::new("ssh-keygen")
            .args(["-lf"])
            .arg(self.key.with_extension("pub"))
            .output()
            .expect("ssh-keygen -l runs");
        String::from_utf8(out.stdout)
            .expect("UTF-8")
            .split_whitespace()
            .nth(1)
            .expect("a fingerprint column")
            .to_string()
    }

    pub fn key_file_name(&self) -> &str {
        self.key
            .file_name()
            .and_then(|n| n.to_str())
            .expect("a UTF-8 key file name")
    }

    /// The commit the served repository is at.
    pub fn repo_head(&self) -> String {
        git_head(&self.repo)
    }
}

impl Drop for SshdFixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The commit a checkout is at.
pub fn git_head(repo: &Path) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse runs");
    assert!(out.status.success(), "{} has no HEAD", repo.display());
    String::from_utf8(out.stdout)
        .expect("UTF-8")
        .trim()
        .to_string()
}

fn seed_a_bare_repository(root: &Path) -> PathBuf {
    let bare = root.join("served.git");
    let seed = root.join("seed");
    run(Command::new("git")
        .args(["init", "-q", "--bare"])
        .arg(&bare));
    run(Command::new("git").args(["init", "-q"]).arg(&seed));
    run(Command::new("git").arg("-C").arg(&seed).args([
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "seed",
    ]));
    run(Command::new("git")
        .arg("-C")
        .arg(&seed)
        .args(["push", "-q"])
        .arg(&bare)
        .arg("HEAD:refs/heads/main"));
    // A bare repo's HEAD names a branch; point it at the one just pushed.
    run(Command::new("git")
        .arg("-C")
        .arg(&bare)
        .args(["symbolic-ref", "HEAD", "refs/heads/main"]));
    bare
}

fn a_free_loopback_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("an ephemeral port")
        .local_addr()
        .expect("its address")
        .port()
}

/// `sshd` is not on a user's `PATH` on most systems; its usual homes are searched in order.
fn sshd_binary() -> PathBuf {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain([PathBuf::from("/usr/sbin"), PathBuf::from("/usr/local/sbin")])
        .map(|dir| dir.join("sshd"))
        .find(|candidate| candidate.is_file())
        .expect("an `sshd` binary (OpenSSH server) on PATH or in /usr/sbin — the nix dev shell provides one")
}

fn run(command: &mut Command) {
    let status = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("the fixture command starts");
    assert!(status.success(), "fixture command failed: {command:?}");
}
