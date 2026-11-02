//! `GetTerminalHistory` for a terminal that is no longer running answers from the host's recording
//! of it — and a running terminal never reads that recording.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-02-agent-terminal-history-persists.md (`#session-tabs 2/7`)

mod support;

use support::{a_history_request, TerminalServiceHost};
use tddy_task::CaptureRecording;
use tddy_terminal_rpc::TerminalHistoryChunk;

/// What a stopped agent terminal left behind: ten bytes, the first 100 already evicted.
fn a_recording_of_a_stopped_agent() -> CaptureRecording {
    CaptureRecording {
        start_offset: 100,
        end_offset: 110,
        data: b"recorded!!".to_vec(),
    }
}

#[tokio::test]
async fn a_stopped_main_terminal_answers_history_from_its_recording() {
    // Given a session whose agent terminal stopped, leaving a recording
    let host =
        TerminalServiceHost::serving_a_recorded_main_terminal(a_recording_of_a_stopped_agent());

    // When the client fills history forward from the oldest retained byte
    let chunks: Vec<TerminalHistoryChunk> = host
        .all_frames("GetTerminalHistory", a_history_request())
        .await;

    // Then it is handed the recording's first bounded chunk, at the recording's offsets
    assert_eq!(
        chunks,
        vec![TerminalHistoryChunk {
            data: b"reco".to_vec(),
            start_offset: 100,
            end_offset: 104,
            at_oldest: true,
            at_end: false,
        }]
    );
}

#[tokio::test]
async fn a_running_main_terminal_never_reads_its_recording() {
    // Given a running agent terminal and an older recording of it
    let host = TerminalServiceHost::serving_main_terminal_with_a_stale_recording(
        b"0123456789",
        a_recording_of_a_stopped_agent(),
    );

    // When the client fills history
    let chunks: Vec<TerminalHistoryChunk> = host
        .all_frames("GetTerminalHistory", a_history_request())
        .await;

    // Then the live ring answers
    assert_eq!(
        chunks,
        vec![TerminalHistoryChunk {
            data: b"0123".to_vec(),
            start_offset: 0,
            end_offset: 4,
            at_oldest: true,
            at_end: false,
        }]
    );
}
