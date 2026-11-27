//! `#fs-port`: this crate reaches disk only through the filesystem port (`tddy-fs`).

use tddy_fs::Fs;

#[test]
fn conversation_directories_are_listed_through_the_fs_port() {
    // Given — conversation worktrees that exist only in memory, plus a file that is none.
    let fs = tddy_fs::MemFs::new();
    let directory = std::path::Path::new("/sessions/s-1/conversations");
    fs.create_dir_all(&directory.join("b")).expect("conversation b");
    fs.create_dir_all(&directory.join("a")).expect("conversation a");
    fs.write(&directory.join("note.txt"), b"not a conversation")
        .expect("a file beside the conversations");

    // When — the close planner lists the directory through the port.
    let names =
        tddy_session_lifecycle::connection_service::svc_close_session::conversation_worktrees::conversation_directory_names_on(
            &fs, directory,
        )
        .expect("a listing");

    // Then — both conversation directories come back sorted, and the file is skipped.
    assert_eq!(
        names,
        vec!["a".to_string(), "b".to_string()],
        "the close plan sees every conversation worktree, in memory rather than on disk"
    );

    // And — a directory that does not exist is no conversation, not an error to hide.
    let missing = tddy_session_lifecycle::connection_service::svc_close_session::conversation_worktrees::conversation_directory_names_on(
        &fs,
        &directory.join("no-such-conversations"),
    );
    assert!(
        matches!(
            missing,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        ),
        "a missing conversations directory reads as none"
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
