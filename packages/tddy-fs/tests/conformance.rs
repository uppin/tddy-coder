//! One behaviour suite, run against both adapters: whatever passes on `MemFs` in a test holds on
//! `DiskFs` in production.

use std::path::{Path, PathBuf};

use tddy_fs::{DiskFs, Fs, FsKind, MemFs};

/// An adapter under test plus the root every path in a check lives under.
struct Subject {
    fs: Box<dyn Fs>,
    root: PathBuf,
    _dir: Option<tempfile::TempDir>,
}

impl Subject {
    fn disk() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        Self {
            fs: Box::new(DiskFs),
            root: dir.path().to_path_buf(),
            _dir: Some(dir),
        }
    }
    fn mem() -> Self {
        Self {
            fs: Box::new(MemFs::new()),
            root: PathBuf::from("/mem"),
            _dir: None,
        }
    }
    fn at(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

fn each_adapter(check: impl Fn(&Subject)) {
    check(&Subject::disk());
    check(&Subject::mem());
}

fn written(s: &Subject, rel: &str, contents: &str) -> PathBuf {
    let path = s.at(rel);
    s.fs.create_dir_all(path.parent().unwrap())
        .expect("parent created");
    s.fs.write(&path, contents.as_bytes()).expect("written");
    path
}

#[test]
fn a_written_file_reads_back_byte_for_byte() {
    each_adapter(|s| {
        let path = written(s, "a/b.txt", "hello");
        assert_eq!(s.fs.read(&path).unwrap(), b"hello");
        assert_eq!(s.fs.read_to_string(&path).unwrap(), "hello");
    });
}

#[test]
fn reading_a_missing_file_is_not_found() {
    each_adapter(|s| {
        let err = s.fs.read(&s.at("missing")).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    });
}

#[test]
fn append_adds_to_the_end_and_creates_an_absent_file() {
    each_adapter(|s| {
        s.fs.create_dir_all(&s.root).unwrap();
        let path = s.at("log");
        s.fs.append(&path, b"one\n").unwrap();
        s.fs.append(&path, b"two\n").unwrap();
        assert_eq!(s.fs.read_to_string(&path).unwrap(), "one\ntwo\n");
    });
}

#[test]
fn rename_replaces_an_existing_target() {
    each_adapter(|s| {
        let from = written(s, "from", "new");
        let to = written(s, "to", "old");
        s.fs.rename(&from, &to).unwrap();
        assert_eq!(s.fs.read_to_string(&to).unwrap(), "new");
        assert!(!s.fs.exists(&from));
    });
}

#[test]
fn remove_dir_all_removes_the_whole_subtree() {
    each_adapter(|s| {
        written(s, "tree/x/y.txt", "y");
        s.fs.remove_dir_all(&s.at("tree")).unwrap();
        assert!(!s.fs.exists(&s.at("tree/x/y.txt")));
        assert!(!s.fs.exists(&s.at("tree")));
    });
}

#[test]
fn remove_file_removes_only_that_file() {
    each_adapter(|s| {
        let gone = written(s, "d/gone", "1");
        let kept = written(s, "d/kept", "2");
        s.fs.remove_file(&gone).unwrap();
        assert!(!s.fs.exists(&gone));
        assert!(s.fs.exists(&kept));
    });
}

#[test]
fn read_dir_lists_full_paths_sorted() {
    each_adapter(|s| {
        written(s, "dir/b", "");
        written(s, "dir/a", "");
        s.fs.create_dir_all(&s.at("dir/c")).unwrap();
        assert_eq!(
            s.fs.read_dir(&s.at("dir")).unwrap(),
            vec![s.at("dir/a"), s.at("dir/b"), s.at("dir/c")]
        );
    });
}

#[test]
fn metadata_tells_a_file_from_a_directory_and_reports_its_length() {
    each_adapter(|s| {
        let file = written(s, "m/f", "12345");
        let file_meta = s.fs.metadata(&file).unwrap();
        assert_eq!((file_meta.kind, file_meta.len), (FsKind::File, 5));
        assert_eq!(s.fs.metadata(&s.at("m")).unwrap().kind, FsKind::Dir);
    });
}

#[cfg(unix)]
#[test]
fn set_mode_is_reported_by_metadata() {
    each_adapter(|s| {
        let file = written(s, "p/secret", "x");
        s.fs.set_mode(&file, 0o600).unwrap();
        assert_eq!(
            s.fs.metadata(&file).unwrap().mode.map(|m| m & 0o777),
            Some(0o600)
        );
    });
}

#[test]
fn canonicalize_resolves_dot_dot() {
    each_adapter(|s| {
        written(s, "c/target", "");
        let through_parent = s.at("c/../c/target");
        let canonical = s.fs.canonicalize(&through_parent).unwrap();
        assert_eq!(canonical, s.fs.canonicalize(&s.at("c/target")).unwrap());
        assert!(!canonical.components().any(|c| c.as_os_str() == ".."));
    });
}

#[test]
fn exists_tells_a_present_path_from_an_absent_one() {
    each_adapter(|s| {
        let present = written(s, "e/here", "");
        assert!(s.fs.exists(&present));
        assert!(!s.fs.exists(Path::new(&s.at("e/nothing-here"))));
    });
}
