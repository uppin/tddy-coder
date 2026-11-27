//! `#fs-port`: this crate reaches disk only through the filesystem port (`tddy-fs`).

use tddy_fs::Fs;

#[test]
fn hook_registrations_are_written_through_the_fs_port() {
    // Given — registrations the slots rendered, one nested below its parent.
    let fs = tddy_fs::MemFs::new();
    let mut files = vec![
        (
            std::path::PathBuf::from("/sessions/s-1/mcp.json"),
            br#"{"mcp": true}"#.to_vec(),
        ),
        (
            std::path::PathBuf::from("/sessions/s-1/hooks/hooks.json"),
            br#"{"hooks": []}"#.to_vec(),
        ),
    ];

    // When — they are written through the port.
    tddy_session_split::agent_argv::write_registrations_on(&fs, &mut files)
        .expect("registrations write through the port");

    // Then — both land with their exact bytes, parents created on the way.
    assert!(files.is_empty(), "written files are drained");
    for (path, want) in [
        ("/sessions/s-1/mcp.json", &br#"{"mcp": true}"#[..]),
        ("/sessions/s-1/hooks/hooks.json", &br#"{"hooks": []}"#[..]),
    ] {
        assert_eq!(
            fs.read(std::path::Path::new(path)).expect("written"),
            want,
            "{path} holds what the slots rendered, in memory rather than on disk"
        );
    }
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
