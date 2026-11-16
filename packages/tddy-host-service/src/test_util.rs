//! Shared test helpers for `tddy-host-service`.
//!
//! The same two constants and the same shape as `tddy_daemon::test_util`, so a test that moved here
//! with its handler reads identically to the one it was. It is a second copy rather than a shared
//! one because `tddy-daemon` depends on this crate and not the other way round, and a test-only
//! dependency back would make the graph cyclic for a `TEST_TOKEN`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;

use crate::service::HostServiceImpl;

/// Token accepted by [`test_service`] as a valid session token.
pub const TEST_TOKEN: &str = "valid-token";
/// OS user returned for [`TEST_TOKEN`] by [`test_service`].
pub const TEST_USER: &str = "testuser";

const CONFIG_YAML: &str = r#""#;
const USERS_YAML: &str = "- github_user: \"testuser\"\n  os_user: \"testdev\"\n";

/// Build a minimal [`DaemonConfig`] suitable for unit/acceptance tests.
pub fn test_config() -> DaemonConfig {
    let dir = tempfile::tempdir().expect("create temp dir for test config");
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, CONFIG_YAML).expect("write test config");
    std::fs::write(dir.path().join("users.yaml"), USERS_YAML).expect("write test users");
    DaemonConfig::load(&path).expect("load test config")
}

/// The standard test token resolver: [`TEST_TOKEN`] is [`TEST_USER`], everything else is nobody.
pub fn test_user_resolver() -> SessionUserResolver {
    Arc::new(|token| {
        if token == TEST_TOKEN {
            Some(TEST_USER.to_string())
        } else {
            None
        }
    })
}

/// Build a [`HostServiceImpl`] rooted at `tddy_data_dir` with the standard test resolvers.
pub fn test_service(tddy_data_dir: &Path) -> HostServiceImpl {
    HostServiceImpl::new(test_config(), tddy_data_dir, test_user_resolver())
}

/// [`test_service`], for a caller that already owns the path.
pub fn test_service_owned(tddy_data_dir: PathBuf) -> HostServiceImpl {
    test_service(&tddy_data_dir)
}

/// Encrypt `plaintext` the way the browser does — RSA-OAEP(SHA-256) under a host's published SPKI
/// DER — so a test can answer a host prompt without a browser.
///
/// A separate implementation from `HostKeypair::decrypt`, on purpose: a round trip whose two halves
/// share code proves only that the code agrees with itself. This one goes through the `rsa` crate's
/// public API alone, the same independence `host_keypair`'s own round-trip test relies on.
pub fn encrypt_for_host(spki_der: &[u8], plaintext: &[u8]) -> Vec<u8> {
    use rsa::pkcs8::DecodePublicKey;
    use rsa::{Oaep, RsaPublicKey};

    let public_key =
        RsaPublicKey::from_public_key_der(spki_der).expect("a published SPKI DER public key");
    public_key
        .encrypt(
            &mut rand::thread_rng(),
            Oaep::new::<sha2::Sha256>(),
            plaintext,
        )
        .expect("a passphrase fits comfortably in an OAEP payload")
}

/// A throwaway X11 socket directory whose `X<N>` sockets are bound by the test, never by an X
/// server — so what is listed depends on nothing the machine running the suite has.
#[cfg(unix)]
pub struct AnX11SocketDir {
    dir: tempfile::TempDir,
    /// Held so the running displays keep answering for the length of the test.
    listeners: Vec<std::os::unix::net::UnixListener>,
}

/// An X11 socket directory with no displays in it yet.
#[cfg(unix)]
pub fn an_x11_socket_dir() -> AnX11SocketDir {
    AnX11SocketDir {
        dir: tempfile::tempdir().expect("a temporary X11 socket directory"),
        listeners: Vec::new(),
    }
}

#[cfg(unix)]
impl AnX11SocketDir {
    /// A display whose server is up: its socket accepts connections.
    pub fn with_a_running_display(mut self, number: u32) -> Self {
        let listener = std::os::unix::net::UnixListener::bind(self.socket_of(number))
            .expect("binding an X11 socket");
        self.listeners.push(listener);
        self
    }

    /// A display whose server is gone but left its socket file behind: the file is there, and a
    /// connect to it is refused.
    pub fn with_a_dead_display(self, number: u32) -> Self {
        drop(
            std::os::unix::net::UnixListener::bind(self.socket_of(number))
                .expect("binding an X11 socket"),
        );
        self
    }

    /// A socket that answers connects but whose `name` may not be a display's: what the name filter
    /// alone has to keep out.
    pub fn with_a_listening_socket_named(mut self, name: &str) -> Self {
        let listener = std::os::unix::net::UnixListener::bind(self.dir.path().join(name))
            .expect("binding a socket in the X11 socket directory");
        self.listeners.push(listener);
        self
    }

