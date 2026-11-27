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
fn create_new_writes_once_then_refuses() {
    each_adapter(|s| {
        let path = s.at("n/only-once");
        s.fs.create_dir_all(path.parent().unwrap()).unwrap();
        s.fs.create_new(&path, b"first").unwrap();
        assert_eq!(s.fs.read(&path).unwrap(), b"first");
        // The loser leaves the stored bytes untouched.
        let err = s.fs.create_new(&path, b"second").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(s.fs.read(&path).unwrap(), b"first");
    });
}

#[test]
fn create_new_refuses_an_existing_directory() {
    each_adapter(|s| {
        let dir = s.at("n/dir");
        s.fs.create_dir_all(&dir).unwrap();
        let err = s.fs.create_new(&dir, b"bytes").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    });
}

#[test]
fn copy_duplicates_bytes_and_replaces() {
    each_adapter(|s| {
        let from = written(s, "c/original", "bytes");
        let to = s.at("c/duplicate");
        s.fs.copy(&from, &to).unwrap();
        assert_eq!(s.fs.read(&to).unwrap(), b"bytes");
        // Still the source's bytes after the source moves on.
        s.fs.write(&from, b"changed").unwrap();
        assert_eq!(s.fs.read(&to).unwrap(), b"bytes");
        // Replacing is the caller's choice, not a refusal.
        s.fs.copy(&from, &to).unwrap();
        assert_eq!(s.fs.read(&to).unwrap(), b"changed");
    });
}

#[test]
fn copy_needs_a_file_source() {
    each_adapter(|s| {
        let err = s.fs.copy(&s.at("missing"), &s.at("to")).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        let dir = s.at("c/dir");
        s.fs.create_dir_all(&dir).unwrap();
        let err = s.fs.copy(&dir, &s.at("to")).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::IsADirectory);
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
fn hard_link_publishes_without_replacing() {
    each_adapter(|s| {
        let original = written(s, "h/original", "key");
        let link = s.at("h/link");
        s.fs.hard_link(&original, &link).unwrap();
        assert_eq!(s.fs.read_to_string(&link).unwrap(), "key");
        // A link never silently replaces the winner: the target must not exist.
        let err = s.fs.hard_link(&original, &link).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(s.fs.read_to_string(&original).unwrap(), "key");
    });
}

#[test]
fn hard_link_needs_an_existing_source() {
    each_adapter(|s| {
        let err = s.fs.hard_link(&s.at("missing"), &s.at("link")).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    });
}

#[test]
fn remove_dir_removes_only_an_empty_directory() {
    each_adapter(|s| {
        let empty = s.at("r/empty");
        s.fs.create_dir_all(&empty).unwrap();
        s.fs.remove_dir(&empty).unwrap();
        assert!(!s.fs.exists(&empty));

        written(s, "r/full/child", "x");
        let err = s.fs.remove_dir(&s.at("r/full")).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::DirectoryNotEmpty);
        assert!(s.fs.exists(&s.at("r/full/child")));

        let file = written(s, "r/file", "x");
        let err = s.fs.remove_dir(&file).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotADirectory);

        let err = s.fs.remove_dir(&s.at("r/absent")).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    });
}

#[test]
fn a_replacement_gets_a_new_identity_while_a_rename_keeps_it() {
    each_adapter(|s| {
        let path = written(s, "i/file", "one");
        let before = s.fs.metadata(&path).unwrap();
        let (dev, ino) = (before.dev, before.ino);
        assert!(dev.is_some() && ino.is_some(), "{before:?}");

        let moved = s.at("i/moved");
        s.fs.rename(&path, &moved).unwrap();
        let after_move = s.fs.metadata(&moved).unwrap();
        assert_eq!((after_move.dev, after_move.ino), (dev, ino));

        s.fs.write_atomic(&moved, b"two", None).unwrap();
        let after_replace = s.fs.metadata(&moved).unwrap();
        assert_eq!(after_replace.dev, dev);
        assert_ne!(after_replace.ino, ino);
    });
}

#[test]
fn a_truncating_write_keeps_the_identity() {
    each_adapter(|s| {
        let path = written(s, "t/file", "one");
        let ino = s.fs.metadata(&path).unwrap().ino;
        s.fs.write(&path, b"two").unwrap();
        assert_eq!(s.fs.metadata(&path).unwrap().ino, ino);
        assert_eq!(s.fs.read_to_string(&path).unwrap(), "two");
    });
}

#[cfg(unix)]
#[test]
fn create_dir_all_with_mode_lands_owner_only_from_birth() {
    use std::os::unix::fs::PermissionsExt;

    each_adapter(|s| {
        let dir = s.at("m/owner-only");
        s.fs.create_dir_all_with_mode(&dir, 0o700).unwrap();
        assert_eq!(
            s.fs.metadata(&dir).unwrap().mode.map(|m| m & 0o777),
            Some(0o700)
        );
        // The mode is in the creation, so a check of the real directory straight after
        // sees no wider window either.
        if let Some(temp) = &s._dir {
            assert_eq!(
                std::fs::metadata(temp.path().join("m/owner-only"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    });
}

#[cfg(unix)]
#[test]
fn metadata_reports_the_owning_uid() {
    each_adapter(|s| {
        let path = written(s, "u/file", "x");
        assert!(s.fs.metadata(&path).unwrap().uid.is_some());
    });
}

#[cfg(unix)]
#[test]
fn metadata_follow_sees_through_a_symlink() {
    each_adapter(|s| {
        let target = written(s, "l/target", "hello");
        let link = s.at("l/link");
        // Only the real disk holds symlinks; the tree resolves lexically, so there a
        // following stat is the same stat.
        if s._dir.is_some() {
            std::os::unix::fs::symlink(&target, &link).unwrap();
            let followed = s.fs.metadata_follow(&link).unwrap();
            assert_eq!((followed.kind, followed.len), (FsKind::File, 5));
            assert_eq!(s.fs.metadata(&link).unwrap().kind, FsKind::Symlink);
        } else {
            let mem = s.fs.metadata_follow(&target).unwrap();
            assert_eq!(mem, s.fs.metadata(&target).unwrap());
        }
    });
}

#[cfg(unix)]
#[test]
fn a_bound_socket_stats_as_a_socket() {
    use std::os::unix::net::UnixListener;

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("daemon.sock");
    let listener = UnixListener::bind(&path).unwrap();
    assert_eq!(DiskFs.metadata(&path).unwrap().kind, FsKind::Socket);
    drop(listener);
}

#[test]
fn a_recorded_socket_stats_as_a_socket_and_refuses_bytes() {
    let mem = MemFs::new();
    let path = PathBuf::from("/s/daemon.sock");
    mem.create_dir_all(path.parent().unwrap()).unwrap();
    mem.create_socket(&path).unwrap();
    assert_eq!(mem.metadata(&path).unwrap().kind, FsKind::Socket);
    assert!(mem.read(&path).is_err());
    assert!(mem.write(&path, b"x").is_err());
}

#[test]
fn exists_tells_a_present_path_from_an_absent_one() {
    each_adapter(|s| {
        let present = written(s, "e/here", "");
        assert!(s.fs.exists(&present));
        assert!(!s.fs.exists(Path::new(&s.at("e/nothing-here"))));
    });
}
