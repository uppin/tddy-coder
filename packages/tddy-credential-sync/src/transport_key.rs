//! The key records are wrapped to, which is not the key the vault is sealed with.
//!
//! **Two daemons that sync end up sharing a record, never a key.** A payload is sealed under a
//! secret derived from the sender's transport key and the recipient's public half; the recipient
//! opens it, and then re-seals the contents under *its own* vault key before anything touches disk.
//! Compromising one daemon therefore does not unwrap the other's vault — which it would if the
//! obvious shortcut, shipping the data key, were taken.
//!
//! The public half is advertised through `#keyring` 1/9's `KeyDirectory` and **signed by the
//! identity key**, so substituting an attacker's transport key is a signature failure rather than a
//! silent downgrade.

use x25519_dalek::{PublicKey, StaticSecret};

use tddy_credentials::SecretBytes;

/// Domain separation for the sealing key an agreement is expanded into — see
/// [`VaultTransportKey::shared_secret`].
const SHARED_SECRET_INFO: &[u8] = b"tddy-credential-sync/v1/transport-shared-secret";

/// The file a daemon's transport key is persisted at, mode `0600`.
const TRANSPORT_KEY_FILE_LEN: usize = 32;

/// The public half of a daemon's transport key, as it is advertised.
///
/// Opaque bytes rather than a typed curve point at this layer: the wire format is 32 bytes and the
/// curve type belongs behind [`VaultTransportKey`], where the one implementation lives.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TransportPublicKey([u8; 32]);

impl TransportPublicKey {
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// This daemon's X25519 keypair for credential transport.
///
/// Separate from 1/9's Ed25519 identity on purpose. The identity key answers *"who sent this"* and
/// signs; this one answers *"seal it so only that daemon opens it"* and agrees. Using one key for
/// both is the classic mistake — it ties the lifetime of an attributable identity to the lifetime
/// of a decryption capability, so rotating either forces rotating both.
///
/// `x25519-dalek` (the companion of `ed25519-dalek`, already approved for `#keyring` 1/9) is
/// approved for this crate too — CLAUDE.md § ASK, recorded in `Cargo.toml`.
pub struct VaultTransportKey {
    _secret: SecretBytes,
}

impl VaultTransportKey {
    /// Take ownership of an existing secret half — what a load from disk produces.
    #[must_use]
    pub fn from_secret(secret: SecretBytes) -> Self {
        Self { _secret: secret }
    }

    /// Generate a fresh keypair for this daemon.
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(rand::rngs::OsRng);
        Self::from_secret(SecretBytes::new(secret.to_bytes()))
    }

    /// Load the keypair persisted beside the daemon's identity key, generating one if absent.
    ///
    /// Persisted for the same reason the identity key is: a transport key minted per start would
    /// make every payload sealed before a restart undeliverable, with nothing in the journal to
    /// explain why. Written mode `0600`, and any file of the wrong length is reported rather than
    /// silently regenerated — regenerating would mint a new identity nobody asked for.
    pub fn load_or_generate(path: &std::path::Path) -> std::io::Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let bytes: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "{} holds {} bytes; a transport key is exactly {TRANSPORT_KEY_FILE_LEN}",
                            path.display(),
                            bytes.len()
                        ),
                    )
                })?;
                Ok(Self::from_secret(SecretBytes::new(bytes)))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let key = Self::generate();
                write_owner_only(path, key._secret.expose())?;
                Ok(key)
            }
            Err(e) => Err(e),
        }
    }

    /// The half that is advertised.
    #[must_use]
    pub fn public_key(&self) -> TransportPublicKey {
        let secret = StaticSecret::from(*self._secret.expose());
        TransportPublicKey::from_bytes(PublicKey::from(&secret).to_bytes())
    }

    /// The secret this daemon and `peer` agree on, and nobody else does.
    ///
    /// Returns [`SecretBytes`] rather than `[u8; 32]` so the agreed secret is zeroed when it is
    /// dropped instead of being copied into every frame it passes through.
    #[must_use]
    pub fn shared_secret(&self, peer: &TransportPublicKey) -> SecretBytes {
        let secret = StaticSecret::from(*self._secret.expose());
        let public = PublicKey::from(*peer.as_bytes());
        let agreement = secret.diffie_hellman(&public);
        crate::crypto::hkdf_expand(agreement.as_bytes(), SHARED_SECRET_INFO)
    }
}

/// Write `bytes` to `path`, mode `0600` on unix, by writing a sibling staging file and renaming it
/// into place — a crash mid-write leaves the old key, never a truncated one.
fn write_owner_only(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let staging = path.with_extension("new");
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&staging)?;
        std::io::Write::write_all(&mut file, bytes)?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&staging, bytes)?;
    }
    std::fs::rename(&staging, path)
}

impl std::fmt::Debug for VaultTransportKey {
    /// Prints the type and nothing else, for the same reason [`SecretBytes`] does.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VaultTransportKey(<redacted>)")
    }
}
