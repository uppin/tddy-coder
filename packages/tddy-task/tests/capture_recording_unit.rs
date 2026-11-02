//! `CaptureRecording`: a terminal capture's retained window on disk, paging exactly like the live
//! capture it was taken from.
//!
//! Feature: docs/ft/daemon/terminal-sessions.md § History of a stopped agent terminal

use std::io::ErrorKind;

use tddy_task::{CaptureChunk, CaptureRecording, TerminalCapture};

/// A capture that has printed more than it retains, so its window starts past offset zero.
fn a_capture_that_evicted_its_oldest_output() -> TerminalCapture {
    let mut capture = TerminalCapture::new();
    for line in 0..20_000 {
        capture.append(format!("line {line:05}\r\n").as_bytes());
    }
    capture
}

fn chunks_equal(recorded: &CaptureChunk, live: &CaptureChunk) {
    assert_eq!(recorded.data, live.data, "chunk bytes");
    assert_eq!(recorded.start_offset, live.start_offset, "start_offset");
    assert_eq!(recorded.end_offset, live.end_offset, "end_offset");
    assert_eq!(recorded.at_oldest, live.at_oldest, "at_oldest");
    assert_eq!(recorded.at_end, live.at_end, "at_end");
}

#[test]
fn a_recording_holds_the_window_and_offsets_its_capture_retains() {
    // Given
    let capture = a_capture_that_evicted_its_oldest_output();

    // When
    let recording = CaptureRecording::of(&capture);

    // Then
    assert!(
        capture.start_offset() > 0,
        "the fixture must have evicted output"
    );
    assert_eq!(recording.start_offset, capture.start_offset());
    assert_eq!(recording.end_offset, capture.end_offset());
    assert_eq!(recording.data, capture.buffered_bytes());
}

#[test]
fn a_recording_written_and_read_back_is_the_same_recording() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("terminals").join("main.capture");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let recording = CaptureRecording::of(&a_capture_that_evicted_its_oldest_output());

    // When
    recording
        .write_atomically(&path)
        .expect("the recording is written");
    let read_back = CaptureRecording::read(&path).expect("the recording is read");

    // Then
    assert_eq!(read_back, recording);
}

#[test]
fn paging_a_recording_answers_the_chunks_its_live_capture_answers() {
    // Given
    let capture = a_capture_that_evicted_its_oldest_output();
    let recording = CaptureRecording::of(&capture);
    let anchor = capture.end_offset() - 1_000;

    // When / Then — a forward fill from below the window, a bounded middle chunk, and the tip
    for (from, until, max) in [
        (0, anchor, 4_096),
        (capture.start_offset() + 10_000, anchor, 2_048),
        (anchor - 10, 0, 64 * 1024),
        (capture.end_offset(), 0, 1_024),
    ] {
        chunks_equal(
            &recording.replay_from(from, until, max),
            &capture.replay_from(from, until, max),
        );
    }
}

#[test]
fn a_file_that_is_not_a_recording_is_refused_as_invalid_data() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.capture");
    std::fs::write(&path, b"definitely not a capture recording").unwrap();

    // When
    let refusal = CaptureRecording::read(&path).unwrap_err();

    // Then
    assert_eq!(refusal.kind(), ErrorKind::InvalidData);
}

#[test]
fn a_failed_write_leaves_no_file_behind() {
    // Given a target whose directory does not exist
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing-dir").join("main.capture");
    let recording = CaptureRecording::of(&a_capture_that_evicted_its_oldest_output());

    // When
    let result = recording.write_atomically(&path);

    // Then the write fails and the target does not exist
    assert_eq!(result.unwrap_err().kind(), ErrorKind::NotFound);
    assert!(!path.exists());
}

#[test]
fn a_write_that_fails_at_the_rename_leaves_no_temporary_file_behind() {
    // Given a target path already taken by a non-empty directory, so only the rename can fail
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.capture");
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("occupant"), b"x").unwrap();
    let recording = CaptureRecording::of(&a_capture_that_evicted_its_oldest_output());

    // When
    let result = recording.write_atomically(&path);

    // Then the write fails and the directory holds only the target, no temporary beside it
    assert!(
        result.is_err(),
        "renaming over a non-empty directory must fail"
    );
    let left: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(left, vec![std::ffi::OsString::from("main.capture")]);
}

#[tokio::test]
async fn recording_a_capture_creates_its_directory_and_writes_its_window() {
    // Given a capture and a recording path whose directory does not exist yet
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("terminals").join("main.capture");
    let capture = std::sync::Mutex::new(a_capture_that_evicted_its_oldest_output());

    // When it is recorded
    CaptureRecording::record(&capture, &path)
        .await
        .expect("the capture is recorded");

    // Then the file holds the window the capture retains
    let expected = CaptureRecording::of(&capture.lock().unwrap());
    assert_eq!(CaptureRecording::read(&path).unwrap(), expected);
}
