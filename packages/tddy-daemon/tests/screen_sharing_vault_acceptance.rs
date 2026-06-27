//! Acceptance tests: Screen Sharing credential vault.
//!
//! PRD: docs/ft/web/screen-sharing-sessions.md (AC-SS-8 — credentials at rest).
//!
//! These tests verify that:
//!   1. `ScreenSharingVault::create` produces an encrypted `.screen-sharing.yaml` file.
//!   2. `ScreenSharingVault::unlock` accepts the correct passphrase and rejects wrong ones.
//!   3. Targets can be added, listed, and removed.
//!   4. Protocol is persisted per-target and round-trips through encrypt→decrypt.
//!   5. Passwords survive an encrypt→decrypt roundtrip.
//!   6. The vault file is written with restrictive permissions (mode 0600 on Unix).
//!   7. A missing vault returns `false` from `is_passphrase_valid`.
//!
//! All tests reference `tddy_daemon::screen_sharing_vault` which does not yet exist —
//! they will fail to compile until the green phase renames the vault module.

use tddy_daemon::screen_sharing_vault::{vault_path, ScreenSharingVault};
use tddy_service::proto::screen_sharing::Protocol;

const PASSPHRASE: &str = "correct-horse-battery-staple";
const WRONG_PASSPHRASE: &str = "wrong-passphrase";

// ---------------------------------------------------------------------------
// Vault-1: create produces .screen-sharing.yaml (not .vnc.yaml)
// ---------------------------------------------------------------------------

/// **vault_create_writes_screen_sharing_yaml_file**: `ScreenSharingVault::create` must
/// write `.screen-sharing.yaml` (not `.vnc.yaml`) to the provided path.
#[test]
fn vault_create_writes_screen_sharing_yaml_file() {
    // Given — a temporary session directory
    let tmp = tempfile::tempdir().unwrap();
    let path = vault_path(tmp.path());

    // Then — the path should end with .screen-sharing.yaml
    assert!(
        path.to_string_lossy().ends_with(".screen-sharing.yaml"),
        "vault_path must return a .screen-sharing.yaml path; got '{}'",
        path.display()
    );

    // When
    let result = ScreenSharingVault::create(&path, PASSPHRASE);

    // Then — no error, file on disk
    assert!(
        result.is_ok(),
        "ScreenSharingVault::create failed: {:?}",
        result.err()
    );
    assert!(
        path.exists(),
        ".screen-sharing.yaml must be written to disk after create"
    );
}

// ---------------------------------------------------------------------------
// Vault-2: unlock accepts correct passphrase, rejects wrong one
// ---------------------------------------------------------------------------

/// **vault_unlock_correct_passphrase_succeeds**: `unlock` with the passphrase used in
/// `create` must succeed and return a vault + key.
#[test]
fn vault_unlock_correct_passphrase_succeeds() {
    // Given — a vault created with PASSPHRASE
    let tmp = tempfile::tempdir().unwrap();
    let path = vault_path(tmp.path());
    ScreenSharingVault::create(&path, PASSPHRASE).expect("create must succeed");

    // When
    let result = ScreenSharingVault::unlock(&path, PASSPHRASE);

    // Then
    assert!(
        result.is_ok(),
        "unlock with correct passphrase must succeed; got: {:?}",
        result.err()
    );
}

/// **vault_unlock_wrong_passphrase_is_rejected**: `unlock` with a different passphrase
/// must return an error (verifier mismatch).
#[test]
fn vault_unlock_wrong_passphrase_is_rejected() {
    // Given — a vault created with PASSPHRASE
    let tmp = tempfile::tempdir().unwrap();
    let path = vault_path(tmp.path());
    ScreenSharingVault::create(&path, PASSPHRASE).expect("create must succeed");

    // When
    let result = ScreenSharingVault::unlock(&path, WRONG_PASSPHRASE);

    // Then
    assert!(
        result.is_err(),
        "unlock with wrong passphrase must fail, but succeeded"
    );
}

// ---------------------------------------------------------------------------
// Vault-3: VNC target protocol roundtrips through vault save/load
// ---------------------------------------------------------------------------

/// **vault_vnc_target_protocol_roundtrips**: a target added with `Protocol::Vnc` must
/// be returned with `protocol == Protocol::Vnc` after a vault save/load cycle.
#[test]
fn vault_vnc_target_protocol_roundtrips() {
    // Given — a fresh vault
    let tmp = tempfile::tempdir().unwrap();
    let path = vault_path(tmp.path());
    let (mut vault, key) =
        ScreenSharingVault::create(&path, PASSPHRASE).expect("create must succeed");

    // When — add a VNC target
    let target = vault
        .add_target(
            "VNC Desktop",
            "192.168.1.100",
            5900,
            "",
            Protocol::Vnc,
            &key,
        )
        .expect("add_target must succeed");

    // Then — protocol is preserved in list
    let targets = vault.list_targets();
    assert_eq!(targets.len(), 1, "vault must contain one target");
    assert_eq!(
        targets[0].protocol,
        Protocol::Vnc,
        "VNC target must have protocol=Vnc; got {:?}",
        targets[0].protocol
    );
    assert_eq!(targets[0].id, target.id);

    // And — protocol roundtrips through unlock
    let (vault2, _key2) =
        ScreenSharingVault::unlock(&path, PASSPHRASE).expect("unlock must succeed");
    let reloaded = vault2.list_targets();
    assert_eq!(
        reloaded[0].protocol,
        Protocol::Vnc,
        "VNC protocol must survive vault reload"
    );
}

