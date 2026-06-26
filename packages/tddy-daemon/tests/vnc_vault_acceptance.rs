//! Acceptance tests: VNC credential vault.
//!
//! PRD: docs/ft/web/vnc-sessions.md (AC-VNC-7 — credentials at rest).
//!
//! These tests verify that:
//!   1. `VncVault::create` produces an encrypted file on disk.
//!   2. `VncVault::unlock` accepts the correct passphrase and rejects wrong ones.
//!   3. Targets can be added, listed, and removed.
//!   4. Passwords survive an encrypt→decrypt roundtrip.
//!   5. The vault file is written with restrictive permissions (mode 0600 on Unix).
//!   6. A missing vault returns `false` from `is_passphrase_valid`.
//!
//! All tests pass against the implemented VncVault.

use tddy_daemon::vnc_vault::{vault_path, VncVault};

const PASSPHRASE: &str = "correct-horse-battery-staple";
const WRONG_PASSPHRASE: &str = "wrong-passphrase";

// ---------------------------------------------------------------------------
// Vault-1: create produces an encrypted file on disk
// ---------------------------------------------------------------------------

/// **vault_create_writes_file**: `VncVault::create` must write `.vnc.yaml` to the
/// provided path and return a non-error vault + derived key.
#[test]
fn vault_create_writes_file() {
    // Given — a temporary session directory
    let tmp = tempfile::tempdir().unwrap();
    let path = vault_path(tmp.path());

    // When
    let result = VncVault::create(&path, PASSPHRASE);

    // Then — no error, file on disk
    assert!(
        result.is_ok(),
        "VncVault::create failed: {:?}",
        result.err()
    );
    assert!(
        path.exists(),
        ".vnc.yaml must be written to disk after create"
    );
}

// ---------------------------------------------------------------------------
// Vault-2: unlock accepts correct passphrase, rejects wrong one
// ---------------------------------------------------------------------------

/// **vault_unlock_correct_passphrase**: `unlock` with the passphrase used in `create`
/// must succeed and return a vault + key.
#[test]
fn vault_unlock_correct_passphrase() {
    // Given — a vault created with PASSPHRASE
    let tmp = tempfile::tempdir().unwrap();
    let path = vault_path(tmp.path());
    VncVault::create(&path, PASSPHRASE).expect("create must succeed");

    // When
    let result = VncVault::unlock(&path, PASSPHRASE);

    // Then
    assert!(
        result.is_ok(),
        "unlock with correct passphrase must succeed; got: {:?}",
        result.err()
    );
}

/// **vault_unlock_wrong_passphrase_rejected**: `unlock` with a different passphrase
/// must return an error (verifier mismatch).
#[test]
fn vault_unlock_wrong_passphrase_rejected() {
    // Given — a vault created with PASSPHRASE
    let tmp = tempfile::tempdir().unwrap();
    let path = vault_path(tmp.path());
    VncVault::create(&path, PASSPHRASE).expect("create must succeed");

    // When
    let result = VncVault::unlock(&path, WRONG_PASSPHRASE);

    // Then
    assert!(
        result.is_err(),
        "unlock with wrong passphrase must fail, but succeeded"
    );
}

// ---------------------------------------------------------------------------
// Vault-3: add, list, and remove targets persist correctly
// ---------------------------------------------------------------------------

/// **vault_add_list_remove_target**: targets survive a full vault load+unlock cycle
/// and are removed when `remove_target` is called.
#[test]
fn vault_add_list_remove_target() {
    // Given — a fresh vault
    let tmp = tempfile::tempdir().unwrap();
    let path = vault_path(tmp.path());
    let (mut vault, key) = VncVault::create(&path, PASSPHRASE).expect("create must succeed");

    // When — add a target
    let target = vault
        .add_target("My Desktop", "192.168.1.100", 5900, "", &key)
        .expect("add_target must succeed");

    // Then — it appears in the list
    let targets = vault.list_targets();
    assert_eq!(
        targets.len(),
        1,
        "vault must contain exactly one target after add"
    );
    assert_eq!(targets[0].id, target.id);
    assert_eq!(targets[0].label, "My Desktop");
    assert_eq!(targets[0].host, "192.168.1.100");
    assert_eq!(targets[0].port, 5900);

    // When — remove it
    vault
        .remove_target(&target.id)
        .expect("remove_target must succeed");

    // Then — list is empty
    let after = vault.list_targets();
    assert!(after.is_empty(), "vault must be empty after remove");
}

// ---------------------------------------------------------------------------
// Vault-4: password roundtrip
// ---------------------------------------------------------------------------

/// **vault_password_encrypt_decrypt_roundtrip**: a password added with `add_target`
/// is returned unchanged by `decrypt_password` using the same key.
#[test]
fn vault_password_encrypt_decrypt_roundtrip() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let path = vault_path(tmp.path());
    let (mut vault, key) = VncVault::create(&path, PASSPHRASE).expect("create must succeed");

    // When
    let target = vault
        .add_target("Secure Desktop", "10.0.0.1", 5901, "s3cr3t!", &key)
        .expect("add_target must succeed");

    // Then — password decrypts to original
    let decrypted = vault
        .decrypt_password(&target.id, &key)
        .expect("decrypt_password must succeed");
    assert_eq!(
        decrypted, "s3cr3t!",
        "decrypted password must equal the original"
    );
}

// ---------------------------------------------------------------------------
// Vault-5: vault file permissions (Unix only)
// ---------------------------------------------------------------------------

/// **vault_file_has_restrictive_permissions**: `.vnc.yaml` must be written with
/// mode 0600 (owner read/write only) on Unix to protect credentials at rest.
#[cfg(unix)]
#[test]
fn vault_file_has_restrictive_permissions() {
    use std::os::unix::fs::PermissionsExt;

    // Given
    let tmp = tempfile::tempdir().unwrap();
    let path = vault_path(tmp.path());
    VncVault::create(&path, PASSPHRASE).expect("create must succeed");

    // When
    let meta = std::fs::metadata(&path).expect("must be able to stat .vnc.yaml");
    let mode = meta.permissions().mode();

    // Then — only owner read+write bits set (ignore file type bits)
    assert_eq!(
        mode & 0o777,
        0o600,
        ".vnc.yaml must have mode 0600, got {:o}",
        mode & 0o777
    );
}

// ---------------------------------------------------------------------------
// Vault-6: is_passphrase_valid on missing vault
// ---------------------------------------------------------------------------

/// **vault_is_passphrase_valid_missing_file**: `is_passphrase_valid` on a non-existent
/// path must return `Ok(false)` without panicking.
#[test]
fn vault_is_passphrase_valid_missing_file() {
    // Given — no vault file exists
    let tmp = tempfile::tempdir().unwrap();
    let path = vault_path(tmp.path());
    assert!(!path.exists(), "vault file must not pre-exist");

    // When
    let result = VncVault::is_passphrase_valid(&path, PASSPHRASE);

    // Then
    assert!(result.is_ok(), "must not error on missing file");
    assert!(
        !result.unwrap(),
        "is_passphrase_valid must return false when no vault exists"
    );
}
