//! Acceptance: a [`TerminalCapture`] snapshot renders identically to the screen it was taken from.
//!
//! The oracle is an independent emulator (`vt100`), never the screen model under test: one reference
//! VT is fed the application's original output, a second is fed only the snapshot, and the two must
//! agree on every cell, the cursor and every mode a client needs to render and drive the screen. A
//! snapshot that merely *contained* the right text would pass a weaker check and still leave a
//! reconnecting terminal drawing scrolls in the wrong region or reporting no mouse events.

use tddy_task::TerminalCapture;

const ROWS: u16 = 24;
const COLS: u16 = 80;

// ---------------------------------------------------------------------------
// A full-screen TUI, as bytes
// ---------------------------------------------------------------------------

/// What a Claude-like full-screen TUI writes once at startup: the alternate screen, a scroll region
/// below a header, a hidden cursor, and the input modes it drives.
fn tui_startup() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"\x1b[?1049h"); // alternate screen
    out.extend_from_slice(b"\x1b[?25l"); // hide cursor
    out.extend_from_slice(b"\x1b[?1h"); // application cursor keys
    out.extend_from_slice(b"\x1b[?2004h"); // bracketed paste
    out.extend_from_slice(b"\x1b[?1002h\x1b[?1006h"); // button-event mouse tracking, SGR encoding
    out.extend_from_slice(b"\x1b[2J\x1b[H");
    out.extend_from_slice(b"\x1b[1;38;2;255;128;0;48;2;10;20;30m Claude \x1b[0m");
    out.extend_from_slice(b"\x1b[2;23r"); // scroll region: rows 2..23
    out
}

/// One incremental repaint: a cursor-addressed write into the body and a status-line update, the
/// diffs a TUI sends between frames. None of them redraws the whole screen.
fn tui_diff(frame: usize) -> Vec<u8> {
    let row = 2 + frame % 21;
    let mut out = Vec::new();
    out.extend_from_slice(
        format!("\x1b[{row};3H\x1b[32m✓\x1b[0m line {frame:06} — 日本").as_bytes(),
    );
    out.extend_from_slice(format!("\x1b[24;1H\x1b[7m tokens {frame:08} \x1b[0m").as_bytes());
    out
}

/// A session long enough that the capture ring has evicted its startup bytes many times over.
fn a_long_tui_session() -> Vec<u8> {
    let mut out = tui_startup();
    let mut frame = 0;
    while out.len() < 3 * TerminalCapture::CAPTURE_LIMIT_BYTES {
        out.extend_from_slice(&tui_diff(frame));
        frame += 1;
    }
    // Park a saved cursor inside the region, as a TUI does before drawing a popup.
    out.extend_from_slice(b"\x1b[12;40H\x1b7\x1b[1;1H");
    out
}

// ---------------------------------------------------------------------------
// The oracle
// ---------------------------------------------------------------------------

/// Everything a client renders and drives a screen with, read off a reference VT.
#[derive(Debug, PartialEq)]
struct RenderedScreen {
    cells: Vec<String>,
    cursor: (u16, u16),
    cursor_hidden: bool,
    alternate_screen: bool,
    application_cursor: bool,
    bracketed_paste: bool,
    mouse: String,
}

fn rendered(parser: &vt100::Parser) -> RenderedScreen {
    let screen = parser.screen();
    let (rows, cols) = screen.size();
    let mut cells = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            let cell = screen.cell(row, col).expect("a cell inside the grid");
            cells.push(format!(
                "{row},{col}:{:?} fg={:?} bg={:?} b={} i={} u={} r={}",
                cell.contents(),
                cell.fgcolor(),
                cell.bgcolor(),
                cell.bold(),
                cell.italic(),
                cell.underline(),
                cell.inverse(),
            ));
        }
    }
    RenderedScreen {
        cells,
        cursor: screen.cursor_position(),
        cursor_hidden: screen.hide_cursor(),
        alternate_screen: screen.alternate_screen(),
        application_cursor: screen.application_cursor(),
        bracketed_paste: screen.bracketed_paste(),
        mouse: format!(
            "{:?}/{:?}",
            screen.mouse_protocol_mode(),
            screen.mouse_protocol_encoding()
        ),
    }
}

/// A reference terminal that saw the application's own output, byte for byte.
fn a_terminal_that_watched(output: &[u8], rows: u16, cols: u16) -> vt100::Parser {
    let mut parser = vt100::Parser::new(rows, cols, 0);
    parser.process(output);
    parser
}

