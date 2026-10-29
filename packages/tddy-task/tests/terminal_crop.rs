//! Cropping a text grid to a box of screen cells (`#home-inbox` 9/9): the cut on cell boundaries,
//! the clipped and re-based style runs, the box-relative cursor, and the clip versus refusal of a
//! box that does not fit.
//!
//! Every test feeds a capture real escape sequences and crops the grid it reads back. No PTY is
//! involved.
//!
//! Feature doc: docs/ft/daemon/1-WIP/PRD-2026-10-29-terminal-styles.md § Crop box.

use tddy_task::{
    crop, CroppedGrid, GridRegion, StyleRun, TerminalCapture, TerminalColour, TextGrid,
};

const ROWS: u16 = 6;
const COLS: u16 = 20;

const RESET: &str = "\x1b[0m";

fn a_screen_showing(output: &str) -> TextGrid {
    let mut capture = TerminalCapture::new();
    capture.resize(ROWS, COLS);
    capture.append(output.as_bytes());
    capture
        .text_grid()
        .expect("a sized model reads as a text grid")
}

fn the_box(row: u16, col: u16, rows: u16, cols: u16) -> GridRegion {
    GridRegion {
        row,
        col,
        rows,
        cols,
    }
}

fn cropped(grid: &TextGrid, region: GridRegion) -> CroppedGrid {
    crop(grid, region).unwrap_or_else(|why| panic!("{region:?} crops: {why}"))
}

fn run(from: u16, to: u16) -> StyleRun {
    StyleRun {
        from,
        to,
        fg: None,
        bg: None,
        bold: false,
        dim: false,
        italic: false,
        underline: false,
        inverse: false,
        strikethrough: false,
        blink: false,
    }
}

fn red(from: u16, to: u16) -> StyleRun {
    StyleRun {
        fg: Some(TerminalColour::Indexed(1)),
        ..run(from, to)
    }
}

// ── the cut ──────────────────────────────────────────────────────────────────────────────────

#[test]
fn the_box_cuts_each_row_on_cell_boundaries_and_keeps_one_line_per_box_row() {
    // Given three rows of letters
    let grid = a_screen_showing("abcdefghij\r\nklmnopqrst\r\nuvwxyz");

    // When a box of two rows and five columns is cropped from row 1, column 2
    let cropped = cropped(&grid, the_box(1, 2, 2, 5));

    // Then those cells are the lines, and the region is echoed
    assert_eq!(cropped.lines, ["mnopq", "wxyz"]);
    assert_eq!(cropped.region, the_box(1, 2, 2, 5));
}

#[test]
fn the_cropped_lines_are_trimmed_at_the_end_and_blank_rows_stay() {
    let grid = a_screen_showing("ab\r\n\r\ncd");

    let cropped = cropped(&grid, the_box(0, 0, 3, 10));

    assert_eq!(cropped.lines, ["ab", "", "cd"]);
}

#[test]
fn a_wide_character_cut_by_the_left_edge_becomes_a_space_and_the_columns_after_it_stay_aligned() {
    // Given "a", a wide character over cells 1 and 2, then "b" at cell 3
    let grid = a_screen_showing("a漢b");

    // When the box starts at cell 2, the wide character's second cell
    let cropped = cropped(&grid, the_box(0, 2, 1, 4));

    // Then the cut character is a space, and "b" follows it
    assert_eq!(cropped.lines, [" b"]);
}

#[test]
fn a_wide_character_cut_by_the_right_edge_becomes_a_space() {
    // Given a blue bar over "ab", a wide character (cells 2 and 3) and "c"
    let grid = a_screen_showing(&format!("\x1b[44mab漢c{RESET}"));

    // When the box ends after cell 2, the wide character's first cell
    let cropped = cropped(&grid, the_box(0, 0, 1, 3));

    // Then the wide character is a space, still under the bar: the run covers all three cells
    assert_eq!(cropped.lines, ["ab"]);
    assert_eq!(cropped.styles.len(), 1);
    assert_eq!(
        cropped.styles[0].runs,
        [StyleRun {
            bg: Some(TerminalColour::Indexed(4)),
            ..run(0, 3)
        }]
    );
}

