//! Encrypted screen-sharing credential vault.
//!
//! Stores screen-sharing target credentials (host/port/password/protocol) encrypted
//! inside the session directory as `.screen-sharing.yaml` (mode 0600).
//!
//! Key derivation: Argon2id(passphrase, random salt) → 32-byte key.
//! Encryption: ChaCha20-Poly1305 AEAD with per-item random nonces.
//! A verifier ciphertext lets the daemon confirm the passphrase without storing the
//! plaintext key.
//!
//! Supports both VNC (Protocol::Vnc) and RDP (Protocol::Rdp) targets.
//!
//! # Vault schema (`.screen-sharing.yaml`, mode 0600)
//! ```yaml
//! kdf_salt: <hex>           # 32-byte random salt for Argon2id
//! verifier_nonce: <hex>     # 12-byte nonce for verifier AEAD
//! verifier_ct: <hex>        # ChaCha20-Poly1305 encrypt of b"tddy-screenshare-vault-v1"
//! targets:
//!   - id: <uuid>
//!     label: "My Desktop"
//!     host: 192.168.1.100
//!     port: 5900
//!     protocol: 1            # Protocol enum as i32 (1=VNC, 2=RDP)
//!     pw_nonce: <hex>
//!     pw_ct: <hex>
//! ```

use std::path::Path;

use chacha20poly1305::{
    aead::{Aead, OsRng},
    AeadCore, ChaCha20Poly1305, KeyInit,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use tddy_service::proto::screen_sharing::Protocol;

const VERIFIER_PLAINTEXT: &[u8] = b"tddy-screenshare-vault-v1";

/// A screen-sharing target stored in the vault (without the encrypted password).
#[derive(Debug, Clone)]
pub struct ScreenSharingTarget {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub protocol: Protocol,
}

/// A 32-byte derived key — the result of Argon2id(passphrase, salt).
///
/// Held in daemon memory only; never written to disk.
#[derive(Clone)]
pub struct DerivedKey(pub [u8; 32]);

// ---------------------------------------------------------------------------
// On-disk serialization types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct VaultFile {
    kdf_salt: String,
    verifier_nonce: String,
    verifier_ct: String,
    targets: Vec<EncryptedTarget>,
}

#[derive(Serialize, Deserialize)]
struct EncryptedTarget {
    id: String,
    label: String,
    host: String,
    port: u16,
    protocol: i32,
    pw_nonce: String,
    pw_ct: String,
}

// ---------------------------------------------------------------------------
// Hex helpers
// ---------------------------------------------------------------------------

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_to_bytes(hex: &str) -> anyhow::Result<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        anyhow::bail!("hex string has odd length");
    }
    (0..hex.len() / 2)
        .map(|i| {
            u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)
                .map_err(|e| anyhow::anyhow!("invalid hex byte: {}", e))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Key derivation
// ---------------------------------------------------------------------------

fn derive_key(passphrase: &str, salt_bytes: &[u8]) -> anyhow::Result<DerivedKey> {
    let mut output = [0u8; 32];
    argon2::Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt_bytes, &mut output)
        .map_err(|e| anyhow::anyhow!("Argon2 key derivation failed: {}", e))?;
    Ok(DerivedKey(output))
}

// ---------------------------------------------------------------------------
// AEAD helpers
// ---------------------------------------------------------------------------

fn encrypt_bytes(key: &DerivedKey, plaintext: &[u8]) -> anyhow::Result<([u8; 12], Vec<u8>)> {
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let cipher = ChaCha20Poly1305::new(&key.0.into());
    let ct = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("AEAD encrypt failed: {}", e))?;
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes.copy_from_slice(&nonce);
    Ok((nonce_bytes, ct))
}

fn decrypt_bytes(
    key: &DerivedKey,
    nonce_bytes: &[u8],
    ciphertext: &[u8],
) -> anyhow::Result<Vec<u8>> {
    if nonce_bytes.len() != 12 {
        anyhow::bail!("nonce must be 12 bytes, got {}", nonce_bytes.len());
    }
    let nonce: chacha20poly1305::Nonce = *chacha20poly1305::Nonce::from_slice(nonce_bytes);
    let cipher = ChaCha20Poly1305::new(&key.0.into());
    cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("AEAD decryption failed — wrong key or corrupted data"))
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

