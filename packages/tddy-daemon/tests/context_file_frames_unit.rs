//! Unit tests: framing a context file for the wire.
//!
//! LiveKit does **not** chunk. Its SCTP data channel negotiates a maximum message size and an
//! oversized `publish_data` is rejected outright — "the transport wedges, retrying the same doomed
//! publish forever" (`packages/tddy-livekit/src/chunking.rs`). The repo's own codec catches that at
//! `MAX_CHUNK_FRAME_BYTES`, but a stream whose frames stay under the budget never engages it at
//! all, which is one fewer layer between an agent and its `CLAUDE.md`.
//!
//! So the frame size is a wire contract, not a tuning knob, and it is pinned here against the
//! budget it has to stay beneath rather than against a bare number.
//!
//! PRD: docs/ft/daemon/agent-context-sync.md § Design.

use pretty_assertions::assert_eq;
use tddy_daemon::context_files::{
    context_file_batch_frames, context_file_frames, CONTEXT_FILE_FRAME_BYTES,
};
use tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn reassembled(frames: &[tddy_service::proto::connection::ContextFileChunk]) -> Vec<u8> {
    frames.iter().flat_map(|f| f.data.clone()).collect()
}

// ---------------------------------------------------------------------------
// The budget
// ---------------------------------------------------------------------------

/// The whole point of the chosen frame size: a context file never reaches the chunking codec, so a
/// `.claude/` tree cannot wedge the transport however large one of its files grows.
// Both operands are constants on purpose — the relation between them is the invariant under test.
// Same allowance, for the same reason, as `remote_git_relay_acceptance`'s frame-budget test.
#[allow(clippy::assertions_on_constants)]
#[test]
fn the_frame_size_stays_under_the_transports_chunking_budget() {
    // Then
    assert!(
        CONTEXT_FILE_FRAME_BYTES < MAX_CHUNK_FRAME_BYTES,
        "a {CONTEXT_FILE_FRAME_BYTES}-byte frame must stay under the {MAX_CHUNK_FRAME_BYTES}-byte \
         budget, or every context read pays for reassembly"
    );
}

/// No frame ever exceeds the declared size, whatever the file length happens to be relative to it.
#[rstest::rstest]
#[case(0)]
#[case(1)]
#[case(4096)]
#[case(CONTEXT_FILE_FRAME_BYTES - 1)]
#[case(CONTEXT_FILE_FRAME_BYTES)]
#[case(CONTEXT_FILE_FRAME_BYTES + 1)]
#[case(CONTEXT_FILE_FRAME_BYTES * 3)]
#[case(CONTEXT_FILE_FRAME_BYTES * 3 + 17)]
fn no_frame_exceeds_the_declared_frame_size(#[case] len: usize) {
    // Given
    let bytes = vec![b'x'; len];

    // When
    let frames = context_file_frames(&bytes);

    // Then
    for (index, frame) in frames.iter().enumerate() {
        assert!(
            frame.data.len() <= CONTEXT_FILE_FRAME_BYTES,
            "frame {index} of a {len}-byte file is {} bytes",
            frame.data.len()
        );
    }
}

// ---------------------------------------------------------------------------
// Round-tripping
// ---------------------------------------------------------------------------

/// Concatenating the frames reproduces the file exactly. A `.claude/` tree may hold a PNG or a file
/// in an encoding that is not UTF-8, and a mangled config is worse than a missing one.
#[rstest::rstest]
#[case(0)]
#[case(1)]
#[case(CONTEXT_FILE_FRAME_BYTES - 1)]
#[case(CONTEXT_FILE_FRAME_BYTES)]
#[case(CONTEXT_FILE_FRAME_BYTES + 1)]
#[case(CONTEXT_FILE_FRAME_BYTES * 2 + 9)]
fn the_frames_reassemble_into_the_original_bytes(#[case] len: usize) {
    // Given
    let bytes: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();

    // When
    let frames = context_file_frames(&bytes);

    // Then
    assert_eq!(reassembled(&frames), bytes);
}

