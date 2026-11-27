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
fn uploads_are_listed_through_the_port() {
    let fs = tddy_fs::MemFs::new();
    let base = std::path::Path::new("/data/sessions");
    let upload = tddy_core::session_lifecycle::unified_session_dir_path(base, "s1")
        .join("uploads/u1/notes.md");
    tddy_fs::Fs::write_atomic(&fs, &upload, b"hello", None).unwrap();

    let listed = tddy_session_files::session_uploads::list_uploads_on(&fs, base, "s1").unwrap();

    assert_eq!(
        listed
            .iter()
            .map(|e| (e.upload_id.as_str(), e.file_name.as_str(), e.size_bytes))
            .collect::<Vec<_>>(),
        vec![("u1", "notes.md", 5)]
    );
}

#[test]
fn a_session_without_uploads_lists_none_through_the_port() {
    let fs = tddy_fs::MemFs::new();
    let listed = tddy_session_files::session_uploads::list_uploads_on(
        &fs,
        std::path::Path::new("/data/sessions"),
        "s2",
    );
    assert_eq!(listed.map(|l| l.len()).ok(), Some(0));
}
