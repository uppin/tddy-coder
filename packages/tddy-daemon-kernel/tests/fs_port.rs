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
fn the_user_config_is_read_through_the_port() {
    let fs = tddy_fs::MemFs::new();
    tddy_fs::Fs::write_atomic(
        &fs,
        std::path::Path::new("/home/u/.tddy/config.yaml"),
        b"spawn_path_extra: /opt/agent/bin\n",
        None,
    )
    .unwrap();

    let config = tddy_daemon_kernel::user_paths::load_tddy_user_config_on(
        &fs,
        std::path::Path::new("/home/u"),
    );

    assert_eq!(
        config.and_then(|c| c.spawn_path_extra).as_deref(),
        Some("/opt/agent/bin")
    );
}
