//! The daemon's own cryptographic identity, and the port that turns other daemons' key ids into
//! public keys.
//!
//! A daemon generates an Ed25519 keypair for itself the first time it boots and keeps it at
//! `<auth dir>/signing_key.pem`, mode `0600` (see [`signing_key_path`] for which directory). That
//! key is what signs session tokens ([`tddy_github::session_token_v2`]), so a daemon can
//! authenticate its own users with no `livekit.api_secret` configured and no secret shared with
//! anybody.
//!
//! Verifying a token a *peer* minted needs that peer's public key, which is a lookup this crate
//! deliberately does not perform: [`KeyDirectory`] is a **port**, implemented by whoever owns a
//! transport the fleet can publish on. That keeps `tddy-daemon-livekit` off this crate's
//! dependency path — a rule its own `dependency_boundary_unit` asserts — and it keeps a desktop
//! daemon, which has no fleet at all, from needing one.

use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::SystemTime;

use anyhow::Context as _;
use async_trait::async_trait;
use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use ed25519_dalek::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use ed25519_dalek::{SigningKey, VerifyingKey};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_github::session_token_v2::{
    spki_der, KeyId, SessionClaims, SessionTokenAuthority, SessionTokenError, SessionTokenSigner,
    SessionTokenVerifier,
};

/// The file name a daemon's keypair takes inside its auth directory.
pub const SIGNING_KEY_FILE: &str = "signing_key.pem";

/// The subdirectory of the data directory that holds auth state when `auth_storage` is not set —
/// the same `<home>/auth` layout `./install` gives `auth_storage` when it does set it.
const DEFAULT_AUTH_SUBDIR: &str = "auth";

/// The private half is readable by its owner and nobody else, from the moment the file exists.
const OWNER_ONLY_FILE: u32 = 0o600;

/// A directory the daemon creates to hold the key is listable by its owner alone.
const OWNER_ONLY_DIR: u32 = 0o700;

/// The permission bits of a file mode — everything but the file type.
const PERMISSION_BITS: u32 = 0o777;

/// The group and other permission bits: any of them set means another account can reach the file.
const GROUP_OR_OTHER_BITS: u32 = 0o077;

/// Where the daemon described by `config` keeps its signing key.
///
/// `auth_storage` when it is configured — it is the directory an operator named for auth state,
/// and a signing key is auth state. Otherwise the `auth` directory under `tddy_data_dir`, which is
/// exactly where `./install` points `auth_storage` anyway. The key has to live *somewhere* a
/// restart finds it: a daemon whose identity changed on every boot would invalidate every live
/// session and be a stranger to every peer that had learned it.
///
/// **Refuses** a config naming neither. Resolving the data directory (`TDDY_DATA_DIR`, the build
/// profile's default, `$HOME/.tddy`) is the runtime's rule, and `runtime::build` pins its answer
/// into `tddy_data_dir` before it asks; a second copy of that rule here would be a second place a
/// private key could land, and one that guessed `/root` with `HOME` unset.
///
/// Changing either setting moves the key, and a daemon that finds no key where it looks
/// generates a new identity — so moving `auth_storage` means moving `signing_key.pem` with it.
pub fn signing_key_path(config: &DaemonConfig) -> anyhow::Result<PathBuf> {
    signing_key_dir(config).map(|(_, dir)| dir.join(SIGNING_KEY_FILE))
}

/// The directory that holds the signing key, and the setting that chose it.
fn signing_key_dir(config: &DaemonConfig) -> anyhow::Result<(&'static str, PathBuf)> {
    match (&config.auth_storage, &config.tddy_data_dir) {
        (Some(auth_storage), _) => Ok(("config.auth_storage", auth_storage.clone())),
        (None, Some(data_dir)) => Ok(("config.tddy_data_dir", data_dir.join(DEFAULT_AUTH_SUBDIR))),
        (None, None) => anyhow::bail!(
            "neither config.auth_storage nor config.tddy_data_dir names a directory for this \
             daemon's session-token signing key"
        ),
    }
}

