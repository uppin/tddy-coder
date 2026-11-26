//! The atomic write as a port operation: a reader sees the whole old file or the whole new one.

use std::io::ErrorKind;
use std::path::Path;

use tddy_fs::{DiskFs, Fs, MemFs};

fn with_old_contents(fs: &MemFs, path: &Path, old: &str) {
    fs.write_atomic(path, old.as_bytes(), None)
        .expect("first write");
}

#[test]
fn an_atomic_write_creates_missing_parents_and_replaces_the_contents() {
    let fs = MemFs::new();
    let path = Path::new("/state/deep/.session.yaml");
    fs.write_atomic(path, b"v1", None).unwrap();
    fs.write_atomic(path, b"v2", None).unwrap();
    assert_eq!(fs.read_to_string(path).unwrap(), "v2");
}

#[test]
fn a_failed_atomic_write_leaves_the_old_contents_whole() {
    let fs = MemFs::new();
    let path = Path::new("/state/.session.yaml");
    with_old_contents(&fs, path, "old session");
    fs.fail_next_write(ErrorKind::StorageFull);

    let err = fs
        .write_atomic(path, b"new session that never lands", None)
        .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::StorageFull);
    assert_eq!(fs.read_to_string(path).unwrap(), "old session");
}

#[test]
fn a_failed_atomic_write_leaves_no_swap_file_behind() {
    let fs = MemFs::new();
    let path = Path::new("/state/.session.yaml");
    with_old_contents(&fs, path, "old");
    fs.fail_next_write(ErrorKind::StorageFull);

    let _ = fs.write_atomic(path, b"new", None);

    assert_eq!(
        fs.paths(),
        vec![
            Path::new("/").to_path_buf(),
            "/state".into(),
            path.to_path_buf()
        ]
    );
}

#[test]
fn a_failed_first_atomic_write_leaves_the_target_absent() {
    let fs = MemFs::new();
    let path = Path::new("/vault/credentials.vault");
    fs.fail_next_write(ErrorKind::StorageFull);

    assert!(fs.write_atomic(path, b"secret", Some(0o600)).is_err());
    assert!(!fs.exists(path));
    fs.write_atomic(path, b"secret", Some(0o600)).expect("the fault fires once");
    assert!(fs.exists(path));
}

#[cfg(unix)]
#[test]
fn a_first_atomic_write_with_a_mode_is_created_at_exactly_that_mode() {
    for fs in [Box::new(MemFs::new()) as Box<dyn Fs>, Box::new(DiskFs)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.vault");
        fs.write_atomic(&path, b"secret", Some(0o600)).unwrap();
        assert_eq!(
            fs.metadata(&path).unwrap().mode.map(|m| m & 0o777),
            Some(0o600)
        );
    }
}

#[cfg(unix)]
#[test]
fn an_atomic_rewrite_without_a_mode_keeps_the_existing_mode() {
    for fs in [Box::new(MemFs::new()) as Box<dyn Fs>, Box::new(DiskFs)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".session.yaml");
        fs.write_atomic(&path, b"token: a", Some(0o600)).unwrap();
        fs.write_atomic(&path, b"token: b", None).unwrap();
        assert_eq!(
            fs.metadata(&path).unwrap().mode.map(|m| m & 0o777),
            Some(0o600)
        );
    }
}

#[test]
fn the_free_function_writes_through_the_disk_adapter() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x/.session");
    tddy_fs::write_atomic(&path, "id-1").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "id-1");
}

#[test]
fn a_labelled_failure_names_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("is-a-file");
    std::fs::write(&blocker, "").unwrap();
    let path = blocker.join("child");
    let err = tddy_fs::write_atomic_labelled(&path, "x").unwrap_err();
    assert!(err.starts_with(&path.display().to_string()), "{err}");
    assert!(!err.contains("TODO"), "{err}");
}
