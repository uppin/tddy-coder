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

const CONFIG_YAML: &str = r#"
users:
  - github_user: "testuser"
    os_user: "testdev"
"#;

/// Build a minimal [`DaemonConfig`] suitable for unit/acceptance tests.
pub fn test_config() -> DaemonConfig {
    let dir = tempfile::tempdir().expect("create temp dir for test config");
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, CONFIG_YAML).expect("write test config");
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
