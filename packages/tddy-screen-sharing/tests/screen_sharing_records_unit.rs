//! A screen-sharing target is a credential record, and nothing about it is provider-specific.
//!
//! These are the tests that decide whether `#keyring` 3/9 built a generic store or a GitHub token
//! file with a longer name: a second provider costs a `ProviderId` and this mapping, and nothing
//! else. Anything here that needs the store to grow a screen-sharing-shaped hole is the finding.

use std::collections::BTreeMap;

use tddy_credentials::{AccountId, CredentialRecord, ProviderId, FIRST_VERSION};
use tddy_screen_sharing::screen_sharing_records::{
    account_for, record_for, screen_sharing_provider, target_from, TargetError, META_HOST,
    META_PORT, META_PROTOCOL, META_USERNAME, SCREEN_SHARING_PROVIDER,
};
use tddy_service::proto::screen_sharing::{Protocol, ScreenSharingTarget};

const A_DESKTOP_PASSWORD: &str = "the-desktop-password";
const WRITTEN_AT: u64 = 1_758_240_000;

fn a_vnc_target() -> ScreenSharingTarget {
    ScreenSharingTarget {
        id: "target-0a1b".to_string(),
        label: "dev box".to_string(),
        host: "10.0.0.5".to_string(),
        port: 5900,
        protocol: Protocol::Vnc as i32,
        username: "ada".to_string(),
    }
}

fn an_rdp_target() -> ScreenSharingTarget {
    ScreenSharingTarget {
        id: "target-7c2d".to_string(),
        label: "windows box".to_string(),
        host: "10.0.0.10".to_string(),
        port: 3389,
        protocol: Protocol::Rdp as i32,
        username: "tester".to_string(),
    }
}

fn a_record_holding(metadata: BTreeMap<String, String>) -> CredentialRecord {
    CredentialRecord {
        provider: screen_sharing_provider(),
        account: AccountId::new("target-0a1b"),
        label: "dev box".to_string(),
        secret: A_DESKTOP_PASSWORD.to_string(),
        metadata,
        updated_at: WRITTEN_AT,
        version: FIRST_VERSION,
    }
}

fn the_metadata_of_a_vnc_target() -> BTreeMap<String, String> {
    BTreeMap::from([
        (META_HOST.to_string(), "10.0.0.5".to_string()),
        (META_PORT.to_string(), "5900".to_string()),
        (META_PROTOCOL.to_string(), "VNC".to_string()),
        (META_USERNAME.to_string(), "ada".to_string()),
    ])
}

#[test]
fn a_target_is_stored_under_the_screen_sharing_provider() {
    // Given a desktop
    let target = a_vnc_target();

    // When it becomes a record
    let record = record_for(&target, A_DESKTOP_PASSWORD, WRITTEN_AT);

    // Then it is one of that provider's credentials — the same string the Accounts screen groups
    // by without knowing what it means
    assert_eq!(record.provider, ProviderId::new(SCREEN_SHARING_PROVIDER));
}

#[test]
fn a_targets_id_is_its_account_at_that_provider() {
    // Given a desktop
    let target = a_vnc_target();

    // When it becomes a record
    let record = record_for(&target, A_DESKTOP_PASSWORD, WRITTEN_AT);

    // Then two desktops are two accounts, exactly as two GitHub logins are
    assert_eq!(record.account, account_for(&target.id));
}

#[test]
fn a_targets_password_is_the_records_secret() {
    // Given a desktop with a password
    let target = a_vnc_target();

    // When it becomes a record
    let record = record_for(&target, A_DESKTOP_PASSWORD, WRITTEN_AT);

    // Then
    assert_eq!(record.secret, A_DESKTOP_PASSWORD);
}

#[test]
fn a_targets_host_port_protocol_and_username_travel_inside_the_record() {
    // Given a desktop
    let target = a_vnc_target();

    // When it becomes a record
    let record = record_for(&target, A_DESKTOP_PASSWORD, WRITTEN_AT);

    // Then every field is metadata, which the store seals together with the secret — the limit
    // `screen_sharing_vault.rs` had, where label and host sat in cleartext beside the ciphertext
    assert_eq!(record.metadata, the_metadata_of_a_vnc_target());
}

