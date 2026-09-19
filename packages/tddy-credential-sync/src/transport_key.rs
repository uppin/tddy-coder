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

use tddy_credentials::SecretBytes;

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
/// ⚠ The X25519 implementation (`x25519-dalek`, the companion of the already-approved
/// `ed25519-dalek`) is **not yet a dependency of this crate**: CLAUDE.md § ASK applies and the
/// request belongs to this node's green phase, not to the commit that publishes this surface.
pub struct VaultTransportKey {
    // TODO(keyring 6/9): implement — an `x25519_dalek::StaticSecret` once the dependency is approved.
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
        todo!("TODO(keyring 6/9): implement — a fresh X25519 static secret from the OS RNG")
    }

    /// Load the keypair persisted beside the daemon's identity key, generating one if absent.
    ///
    /// Persisted for the same reason the identity key is: a transport key minted per start would
    /// make every payload sealed before a restart undeliverable, with nothing in the journal to
    /// explain why.
    pub fn load_or_generate(_path: &std::path::Path) -> std::io::Result<Self> {
        todo!("TODO(keyring 6/9): implement — read mode-0600, or generate and write atomically")
    }

    /// The half that is advertised.
    #[must_use]
    pub fn public_key(&self) -> TransportPublicKey {
        todo!("TODO(keyring 6/9): implement — derive the public half")
    }

    /// The secret this daemon and `peer` agree on, and nobody else does.
    ///
    /// Returns [`SecretBytes`] rather than `[u8; 32]` so the agreed secret is zeroed when it is
    /// dropped instead of being copied into every frame it passes through.
    #[must_use]
    pub fn shared_secret(&self, _peer: &TransportPublicKey) -> SecretBytes {
        todo!("TODO(keyring 6/9): implement — X25519 agreement, then HKDF to a sealing key")
    }
}

impl std::fmt::Debug for VaultTransportKey {
    /// Prints the type and nothing else, for the same reason [`SecretBytes`] does.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VaultTransportKey(<redacted>)")
    }
}
