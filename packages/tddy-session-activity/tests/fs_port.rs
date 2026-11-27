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
fn an_answer_wait_round_trips_through_the_port() {
    use std::time::Duration;
    use tddy_session_activity::answer_wait::{record_answer_wait_on, recorded_answer_wait_on};
    let fs = tddy_fs::MemFs::new();
    let dir = std::path::Path::new("/data/sessions/s1");
    tddy_fs::Fs::create_dir_all(&fs, dir).unwrap();

    record_answer_wait_on(&fs, dir, Duration::from_secs(90)).unwrap();

    assert_eq!(
        recorded_answer_wait_on(&fs, dir),
        Some(Duration::from_secs(90))
    );
}
