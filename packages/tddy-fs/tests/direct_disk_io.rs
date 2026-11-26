//! The guard a migrated crate runs over itself: production code reaches disk only through the port.

use std::path::Path;

use tddy_fs::testing::direct_disk_io_in;

fn a_crate_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, text) in files {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

#[test]
fn direct_disk_io_in_production_code_is_reported_with_file_and_line() {
    let krate = a_crate_with(&[(
        "src/store.rs",
        "use std::path::Path;\n\npub fn load(p: &Path) -> String {\n    std::fs::read_to_string(p).unwrap()\n}\n",
    )]);

    assert_eq!(
        direct_disk_io_in(krate.path()),
        vec!["src/store.rs:4: std::fs::read_to_string(p).unwrap()".to_string()]
    );
}

#[test]
fn tokio_fs_and_an_imported_fs_module_are_reported_too() {
    let krate = a_crate_with(&[(
        "src/lib.rs",
        "use std::fs;\npub async fn a() { tokio::fs::remove_file(\"x\").await.ok(); }\npub fn b() { fs::remove_file(\"y\").ok(); }\n",
    )]);

    let hits = direct_disk_io_in(krate.path());

    assert_eq!(hits.len(), 3, "{hits:?}");
}

#[test]
fn test_code_comments_and_port_calls_are_not_reported() {
    let krate = a_crate_with(&[(
        "src/lib.rs",
        "// std::fs::write is what we used to call\npub fn save(fs: &dyn tddy_fs::Fs) { fs.write(\"a\".as_ref(), b\"\").ok(); }\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() { std::fs::write(\"/tmp/x\", \"\").ok(); }\n}\n",
    )]);

    assert_eq!(direct_disk_io_in(krate.path()), Vec::<String>::new());
}

#[test]
fn this_crate_itself_is_exempt_only_in_its_disk_adapter() {
    let hits = direct_disk_io_in(Path::new(env!("CARGO_MANIFEST_DIR")));
    assert!(
        hits.iter()
            .all(|h| h.starts_with("src/disk.rs:") || h.starts_with("src/testing.rs:")),
        "{hits:?}"
    );
    assert!(
        hits.iter().any(|h| h.starts_with("src/disk.rs:")),
        "the real adapter touches disk"
    );
}
