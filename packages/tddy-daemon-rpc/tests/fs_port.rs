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
fn sessions_referencing_work_queues_are_listed_through_the_fs_port() {
    use std::collections::HashMap;

    // Given a data dir where one session owns a work queue and one does not
    let fs = tddy_fs::MemFs::new();
    let queue_file = std::path::Path::new("/data/sessions/s1").join(
        tddy_core::session_work_queue::WORK_QUEUE_FILE,
    );
    tddy_fs::Fs::create_dir_all(&fs, std::path::Path::new("/data/sessions/s1")).unwrap();
    tddy_fs::Fs::create_dir_all(&fs, std::path::Path::new("/data/sessions/s2")).unwrap();
    tddy_fs::Fs::write(&fs, &queue_file, b"{}").unwrap();

    // When the sessions referencing a work queue are listed through the port
    let by_stack = tddy_daemon_rpc::work_queue::referencing_sessions_by_stack_on(
        &fs,
        std::path::Path::new("/data"),
    );

    // Then the owning session is listed under its own id, with no temp dir involved
    assert_eq!(
        by_stack,
        HashMap::from([("s1".to_string(), vec!["s1".to_string()])])
    );
}
