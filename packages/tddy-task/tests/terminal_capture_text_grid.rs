//! `TerminalCapture::text_grid` reads the screen model as text: the active buffer's visible rows,
//! the cursor, the grid size, the stream offset and the bracketed-paste mode (`#home-inbox` 2/8).
//!
//! Every test feeds the capture real escape sequences and reads the grid back; no PTY is involved.

use tddy_task::{GridCursor, TerminalCapture, TextGrid};

const ROWS: u16 = 6;
const COLS: u16 = 20;

/// A capture whose model is sized to the PTY's grid, as every spawned terminal's is.
fn a_sized_capture() -> TerminalCapture {
    let mut capture = TerminalCapture::new();
    capture.resize(ROWS, COLS);
    capture
}

trait TerminalOutput: Sized {
    fn after_the_application_writes(self, output: &str) -> Self;
}

impl TerminalOutput for TerminalCapture {
    fn after_the_application_writes(mut self, output: &str) -> Self {
        self.append(output.as_bytes());
        self
    }
}

fn grid_of(capture: &TerminalCapture) -> TextGrid {
    capture
        .text_grid()
        .expect("a sized model reads as a text grid")
}

trait TextGridAssertions {
    /// The rows that hold text, top first.
    fn assert_non_blank_lines(&self, expected: &[&str]) -> &Self;
    /// The row at `row`, whole.
    fn assert_line(&self, row: usize, expected: &str) -> &Self;
    fn assert_cursor(&self, row: u16, col: u16, visible: bool) -> &Self;
    fn assert_size(&self, cols: u16, rows: u16) -> &Self;
    fn assert_bracketed_paste(&self, on: bool) -> &Self;
}

impl TextGridAssertions for TextGrid {
    fn assert_non_blank_lines(&self, expected: &[&str]) -> &Self {
        let shown: Vec<&str> = self
            .lines
            .iter()
            .map(String::as_str)
            .filter(|line| !line.is_empty())
            .collect();
        assert_eq!(shown, expected, "the rows that hold text");
        self
    }

    fn assert_line(&self, row: usize, expected: &str) -> &Self {
        assert_eq!(self.lines[row], expected, "row {row}");
        self
    }

    fn assert_cursor(&self, row: u16, col: u16, visible: bool) -> &Self {
        assert_eq!(self.cursor, GridCursor { row, col, visible }, "the cursor");
        self
    }

    fn assert_size(&self, cols: u16, rows: u16) -> &Self {
        assert_eq!((self.cols, self.rows), (cols, rows), "the grid's size");
        assert_eq!(self.lines.len(), usize::from(rows), "one line per row");
        self
    }

    fn assert_bracketed_paste(&self, on: bool) -> &Self {
        assert_eq!(self.bracketed_paste, on, "bracketed paste");
        self
    }
}

#[test]
fn a_model_that_was_never_sized_has_no_grid_until_it_is() {
    // Given output that arrived before the PTY's size was known
    let mut capture = TerminalCapture::new().after_the_application_writes("hello");
    let before = capture.text_grid();

    // When the model is told the PTY's size
    capture.resize(ROWS, COLS);

    // Then there was no grid before, and the grid is the PTY's size after
    assert_eq!(before, None);
    grid_of(&capture).assert_size(COLS, ROWS);
}

#[test]
fn printable_text_is_the_first_row_and_the_cursor_waits_after_it() {
    // Given
    let capture = a_sized_capture().after_the_application_writes("hello world");

    // When
    let grid = grid_of(&capture);

    // Then
    grid.assert_non_blank_lines(&["hello world"])
        .assert_line(0, "hello world")
        .assert_cursor(0, 11, true)
        .assert_size(COLS, ROWS);
}

#[test]
fn carriage_return_and_line_feed_start_the_next_row_at_column_zero() {
    // Given
    let capture = a_sized_capture().after_the_application_writes("first\r\nsecond\r\nthird");

    // When
    let grid = grid_of(&capture);

    // Then
    grid.assert_non_blank_lines(&["first", "second", "third"])
        .assert_cursor(2, 5, true);
}

#[test]
fn a_bare_carriage_return_rewinds_the_row_so_the_next_text_overwrites_it() {
    // Given a progress line that is redrawn in place
    let capture = a_sized_capture().after_the_application_writes("loading 10%\rloading 90%");

    // When
    let grid = grid_of(&capture);

    // Then
    grid.assert_non_blank_lines(&["loading 90%"]);
}