    /// Where display `number`'s socket is.
    pub fn socket_of(&self, number: u32) -> PathBuf {
        self.dir.path().join(format!("X{number}"))
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// One signed-in person's vault, as the Coder ports see it: the accounts it holds, each with its
/// deployment URL and token, and whether it is open.
///
/// It answers for its owner alone. Anybody else is a person with no Coder account, which is what
/// lets a test tell a listing made as the caller's subject from one made as their OS user. It can
/// be locked and unlocked while a test holds it, as a person's vault is between a start and a
/// resume.
pub struct ACoderVault {
    subject: String,
    locked: std::sync::atomic::AtomicBool,
    accounts: Vec<(
        crate::coder_derived_hosts::CoderAccountRef,
        crate::coder_workspaces::CoderCredentials,
    )>,
}

/// `subject`'s vault: open, and holding no Coder account yet.
pub fn a_coder_vault_of(subject: &str) -> ACoderVault {
    ACoderVault {
        subject: subject.to_string(),
        locked: std::sync::atomic::AtomicBool::new(false),
        accounts: Vec::new(),
    }
}

impl ACoderVault {
    /// This vault, also holding `account` with its `credentials`.
    #[must_use]
    pub fn holding(
        mut self,
        account: crate::coder_derived_hosts::CoderAccountRef,
        credentials: crate::coder_workspaces::CoderCredentials,
    ) -> Self {
        self.accounts.push((account, credentials));
        self
    }

    /// This vault, locked.
    #[must_use]
    pub fn locked(self) -> Self {
        self.lock();
        self
    }

    /// Lock the vault: nothing in it can be read until it is unlocked.
    pub fn lock(&self) {
        self.locked.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Unlock the vault again.
    pub fn unlock(&self) {
        self.locked
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    fn is_locked(&self) -> bool {
        self.locked.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl crate::coder_derived_hosts::CoderAccountSource for ACoderVault {
    fn accounts_of(
        &self,
        subject: &str,
    ) -> Result<
        Vec<crate::coder_derived_hosts::CoderAccountRef>,
        crate::coder_workspaces::CoderCredentialError,
    > {
        if subject != self.subject {
            return Ok(Vec::new());
        }
        if self.is_locked() {
            return Err(crate::coder_workspaces::CoderCredentialError::VaultLocked);
        }
        Ok(self
            .accounts
            .iter()
            .map(|(account, _)| account.clone())
            .collect())
    }
}

impl crate::coder_workspaces::CoderCredentialSource for ACoderVault {
    fn credentials_for(
        &self,
        subject: &str,
        account_id: &str,
    ) -> Result<
        crate::coder_workspaces::CoderCredentials,
        crate::coder_workspaces::CoderCredentialError,
    > {
        use crate::coder_workspaces::CoderCredentialError;

        if subject == self.subject && self.is_locked() {
            return Err(CoderCredentialError::VaultLocked);
        }
        self.accounts
            .iter()
            .find(|(account, _)| subject == self.subject && account.account_id == account_id)
            .map(|(_, credentials)| credentials.clone())
            .ok_or_else(|| CoderCredentialError::NoSuchAccount(account_id.to_string()))
    }
}

/// The Coder deployments a test's accounts are on, by their URL: each answers a listing with one
/// fixed outcome. A URL no answer was given for is a deployment nothing answers at.
#[derive(Default)]
pub struct CoderDeployments {
    answers: Vec<(
        String,
        Result<
            Vec<crate::coder_workspaces::CoderWorkspace>,
            crate::coder_workspaces::CoderWorkspaceListError,
        >,
    )>,
}

/// No deployments yet.
pub fn coder_deployments() -> CoderDeployments {
    CoderDeployments::default()
}

impl CoderDeployments {
    /// These deployments, with the one at `url` listing `workspaces`.
    #[must_use]
    pub fn listing(
        mut self,
        url: &str,
        workspaces: Vec<crate::coder_workspaces::CoderWorkspace>,
    ) -> Self {
        self.answers.push((url.to_string(), Ok(workspaces)));
        self
    }

    /// These deployments, with the one at `url` failing every listing with `error`.
    #[must_use]
    pub fn failing(
        mut self,
        url: &str,
        error: crate::coder_workspaces::CoderWorkspaceListError,
    ) -> Self {
        self.answers.push((url.to_string(), Err(error)));
        self
    }
}

#[async_trait::async_trait]
impl crate::coder_workspaces::CoderWorkspaceSource for CoderDeployments {
    async fn workspaces(
        &self,
        credentials: &crate::coder_workspaces::CoderCredentials,
    ) -> Result<
        Vec<crate::coder_workspaces::CoderWorkspace>,
        crate::coder_workspaces::CoderWorkspaceListError,
    > {
        self.answers
            .iter()
            .find(|(url, _)| *url == credentials.url)
            .map(|(_, answer)| answer.clone())
            .unwrap_or_else(|| {
                Err(
                    crate::coder_workspaces::CoderWorkspaceListError::Unreachable(format!(
                        "no stand-in deployment at {}",
                        credentials.url
                    )),
                )
            })
    }
}

/// `owner`'s workspace `name`, whose deployment reports `status`.
pub fn a_coder_workspace(
    owner: &str,
    name: &str,
    status: &str,
) -> crate::coder_workspaces::CoderWorkspace {
    crate::coder_workspaces::CoderWorkspace {
        owner: owner.to_string(),
        name: name.to_string(),
        status: status.to_string(),
    }
}