/// Load the signing key of the daemon described by `config`, generating it on first boot.
///
/// A failure names the setting that chose the path, because that is what an operator changes:
/// `config.auth_storage` when it is set, `config.tddy_data_dir` otherwise.
pub fn load_signing_key(config: &DaemonConfig) -> anyhow::Result<DaemonSigningKey> {
    let (setting, dir) = signing_key_dir(config)?;
    let path = dir.join(SIGNING_KEY_FILE);
    DaemonSigningKey::load_or_generate(&path).with_context(|| {
        format!(
            "{setting} ({}) cannot hold this daemon's session-token signing key {}; no session \
             can be signed or verified until it can",
            dir.display(),
            path.display()
        )
    })
}

/// This daemon's Ed25519 identity: generated once, persisted, and never leaving the host.
pub struct DaemonSigningKey {
    signing_key: SigningKey,
    key_id: KeyId,
}

impl DaemonSigningKey {
    /// Load the keypair at `path`, generating and writing one when the file is absent.
    ///
    /// Writes with mode `0600` and refuses to load a file that is readable by anyone else: a
    /// signing key that a second account can read is a signing key the fleet cannot attribute.
    /// There is **no** in-memory fallback for an unwritable path — a daemon that cannot persist
    /// its identity would mint a fresh one on every restart and invalidate every live session.
    pub fn load_or_generate(path: &Path) -> anyhow::Result<Self> {
        match std::fs::read(path) {
            Ok(pem) => Self::from_stored(path, &pem),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Self::generate_into(path),
            Err(e) => Err(e).with_context(|| format!("reading the signing key {}", path.display())),
        }
    }

    /// This daemon's key id — what its tokens stamp and what peers resolve.
    pub fn key_id(&self) -> KeyId {
        self.key_id.clone()
    }

    /// The public half — what peers verify this daemon's tokens with.
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// The public half in SPKI DER, which is what goes on the wire and what [`KeyId`] digests.
    pub fn public_spki_der(&self) -> Vec<u8> {
        spki_der(&self.verifying_key())
    }

    /// A signer over this key. Cheap; the key is copied, not shared.
    pub fn signer(&self) -> SessionTokenSigner {
        SessionTokenSigner::new(self.signing_key.clone())
    }

    fn from_signing_key(signing_key: SigningKey) -> Self {
        let key_id = KeyId::of(&signing_key.verifying_key());
        Self {
            signing_key,
            key_id,
        }
    }

    /// Parse a key file that already exists — never regenerating over it.
    ///
    /// A file that is present but unusable is a damaged or exposed identity, and replacing it would
    /// destroy the only copy of the key every peer has learned. The operator decides what happens
    /// to it; the daemon refuses to start.
    fn from_stored(path: &Path, pem: &[u8]) -> anyhow::Result<Self> {
        refuse_if_readable_by_others(path)?;
        let pem = std::str::from_utf8(pem)
            .map_err(|_| anyhow::anyhow!("{} is not a PEM signing key", path.display()))?;
        let signing_key = SigningKey::from_pkcs8_pem(pem).map_err(|e| {
            anyhow::anyhow!(
                "{} is not a usable Ed25519 signing key: {e}",
                path.display()
            )
        })?;
        Ok(Self::from_signing_key(signing_key))
    }

