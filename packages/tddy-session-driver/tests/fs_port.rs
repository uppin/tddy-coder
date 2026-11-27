//! `#fs-port`: this crate reaches disk only through the filesystem port (`tddy-fs`).

use tddy_fs::Fs;

#[test]
fn the_session_directory_is_created_through_the_fs_port() {
    // Given — a base that exists only in memory.
    let fs = tddy_fs::MemFs::new();
    let base = std::path::Path::new("/driver-sessions");

    // When — the session directory is created through the port.
    let dir = tddy_session_driver::session::create_session_dir_on(&fs, base, "s-1")
        .expect("a session directory is created");

    // Then — it names the canonical `<base>/<session id>`, and a repeat run keeps it.
    assert_eq!(dir, base.join("s-1"));
    assert_eq!(
        fs.metadata(&dir).expect("the session dir").kind,
        tddy_fs::FsKind::Dir,
        "the session directory exists, in memory rather than on disk"
    );
    let again = tddy_session_driver::session::create_session_dir_on(&fs, base, "s-1")
        .expect("an existing session directory is kept");
    assert_eq!(again, dir, "creating twice names the same directory");
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
