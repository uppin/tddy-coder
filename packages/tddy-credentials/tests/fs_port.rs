//! The vault persists through the filesystem port: owner-only, crash-safe, testable in memory.

use std::path::Path;
use std::sync::Arc;

use tddy_credentials::{CredentialStore, SecretString};
use tddy_fs::{Fs, MemFs};

const THE_OPERATOR: &str = "operator";

fn the_passphrase() -> SecretString {
    SecretString::new("correct horse battery staple")
}

fn the_vault_path() -> &'static Path {
    Path::new("/auth/credentials-6f70.vault")
}

#[test]
fn a_vault_created_on_the_port_opens_again_from_the_port() {
    let fs = Arc::new(MemFs::new());
    CredentialStore::create_on(
        fs.clone(),
        the_vault_path(),
        &the_passphrase(),
        THE_OPERATOR,
    )
    .expect("created");

    let reopened = CredentialStore::open_with_passphrase_on(
        fs.clone(),
        the_vault_path(),
        &the_passphrase(),
        THE_OPERATOR,
    );

    assert!(reopened.is_ok(), "{:?}", reopened.err());
}

#[cfg(unix)]
#[test]
fn a_vault_is_written_owner_only_through_the_port() {
    let fs = Arc::new(MemFs::new());
    CredentialStore::create_on(
        fs.clone(),
        the_vault_path(),
        &the_passphrase(),
        THE_OPERATOR,
    )
    .expect("created");

    let mode = fs
        .metadata(the_vault_path())
        .unwrap()
        .mode
        .map(|m| m & 0o777);

    assert_eq!(mode, Some(0o600));
}

#[test]
fn a_vault_that_cannot_be_written_is_not_left_half_created() {
    let fs = Arc::new(MemFs::new());
    fs.fail_next_write(std::io::ErrorKind::StorageFull);

    let created = CredentialStore::create_on(
        fs.clone(),
        the_vault_path(),
        &the_passphrase(),
        THE_OPERATOR,
    );

    assert!(created.is_err());
    assert!(!fs.exists(the_vault_path()));
    // and the disk once it has room again creates it normally
    CredentialStore::create_on(
        fs.clone(),
        the_vault_path(),
        &the_passphrase(),
        THE_OPERATOR,
    )
    .expect("a retry creates the vault");
}

#[test]
fn production_code_reaches_disk_only_through_the_port() {
    let hits = tddy_fs::testing::direct_disk_io_in(Path::new(env!("CARGO_MANIFEST_DIR")));
    assert!(
        hits.is_empty(),
        "direct disk I/O outside the port:\n{}",
        hits.join("\n")
    );
}