    /// Generate a key and publish it at `path` — unless another process got there first.
    ///
    /// The key is written to a private staging file and then *hard-linked* into place, because a
    /// link, unlike a rename, refuses to replace a file that exists. Two daemons booting against
    /// one data directory would otherwise each generate a key and the second rename would
    /// silently replace the first's — leaving the first signing with a key no restart will find.
    /// The loser discards its key and loads the winner's.
    fn generate_into(path: &Path) -> anyhow::Result<Self> {
        let signing_key = SigningKey::generate(&mut rand_core::OsRng);
        let pem = signing_key
            .to_pkcs8_pem(LineEnding::LF)
            .map_err(|e| anyhow::anyhow!("encoding a new signing key: {e}"))?;
        if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            create_owner_only_dir(dir)
                .with_context(|| format!("creating the signing key directory {}", dir.display()))?;
        }
        let staging = staging_path(path);
        // `write_atomic_with_mode`, not `write_atomic`: the mode is applied when the file is
        // created, so the private key is never on disk at the process umask, not even briefly.
        tddy_core::atomic_file::write_atomic_with_mode(&staging, pem.as_bytes(), OWNER_ONLY_FILE)
            .with_context(|| format!("writing the signing key {}", staging.display()))?;
        let published = std::fs::hard_link(&staging, path);
        // The staging name is this call's alone, published or not.
        let _ = std::fs::remove_file(&staging);
        match published {
            Ok(()) => {
                let key = Self::from_signing_key(signing_key);
                log::info!(
                    target: crate::AUTH_LOG_TARGET,
                    "generated this daemon's session-token signing key {} at {}",
                    key.key_id,
                    path.display()
                );
                Ok(key)
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Self::adopt_concurrent_key(path),
            Err(e) => {
                Err(e).with_context(|| format!("publishing the signing key {}", path.display()))
            }
        }
    }

    /// Load the key another process published at `path` while this one was generating its own —
    /// the loser of a first-boot race keeps the winner's identity, never a second one.
    fn adopt_concurrent_key(path: &Path) -> anyhow::Result<Self> {
        let pem = std::fs::read(path).with_context(|| {
            format!(
                "reading the signing key {} another process wrote",
                path.display()
            )
        })?;
        Self::from_stored(path, &pem)
    }
}

/// A sibling of `path` no other call will name.
fn staging_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| SIGNING_KEY_FILE.to_string());
    path.with_file_name(format!(".{name}.{}.new", uuid::Uuid::new_v4()))
}

#[cfg(unix)]
fn create_owner_only_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(OWNER_ONLY_DIR)
        .create(dir)
}

#[cfg(not(unix))]
fn create_owner_only_dir(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)
}

