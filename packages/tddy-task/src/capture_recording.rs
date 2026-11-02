//! A terminal capture's retained window, written to disk so its history outlives the process.
//!
//! [`TerminalCapture`] lives as long as its PTY. A session's agent terminal is recorded as a
//! [`CaptureRecording`] — the same bounded window with the same absolute offsets — so a stopped
//! session's history pages exactly like the live terminal's did, including after a daemon restart.
//!
//! The file is a small versioned header (magic, version, start and end offsets) followed by the raw
//! retained bytes. It is replaced atomically, so a reader never sees half a recording.

use std::io;
use std::path::Path;

use crate::terminal_capture::{CaptureChunk, TerminalCapture};

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
        // TODO(agent-history): implement
        let _ = capture;
        CaptureRecording {
            start_offset: 0,
            end_offset: 0,
            data: Vec::new(),
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
        // TODO(agent-history): implement
        let _ = (from_offset, until_offset, max_bytes);
        CaptureChunk {
            data: Vec::new(),
            start_offset: 0,
            end_offset: 0,
            at_oldest: true,
            at_end: true,
        }
    }

    /// Replace `path` with this recording, through a temporary file renamed into place.
    pub fn write_atomically(&self, path: &Path) -> io::Result<()> {
        // TODO(agent-history): implement
        let _ = path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "capture recordings are not written yet",
        ))
    }

    /// The recording at `path`. A wrong magic or an unknown version is `InvalidData`.
    pub fn read(path: &Path) -> io::Result<Self> {
        // TODO(agent-history): implement
        let _ = path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "capture recordings are not read yet",
        ))
    }
}