/// A zero-byte file yields exactly one empty frame, which is how "the file is empty" stays
/// distinguishable from "the stream failed and sent nothing" — the same rule
/// `StreamReadWorktreeFile` already follows.
#[test]
fn a_zero_byte_file_yields_exactly_one_empty_frame() {
    // When
    let frames = context_file_frames(b"");

    // Then
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].data, Vec::<u8>::new());
}

/// A file that fits exactly is not split into a full frame plus an empty one.
#[test]
fn a_file_exactly_one_frame_long_is_not_split_in_two() {
    // Given
    let bytes = vec![b'x'; CONTEXT_FILE_FRAME_BYTES];

    // When
    let frames = context_file_frames(&bytes);

    // Then
    assert_eq!(frames.len(), 1);
}

/// One byte over the boundary is two frames, the second holding the single byte.
#[test]
fn a_file_one_byte_over_a_frame_yields_two_frames() {
    // Given
    let bytes = vec![b'x'; CONTEXT_FILE_FRAME_BYTES + 1];

    // When
    let frames = context_file_frames(&bytes);

    // Then
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[1].data.len(), 1);
}

// ---------------------------------------------------------------------------
// The size a reader needs up front
// ---------------------------------------------------------------------------

/// Every frame repeats the file's full size, so a reader knows the total from the first one and can
/// tell a completed stream from a truncated one without counting frames.
#[test]
fn every_frame_carries_the_files_full_size() {
    // Given
    let len = CONTEXT_FILE_FRAME_BYTES * 2 + 5;
    let bytes = vec![b'x'; len];

    // When
    let frames = context_file_frames(&bytes);

    // Then
    assert_eq!(frames.len(), 3);
    for (index, frame) in frames.iter().enumerate() {
        assert_eq!(
            frame.total_byte_size, len as u64,
            "frame {index} must report the whole file's size"
        );
    }
}

#[test]
fn an_empty_files_single_frame_reports_a_size_of_zero() {
    // When
    let frames = context_file_frames(b"");

    // Then
    assert_eq!(frames[0].total_byte_size, 0);
}

// ---------------------------------------------------------------------------
// Framing a whole batch
// ---------------------------------------------------------------------------

/// A batch carries several files down one stream, so every frame has to say which file it belongs
/// to — reassembling by arrival order alone would make a single reordered or dropped frame silently
/// graft one file's bytes onto another's.
#[test]
fn every_frame_of_a_batch_names_the_file_it_carries() {
    // Given
    let files = vec![
        (
            "CLAUDE.md".to_string(),
            vec![b'a'; CONTEXT_FILE_FRAME_BYTES + 1],
        ),
        (".mcp.json".to_string(), b"{}".to_vec()),
    ];

    // When
    let frames = context_file_batch_frames(&files);

    // Then
    let named: Vec<&str> = frames.iter().map(|f| f.rel_path.as_str()).collect();
    assert_eq!(named, vec!["CLAUDE.md", "CLAUDE.md", ".mcp.json"]);
}

/// Each file's frames reassemble into that file's bytes, and nothing bleeds between them.
#[test]
fn a_batchs_frames_reassemble_into_each_files_own_bytes() {
    // Given
    let first: Vec<u8> = (0..CONTEXT_FILE_FRAME_BYTES * 2 + 7)
        .map(|i| (i % 251) as u8)
        .collect();
    let second: Vec<u8> = b"# rules\n".to_vec();
    let files = vec![
        ("CLAUDE.md".to_string(), first.clone()),
        (".claude/settings.json".to_string(), second.clone()),
    ];

    // When
    let frames = context_file_batch_frames(&files);

    // Then
    let rebuilt = |path: &str| -> Vec<u8> {
        frames
            .iter()
            .filter(|f| f.rel_path == path)
            .flat_map(|f| f.data.clone())
            .collect()
    };
    assert_eq!(rebuilt("CLAUDE.md"), first);
    assert_eq!(rebuilt(".claude/settings.json"), second);
}