#[test]
fn a_wide_character_inside_the_box_keeps_its_place_and_the_runs_after_it_stay_aligned() {
    let grid = a_screen_showing(&format!("x漢字\x1b[31mY{RESET}"));

    let cropped = cropped(&grid, the_box(0, 1, 1, 8));

    assert_eq!(cropped.lines, ["漢字Y"]);
    assert_eq!(cropped.styles[0].runs, [red(2, 3)]);
}

// ── styles ───────────────────────────────────────────────────────────────────────────────────

#[test]
fn runs_are_clipped_at_both_edges_and_rebased_to_the_box() {
    // Given a red run over cells 0 to 3 and a bold run over cells 4 to 7
    let grid = a_screen_showing(&format!("\x1b[31mabcd{RESET}\x1b[1mefgh{RESET}"));

    // When the box covers cells 2 to 5
    let cropped = cropped(&grid, the_box(0, 2, 1, 4));

    // Then each run is cut at the edge it crosses, and columns count from the box's left
    assert_eq!(cropped.lines, ["cdef"]);
    assert_eq!(
        cropped.styles[0].runs,
        [
            red(0, 2),
            StyleRun {
                bold: true,
                ..run(2, 4)
            }
        ]
    );
}

#[test]
fn a_run_outside_the_box_is_dropped_and_its_row_has_no_entry() {
    let grid = a_screen_showing(&format!("ab\x1b[31mcd{RESET}ef\r\nghijkl"));

    let cropped = cropped(&grid, the_box(0, 6, 2, 3));

    assert!(cropped.styles.is_empty());
}

#[test]
fn styled_rows_are_numbered_from_the_boxs_top() {
    // Given a styled word on grid row 3
    let grid = a_screen_showing(&format!("\r\n\r\n\r\nab\x1b[31mcd{RESET}"));

    // When the box starts at row 2
    let cropped = cropped(&grid, the_box(2, 0, 3, 10));

    // Then it is row 1 of the box
    assert_eq!(cropped.styles.len(), 1);
    assert_eq!(cropped.styles[0].row, 1);
    assert_eq!(cropped.styles[0].runs, [red(2, 4)]);
}

#[test]
fn highlighted_rows_are_recomputed_on_the_cropped_rows() {
    // Given a row holding plain text and then an inverse item
    let grid = a_screen_showing(&format!("files     \x1b[7m> develop{RESET}"));

    // When the whole row is read, and when only the inverse item's box is
    let whole = cropped(&grid, the_box(0, 0, 1, COLS));
    let item = cropped(&grid, the_box(0, 10, 1, 9));

    // Then the bar covers the row's text only in the box, in the box's row numbers
    assert!(whole.highlighted_rows.is_empty());
    assert_eq!(item.highlighted_rows, [0]);
    assert_eq!(item.lines, ["> develop"]);
}

#[test]
fn a_box_with_no_styled_text_has_no_styles_and_no_highlighted_rows() {
    let grid = a_screen_showing("plain\r\ntext");

    let cropped = cropped(&grid, the_box(0, 0, 2, 5));

    assert!(cropped.styles.is_empty());
    assert!(cropped.highlighted_rows.is_empty());
}

// ── the cursor ───────────────────────────────────────────────────────────────────────────────

#[test]
fn a_cursor_inside_the_box_is_reported_relative_to_it() {
    // Given the cursor on row 2, column 5
    let grid = a_screen_showing("\x1b[3;6H");

    // When the box starts at row 1, column 3
    let cropped = cropped(&grid, the_box(1, 3, 3, 8));

    // Then it is at row 1, column 2 of the box
    assert_eq!(
        (
            cropped.cursor.row,
            cropped.cursor.col,
            cropped.cursor.visible,
            cropped.cursor.inside
        ),
        (1, 2, true, true)
    );
}

