//! `#fs-port`: this crate reaches disk only through the filesystem port (`tddy-fs`).

use tddy_fs::Fs;

#[test]
fn the_signing_key_is_generated_through_the_fs_port() {
    let fs = tddy_fs::MemFs::new();
    let path = std::path::Path::new("/auth/signing_key.pem");

    let first = tddy_daemon_auth::signing_key::DaemonSigningKey::load_or_generate_on(&fs, path)
        .expect("a daemon generates a keypair on first use");
    let again = tddy_daemon_auth::signing_key::DaemonSigningKey::load_or_generate_on(&fs, path)
        .expect("a daemon reuses the keypair it already generated");

    assert_eq!(
        first.key_id(),
        again.key_id(),
        "a restart must find the same identity, not mint a second one"
    );
    assert_eq!(
        fs.metadata(path).unwrap().mode.map(|mode| mode & 0o777),
        Some(0o600),
        "the key is owner-only from the moment it exists"
    );
}

#[test]
fn production_code_reaches_disk_only_through_the_fs_port() {
    let hits =
        tddy_fs::testing::direct_disk_io_in(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
    assert!(
        hits.is_empty(),
        "direct disk I/O left in production code:\n{}",
        hits.join("\n")
    );
}
