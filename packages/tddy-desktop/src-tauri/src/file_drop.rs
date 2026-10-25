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
use tauri::ipc::Response;
use tauri::{DragDropEvent, PhysicalPosition, State};

/// The event the page hears a drag step on — Tauri's own drag events with their positions put in
/// the page's coordinates (see [`drag_step_of`]).
pub const NATIVE_DRAG_EVENT: &str = "tddy://native-file-drop";

/// Whether this platform's webview reports drag positions in device pixels. Tauri types every drag
/// position as a [`PhysicalPosition`], but only WebView2 measures in device pixels: WKWebView's
/// `draggingLocation` and GTK's drag coordinates are already in points — CSS pixels — so dividing
/// them by the scale factor would put a drop on a Retina display at half the pointer's coordinates.
const DRAG_POSITIONS_ARE_DEVICE_PIXELS: bool = cfg!(windows);

/// A point in the page's viewport, in CSS pixels — what `document.elementFromPoint` takes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ViewportPoint {
    pub x: f64,
    pub y: f64,
}

/// One step of a drag over the window, as the page is told about it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum NativeDragStep {
    Enter {
        paths: Vec<String>,
        point: ViewportPoint,
    },
    Over {
        point: ViewportPoint,
    },
    Drop {
        paths: Vec<String>,
        point: ViewportPoint,
    },
    Leave,
}

/// `event` as the page's drag step, its position in CSS pixels for a window at `scale_factor`.
pub fn drag_step_of(event: &DragDropEvent, scale_factor: f64) -> Option<NativeDragStep> {
    let point = |position| viewport_point(position, scale_factor, DRAG_POSITIONS_ARE_DEVICE_PIXELS);
    let texts = |paths: &[PathBuf]| -> Vec<String> {
        paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect()
    };
    match event {
        DragDropEvent::Enter { paths, position } => Some(NativeDragStep::Enter {
            paths: texts(paths),
            point: point(position),
        }),
        DragDropEvent::Over { position } => Some(NativeDragStep::Over {
            point: point(position),
        }),
        DragDropEvent::Drop { paths, position } => Some(NativeDragStep::Drop {
            paths: texts(paths),
            point: point(position),
        }),
        DragDropEvent::Leave => Some(NativeDragStep::Leave),
        // `DragDropEvent` is non-exhaustive; a kind of step this page does not know is not one
        // it can act on.
        _ => None,
    }
}

/// `position` in CSS pixels, given whether the webview measured it in device pixels.
fn viewport_point(
    position: &PhysicalPosition<f64>,
    scale_factor: f64,
    in_device_pixels: bool,
) -> ViewportPoint {
    let divisor = if in_device_pixels { scale_factor } else { 1.0 };
    ViewportPoint {
        x: position.x / divisor,
        y: position.y / divisor,
    }
}

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
        let mut latest = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        *latest = paths.iter().cloned().collect();
    }

    /// Describe each of `paths`, in order. Refused as a whole if any was not part of the last drop.
    pub fn describe(&self, paths: &[String]) -> Result<Vec<DroppedFile>, String> {
        paths
            .iter()
            .map(|path| {
                let path = self.dropped(path)?;
                let metadata = std::fs::metadata(&path)
                    .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
                Ok(DroppedFile {
                    path: path.to_string_lossy().into_owned(),
                    name: path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.to_string_lossy().into_owned()),
                    size: if metadata.is_file() {
                        metadata.len()
                    } else {
                        0
                    },
                    is_file: metadata.is_file(),
                })
            })
            .collect()
    }

    /// Up to `length` bytes of the dropped file at `path`, starting at `offset`. Fewer at the end of
    /// the file, none past it.
    pub fn read(&self, path: &str, offset: u64, length: u32) -> Result<Vec<u8>, String> {
        if length > MAX_READ_BYTES {
            return Err(format!(
                "a read of {length} bytes is over the {MAX_READ_BYTES}-byte limit"
            ));
        }
        let path = self.dropped(path)?;
        if !path.is_file() {
            return Err(format!("{} is not a file", path.display()));
        }
        let mut file = std::fs::File::open(&path)
            .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| format!("cannot seek {}: {e}", path.display()))?;
        let mut bytes = Vec::with_capacity(length as usize);
        file.take(u64::from(length))
            .read_to_end(&mut bytes)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Ok(bytes)
    }

    /// `path` as one the last drop carried, or the refusal naming it.
    fn dropped(&self, path: &str) -> Result<PathBuf, String> {
        let path = Path::new(path);
        let latest = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        if latest.contains(path) {
            Ok(path.to_path_buf())
        } else {
            Err(format!("{} was not dropped on this window", path.display()))
        }
    }
}

/// Describe the dropped `paths`, so the page can tell a file it can upload from a folder it cannot.
#[tauri::command]
pub async fn tddy_dropped_files_describe(
    paths: Vec<String>,
    dropped: State<'_, DroppedFiles>,
) -> Result<Vec<DroppedFile>, String> {
    dropped.describe(&paths)
}

