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
fn self_bind_socket_dir_is_prepared_through_the_fs_port() {
    // Given a data dir holding a stale socket file a dead daemon left
    let fs = tddy_fs::MemFs::new();
    let socket = std::path::Path::new("/data/run/u/daemon.sock");
    tddy_fs::Fs::create_dir_all(&fs, std::path::Path::new("/data/run/u")).unwrap();
    tddy_fs::Fs::write(&fs, socket, b"stale").unwrap();

    // When the local-socket server prepares a self-bind through the port
    tddy_daemon::local_socket_server::prepare_self_bind_socket_on(&fs, socket)
        .expect("preparing a self-bind succeeds");

    // Then its directory exists and the stale file is gone — `bind` meets a clear path
    assert!(tddy_fs::Fs::metadata(&fs, std::path::Path::new("/data/run/u")).is_ok());
    assert!(
        tddy_fs::Fs::read(&fs, socket).is_err(),
        "the stale socket file must be gone"
    );
}