/// A zero-byte file in a batch still yields exactly one frame, and that frame says the file ended.
/// Without both halves an empty file and a file whose bytes never arrived are the same stream, and
/// the setup sync would start an agent against guidance it never received.
#[test]
fn an_empty_file_in_a_batch_yields_one_frame_that_declares_the_end() {
    // Given
    let files = vec![
        ("CLAUDE.md".to_string(), b"# rules\n".to_vec()),
        (".mcp.json".to_string(), Vec::new()),
    ];

    // When
    let frames = context_file_batch_frames(&files);

    // Then
    let empty: Vec<_> = frames
        .iter()
        .filter(|f| f.rel_path == ".mcp.json")
        .collect();
    assert_eq!(empty.len(), 1, "an empty file must still be framed once");
    assert_eq!(empty[0].data, Vec::<u8>::new());
    assert_eq!(empty[0].total_byte_size, 0);
    assert!(
        empty[0].end_of_file,
        "the only frame of an empty file must declare the file finished"
    );
}

/// `end_of_file` marks the last frame of each file and no other, so a client knows a file is whole
/// without inferring it from a byte count a truncated stream would also be climbing towards.
#[test]
fn end_of_file_marks_the_last_frame_of_each_file_and_no_other() {
    // Given
    let files = vec![
        (
            "CLAUDE.md".to_string(),
            vec![b'a'; CONTEXT_FILE_FRAME_BYTES * 2],
        ),
        ("AGENTS.md".to_string(), vec![b'b'; 3]),
    ];

    // When
    let frames = context_file_batch_frames(&files);

    // Then
    let flags: Vec<bool> = frames.iter().map(|f| f.end_of_file).collect();
    assert_eq!(flags, vec![false, true, true]);
}

/// Every frame repeats *its own file's* size, not the batch's, so a client sizes each buffer from
/// the first frame it sees for that path.
#[test]
fn each_frame_reports_the_size_of_the_file_it_belongs_to() {
    // Given
    let files = vec![
        (
            "CLAUDE.md".to_string(),
            vec![b'a'; CONTEXT_FILE_FRAME_BYTES + 5],
        ),
        ("AGENTS.md".to_string(), vec![b'b'; 3]),
    ];

    // When
    let frames = context_file_batch_frames(&files);

    // Then
    let sizes: Vec<(String, u64)> = frames
        .iter()
        .map(|f| (f.rel_path.clone(), f.total_byte_size))
        .collect();
    assert_eq!(
        sizes,
        vec![
            (
                "CLAUDE.md".to_string(),
                (CONTEXT_FILE_FRAME_BYTES + 5) as u64
            ),
            (
                "CLAUDE.md".to_string(),
                (CONTEXT_FILE_FRAME_BYTES + 5) as u64
            ),
            ("AGENTS.md".to_string(), 3),
        ]
    );
}

/// No frame of a batch exceeds the wire budget either — the batch shares the single read's framing
/// precisely so a `.claude/` tree cannot wedge the transport whichever RPC carries it.
#[test]
fn no_frame_of_a_batch_exceeds_the_declared_frame_size() {
    // Given
    let files = vec![
        (
            "CLAUDE.md".to_string(),
            vec![b'a'; CONTEXT_FILE_FRAME_BYTES * 3 + 11],
        ),
        (
            "AGENTS.md".to_string(),
            vec![b'b'; CONTEXT_FILE_FRAME_BYTES],
        ),
    ];

    // When
    let frames = context_file_batch_frames(&files);

    // Then
    for (index, frame) in frames.iter().enumerate() {
        assert!(
            frame.data.len() <= CONTEXT_FILE_FRAME_BYTES,
            "frame {index} of {} is {} bytes",
            frame.rel_path,
            frame.data.len()
        );
    }
}
