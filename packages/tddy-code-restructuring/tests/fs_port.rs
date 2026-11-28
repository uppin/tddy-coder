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
fn a_file_is_hashed_through_the_port_and_absent_hashes_as_empty() {
    use tddy_code_restructuring::apply::hash_file_on;
    let fs = tddy_fs::MemFs::new();
    let path = std::path::Path::new("/ws/src/lib.rs");
    let absent = hash_file_on(&fs, path).unwrap();
    tddy_fs::Fs::write_atomic(&fs, path, b"", None).unwrap();
    let empty = hash_file_on(&fs, path).unwrap();
    tddy_fs::Fs::write_atomic(&fs, path, b"fn main() {}", None).unwrap();
    let full = hash_file_on(&fs, path).unwrap();

    assert_eq!(absent, empty);
    assert_ne!(empty, full);
    assert!(full.starts_with("sha256:"), "{full}");
}
