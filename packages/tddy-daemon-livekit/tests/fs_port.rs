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
fn wip_scratch_index_is_seeded_through_the_fs_port() {
    // Given an agent's index on the port's filesystem
    let fs = tddy_fs::MemFs::new();
    let agents_index = std::path::Path::new("/repo/.git/index");
    tddy_fs::Fs::create_dir_all(&fs, std::path::Path::new("/repo/.git")).unwrap();
    tddy_fs::Fs::create_dir_all(&fs, std::path::Path::new("/tmp/tddy-wip-index")).unwrap();
    tddy_fs::Fs::write(&fs, agents_index, b"DIRC-stat-cache").unwrap();
    let scratch_index = std::path::Path::new("/tmp/tddy-wip-index/index");

    // When the WIP measurement seeds its scratch index through the port
    tddy_daemon_livekit::session_room::seed_scratch_index_on(&fs, agents_index, scratch_index)
        .expect("seeding a scratch index succeeds");

    // Then the scratch index carries the agent's bytes, with no temp dir involved
    assert_eq!(
        tddy_fs::Fs::read(&fs, scratch_index).expect("scratch index was seeded"),
        b"DIRC-stat-cache"
    );
}

#[test]
fn a_missing_agent_index_seeds_nothing_through_the_fs_port() {
    // Given no agent index at all
    let fs = tddy_fs::MemFs::new();
    let scratch_index = std::path::Path::new("/tmp/tddy-wip-index/index");

    // When the WIP measurement seeds through the port
    tddy_daemon_livekit::session_room::seed_scratch_index_on(
        &fs,
        std::path::Path::new("/repo/.git/index"),
        scratch_index,
    )
    .expect("a missing agent index is not an error");

    // Then nothing was seeded — staging starts from an empty index, as before
    assert!(
        tddy_fs::Fs::read(&fs, scratch_index).is_err(),
        "no agent index means no scratch index"
    );
}