#[test]
fn a_targets_label_is_the_records_label() {
    // Given a desktop
    let target = a_vnc_target();

    // When it becomes a record
    let record = record_for(&target, A_DESKTOP_PASSWORD, WRITTEN_AT);

    // Then — the Accounts screen renames a record by its label, and a desktop is no exception
    assert_eq!(record.label, "dev box");
}

#[test]
fn a_record_maps_back_to_the_target_it_was_made_from() {
    // Given a desktop stored as a record
    let target = an_rdp_target();
    let record = record_for(&target, A_DESKTOP_PASSWORD, WRITTEN_AT);

    // When the record is read back
    let read_back = target_from(&record).expect("a record this mapping wrote is a usable target");

    // Then
    assert_eq!(read_back, target);
}

#[test]
fn the_protocol_travels_as_its_name_rather_than_its_number() {
    // Given a desktop on a protocol whose enum number could be renumbered
    let target = an_rdp_target();

    // When it becomes a record
    let record = record_for(&target, A_DESKTOP_PASSWORD, WRITTEN_AT);

    // Then the stored form survives a renumbering, which a record outliving its build must
    assert_eq!(record.metadata.get(META_PROTOCOL), Some(&"RDP".to_string()));
}

#[test]
fn a_record_with_no_host_is_not_a_usable_target() {
    // Given a record whose host metadata is gone
    let mut metadata = the_metadata_of_a_vnc_target();
    metadata.remove(META_HOST);
    let record = a_record_holding(metadata);

    // When it is read as a target
    let read_back = target_from(&record);

    // Then it is refused rather than defaulted — a target with a defaulted host is a target that
    // connects somewhere nobody asked for
    assert_eq!(
        read_back,
        Err(TargetError::Malformed(format!("missing {META_HOST}")))
    );
}

#[test]
fn a_record_whose_port_is_not_a_number_is_not_a_usable_target() {
    // Given a record whose port metadata was altered
    let mut metadata = the_metadata_of_a_vnc_target();
    metadata.insert(META_PORT.to_string(), "not-a-port".to_string());
    let record = a_record_holding(metadata);

    // When it is read as a target
    let read_back = target_from(&record);

    // Then
    assert_eq!(
        read_back,
        Err(TargetError::Malformed(format!(
            "{META_PORT} is not a port: not-a-port"
        )))
    );
}

// ---------------------------------------------------------------------------
// The metadata is inside the AEAD, and this is the test that proves it
// ---------------------------------------------------------------------------

/// Tampering with a stored target's `host` fails the open rather than redirecting the connection.
///
/// This goes through the **real** sealed file rather than the mapping alone, because the claim is
/// about where `host` lives relative to the ciphertext. `screen_sharing_vault.rs` — the file this
/// node retires — keeps a target's label and host in cleartext beside the sealed password, so
/// anyone who can write that file can point a desktop connection somewhere else and anyone who can
/// read it learns which machines the operator reaches. Neither is possible here.
#[test]
fn tampering_with_a_stored_targets_host_fails_the_open() {
    use tddy_credentials::{CredentialStore, VaultError};

    // Given a desktop sealed into a real credential store
    let storage = tempfile::tempdir().expect("a storage directory");
    let path = CredentialStore::path_in(storage.path());
    let login_key = b"the-key-a-login-derived";
    let vault = CredentialStore::open_or_create(&path, login_key, "an-operator")
        .expect("a fresh store opens");
    vault
        .put(record_for(&a_vnc_target(), A_DESKTOP_PASSWORD, WRITTEN_AT))
        .expect("sealing a desktop");

    // When somebody who can write the file redirects the desktop's host
    let sealed = std::fs::read_to_string(&path).expect("the sealed file");
    assert!(
        !sealed.contains("10.0.0.5"),
        "the host must not be readable in the sealed file"
    );
    let tampered = sealed.replace("\"ciphertext\":\"", "\"ciphertext\":\"00");
    std::fs::write(&path, tampered).expect("the tampered file");

    // Then the store refuses to open it — never a target with a host somebody else chose
    let reopened = CredentialStore::open_or_create(&path, login_key, "an-operator")
        .expect("the header still opens")
        .list(Some(&screen_sharing_provider()));

    assert_eq!(reopened, Err(VaultError::Crypto));
}