/// Refuse a key file any other account can read or write.
///
/// There is deliberately no repair-and-continue: a key that has been readable may already have
/// been read, and tightening the mode would hide that from the one person who can judge it.
#[cfg(unix)]
fn refuse_if_readable_by_others(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path)
        .with_context(|| format!("inspecting the signing key {}", path.display()))?
        .permissions()
        .mode()
        & PERMISSION_BITS;
    if mode & GROUP_OR_OTHER_BITS != 0 {
        anyhow::bail!(
            "the signing key {} has mode {mode:03o}, so accounts other than its owner can reach \
             it. A key that has been readable may already have been read: if that is acceptable, \
             restore mode 600; otherwise delete it, and this daemon generates a new identity \
             (every session it issued ends)",
            path.display()
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn refuse_if_readable_by_others(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

/// Why a directory holding auth state is more open than it should be, if it is.
///
/// A directory's mode governs listing and traversal, not the contents of the `0600` files inside
/// it, so this is a warning and not a refusal — and the daemon does not re-impose `0700` itself,
/// because an operator's deliberate `chmod` is theirs to make. What it must not do is let a
/// directory holding a private signing key drift open with nothing ever saying so.
#[cfg(unix)]
pub fn auth_storage_looser_than_owner_only(dir: &Path) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(dir).ok()?.permissions().mode() & PERMISSION_BITS;
    (mode & GROUP_OR_OTHER_BITS != 0).then(|| {
        format!(
            "auth_storage {} has mode {mode:03o}: other accounts can list the directory that \
             holds this daemon's signing key and retained GitHub tokens. The files themselves are \
             0600; run `chmod 700 {}` unless the wider mode is deliberate",
            dir.display(),
            dir.display()
        )
    })
}

#[cfg(not(unix))]
pub fn auth_storage_looser_than_owner_only(_dir: &Path) -> Option<String> {
    None
}

/// Where a daemon learns other daemons' public keys.
///
/// A port rather than a concrete type because the answer is deployment-shaped: a LiveKit fleet
/// resolves it off the common room, and a desktop install has no peers at all and answers `None`
/// to every id — its own key is held by the verifier directly.
///
/// Resolving is all a directory does. How a daemon's own key reaches its peers is the transport's
/// business, not this port's: on a LiveKit fleet the key rides in the common-room advertisement the
/// discovery loop publishes on every connection (`tddy_daemon_livekit::AdvertisedSigningKey`), and
/// that loop lives in a crate this one may not depend on.
///
/// **An implementation answers from what it already holds.** Every token-gated RPC on the daemon
/// authenticates through the synchronous `SessionUserResolver`, which polls
/// [`KeyDirectory::public_key_for`] exactly once
/// ([`DirectorySessionTokenVerifier::verify_now`]); a lookup still pending on that poll is refused
/// as an unknown key. See that method for what this asks of an I/O-backed directory.
#[async_trait]
pub trait KeyDirectory: Send + Sync {
    /// The public key for `key_id`, or `None` when no peer has announced it.
    ///
    /// `None` is a real answer — an unknown daemon, or one that has not announced yet — and is
    /// distinct from `Err`, which means the lookup itself failed.
    async fn public_key_for(&self, key_id: &KeyId) -> anyhow::Result<Option<VerifyingKey>>;
}

/// The directory of a daemon that belongs to no fleet: it knows no key but its own — which
/// [`DirectorySessionTokenVerifier`] holds directly.
///
/// Tddy Desktop's shape, and every daemon's without a common room.
pub struct StandaloneKeyDirectory;

#[async_trait]
impl KeyDirectory for StandaloneKeyDirectory {
    async fn public_key_for(&self, _key_id: &KeyId) -> anyhow::Result<Option<VerifyingKey>> {
        Ok(None)
    }
}

/// Verifies any daemon's session token: reads the `kid`, resolves it, then checks the signature.
///
/// The local key is held directly rather than looked up, so a daemon can always verify the tokens
/// it minted itself — including a desktop daemon whose directory knows nobody.
pub struct DirectorySessionTokenVerifier {
    local_key_id: KeyId,
    local_public_key: VerifyingKey,
    directory: Arc<dyn KeyDirectory>,
}

impl DirectorySessionTokenVerifier {
    pub fn new(local: &DaemonSigningKey, directory: Arc<dyn KeyDirectory>) -> Self {
        Self {
            local_key_id: local.key_id(),
            local_public_key: local.verifying_key(),
            directory,
        }
    }

    /// Verify `token`, resolving whichever daemon signed it.
    ///
    /// Returns [`SessionTokenError::UnknownKeyId`] when the token is well-formed but names a key
    /// this daemon has not learned — the one rejection a later retry can resolve.
    pub async fn verify(&self, token: &str) -> Result<SessionClaims, SessionTokenError> {
        let key_id = SessionTokenVerifier::key_id_of(token)?;
        let key = if key_id == self.local_key_id {
            self.local_public_key
        } else {
            match self.directory.public_key_for(&key_id).await {
                Ok(Some(key)) => key,
                Ok(None) => return Err(SessionTokenError::UnknownKeyId(key_id)),
                Err(e) => {
                    log::warn!(
                        target: crate::AUTH_LOG_TARGET,
                        "could not look up the public key for session-token key id {key_id}: {e}"
                    );
                    return Err(SessionTokenError::UnknownKeyId(key_id));
                }
            }
        };
        SessionTokenVerifier::verify(token, &key, SystemTime::now())
    }

    /// [`Self::verify`], for the synchronous gates every daemon RPC authenticates through.
    ///
    /// **Polls the verification exactly once**, with a no-op waker, and never again. A verification
    /// that is `Ready` on that poll is the answer; one that is `Pending` is refused as
    /// [`SessionTokenError::UnknownKeyId`] and dropped — it is not waited for, re-polled or
    /// retried, because blocking an RPC worker on a directory's I/O would stall every caller
    /// behind it, and this gate (`SessionUserResolver`) is a synchronous closure with 60-odd call
    /// sites.
    ///
    /// Safe with the directories that exist: the local key is compared without a lookup,
    /// [`StandaloneKeyDirectory`] answers immediately, and the common-room directory reads an
    /// in-memory snapshot behind a `std` lock — none of them ever awaits, so the single poll is the
    /// whole verification. A **future I/O-backed directory** (a network fetch, a file read, an async
    /// lock) must not await inside `public_key_for` on this path: it must keep a local view that a
    /// background task refreshes and answer from it, as the common-room directory does. One that
    /// awaited would not fail loudly — it would refuse every peer token as an unknown key, with a
    /// warning in the log naming the key id.
    pub fn verify_now(&self, token: &str) -> Result<SessionClaims, SessionTokenError> {
        let mut verification = pin!(self.verify(token));
        match verification
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(outcome) => outcome,
            Poll::Pending => {
                let key_id = SessionTokenVerifier::key_id_of(token)?;
                log::warn!(
                    target: crate::AUTH_LOG_TARGET,
                    "the key directory could not answer for key id {key_id} without waiting; \
                     refusing the token"
                );
                Err(SessionTokenError::UnknownKeyId(key_id))
            }
        }
    }
}

#[async_trait]
impl SessionTokenAuthority for DirectorySessionTokenVerifier {
    async fn verify(&self, token: &str) -> Result<SessionClaims, SessionTokenError> {
        DirectorySessionTokenVerifier::verify(self, token).await
    }
}

/// Everything a daemon signs and verifies session tokens with: its own signer, and a verifier for
/// every daemon's tokens, its own included.
///
/// One value, built once per daemon, so the login flow, the local-socket mint and a split
/// session's agent credential all sign with the same key the fleet was told about.
#[derive(Clone)]
pub struct SessionTokens {
    signer: SessionTokenSigner,
    verifier: Arc<DirectorySessionTokenVerifier>,
}

impl SessionTokens {
    pub fn new(key: &DaemonSigningKey, directory: Arc<dyn KeyDirectory>) -> Self {
        Self {
            signer: key.signer(),
            verifier: Arc::new(DirectorySessionTokenVerifier::new(key, directory)),
        }
    }

    /// Signs with this daemon's own key.
    pub fn signer(&self) -> &SessionTokenSigner {
        &self.signer
    }

    /// Verifies this daemon's tokens and every peer's the directory can resolve.
    pub fn verifier(&self) -> &Arc<DirectorySessionTokenVerifier> {
        &self.verifier
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn generates_a_keypair_on_first_use_and_reuses_it_across_restarts() {
        // Given a daemon that has booted once and generated its identity
        let home = a_data_directory();
        let first_boot = DaemonSigningKey::load_or_generate(&the_key_path(&home))
            .expect("a daemon generates a keypair on first use");

        let on_disk_before = std::fs::read(the_key_path(&home)).expect("the key file exists");

        // When it restarts and loads from the same data directory
        let after_restart = DaemonSigningKey::load_or_generate(&the_key_path(&home))
            .expect("a daemon reuses the keypair it already generated");

        // Then it is the same identity, read from a file left exactly as it was and still owner-only
        // — a fresh key on every restart would invalidate every live session and make the daemon a
        // stranger to every peer that had learned it
        let on_disk_after = std::fs::read(the_key_path(&home)).expect("the key file exists");
        assert_eq!(
            (
                after_restart.key_id() == first_boot.key_id(),
                on_disk_after == on_disk_before,
                mode_of(&the_key_path(&home)),
            ),
            (true, true, 0o600)
        );
    }

    #[test]
    fn two_daemons_generate_two_different_identities() {
        // Given two daemons with data directories of their own
        let one = a_data_directory();
        let other = a_data_directory();

        // When each generates its identity
        let one = DaemonSigningKey::load_or_generate(&the_key_path(&one)).expect("one generates");
        let other =
            DaemonSigningKey::load_or_generate(&the_key_path(&other)).expect("the other generates");

        // Then they are distinct, which is what lets a fleet attribute a token to its signer
        assert_ne!(one.key_id(), other.key_id());
    }

    #[test]
    fn writes_the_private_key_readable_only_by_the_account_that_owns_it() {
        // Given a daemon generating its identity for the first time
        let home = a_data_directory();
        let path = the_key_path(&home);
        DaemonSigningKey::load_or_generate(&path).expect("a daemon generates a keypair");

        // When the file it wrote is inspected
        let mode = mode_of(&path);

        // Then no other account can read it — a signing key a second account can read is a signing
        // key the fleet cannot attribute to one daemon
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn refuses_a_key_file_other_accounts_can_read() {
        // Given a key file left world-readable
        let home = a_data_directory();
        let path = the_key_path(&home);
        DaemonSigningKey::load_or_generate(&path).expect("a daemon generates a keypair");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("the permissions are loosened");

        // When the daemon restarts and loads it
        let refusal = DaemonSigningKey::load_or_generate(&path)
            .err()
            .map(|e| format!("{e:#}"));

        // Then it refuses rather than signing with a key anybody on the host could have taken —
        // for its mode, not for some other fault — and leaves the file as it found it. There is
        // deliberately no repair-and-continue: a key that has been readable may already have been
        // read, and tightening the mode would hide that.
        assert!(
            refusal.as_deref().is_some_and(|e| e.contains("mode 644")),
            "a signing key readable by other accounts must be refused for its mode, got {refusal:?}"
        );
        assert_eq!(
            mode_of(&path),
            0o644,
            "the refusal must not re-secure the file"
        );
    }

    #[test]
    fn the_key_id_is_the_one_its_own_tokens_carry() {
        // Given a daemon's identity
        let home = a_data_directory();
        let daemon = DaemonSigningKey::load_or_generate(&the_key_path(&home))
            .expect("a daemon generates a keypair");

        // When its signer stamps a token
        let token = daemon.signer().mint_access(&an_operator());

        // Then the id the token names is the id the daemon publishes — otherwise a peer resolves
        // a key that cannot verify what it was fetched for
        assert_eq!(SessionTokenVerifier::key_id_of(&token), Ok(daemon.key_id()));
    }

    #[test]
    fn the_key_id_is_derived_from_the_public_half_it_publishes() {
        // Given a daemon's identity
        let home = a_data_directory();
        let daemon = DaemonSigningKey::load_or_generate(&the_key_path(&home))
            .expect("a daemon generates a keypair");

        // Then its id is exactly what a peer computes from the public key it receives, so no
        // separate announcement of the id can disagree with the key
        assert_eq!(daemon.key_id(), KeyId::of(&daemon.verifying_key()));
    }

    #[test]
    fn warns_about_an_auth_storage_directory_other_accounts_can_list() {
        // Given an auth_storage directory an operator left group- and world-readable
        let auth_storage = a_data_directory();
        std::fs::set_permissions(auth_storage.path(), std::fs::Permissions::from_mode(0o755))
            .expect("the permissions are loosened");

        // When its posture is checked
        let warning = auth_storage_looser_than_owner_only(auth_storage.path());

        // Then the operator is told, naming the mode they would have to change
        assert!(
            warning.as_deref().is_some_and(|w| w.contains("755")),
            "a listable auth_storage must be reported, got {warning:?}"
        );
    }

    #[test]
    fn says_nothing_about_an_auth_storage_directory_only_its_owner_can_list() {
        // Given an owner-only auth_storage directory
        let auth_storage = a_data_directory();
        std::fs::set_permissions(auth_storage.path(), std::fs::Permissions::from_mode(0o700))
            .expect("the permissions are set");

        // When its posture is checked
        let warning = auth_storage_looser_than_owner_only(auth_storage.path());

        // Then there is nothing to say
        assert_eq!(warning, None);
    }

    #[test]
    fn keeps_the_signing_key_in_the_auth_storage_an_operator_configured() {
        // Given a daemon whose operator named an auth_storage directory
        let config = DaemonConfig {
            auth_storage: Some(PathBuf::from("/var/lib/tddy/auth")),
            tddy_data_dir: Some(PathBuf::from("/home/operator/.tddy")),
            ..DaemonConfig::default()
        };

        // Then the key lives beside the rest of its auth state, not in the data directory
        assert_eq!(
            signing_key_path(&config).ok(),
            Some(PathBuf::from("/var/lib/tddy/auth").join(SIGNING_KEY_FILE))
        );
    }

    #[test]
    fn keeps_the_signing_key_under_the_data_directory_when_no_auth_storage_is_configured() {
        // Given a daemon with a data directory and no auth_storage
        let config = DaemonConfig {
            tddy_data_dir: Some(PathBuf::from("/home/operator/.tddy")),
            ..DaemonConfig::default()
        };

        // Then the key lives where `./install` would have pointed auth_storage
        assert_eq!(
            signing_key_path(&config).ok(),
            Some(PathBuf::from("/home/operator/.tddy/auth").join(SIGNING_KEY_FILE))
        );
    }

    #[test]
    fn refuses_to_place_a_signing_key_when_no_directory_is_configured() {
        // Given a config naming neither auth_storage nor a data directory
        let config = DaemonConfig::default();

        // When the key's path is asked for
        let refusal = signing_key_path(&config).expect_err("there is nowhere to put the key");

        // Then it is refused, naming both settings — never guessed from `$HOME` or `/root`
        let message = refusal.to_string();
        assert!(
            message.contains("config.auth_storage") && message.contains("config.tddy_data_dir"),
            "the refusal must name the settings that would place the key, got: {message}"
        );
    }

    #[test]
    fn refuses_a_peer_token_whose_key_the_directory_cannot_answer_for_without_waiting() {
        // Given a peer's token, and a directory whose lookup would have to wait on I/O
        let home = a_data_directory();
        let daemon = DaemonSigningKey::load_or_generate(&the_key_path(&home))
            .expect("a daemon generates a keypair");
        let peer_home = a_data_directory();
        let peer = DaemonSigningKey::load_or_generate(&the_key_path(&peer_home))
            .expect("a peer generates a keypair");
        let token = peer.signer().mint_access(&an_operator());
        let verifier = DirectorySessionTokenVerifier::new(&daemon, Arc::new(AlwaysWaiting));

        // When the synchronous gate verifies it
        let refusal = verifier.verify_now(&token);

        // Then it is refused as an unknown key rather than blocking the RPC worker
        assert_eq!(
            refusal.map(|_| ()),
            Err(SessionTokenError::UnknownKeyId(peer.key_id()))
        );
    }

    /// A directory whose every lookup is still in flight.
    struct AlwaysWaiting;

    #[async_trait]
    impl KeyDirectory for AlwaysWaiting {
        async fn public_key_for(&self, _: &KeyId) -> anyhow::Result<Option<VerifyingKey>> {
            std::future::pending().await
        }
    }

    fn an_operator() -> tddy_github::GitHubUser {
        tddy_github::GitHubUser {
            id: 1,
            login: "operator".to_string(),
            avatar_url: String::new(),
            name: "operator".to_string(),
        }
    }

    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path)
            .expect("the key file exists")
            .permissions()
            .mode()
            & PERMISSION_BITS
    }

    fn a_data_directory() -> tempfile::TempDir {
        tempfile::tempdir().expect("a temporary directory")
    }

    fn the_key_path(home: &tempfile::TempDir) -> std::path::PathBuf {
        home.path().join(SIGNING_KEY_FILE)
    }
}
