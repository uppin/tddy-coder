//! `#fs-port`: this crate reaches disk only through the filesystem port (`tddy-fs`).

use tddy_fs::Fs;

#[test]
fn a_session_dir_records_a_session_through_the_fs_port() {
    // Given — a session directory that exists only in memory.
    let fs = tddy_fs::MemFs::new();
    let session_dir = std::path::Path::new("/sessions/s-1");
    fs.create_dir_all(session_dir).expect("the session dir");

    // Then — no record yet: only "no such file" is none.
    assert!(
        !tddy_session_agents::svc_provision_agent_clone::records_a_session_on(&fs, session_dir)
            .expect("a missing record is not an error"),
        "a session directory without a record names no session"
    );

    // When — the record is written through the port.
    fs.write(
        &session_dir.join(tddy_core::SESSION_METADATA_FILENAME),
        b"session_id: s-1\n",
    )
    .expect("the session record");

    // Then — the directory records a session.
    assert!(
        tddy_session_agents::svc_provision_agent_clone::records_a_session_on(&fs, session_dir)
            .expect("a present record reads"),
        "a session directory with a record names a session"
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