#[test]
fn cursor_addressing_puts_text_and_the_cursor_where_the_application_says() {
    // Given text written at row 4, column 6 (one-based), and the cursor parked at row 2, column 3
    let capture = a_sized_capture()
        .after_the_application_writes("\x1b[4;6Hhere")
        .after_the_application_writes("\x1b[2;3H");

    // When
    let grid = grid_of(&capture);

    // Then
    grid.assert_line(3, "     here")
        .assert_non_blank_lines(&["     here"])
        .assert_cursor(1, 2, true);
}

#[test]
fn each_row_is_trimmed_at_its_end() {
    // Given a row the application padded with spaces, and one it erased the end of
    let capture = a_sized_capture()
        .after_the_application_writes("padded     \r\n")
        .after_the_application_writes("erased tail\x1b[5D\x1b[K");

    // When
    let grid = grid_of(&capture);

    // Then neither row ends in whitespace, and leading whitespace is kept
    grid.assert_line(0, "padded").assert_line(1, "erased");
}

#[test]
fn a_cursor_the_application_hid_reads_as_not_visible_until_it_shows_it_again() {
    // Given
    let hidden = a_sized_capture().after_the_application_writes("\x1b[?25lmenu");
    let shown_again = a_sized_capture().after_the_application_writes("\x1b[?25lmenu\x1b[?25h");

    // When
    let (hidden, shown_again) = (grid_of(&hidden), grid_of(&shown_again));

    // Then
    hidden.assert_cursor(0, 4, false);
    shown_again.assert_cursor(0, 4, true);
}

#[test]
fn on_the_alternate_screen_the_grid_is_the_alternate_screen_and_not_the_primary_one() {
    // Given an application that wrote on the primary screen, then entered the alternate one
    let capture = a_sized_capture()
        .after_the_application_writes("primary text")
        .after_the_application_writes("\x1b[?1049h\x1b[2J\x1b[Halternate text");

    // When
    let grid = grid_of(&capture);

    // Then
    grid.assert_non_blank_lines(&["alternate text"])
        .assert_cursor(0, 14, true);
}

#[test]
fn leaving_the_alternate_screen_brings_the_primary_screen_back() {
    // Given an application that entered the alternate screen and left it again
    let capture = a_sized_capture()
        .after_the_application_writes("primary text")
        .after_the_application_writes("\x1b[?1049h\x1b[2J\x1b[Halternate text")
        .after_the_application_writes("\x1b[?1049l");

    // When
    let grid = grid_of(&capture);

    // Then
    grid.assert_non_blank_lines(&["primary text"])
        .assert_cursor(0, 12, true);
}

#[test]
fn a_wide_character_is_one_character_in_the_text_and_two_columns_at_the_cursor() {
    // Given two double-width characters followed by ASCII
    let capture = a_sized_capture().after_the_application_writes("日本ab");

    // When
    let grid = grid_of(&capture);

    // Then the text has no gap after a wide character, and the cursor counts its cells
    grid.assert_non_blank_lines(&["日本ab"])
        .assert_cursor(0, 6, true);
}

#[test]
fn a_character_split_across_two_chunks_is_read_whole() {
    // Given the bytes of "日" arriving in two reads
    let bytes = "日".as_bytes();
    let mut capture = a_sized_capture();
    capture.append(&bytes[..2]);
    capture.append(&bytes[2..]);

    // When
    let grid = grid_of(&capture);

    // Then
    grid.assert_non_blank_lines(&["日"])
        .assert_cursor(0, 2, true);
}

#[test]
fn the_grid_carries_the_offset_of_the_output_it_reflects() {
    // Given 11 bytes, then 5 more
    let mut capture = a_sized_capture().after_the_application_writes("hello world");
    let first = grid_of(&capture).offset;
    capture.append(b"\r\nmore");

    // When
    let second = grid_of(&capture).offset;

    // Then each grid names the end of the output it was read after
    assert_eq!((first, second), (11, 17));
}

#[test]
fn a_resize_changes_the_grid_the_rows_are_read_at() {
    // Given a sized capture that is then resized to a taller, narrower grid
    let mut capture = a_sized_capture().after_the_application_writes("hello");
    capture.resize(10, 8);

    // When
    let grid = grid_of(&capture);

    // Then
    grid.assert_size(8, 10);
}

#[test]
fn bracketed_paste_follows_the_application_switching_mode_2004_on_and_off() {
    // Given an application that switches bracketed paste on, then off
    let fresh = a_sized_capture();
    let on = a_sized_capture().after_the_application_writes("\x1b[?2004h");
    let off = a_sized_capture().after_the_application_writes("\x1b[?2004h\x1b[?2004l");

    // When
    let (fresh, on, off) = (grid_of(&fresh), grid_of(&on), grid_of(&off));

    // Then
    fresh.assert_bracketed_paste(false);
    on.assert_bracketed_paste(true);
    off.assert_bracketed_paste(false);
}
