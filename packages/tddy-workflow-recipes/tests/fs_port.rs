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
fn a_recipe_session_id_round_trips_through_the_port() {
    use tddy_workflow_recipes::writer::{read_session_file_on, write_session_file_on};
    let fs = tddy_fs::MemFs::new();
    let dir = std::path::Path::new("/plans/feature");

    write_session_file_on(&fs, dir, "019a-recipe").unwrap();

    assert_eq!(read_session_file_on(&fs, dir).unwrap(), "019a-recipe");
}