fn write_vault_file(path: &Path, vault_file: &VaultFile) -> anyhow::Result<()> {
    use std::io::Write;

    let yaml = serde_yaml::to_string(vault_file)
        .map_err(|e| anyhow::anyhow!("failed to serialize vault: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| anyhow::anyhow!("failed to open vault file {}: {}", path.display(), e))?;
        file.write_all(yaml.as_bytes())
            .map_err(|e| anyhow::anyhow!("failed to write vault file: {}", e))?;
    }

    #[cfg(not(unix))]
    {
        std::fs::write(path, yaml.as_bytes())
            .map_err(|e| anyhow::anyhow!("failed to write vault file: {}", e))?;
    }

    Ok(())
}

fn read_vault_file(path: &Path) -> anyhow::Result<VaultFile> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read vault file {}: {}", path.display(), e))?;
    serde_yaml::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("failed to parse vault file: {}", e))
}

fn protocol_from_i32(v: i32) -> Protocol {
    Protocol::try_from(v).unwrap_or(Protocol::Unspecified)
}

fn encrypted_to_target(t: &EncryptedTarget) -> ScreenSharingTarget {
    ScreenSharingTarget {
        id: t.id.clone(),
        label: t.label.clone(),
        host: t.host.clone(),
        port: t.port,
        protocol: protocol_from_i32(t.protocol),
    }
}

// ---------------------------------------------------------------------------
// ScreenSharingVault
// ---------------------------------------------------------------------------

/// The encrypted screen-sharing credential vault for one session.
pub struct ScreenSharingVault {
    path: std::path::PathBuf,
    data: VaultFile,
}

impl ScreenSharingVault {
    /// Create a new vault at `path` protected by `passphrase`.
    pub fn create(path: &Path, passphrase: &str) -> anyhow::Result<(Self, DerivedKey)> {
        use rand::RngCore;

        let mut salt_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut salt_bytes);

        let key = derive_key(passphrase, &salt_bytes)?;

        let (verifier_nonce, verifier_ct) = encrypt_bytes(&key, VERIFIER_PLAINTEXT)?;

        let data = VaultFile {
            kdf_salt: bytes_to_hex(&salt_bytes),
            verifier_nonce: bytes_to_hex(&verifier_nonce),
            verifier_ct: bytes_to_hex(&verifier_ct),
            targets: Vec::new(),
        };

        write_vault_file(path, &data)?;

