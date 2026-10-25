//! Files the operator dropped onto the window, and the two commands that let the page read them.
//!
//! The window keeps Tauri's native drag-drop handler, because it is the only thing that knows the
//! dropped files' real paths — the webview's own HTML5 `drop` hands a page `File` objects with no
//! path. The page hit-tests the drop's position against its terminals (see `tddy-tauri-web`'s
//! `nativeFileDrop`) and either types the paths into a session that reads this host's filesystem
//! or, for one that cannot, uploads the files' bytes into the session directory. Those bytes are
//! what the commands here serve.
//!
//! **Only what was dropped is readable.** [`DroppedFiles`] holds the paths of the most recent drop,
//! recorded from the window event *before* the page hears about the drop, and every read is checked
//! against it — so these commands are not a general file reader for whatever the page names.

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;

/// The most a single [`DroppedFiles::read`] returns. The page reads in upload-chunk-sized slices
/// (48 KiB); the cap keeps a page that asks for more from making this process buffer a whole file.
pub const MAX_READ_BYTES: u32 = 1024 * 1024;

/// What the page is told about one dropped path before it decides how to deliver it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DroppedFile {
    pub path: String,
    /// The final path segment — the name an upload stores the file under.
    pub name: String,
    /// Bytes, for a regular file; 0 for anything else.
    pub size: u64,
    /// `false` for a folder (or anything else that is not a regular file), which has no bytes to
    /// upload.
    pub is_file: bool,
}

/// The paths of the most recent drop onto the window.
#[derive(Debug, Default)]
pub struct DroppedFiles {
    latest: Mutex<HashSet<PathBuf>>,
}

impl DroppedFiles {
    /// Remember `paths` as the only ones readable from now on, forgetting the previous drop's.
    pub fn record_drop(&self, paths: &[PathBuf]) {
        let _ = paths;
        todo!("record the drop")
    }

    /// Describe each of `paths`, in order. Refused as a whole if any was not part of the last drop.
    pub fn describe(&self, paths: &[String]) -> Result<Vec<DroppedFile>, String> {
        let _ = paths;
        todo!("describe dropped paths")
    }

    /// Up to `length` bytes of the dropped file at `path`, starting at `offset`. Fewer at the end of
    /// the file, none past it.
    pub fn read(&self, path: &str, offset: u64, length: u32) -> Result<Vec<u8>, String> {
        let _ = (path, offset, length);
        todo!("read a dropped file")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory of its own per test, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "tddy-desktop-file-drop-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("scratch dir");
            Self(dir)
        }

        fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, bytes).expect("scratch file");
            path
        }

        fn folder(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::create_dir_all(&path).expect("scratch folder");
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn text(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    fn files_dropped(paths: &[PathBuf]) -> DroppedFiles {
        let dropped = DroppedFiles::default();
        dropped.record_drop(paths);
        dropped
    }

    #[test]
    fn describes_each_dropped_path_in_the_order_asked() {
        // Given a file whose name needs quoting and a folder, both dropped
        let scratch = Scratch::new("describe");
        let file = scratch.file("Screen Shot 'final' ✓.png", b"0123456789");
        let folder = scratch.folder("a folder");
        let dropped = files_dropped(&[file.clone(), folder.clone()]);

        // When
        let described = dropped
            .describe(&[text(&folder), text(&file)])
            .expect("both were dropped");

        // Then
        assert_eq!(
            described,
            vec![
                DroppedFile {
                    path: text(&folder),
                    name: "a folder".to_string(),
                    size: 0,
                    is_file: false,
                },
                DroppedFile {
                    path: text(&file),
                    name: "Screen Shot 'final' ✓.png".to_string(),
                    size: 10,
                    is_file: true,
                },
            ]
        );
    }

    #[test]
    fn refuses_to_describe_a_path_that_was_not_dropped() {
        // Given one file dropped and another that exists but was not
        let scratch = Scratch::new("describe-refuse");
        let dropped_file = scratch.file("dropped.txt", b"a");
        let other = scratch.file("other.txt", b"b");
        let dropped = files_dropped(&[dropped_file.clone()]);

        // When
        let result = dropped.describe(&[text(&dropped_file), text(&other)]);

        // Then
        let error = result.expect_err("a path outside the drop must be refused");
        assert!(error.contains("other.txt"), "the refusal names the path: {error}");
    }

    #[test]
    fn a_new_drop_replaces_what_the_last_one_allowed() {
        // Given two drops, one after the other
        let scratch = Scratch::new("replace");
        let first = scratch.file("first.txt", b"1");
        let second = scratch.file("second.txt", b"2");
        let dropped = files_dropped(&[first.clone()]);
        dropped.record_drop(&[second.clone()]);

        // When / Then — only the second drop's file is readable
        assert!(dropped.read(&text(&first), 0, 1).is_err());
        assert_eq!(dropped.read(&text(&second), 0, 1), Ok(b"2".to_vec()));
    }

    #[test]
    fn reads_the_requested_slice_of_a_dropped_file() {
        // Given
        let scratch = Scratch::new("read-slice");
        let file = scratch.file("data.bin", b"0123456789");
        let dropped = files_dropped(&[file.clone()]);

        // When
        let bytes = dropped.read(&text(&file), 3, 4);

        // Then
        assert_eq!(bytes, Ok(b"3456".to_vec()));
    }

    #[test]
    fn reads_only_what_remains_at_the_end_of_the_file() {
        // Given
        let scratch = Scratch::new("read-tail");
        let file = scratch.file("data.bin", b"0123456789");
        let dropped = files_dropped(&[file.clone()]);

        // When
        let tail = dropped.read(&text(&file), 8, 48);
        let past_the_end = dropped.read(&text(&file), 10, 48);

        // Then
        assert_eq!(tail, Ok(b"89".to_vec()));
        assert_eq!(past_the_end, Ok(Vec::new()));
    }

    #[test]
    fn refuses_to_read_a_path_that_was_not_dropped() {
        // Given
        let scratch = Scratch::new("read-refuse");
        let dropped_file = scratch.file("dropped.txt", b"a");
        let secret = scratch.file("secret.txt", b"do not read");
        let dropped = files_dropped(&[dropped_file]);

        // When
        let result = dropped.read(&text(&secret), 0, 16);

        // Then
        assert!(result.is_err(), "read a file that was never dropped: {result:?}");
    }

    #[test]
    fn refuses_a_read_larger_than_the_cap() {
        // Given
        let scratch = Scratch::new("read-cap");
        let file = scratch.file("data.bin", b"0123456789");
        let dropped = files_dropped(&[file.clone()]);

        // When
        let result = dropped.read(&text(&file), 0, MAX_READ_BYTES + 1);

        // Then
        assert!(result.is_err(), "an oversized read was served: {result:?}");
    }

    #[test]
    fn refuses_to_read_a_dropped_folder() {
        // Given
        let scratch = Scratch::new("read-folder");
        let folder = scratch.folder("a folder");
        let dropped = files_dropped(&[folder.clone()]);

        // When
        let result = dropped.read(&text(&folder), 0, 16);

        // Then
        assert!(result.is_err(), "a folder was read as a file: {result:?}");
    }
}
