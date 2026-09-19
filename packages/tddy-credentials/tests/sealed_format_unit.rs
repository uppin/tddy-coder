//! The file format and the key material, at the level a byte can be inspected.
//!
//! Separate from `credential_store_acceptance.rs` because these are not properties an operator
//! would describe. They are the reasons the properties in that file hold: a header that cannot be
//! edited into a weaker derivation, an unrecognised format that is *named* rather than guessed at,
//! and key material that does not survive the thing that owned it.

use tddy_credentials::{CredentialStore, SecretBytes, VaultError};

const THE_OPERATOR: &str = "operator";
const THE_LOGIN_CREDENTIAL: &[u8] = b"gho_the_token_this_login_granted";

#[test]
fn a_header_naming_a_kdf_this_build_does_not_produce_is_reported_as_a_format_mismatch() {
    // Given a vault whose header names a derivation from a later build
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path());
    CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
        .expect("a fresh vault opens");
    rewrite_the_header_field(&path, "kdf", "argon2id-and-something-else");

    // When this build opens it
    let refused = CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR).err();

    // Then it says which format it found and which it writes, rather than reporting a wrong key —
    // the remedies differ, and an operator told "wrong key" would re-link accounts for nothing
    assert_eq!(
        refused,
        Some(VaultError::FormatMismatch {
            expected: "hkdf-sha256".to_string(),
            found: "argon2id-and-something-else".to_string(),
        })
    );
}

#[test]
fn a_header_from_a_later_format_version_is_reported_rather_than_read_as_this_one() {
    // Given a vault written by a newer build
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path());
    CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
        .expect("a fresh vault opens");
    rewrite_the_header_field(&path, "format_version", "2");

    // When this build opens it
    let refused = CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR).err();

    // Then it refuses by version, which is what makes a future format change safe to ship
    assert_eq!(
        refused,
        Some(VaultError::FormatMismatch {
            expected: "1".to_string(),
            found: "2".to_string(),
        })
    );
}

#[test]
fn editing_the_headers_salt_cannot_quietly_move_the_vault_onto_another_key() {
    // Given a vault whose salt somebody replaced with one of their own choosing
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path());
    CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
        .expect("a fresh vault opens");
    rewrite_the_header_field(&path, "salt", &"ab".repeat(32));

    // When the rightful owner opens it with the credential that used to work
    let refused = CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR).err();

    // Then the derivation is bound to the header, so an edited parameter is a key that does not
    // open the vault — never a vault that opens under a derivation somebody else chose
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

    // Then the bytes are not in the line — a derived `Debug` would put a live key in every log
    // statement that formats a value containing one
    assert_eq!(printed, "SecretBytes(<redacted>)");
}

/// Replace one field of the vault's header, standing in for an edit made outside this build.
///
/// String and numeric fields are both rewritten by value, which is what lets one helper cover a KDF
/// name, a version number and a hex salt.
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