// ---------------------------------------------------------------------------
// Vault-4: RDP target protocol roundtrips through vault save/load
// ---------------------------------------------------------------------------

/// **vault_rdp_target_protocol_roundtrips**: a target added with `Protocol::Rdp` must
/// be returned with `protocol == Protocol::Rdp` after a vault save/load cycle.
#[test]
fn vault_rdp_target_protocol_roundtrips() {
    // Given — a fresh vault
    let tmp = tempfile::tempdir().unwrap();
    let path = vault_path(tmp.path());
    let (mut vault, key) =
        ScreenSharingVault::create(&path, PASSPHRASE).expect("create must succeed");

    // When — add an RDP target
    let target = vault
        .add_target("Windows Server", "10.0.0.10", 3389, "", Protocol::Rdp, &key)
        .expect("add_target must succeed");

    // Then — protocol is preserved in list
    let targets = vault.list_targets();
    assert_eq!(
        targets[0].protocol,
        Protocol::Rdp,
        "RDP target must have protocol=Rdp; got {:?}",
        targets[0].protocol
    );
    assert_eq!(targets[0].id, target.id);

    // And — protocol roundtrips through unlock
    let (vault2, _key2) =
        ScreenSharingVault::unlock(&path, PASSPHRASE).expect("unlock must succeed");
    let reloaded = vault2.list_targets();
    assert_eq!(
        reloaded[0].protocol,
        Protocol::Rdp,
        "RDP protocol must survive vault reload"
    );
}

// ---------------------------------------------------------------------------
// Vault-5: add, list, and remove targets persist correctly
// ---------------------------------------------------------------------------

/// **vault_add_list_remove_target**: targets survive a full vault load+unlock cycle
/// and are removed when `remove_target` is called.
#[test]
fn vault_add_list_remove_target() {
    // Given — a fresh vault
    let tmp = tempfile::tempdir().unwrap();
    let path = vault_path(tmp.path());
    let (mut vault, key) =
        ScreenSharingVault::create(&path, PASSPHRASE).expect("create must succeed");

    // When — add a target
    let target = vault
        .add_target("My Desktop", "192.168.1.100", 5900, "", Protocol::Vnc, &key)
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
// Vault-6: password roundtrip
// ---------------------------------------------------------------------------

/// **vault_password_encrypt_decrypt_roundtrip**: a password added with `add_target`
/// is returned unchanged by `decrypt_password` using the same key.
#[test]
fn vault_password_encrypt_decrypt_roundtrip() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let path = vault_path(tmp.path());
    let (mut vault, key) =
        ScreenSharingVault::create(&path, PASSPHRASE).expect("create must succeed");

    // When
    let target = vault
        .add_target(
            "Secure Desktop",
            "10.0.0.1",
            5901,
            "s3cr3t!",
            Protocol::Vnc,
            &key,
        )
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
// Vault-7: vault file permissions (Unix only)
// ---------------------------------------------------------------------------

/// **vault_file_has_restrictive_permissions**: `.screen-sharing.yaml` must be written
/// with mode 0600 (owner read/write only) on Unix to protect credentials at rest.
#[cfg(unix)]
#[test]
fn vault_file_has_restrictive_permissions() {
    use std::os::unix::fs::PermissionsExt;

    // Given
    let tmp = tempfile::tempdir().unwrap();
    let path = vault_path(tmp.path());
    ScreenSharingVault::create(&path, PASSPHRASE).expect("create must succeed");

    // When
    let meta = std::fs::metadata(&path).expect("must be able to stat .screen-sharing.yaml");
    let mode = meta.permissions().mode();

    // Then — only owner read+write bits set (ignore file type bits)
    assert_eq!(
        mode & 0o777,
        0o600,
        ".screen-sharing.yaml must have mode 0600, got {:o}",
        mode & 0o777
    );
}

// ---------------------------------------------------------------------------
// Vault-8: is_passphrase_valid on missing vault
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
    let result = ScreenSharingVault::is_passphrase_valid(&path, PASSPHRASE);

    // Then
    assert!(result.is_ok(), "must not error on missing file");
    assert!(
        !result.unwrap(),
        "is_passphrase_valid must return false when no vault exists"
    );
}