        Ok((
            Self {
                path: path.to_path_buf(),
                data,
            },
            key,
        ))
    }

    /// Open an existing vault at `path` and validate the passphrase.
    pub fn unlock(path: &Path, passphrase: &str) -> anyhow::Result<(Self, DerivedKey)> {
        let data = read_vault_file(path)?;

        let salt_bytes = hex_to_bytes(&data.kdf_salt)?;
        let key = derive_key(passphrase, &salt_bytes)?;

        let verifier_nonce = hex_to_bytes(&data.verifier_nonce)?;
        let verifier_ct = hex_to_bytes(&data.verifier_ct)?;
        let plaintext = decrypt_bytes(&key, &verifier_nonce, &verifier_ct)
            .map_err(|_| anyhow::anyhow!("invalid passphrase"))?;

        if plaintext != VERIFIER_PLAINTEXT {
            anyhow::bail!("invalid passphrase (verifier mismatch)");
        }

        Ok((
            Self {
                path: path.to_path_buf(),
                data,
            },
            key,
        ))
    }

    /// Returns `true` if a vault file already exists at `path`.
    pub fn exists(path: &Path) -> bool {
        path.exists()
    }

    /// Add a new target to the vault with the given protocol.
    ///
    /// `password` is encrypted with `key` before being persisted. An empty
    /// `password` is valid (password-less target).
    pub fn add_target(
        &mut self,
        label: &str,
        host: &str,
        port: u16,
        password: &str,
        protocol: Protocol,
        key: &DerivedKey,
    ) -> anyhow::Result<ScreenSharingTarget> {
        let id = Uuid::new_v4().to_string();

        let (pw_nonce, pw_ct) = encrypt_bytes(key, password.as_bytes())?;

        let encrypted = EncryptedTarget {
            id: id.clone(),
            label: label.to_string(),
            host: host.to_string(),
            port,
            protocol: protocol as i32,
            pw_nonce: bytes_to_hex(&pw_nonce),
            pw_ct: bytes_to_hex(&pw_ct),
        };

        self.data.targets.push(encrypted);
        write_vault_file(&self.path, &self.data)?;

        Ok(ScreenSharingTarget {
            id,
            label: label.to_string(),
            host: host.to_string(),
            port,
            protocol,
        })
    }

    /// Return all known targets (without their decrypted passwords).
    pub fn list_targets(&self) -> Vec<ScreenSharingTarget> {
        self.data.targets.iter().map(encrypted_to_target).collect()
    }

    /// Remove a target by id.
    pub fn remove_target(&mut self, id: &str) -> anyhow::Result<()> {
        let before = self.data.targets.len();
        self.data.targets.retain(|t| t.id != id);
        if self.data.targets.len() == before {
            anyhow::bail!("target '{}' not found in vault", id);
        }
        write_vault_file(&self.path, &self.data)?;
        Ok(())
    }

    /// Decrypt and return the password for the given target.
    pub fn decrypt_password(&self, target_id: &str, key: &DerivedKey) -> anyhow::Result<String> {
        let target = self
            .data
            .targets
            .iter()
            .find(|t| t.id == target_id)
            .ok_or_else(|| anyhow::anyhow!("target '{}' not found in vault", target_id))?;

        let pw_nonce = hex_to_bytes(&target.pw_nonce)?;
        let pw_ct = hex_to_bytes(&target.pw_ct)?;
        let plaintext = decrypt_bytes(key, &pw_nonce, &pw_ct)?;

        String::from_utf8(plaintext)
            .map_err(|e| anyhow::anyhow!("password is not valid UTF-8: {}", e))
    }

    /// Load an existing vault from disk, verifying a cached derived key against the
    /// stored verifier.
    pub(crate) fn load_with_key(
        path: &Path,
        key: &DerivedKey,
    ) -> anyhow::Result<(Self, DerivedKey)> {
        let data = read_vault_file(path)?;

        let verifier_nonce = hex_to_bytes(&data.verifier_nonce)?;
        let verifier_ct = hex_to_bytes(&data.verifier_ct)?;
        let plaintext = decrypt_bytes(key, &verifier_nonce, &verifier_ct)
            .map_err(|_| anyhow::anyhow!("key verification failed"))?;
        if plaintext != VERIFIER_PLAINTEXT {
            anyhow::bail!("key verification failed (verifier mismatch)");
        }

        Ok((
            Self {
                path: path.to_path_buf(),
                data,
            },
            DerivedKey(key.0),
        ))
    }

    /// Read non-secret target metadata from a vault file without needing the key.
    pub(crate) fn list_targets_from_file(
        path: &Path,
    ) -> anyhow::Result<Vec<ScreenSharingTarget>> {
        let data = read_vault_file(path)?;
        Ok(data.targets.iter().map(encrypted_to_target).collect())
    }

    /// Check whether the passphrase is correct for an existing vault without fully
    /// loading it. Returns `false` if the file does not exist.
    pub fn is_passphrase_valid(path: &Path, passphrase: &str) -> anyhow::Result<bool> {
        if !path.exists() {
            return Ok(false);
        }
        match Self::unlock(path, passphrase) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

pub const VAULT_FILENAME: &str = ".screen-sharing.yaml";

/// Return the vault path for a session directory.
pub fn vault_path(session_dir: &Path) -> std::path::PathBuf {
    session_dir.join(VAULT_FILENAME)
}
