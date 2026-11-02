//! A terminal capture's retained window, written to disk so its history outlives the process.
//!
//! [`TerminalCapture`] lives as long as its PTY. A session's agent terminal is recorded as a
//! [`CaptureRecording`] — the same bounded window with the same absolute offsets — so a stopped
//! session's history pages exactly like the live terminal's did, including after a daemon restart.
//!
//! The file is a small versioned header (magic, version, start and end offsets) followed by the raw
//! retained bytes. It is replaced atomically, so a reader never sees half a recording.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use crate::terminal_capture::{forward_chunk, CaptureChunk, TerminalCapture};

/// The first bytes of every recording file.
const MAGIC: [u8; 8] = *b"TDDYCAP\0";

/// The layout this build writes and reads. A file written at another version is refused.
const VERSION: u32 = 1;

/// How often a running terminal's capture is rewritten to its recording — what a daemon crash can
/// lose at most. One value for every recorder, jailed or not.
pub const RECORDING_INTERVAL: Duration = Duration::from_secs(30);

/// The retained window of a terminal's output, with its absolute offsets in the cumulative stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureRecording {
    /// Absolute offset of `data[0]`.
    pub start_offset: u64,
    /// Absolute offset just past the last byte — the total bytes the terminal ever printed.
    pub end_offset: u64,
    /// The retained bytes.
    pub data: Vec<u8>,
}

impl CaptureRecording {
    /// The window `capture` currently retains.
    #[must_use]
    pub fn of(capture: &TerminalCapture) -> Self {
        Self {
            start_offset: capture.start_offset(),
            end_offset: capture.end_offset(),
            data: capture.buffered_bytes().to_vec(),
        }
    }

    /// Up to `max_bytes` from `from_offset` forward, bounded by `until_offset` (0 = the end) — the
    /// same chunks [`TerminalCapture::replay_from`] answers for the same window.
    #[must_use]
    pub fn replay_from(
        &self,
        from_offset: u64,
        until_offset: u64,
        max_bytes: usize,
    ) -> CaptureChunk {
        forward_chunk(
            &self.data,
            self.start_offset,
            self.end_offset,
            from_offset,
            until_offset,
            max_bytes,
        )
    }

    /// Replace `path` with this recording, through a temporary file renamed into place.
    ///
    /// The temporary file sits beside `path`, so the rename never crosses a filesystem. A write that
    /// fails leaves `path` as it was and no temporary file behind.
    pub fn write_atomically(&self, path: &Path) -> io::Result<()> {
        let temporary = temporary_beside(path);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let written = file
            .write_all(&self.encode())
            .and_then(|()| file.sync_all());
        drop(file);
        let replaced = written.and_then(|()| fs::rename(&temporary, path));
        if let Err(error) = replaced {
            discard(&temporary);
            return Err(error);
        }
        Ok(())
    }

    /// Snapshot `capture` and replace the recording at `path` with it, on a blocking thread,
    /// creating the recording's directory first — what a recorder does on every write.
    ///
    /// A poisoned capture is still recorded: its bytes and offsets stay consistent, because every
    /// append updates them together.
    pub async fn record(capture: &Mutex<TerminalCapture>, path: &Path) -> io::Result<()> {
        let snapshot = Self::of(&capture.lock().unwrap_or_else(PoisonError::into_inner));
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            // A session's `terminals/` directory exists only once something is recorded into it.
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            snapshot.write_atomically(&path)
        })
        .await
        .map_err(io::Error::other)?
    }

    /// The recording at `path`. A wrong magic, an unknown version or offsets that do not match the
    /// data are `InvalidData`.
    pub fn read(path: &Path) -> io::Result<Self> {
        Self::decode(&fs::read(path)?)
    }

    /// The header followed by the retained bytes.
    fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEADER_LEN + self.data.len());
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.start_offset.to_le_bytes());
        bytes.extend_from_slice(&self.end_offset.to_le_bytes());
        bytes.extend_from_slice(&self.data);
        bytes
    }

    fn decode(bytes: &[u8]) -> io::Result<Self> {
        let mut cursor = bytes;
        let magic: [u8; 8] = take(&mut cursor)?;
        if magic != MAGIC {
            return Err(invalid("the file is not a capture recording"));
        }
        let version = u32::from_le_bytes(take(&mut cursor)?);
        if version != VERSION {
            return Err(invalid(&format!(
                "capture recording version {version} is not version {VERSION}"
            )));
        }
        let start_offset = u64::from_le_bytes(take(&mut cursor)?);
        let end_offset = u64::from_le_bytes(take(&mut cursor)?);
        if end_offset < start_offset || end_offset - start_offset != cursor.len() as u64 {
            return Err(invalid(
                "the capture recording's offsets do not match its data",
            ));
        }
        Ok(Self {
            start_offset,
            end_offset,
            data: cursor.to_vec(),
        })
    }
}

/// Magic, version, start offset and end offset: the bytes before the retained data.
const HEADER_LEN: usize = MAGIC.len() + 4 + 8 + 8;

/// The next `N` bytes of `cursor`, which then starts after them.
fn take<const N: usize>(cursor: &mut &[u8]) -> io::Result<[u8; N]> {
    let (head, rest) = cursor
        .split_first_chunk::<N>()
        .ok_or_else(|| invalid("the file ends inside the capture recording header"))?;
    *cursor = rest;
    Ok(*head)
}

fn invalid(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason.to_owned())
}

/// A path beside `path` that no other write in this process uses: the process id and a counter.
fn temporary_beside(path: &Path) -> PathBuf {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!(".{name}.{}.{sequence}.tmp", std::process::id()))
}

/// Remove a temporary file a failed write left behind. The write's own error is what the caller
/// sees, so a removal that also fails is logged rather than returned.
fn discard(temporary: &Path) {
    if let Err(error) = fs::remove_file(temporary) {
        log::warn!(
            "could not remove the temporary capture recording {}: {error}",
            temporary.display()
        );
    }
}