/// A reference terminal that saw nothing but the snapshot — what a reconnecting client has.
fn a_terminal_shown_the_snapshot_of(capture: &TerminalCapture) -> vt100::Parser {
    let snapshot = capture
        .snapshot()
        .expect("a capture that knows its size has a snapshot");
    let mut parser = vt100::Parser::new(snapshot.rows, snapshot.cols, 0);
    parser.process(&snapshot.data);
    parser
}

fn a_capture_of(output: &[u8]) -> TerminalCapture {
    let mut capture = TerminalCapture::new();
    capture.resize(ROWS, COLS);
    capture.append(output);
    capture
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn a_snapshot_renders_identically_to_the_screen_it_was_taken_from() {
    // Given a full-screen TUI that has been running far longer than the capture ring holds
    let output = a_long_tui_session();
    let capture = a_capture_of(&output);

    // When a client that saw nothing is shown only the snapshot
    let reconnected = a_terminal_shown_the_snapshot_of(&capture);

    // Then it renders exactly what a client that watched every byte renders
    let watched = a_terminal_that_watched(&output, ROWS, COLS);
    assert_eq!(rendered(&reconnected), rendered(&watched));
}

#[test]
fn the_snapshot_is_taken_at_the_capture_tip() {
    // Given a capture of a running TUI
    let output = a_long_tui_session();
    let capture = a_capture_of(&output);

    // When it is snapshotted
    let snapshot = capture.snapshot().expect("a snapshot");

    // Then its sequence point is the total output so far, at the PTY's grid
    assert_eq!(
        (snapshot.offset, snapshot.rows, snapshot.cols),
        (output.len() as u64, ROWS, COLS)
    );
}

#[test]
fn the_snapshot_keeps_the_scroll_region_and_the_saved_cursor() {
    // Given a client shown the snapshot of a TUI with a scroll region and a saved cursor
    let output = a_long_tui_session();
    let capture = a_capture_of(&output);
    let mut reconnected = a_terminal_shown_the_snapshot_of(&capture);
    let mut watched = a_terminal_that_watched(&output, ROWS, COLS);

    // When the TUI goes on to scroll inside its region and restore the saved cursor
    let more = b"\x1b[23;1H\n\n\nscrolled\x1b8restored";
    reconnected.process(more);
    watched.process(more);

    // Then both clients still agree — the region and the saved cursor travelled in the snapshot
    assert_eq!(rendered(&reconnected), rendered(&watched));
}

#[test]
fn a_chunk_boundary_inside_an_escape_or_a_code_point_does_not_corrupt_the_snapshot() {
    // Given output whose tail holds a truecolor CSI and a four-byte code point
    let mut output = tui_startup();
    let tail_start = output.len();
    output.extend_from_slice("\x1b[5;5H\x1b[38;2;1;2;3mok 🦀 done".as_bytes());
    let watched = a_terminal_that_watched(&output, ROWS, COLS);

    for split in tail_start..output.len() {
        // When the PTY delivers it in two reads split at that byte
        let mut capture = TerminalCapture::new();
        capture.resize(ROWS, COLS);
        capture.append(&output[..split]);
        capture.append(&output[split..]);

        // Then the snapshot renders as if it had arrived whole
        assert_eq!(
            rendered(&a_terminal_shown_the_snapshot_of(&capture)),
            rendered(&watched),
            "split at byte {split}"
        );
    }
}

#[test]
fn a_resize_reserializes_the_snapshot_at_the_new_grid() {
    // Given a TUI that was resized and repainted at the new size
    let mut capture = TerminalCapture::new();
    capture.resize(ROWS, COLS);
    capture.append(&tui_startup());
    capture.resize(40, 120);
    let repaint = b"\x1b[2J\x1b[H\x1b[1mresized\x1b[0m\x1b[40;120Hx";
    capture.append(repaint);

    // When it is snapshotted
    let snapshot = capture.snapshot().expect("a snapshot");

    // Then the snapshot is at the new grid and renders the repaint
    let mut watched = vt100::Parser::new(40, 120, 0);
    watched.process(&tui_startup());
    watched.process(repaint);
    assert_eq!((snapshot.rows, snapshot.cols), (40, 120));
    assert_eq!(
        rendered(&a_terminal_shown_the_snapshot_of(&capture)).cells,
        rendered(&watched).cells
    );
}

#[test]
fn a_capture_that_was_never_sized_has_no_snapshot() {
    // Given a capture whose PTY size nobody reported
    let mut capture = TerminalCapture::new();
    capture.append(b"hello");

    // When it is asked for a snapshot / Then there is none: it would be serialized at a guess
    assert_eq!(capture.snapshot(), None);
}
