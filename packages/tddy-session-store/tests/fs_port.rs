//! Session state reads and writes through the filesystem port, so its behaviour is testable
//! without a temporary directory.

use std::path::Path;

use tddy_fs::{Fs, MemFs};
use tddy_session_store::output::{
    create_session_dir_with_id_on, read_session_file_on, write_session_file_on,
};
use tddy_session_store::session_actions::parse_action_manifest_file_on;

const THE_SESSION: &str = "019a-session";

#[test]
fn a_session_id_round_trips_through_the_port() {
    let fs = MemFs::new();
    let dir = create_session_dir_with_id_on(&fs, Path::new("/data"), THE_SESSION).unwrap();

    write_session_file_on(&fs, &dir, THE_SESSION).unwrap();

    assert_eq!(read_session_file_on(&fs, &dir).unwrap(), THE_SESSION);
    assert!(fs.exists(
        &Path::new("/data/sessions")
            .join(THE_SESSION)
            .join(".session")
    ));
}

#[test]
fn a_session_file_write_that_hits_a_full_disk_keeps_the_previous_id() {
    let fs = MemFs::new();
    let dir = create_session_dir_with_id_on(&fs, Path::new("/data"), THE_SESSION).unwrap();
    write_session_file_on(&fs, &dir, "previous").unwrap();
    fs.fail_next_write(std::io::ErrorKind::StorageFull);

    assert!(write_session_file_on(&fs, &dir, "next").is_err());
    assert_eq!(read_session_file_on(&fs, &dir).unwrap(), "previous");
}

#[test]
fn an_action_manifest_is_parsed_from_the_port() {
    let fs = MemFs::new();
    let path = Path::new("/repo/.tddy/actions/build.yaml");
    fs.write_atomic(
        path,
        b"id: build\nversion: 1\ncommand: [\"cargo\", \"build\"]\n",
        None,
    )
    .unwrap();

    let manifest = parse_action_manifest_file_on(&fs, path).unwrap();

    assert_eq!(manifest.id, "build");
}

#[test]
fn production_code_reaches_disk_only_through_the_port() {
    let hits = tddy_fs::testing::direct_disk_io_in(Path::new(env!("CARGO_MANIFEST_DIR")));
    assert!(
        hits.is_empty(),
        "direct disk I/O outside the port:\n{}",
        hits.join("\n")
    );
}
