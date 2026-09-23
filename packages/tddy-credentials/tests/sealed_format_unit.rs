//! The file format and the key material, at the level a byte can be inspected.
//!
//! Separate from `credential_store_acceptance.rs` because these are not properties an operator
//! would describe. They are the reasons the properties in that file hold: a header that cannot be
//! edited into a weaker derivation, an unrecognised format that is *named* rather than guessed at,
//! and key material that does not survive the thing that owned it.

use tddy_credentials::{CredentialStore, SecretBytes, SecretString, VaultError};

const THE_OPERATOR: &str = "operator";
const THE_PASSPHRASE: &str = "correct horse battery staple";

#[test]
fn a_new_vault_names_argon2id_and_its_parameters_in_its_header() {
    // Given a freshly created vault
    let (_dir, path) = a_fresh_vault();

    // When its header is read off the disk
    let header = the_header_of(&path);

    // Then it names the derivation and every parameter a later build needs to reproduce it
    assert_eq!(
        (
            header["format_version"].clone(),
            header["kdf"].clone(),
            header["kdf_version"].clone(),
            header["m_cost_kib"].clone(),
            header["t_cost"].clone(),
            header["p_cost"].clone(),
        ),
        (
            serde_json::json!(2),
            serde_json::json!("argon2id"),
            serde_json::json!(19),
            serde_json::json!(19456),
            serde_json::json!(2),
            serde_json::json!(1),
        )
    );
}

#[test]
fn a_header_naming_a_kdf_this_build_does_not_produce_is_reported_as_a_format_mismatch() {
    // Given a vault whose header names a derivation from a later build
    let (_dir, path) = a_fresh_vault();
    rewrite_the_header_field(&path, "kdf", "scrypt");

    // When this build opens it
    let refused = open(&path).err();

    // Then it says which format it found and which it writes, rather than reporting a wrong key —
    // the remedies differ, and an operator told "wrong key" would reset a vault for nothing
    assert_eq!(
        refused,
        Some(VaultError::FormatMismatch {
            expected: "argon2id".to_string(),
            found: "scrypt".to_string(),
        })
    );
}

#[test]
fn a_header_from_a_later_format_version_is_reported_rather_than_read_as_this_one() {
    // Given a vault written by a newer build
    let (_dir, path) = a_fresh_vault();
    rewrite_the_header_field(&path, "format_version", "3");

    // When this build opens it
    let refused = open(&path).err();

    // Then it refuses by version, which is what makes a future format change safe to ship
    assert_eq!(
        refused,
        Some(VaultError::FormatMismatch {
            expected: "2".to_string(),
            found: "3".to_string(),
        })
    );
}

#[test]
fn a_header_from_another_argon2_version_is_reported_as_a_format_mismatch() {
    // Given a vault whose header names Argon2 v1.0 rather than v1.3
    let (_dir, path) = a_fresh_vault();
    rewrite_the_header_field(&path, "kdf_version", "16");

    // When this build opens it
    let refused = open(&path).err();

    // Then the version is named, not guessed at
    assert_eq!(
        refused,
        Some(VaultError::FormatMismatch {
            expected: "argon2id v19".to_string(),
            found: "argon2id v16".to_string(),
        })
    );
}

#[test]
fn a_header_whose_argon2_memory_cost_was_changed_is_a_format_mismatch_not_a_failed_decrypt() {
    // Given a vault whose memory cost somebody lowered on disk
    let (_dir, path) = a_fresh_vault();
    rewrite_the_header_field(&path, "m_cost_kib", "8");

    // When this build opens it
    let refused = open(&path).err();

    // Then the parameters are refused by name before anything is derived from them — a weakened
    // derivation is never run, and an inflated one is never allowed to exhaust the daemon's memory
    assert_eq!(
        refused,
        Some(VaultError::FormatMismatch {
            expected: "argon2id m=19456,t=2,p=1".to_string(),
            found: "argon2id m=8,t=2,p=1".to_string(),
        })
    );
}

#[test]
fn editing_the_headers_salt_cannot_quietly_move_the_vault_onto_another_key() {
    // Given a vault whose salt somebody replaced with one of their own choosing
    let (_dir, path) = a_fresh_vault();
    rewrite_the_header_field(&path, "salt", &"ab".repeat(16));

    // When the rightful owner opens it with the passphrase that used to work
    let refused = open(&path).err();

    // Then the derivation is bound to the header, so an edited salt is a key that does not open
    // the vault — never a vault that opens under a derivation somebody else chose
    assert_eq!(refused, Some(VaultError::Locked));
}

#[test]
fn key_material_is_zeroed_when_the_thing_holding_it_is_dropped() {
    // Given key material in a slot this test owns, so the bytes are still readable afterwards
    let mut slot = std::mem::MaybeUninit::<SecretBytes>::uninit();
    let held = slot.as_mut_ptr();
    unsafe { held.write(SecretBytes::new([0x5a; 32])) };

    // When its owner is dropped
    unsafe { std::ptr::drop_in_place(held) };

    // Then nothing of it is left behind for whatever allocates that memory next
    let left_behind = unsafe { std::ptr::read(held.cast::<[u8; 32]>()) };
    assert_eq!(left_behind, [0u8; 32]);
}

#[test]
fn key_material_does_not_print_itself() {
    // Given key material that ends up inside a struct somebody logs
    let key = SecretBytes::new([0x5a; 32]);

    // When it is formatted
    let printed = format!("{key:?}");

    // Then the bytes are not in the line
    assert_eq!(printed, "SecretBytes(<redacted>)");
}

#[test]
fn a_secret_string_does_not_print_itself() {
    // Given a passphrase that ends up inside a struct somebody logs
    let passphrase = SecretString::new(THE_PASSPHRASE);

    // When it is formatted
    let printed = format!("{passphrase:?}");

    // Then the text is not in the line
    assert_eq!(printed, "SecretString(<redacted>)");
}

fn a_fresh_vault() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    CredentialStore::create(&path, &SecretString::new(THE_PASSPHRASE), THE_OPERATOR)
        .expect("a fresh vault is created");
    (dir, path)
}

fn open(path: &std::path::Path) -> Result<tddy_credentials::SessionVault, VaultError> {
    CredentialStore::open_with_passphrase(path, &SecretString::new(THE_PASSPHRASE), THE_OPERATOR)
}

fn the_header_of(path: &std::path::Path) -> serde_json::Value {
    let raw = std::fs::read_to_string(path).expect("the vault is on disk");
    let document: serde_json::Value =
        serde_json::from_str(&raw).expect("the vault is a json document");
    document["header"].clone()
}

/// Replace one field of the vault's header, standing in for an edit made outside this build.
///
/// String and numeric fields are both rewritten by value, which is what lets one helper cover a KDF
/// name, a version number, a cost parameter and a hex salt.
fn rewrite_the_header_field(path: &std::path::Path, field: &str, value: &str) {
    let raw = std::fs::read_to_string(path).expect("the vault is on disk");
    let mut document: serde_json::Value =
        serde_json::from_str(&raw).expect("the vault is a json document");
    let header = document
        .get_mut("header")
        .and_then(serde_json::Value::as_object_mut)
        .expect("the vault carries a header");
    let replacement = match header.get(field) {
        Some(serde_json::Value::Number(_)) => serde_json::Value::Number(
            value
                .parse()
                .expect("a numeric header field takes a number"),
        ),
        _ => serde_json::Value::String(value.to_string()),
    };
    header.insert(field.to_string(), replacement);
    std::fs::write(
        path,
        serde_json::to_string_pretty(&document).expect("the document serialises"),
    )
    .expect("the vault is writable");
}