/// A slice of one dropped file, as raw bytes rather than a JSON array of numbers.
#[tauri::command]
pub async fn tddy_dropped_file_read(
    path: String,
    offset: u64,
    length: u32,
    dropped: State<'_, DroppedFiles>,
) -> Result<Response, String> {
    dropped.read(&path, offset, length).map(Response::new)
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
        let dropped = files_dropped(std::slice::from_ref(&dropped_file));

        // When
        let result = dropped.describe(&[text(&dropped_file), text(&other)]);

        // Then
        let error = result.expect_err("a path outside the drop must be refused");
        assert!(
            error.contains("other.txt"),
            "the refusal names the path: {error}"
        );
    }

    #[test]
    fn a_new_drop_replaces_what_the_last_one_allowed() {
        // Given two drops, one after the other
        let scratch = Scratch::new("replace");
        let first = scratch.file("first.txt", b"1");
        let second = scratch.file("second.txt", b"2");
        let dropped = files_dropped(std::slice::from_ref(&first));
        dropped.record_drop(std::slice::from_ref(&second));

        // When / Then — only the second drop's file is readable
        assert!(dropped.read(&text(&first), 0, 1).is_err());
        assert_eq!(dropped.read(&text(&second), 0, 1), Ok(b"2".to_vec()));
    }

    #[test]
    fn reads_the_requested_slice_of_a_dropped_file() {
        // Given
        let scratch = Scratch::new("read-slice");
        let file = scratch.file("data.bin", b"0123456789");
        let dropped = files_dropped(std::slice::from_ref(&file));

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
        let dropped = files_dropped(std::slice::from_ref(&file));

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
        assert!(
            result.is_err(),
            "read a file that was never dropped: {result:?}"
        );
    }

    #[test]
    fn refuses_a_read_larger_than_the_cap() {
        // Given
        let scratch = Scratch::new("read-cap");
        let file = scratch.file("data.bin", b"0123456789");
        let dropped = files_dropped(std::slice::from_ref(&file));

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
        let dropped = files_dropped(std::slice::from_ref(&folder));

        // When
        let result = dropped.read(&text(&folder), 0, 16);

        // Then
        assert!(result.is_err(), "a folder was read as a file: {result:?}");
    }

    fn at(x: f64, y: f64) -> PhysicalPosition<f64> {
        PhysicalPosition::new(x, y)
    }

    #[test]
    fn an_entering_drag_carries_its_paths_unchanged() {
        // Given paths that need quoting in a shell, and one that is not ASCII
        let paths = vec![
            PathBuf::from("/Users/me/Screen Shot 'final'.png"),
            PathBuf::from("/Users/me/žinutė ✓.txt"),
        ];

        // When
        let step = drag_step_of(
            &DragDropEvent::Enter {
                paths,
                position: at(10.0, 20.0),
            },
            1.0,
        );

        // Then — quoting is the page's job; the paths travel exactly as the OS gave them
        assert_eq!(
            step,
            Some(NativeDragStep::Enter {
                paths: vec![
                    "/Users/me/Screen Shot 'final'.png".to_string(),
                    "/Users/me/žinutė ✓.txt".to_string(),
                ],
                point: ViewportPoint { x: 10.0, y: 20.0 },
            })
        );
    }

    #[test]
    fn a_drop_and_a_leave_map_to_their_steps() {
        // When
        let drop = drag_step_of(
            &DragDropEvent::Drop {
                paths: vec![PathBuf::from("/a")],
                position: at(1.0, 2.0),
            },
            1.0,
        );
        let leave = drag_step_of(&DragDropEvent::Leave, 1.0);

        // Then
        assert_eq!(
            drop,
            Some(NativeDragStep::Drop {
                paths: vec!["/a".to_string()],
                point: ViewportPoint { x: 1.0, y: 2.0 },
            })
        );
        assert_eq!(leave, Some(NativeDragStep::Leave));
    }

    #[test]
    fn a_position_already_in_points_is_not_scaled() {
        // Given a Retina display, where WKWebView reports the pointer in points
        // When
        let point = viewport_point(&at(400.0, 300.0), 2.0, false);

        // Then
        assert_eq!(point, ViewportPoint { x: 400.0, y: 300.0 });
    }

    #[test]
    fn a_position_in_device_pixels_is_divided_by_the_scale() {
        // Given a 150% display whose webview reports device pixels
        // When
        let point = viewport_point(&at(300.0, 150.0), 1.5, true);

        // Then
        assert_eq!(point, ViewportPoint { x: 200.0, y: 100.0 });
    }

    #[cfg(not(windows))]
    #[test]
    fn this_platforms_drag_positions_are_already_css_pixels() {
        // When a drag hovers on a Retina display
        let step = drag_step_of(
            &DragDropEvent::Over {
                position: at(400.0, 300.0),
            },
            2.0,
        );

        // Then the page is told the same point — the one under the pointer
        assert_eq!(
            step,
            Some(NativeDragStep::Over {
                point: ViewportPoint { x: 400.0, y: 300.0 }
            })
        );
    }

    #[test]
    fn a_drag_step_serializes_as_the_page_reads_it() {
        // Given
        let step = NativeDragStep::Drop {
            paths: vec!["/a b".to_string()],
            point: ViewportPoint { x: 1.5, y: 2.0 },
        };

        // When
        let json = serde_json::to_value(&step).unwrap();

        // Then — the shape `tddy-tauri-web`'s `NativeDragEvent` declares
        assert_eq!(
            json,
            serde_json::json!({ "type": "drop", "paths": ["/a b"], "point": { "x": 1.5, "y": 2.0 } })
        );
    }
}