#[test]
fn a_cursor_above_and_left_of_the_box_has_negative_coordinates_and_is_not_inside() {
    let grid = a_screen_showing("\x1b[1;1H");

    let cropped = cropped(&grid, the_box(1, 3, 3, 8));

    assert_eq!(
        (
            cropped.cursor.row,
            cropped.cursor.col,
            cropped.cursor.inside
        ),
        (-1, -3, false)
    );
}

#[test]
fn a_cursor_below_and_right_of_the_box_is_past_its_size_and_not_inside() {
    // Given the cursor on row 5, column 15
    let grid = a_screen_showing("\x1b[6;16H");

    // When the box is rows 1 to 2 and columns 3 to 10
    let cropped = cropped(&grid, the_box(1, 3, 2, 8));

    // Then it is at row 4, column 12 of the box, beyond its two rows and eight columns
    assert_eq!(
        (
            cropped.cursor.row,
            cropped.cursor.col,
            cropped.cursor.inside
        ),
        (4, 12, false)
    );
}

#[test]
fn the_cursor_on_the_boxs_last_cell_is_inside_and_one_past_it_is_not() {
    // Given a box of rows 1 to 2 and columns 3 to 10
    let last = a_screen_showing("\x1b[3;11H");
    let past = a_screen_showing("\x1b[4;12H");

    // Then row 2, column 10 is its last cell, and row 3, column 11 is outside
    assert!(cropped(&last, the_box(1, 3, 2, 8)).cursor.inside);
    assert!(!cropped(&past, the_box(1, 3, 2, 8)).cursor.inside);
}

#[test]
fn a_hidden_cursor_stays_hidden_in_the_crop() {
    let grid = a_screen_showing("\x1b[2;3H\x1b[?25l");

    let cropped = cropped(&grid, the_box(0, 0, 4, 8));

    assert!(!cropped.cursor.visible);
    assert!(cropped.cursor.inside);
}

// ── clip versus refusal ──────────────────────────────────────────────────────────────────────

#[test]
fn a_box_running_past_the_screen_is_clipped_and_the_region_shows_the_clip() {
    let grid = a_screen_showing("abcdefghij\r\nklmnopqrst\r\nuvwxyz");

    // When a ten-by-ten box starts at row 4, column 15 of a six-row, twenty-column screen
    let cropped = cropped(&grid, the_box(4, 15, 10, 10));

    // Then it is the two rows and five columns that exist
    assert_eq!(cropped.region, the_box(4, 15, 2, 5));
    assert_eq!(cropped.lines.len(), 2);
}

#[test]
fn a_box_that_exactly_fits_is_not_clipped() {
    let grid = a_screen_showing("abc");

    let cropped = cropped(&grid, the_box(0, 0, ROWS, COLS));

    assert_eq!(cropped.region, the_box(0, 0, ROWS, COLS));
    assert_eq!(cropped.lines.len(), usize::from(ROWS));
}

#[test]
fn a_box_with_no_size_or_off_the_screen_is_refused_naming_the_screens_size() {
    let grid = a_screen_showing("abc");

    for (why, region) in [
        ("no rows", the_box(0, 0, 0, 5)),
        ("no columns", the_box(0, 0, 5, 0)),
        ("below the screen", the_box(6, 0, 2, 5)),
        ("right of the screen", the_box(0, 20, 2, 5)),
        ("far off the screen", the_box(u16::MAX, u16::MAX, 5, 5)),
    ] {
        let refusal = crop(&grid, region).expect_err(why);
        assert!(
            refusal.contains("20 columns by 6 rows"),
            "{why}: refused, but not naming the screen's size: {refusal}"
        );
    }
}
