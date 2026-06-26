//! Encrypted VNC credential vault.
//!
//! Stores VNC target credentials (host/port/password) encrypted inside the session
//! directory as `.vnc.yaml` (mode 0600).
//!
//! Key derivation: Argon2id(passphrase, random salt) → 32-byte key.
//! Encryption: ChaCha20-Poly1305 AEAD with per-item random nonces.
//! A verifier ciphertext lets the daemon confirm the passphrase without storing the
//! plaintext key.
//!
//! # Vault schema (`.vnc.yaml`, mode 0600)
//! ```yaml
//! kdf_salt: <hex>           # 32-byte random salt for Argon2id
//! verifier_nonce: <hex>     # 12-byte nonce for verifier AEAD
//! verifier_ct: <hex>        # ChaCha20-Poly1305 encrypt of b"tddy-vnc-vault-v1" using derived key
//! targets:
//!   - id: <uuid>
//!     label: "My Desktop"
//!     host: 192.168.1.100
//!     port: 5900
//!     pw_nonce: <hex>        # 12-byte nonce, hex-encoded
//!     pw_ct: <hex>           # ChaCha20-Poly1305 encrypt of password bytes, hex-encoded
//! ```

use std::path::Path;

use chacha20poly1305::{
    aead::{Aead, OsRng},
    AeadCore, ChaCha20Poly1305, KeyInit,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The verifier plaintext — encrypt this to prove the key is correct.
const VERIFIER_PLAINTEXT: &[u8] = b"tddy-vnc-vault-v1";

/// A VNC target stored in the vault. The password is stored encrypted; `decrypt_password`
/// returns the plaintext when given the correct key.
#[derive(Debug, Clone)]
pub struct VncTarget {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
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
    kdf_salt: String,       // 32-byte salt, hex-encoded
    verifier_nonce: String, // 12-byte nonce, hex-encoded
    verifier_ct: String,    // ciphertext, hex-encoded
    targets: Vec<EncryptedTarget>,
}

#[derive(Serialize, Deserialize)]
struct EncryptedTarget {
    id: String,
    label: String,
    host: String,
    port: u16,
    pw_nonce: String,
    pw_ct: String,
}

// ---------------------------------------------------------------------------
// Hex helpers (no external hex crate available)
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

// ---------------------------------------------------------------------------
// VncVault
// ---------------------------------------------------------------------------

/// The encrypted VNC credential vault for one session.
pub struct VncVault {
    /// Path to the `.vnc.yaml` file.
    path: std::path::PathBuf,
    /// In-memory representation of the vault.
    data: VaultFile,
}

impl VncVault {
    /// Create a new vault at `path` protected by `passphrase`.
    ///
    /// Generates a random salt, derives the key, writes `.vnc.yaml` (mode 0600).
    ///
    /// # Errors
    /// Returns `Err` if the vault cannot be created.
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
    ///
    /// Reads the YAML, derives the key, verifies it against the stored verifier.
    ///
    /// # Errors
    /// Returns `Err` if the file is missing, the passphrase is wrong, or parsing fails.
    pub fn unlock(path: &Path, passphrase: &str) -> anyhow::Result<(Self, DerivedKey)> {
        let data = read_vault_file(path)?;

        let salt_bytes = hex_to_bytes(&data.kdf_salt)?;
        let key = derive_key(passphrase, &salt_bytes)?;

        // Verify the key against the stored verifier.
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

    /// Add a new VNC target to the vault.
    ///
    /// `password` is the plaintext VNC password; it is encrypted with `key` before
    /// being persisted. Passing an empty `password` is valid (password-less VNC).
    ///
    /// # Errors
    /// Returns `Err` if encryption or file write fails.
    pub fn add_target(
        &mut self,
        label: &str,
        host: &str,
        port: u16,
        password: &str,
        key: &DerivedKey,
    ) -> anyhow::Result<VncTarget> {
        let id = Uuid::new_v4().to_string();

        let (pw_nonce, pw_ct) = encrypt_bytes(key, password.as_bytes())?;

        let encrypted = EncryptedTarget {
            id: id.clone(),
            label: label.to_string(),
            host: host.to_string(),
            port,
            pw_nonce: bytes_to_hex(&pw_nonce),
            pw_ct: bytes_to_hex(&pw_ct),
        };

        self.data.targets.push(encrypted);
        write_vault_file(&self.path, &self.data)?;

        Ok(VncTarget {
            id,
            label: label.to_string(),
            host: host.to_string(),
            port,
        })
    }

    /// Return all known targets (without their decrypted passwords).
    pub fn list_targets(&self) -> Vec<VncTarget> {
        self.data
            .targets
            .iter()
            .map(|t| VncTarget {
                id: t.id.clone(),
                label: t.label.clone(),
                host: t.host.clone(),
                port: t.port,
            })
            .collect()
    }

    /// Remove a target by id.
    ///
    /// # Errors
    /// Returns `Err` if the target does not exist.
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
    ///
    /// # Errors
    /// Returns `Err` if the target does not exist or decryption fails.
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
    ///
    /// Used when the passphrase is no longer available but the in-memory key is cached.
    ///
    /// # Errors
    /// Returns `Err` if the file cannot be read, parsed, or the key fails to verify.
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

    /// Read the non-secret target metadata from a vault file without needing the key.
    ///
    /// Returns the target list (id, label, host, port) without decrypting passwords.
    /// Useful for `ListVncTargets` when the vault is locked.
    pub(crate) fn list_targets_from_file(path: &Path) -> anyhow::Result<Vec<VncTarget>> {
        let data = read_vault_file(path)?;
        Ok(data
            .targets
            .iter()
            .map(|t| VncTarget {
                id: t.id.clone(),
                label: t.label.clone(),
                host: t.host.clone(),
                port: t.port,
            })
            .collect())
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

/// The filename used for the vault inside a session directory.
pub const VAULT_FILENAME: &str = ".vnc.yaml";

/// Return the vault path for a session directory.
pub fn vault_path(session_dir: &Path) -> std::path::PathBuf {
    session_dir.join(VAULT_FILENAME)
}
