//! `#fs-port`: this crate reaches disk only through the filesystem port (`tddy-fs`).

use tddy_fs::Fs;

#[test]
fn the_registry_key_is_the_canonical_session_dir_through_the_fs_port() {
    // Given — a session directory that exists only in memory.
    let fs = tddy_fs::MemFs::new();
    let session_dir = std::path::Path::new("/sessions/abc");
    fs.create_dir_all(session_dir).expect("the session dir");

    // When — the key is computed for the plain path and for a roundabout one.
    let plain = tddy_session_catalog::read::catalog_key_on(&fs, session_dir);
    let roundabout = tddy_session_catalog::read::catalog_key_on(&fs, &session_dir.join("../abc"));

    // Then — both name the same canonical directory, so insertion and lookup meet.
    assert_eq!(plain, session_dir);
    assert_eq!(
        roundabout, plain,
        "a non-canonical form keys the same session, through the port"
    );

    // And — a directory that does not exist yet keys by what it will be called.
    let missing = std::path::Path::new("/sessions/not-yet");
    assert_eq!(
        tddy_session_catalog::read::catalog_key_on(&fs, missing),
        missing
    );
    assert!(
        tddy_session_catalog::read::session_catalog_on(&fs, missing).is_none(),
        "no catalog is registered for an unknown session"
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
