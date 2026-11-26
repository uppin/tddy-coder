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
fn egress_lines_are_appended_through_the_port_one_per_line() {
    let fs = tddy_fs::MemFs::new();
    let log = std::path::Path::new("/scratch/egress/allowed.log");

    tddy_sandbox::append_line_on(&fs, log, "github.com:443").unwrap();
    tddy_sandbox::append_line_on(&fs, log, "crates.io:443\n").unwrap();

    assert_eq!(
        tddy_fs::Fs::read_to_string(&fs, log).unwrap(),
        "github.com:443\ncrates.io:443\n"
    );
}
