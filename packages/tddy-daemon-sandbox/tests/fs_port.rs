//! `#fs-port`: this crate reaches disk only through the filesystem port (`tddy-fs`).

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

#[test]
fn workspace_jail_tree_is_prepared_through_the_fs_port() {
    // Given a session dir on the port's filesystem
    let fs = tddy_fs::MemFs::new();
    let session = std::path::Path::new("/sessions/s1");

    // When the workspace jail tree is prepared through the port
    tddy_daemon_sandbox::workspace_tool_sandbox::prepare_jail_tree_on(&fs, session)
        .expect("preparing a workspace jail tree succeeds");

    // Then the jail's directories exist, with no temp dir involved
    for dir in [
        "sandbox",
        "sandbox/.work/home",
        "sandbox/.work/tmp",
        "sandbox/context",
        "sandbox/egress",
    ] {
        assert!(
            tddy_fs::Fs::metadata(&fs, &session.join(dir)).is_ok(),
            "workspace jail dir {dir} must exist through the port"
        );
    }
}
