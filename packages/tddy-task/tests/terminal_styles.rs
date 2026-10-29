//! The styles of a terminal screen's text: style runs per row, the colour naming and the rule for
//! which rows a bar paints (`#home-inbox` 9/9).
//!
//! The extraction tests feed a capture real SGR sequences and read the runs back from
//! `TerminalCapture::text_grid`; the highlight rule is tested on hand-built runs. No PTY is involved.
//!
//! Feature doc: docs/ft/daemon/1-WIP/PRD-2026-10-29-terminal-styles.md.

use tddy_task::{highlighted_rows, RowStyles, StyleRun, TerminalCapture, TerminalColour};

const ROWS: u16 = 6;
const COLS: u16 = 30;

const RESET: &str = "\x1b[0m";

/// A capture whose model is sized to the PTY's grid, as every spawned terminal's is.
fn a_screen_showing(output: &str) -> TerminalCapture {
    let mut capture = TerminalCapture::new();
    capture.resize(ROWS, COLS);
    capture.append(output.as_bytes());
    capture
}

/// The run `[from, to)` in the terminal's default colours and with no attribute.
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

fn the_runs_of(capture: &TerminalCapture, row: u16) -> Vec<StyleRun> {
    capture
        .text_grid()
        .expect("a sized model reads as a text grid")
        .styles
        .into_iter()
        .find(|styled| styled.row == row)
        .map(|styled| styled.runs)
        .unwrap_or_default()
}

fn the_rows_with_runs(capture: &TerminalCapture) -> Vec<u16> {
    capture
        .text_grid()
        .expect("a sized model reads as a text grid")
        .styles
        .iter()
        .map(|styled| styled.row)
        .collect()
}

// ── the colour naming ────────────────────────────────────────────────────────────────────────

#[test]
fn the_sixteen_standard_indices_are_named_and_the_other_indices_are_idx_n() {
    let names = [
        "black",
        "red",
        "green",
        "yellow",
        "blue",
        "magenta",
        "cyan",
        "white",
        "bright-black",
        "bright-red",
        "bright-green",
        "bright-yellow",
        "bright-blue",
        "bright-magenta",
        "bright-cyan",
        "bright-white",
    ];
    for (index, name) in names.iter().enumerate() {
        assert_eq!(
            TerminalColour::Indexed(index as u8).to_string(),
            *name,
            "index {index}"
        );
    }
    assert_eq!(TerminalColour::Indexed(16).to_string(), "idx:16");
    assert_eq!(TerminalColour::Indexed(200).to_string(), "idx:200");
    assert_eq!(TerminalColour::Indexed(255).to_string(), "idx:255");
}

#[test]
fn a_true_colour_is_named_as_lowercase_hex() {
    assert_eq!(TerminalColour::Rgb(255, 136, 0).to_string(), "#ff8800");
    assert_eq!(TerminalColour::Rgb(0, 0, 0).to_string(), "#000000");
    assert_eq!(TerminalColour::Rgb(1, 2, 171).to_string(), "#0102ab");
}

#[test]
fn the_colours_a_screen_was_painted_with_come_back_as_their_indexed_or_rgb_form() {
    // Given a row painted with a named, a bright, a 256-palette and a true colour, in turn
    let screen = a_screen_showing(&format!(
        "\x1b[31mA{RESET}\x1b[91mB{RESET}\x1b[38;5;200mC{RESET}\x1b[38;2;255;136;0mD{RESET}"
    ));

    // When its runs are read
    let runs = the_runs_of(&screen, 0);

    // Then each carries its colour
    let colours: Vec<_> = runs.iter().map(|run| run.fg).collect();
    assert_eq!(
        colours,
        [
            Some(TerminalColour::Indexed(1)),
            Some(TerminalColour::Indexed(9)),
            Some(TerminalColour::Indexed(200)),
            Some(TerminalColour::Rgb(255, 136, 0)),
        ]
    );
}

#[test]
fn a_background_is_read_as_bg_and_does_not_set_fg() {
    let screen = a_screen_showing(&format!("\x1b[44mab{RESET}\x1b[104mcd{RESET}"));

    assert_eq!(
        the_runs_of(&screen, 0),
        [
            StyleRun {
                bg: Some(TerminalColour::Indexed(4)),
                ..run(0, 2)
            },
            StyleRun {
                bg: Some(TerminalColour::Indexed(12)),
                ..run(2, 4)
            },
        ]
    );
}

// ── attributes ───────────────────────────────────────────────────────────────────────────────

#[test]
fn each_attribute_is_read_as_its_own_flag() {
    let cases: [(&str, StyleRun); 7] = [
        (
            "1",
            StyleRun {
                bold: true,
                ..run(0, 2)
            },
        ),
        (
            "2",
            StyleRun {
                dim: true,
                ..run(0, 2)
            },
        ),
        (
            "3",
            StyleRun {
                italic: true,
                ..run(0, 2)
            },
        ),
        (
            "4",
            StyleRun {
                underline: true,
                ..run(0, 2)
            },
        ),
        (
            "5",
            StyleRun {
                blink: true,
                ..run(0, 2)
            },
        ),
        (
            "7",
            StyleRun {
                inverse: true,
                ..run(0, 2)
            },
        ),
        (
            "9",
            StyleRun {
                strikethrough: true,
                ..run(0, 2)
            },
        ),
    ];
    for (sgr, expected) in cases {
        let screen = a_screen_showing(&format!("\x1b[{sgr}mab{RESET}cd"));
        assert_eq!(the_runs_of(&screen, 0), [expected], "SGR {sgr}");
    }
}

// ── runs: merging, omission, reach ───────────────────────────────────────────────────────────

#[test]
fn adjacent_cells_with_the_same_pen_merge_into_one_run() {
    // Given a row whose pen is set again, unchanged, between characters
    let screen = a_screen_showing(&format!("\x1b[31mab\x1b[31mcd{RESET}"));

    // Then it is one run over all four cells
    assert_eq!(
        the_runs_of(&screen, 0),
        [StyleRun {
            fg: Some(TerminalColour::Indexed(1)),
            ..run(0, 4)
        }]
    );
}

#[test]
fn adjacent_cells_with_different_pens_are_separate_runs_that_touch() {
    let screen = a_screen_showing(&format!("\x1b[31mab\x1b[1mcd{RESET}"));

    assert_eq!(
        the_runs_of(&screen, 0),
        [
            StyleRun {
                fg: Some(TerminalColour::Indexed(1)),
                ..run(0, 2)
            },
            StyleRun {
                fg: Some(TerminalColour::Indexed(1)),
                bold: true,
                ..run(2, 4)
            },
        ]
    );
}

#[test]
fn default_styled_cells_make_no_run_and_a_plain_row_has_no_entry() {
    // Given a plain row, a row with one styled word between plain text, and a reset-only row
    let screen = a_screen_showing(&format!(
        "plain text\r\nab \x1b[31mred{RESET} cd\r\n{RESET}reset only"
    ));

    // Then only the styled word is a run, and only its row has an entry
    assert_eq!(the_rows_with_runs(&screen), [1]);
    assert_eq!(
        the_runs_of(&screen, 1),
        [StyleRun {
            fg: Some(TerminalColour::Indexed(1)),
            ..run(3, 6)
        }]
    );
}

#[test]
fn a_bar_wider_than_its_text_is_a_run_that_reaches_past_the_trimmed_text() {
    // Given a blue bar of eight cells holding five characters of text
    let screen = a_screen_showing(&format!("\x1b[44m  abc   {RESET}"));

    // Then the row's text is trimmed to five characters and the run still covers all eight cells
    let grid = screen.text_grid().unwrap();
    assert_eq!(grid.lines[0], "  abc");
    assert_eq!(
        the_runs_of(&screen, 0),
        [StyleRun {
            bg: Some(TerminalColour::Indexed(4)),
            ..run(0, 8)
        }]
    );
}

// ── wide characters ──────────────────────────────────────────────────────────────────────────

#[test]
fn a_wide_character_counts_once_so_the_columns_after_it_stay_aligned_with_the_text() {
    // Given a row of two wide characters, then a red X
    let screen = a_screen_showing(&format!("漢字\x1b[31mX{RESET}"));

    // Then the text has three characters, and the run is on the third
    let grid = screen.text_grid().unwrap();
    assert_eq!(grid.lines[0], "漢字X");
    assert_eq!(
        the_runs_of(&screen, 0),
        [StyleRun {
            fg: Some(TerminalColour::Indexed(1)),
            ..run(2, 3)
        }]
    );
}

#[test]
fn an_inverse_wide_character_is_one_column_of_its_run() {
    let screen = a_screen_showing(&format!("a\x1b[7m漢字{RESET}b"));

    assert_eq!(
        the_runs_of(&screen, 0),
        [StyleRun {
            inverse: true,
            ..run(1, 3)
        }]
    );
}

// ── the highlighted-row rule ─────────────────────────────────────────────────────────────────

fn inverse(from: u16, to: u16) -> StyleRun {
    StyleRun {
        inverse: true,
        ..run(from, to)
    }
}

fn on(bg: u8, from: u16, to: u16) -> StyleRun {
    StyleRun {
        bg: Some(TerminalColour::Indexed(bg)),
        ..run(from, to)
    }
}

fn in_row(row: u16, runs: Vec<StyleRun>) -> RowStyles {
    RowStyles { row, runs }
}

fn lines(rows: &[&str]) -> Vec<String> {
    rows.iter().map(|row| row.to_string()).collect()
}

#[test]
fn a_row_whose_every_character_is_inverse_is_highlighted() {
    let styles = [in_row(1, vec![inverse(0, 9)])];

    assert_eq!(
        highlighted_rows(&styles, &lines(&["Pick", "> develop", "  main"])),
        [1]
    );
}

#[test]
fn a_background_bar_is_highlighted_and_may_be_wider_than_the_text() {
    let styles = [in_row(0, vec![on(4, 0, 20)])];

    assert_eq!(highlighted_rows(&styles, &lines(&["  Open file"])), [0]);
}

#[test]
fn blank_cells_before_the_first_character_need_no_run() {
    let styles = [in_row(0, vec![inverse(2, 9)])];

    assert_eq!(highlighted_rows(&styles, &lines(&["  > main"])), [0]);
}

#[test]
fn a_bar_that_stops_short_of_a_character_is_not_highlighted() {
    // Given a run over the first six cells of a nine-character row
    let styles = [in_row(0, vec![inverse(0, 6)])];

    // Then it is partial cover
    assert!(highlighted_rows(&styles, &lines(&["> develop"])).is_empty());
}

#[test]
fn a_bar_that_starts_after_the_first_character_is_not_highlighted() {
    let styles = [in_row(0, vec![inverse(2, 9)])];

    assert!(highlighted_rows(&styles, &lines(&["> develop"])).is_empty());
}

#[test]
fn a_row_with_no_text_is_not_highlighted_however_it_is_painted() {
    // Given a blank row and an all-spaces row (trimmed to nothing), each under a bar
    let styles = [
        in_row(0, vec![on(4, 0, 20)]),
        in_row(1, vec![inverse(0, 20)]),
    ];

    // Then neither is highlighted
    assert!(highlighted_rows(&styles, &lines(&["", ""])).is_empty());
}

#[test]
fn a_coloured_word_inside_an_inverse_bar_still_highlights_the_row() {
    // Given an inverse bar split in three runs by a red word in the middle
    let styles = [in_row(
        0,
        vec![
            inverse(0, 2),
            StyleRun {
                fg: Some(TerminalColour::Indexed(1)),
                ..inverse(2, 8)
            },
            inverse(8, 15),
        ],
    )];

    // Then the runs are contiguous and all inverse, so the row is highlighted
    assert_eq!(highlighted_rows(&styles, &lines(&["> hotfix branch"])), [0]);
}

#[test]
fn a_background_bar_split_by_a_coloured_word_is_highlighted_when_every_run_has_that_background() {
    let styles = [in_row(
        0,
        vec![
            on(4, 0, 2),
            StyleRun {
                fg: Some(TerminalColour::Indexed(1)),
                ..on(4, 2, 8)
            },
            on(4, 8, 15),
        ],
    )];

    assert_eq!(highlighted_rows(&styles, &lines(&["> hotfix branch"])), [0]);
}

#[test]
fn a_plain_gap_between_the_runs_means_the_row_is_not_highlighted() {
    // Given inverse over "> " and "branch", with plain text between
    let styles = [in_row(0, vec![inverse(0, 2), inverse(8, 15)])];

    assert!(highlighted_rows(&styles, &lines(&["> hotfix branch"])).is_empty());
}

#[test]
fn runs_with_different_backgrounds_are_not_a_bar() {
    let styles = [in_row(0, vec![on(4, 0, 5), on(2, 5, 10)])];

    assert!(highlighted_rows(&styles, &lines(&["abcdefghij"])).is_empty());
}

#[test]
fn an_inverse_run_beside_a_background_run_is_not_a_bar() {
    let styles = [in_row(0, vec![inverse(0, 5), on(4, 5, 10)])];

    assert!(highlighted_rows(&styles, &lines(&["abcdefghij"])).is_empty());
}

#[test]
fn coloured_or_bold_words_in_an_ordinary_row_do_not_highlight_it() {
    // Given a row with a red word and a bold word among plain text
    let red = StyleRun {
        fg: Some(TerminalColour::Indexed(1)),
        ..run(8, 13)
    };
    let bold = StyleRun {
        bold: true,
        ..run(18, 25)
    };
    let styles = [in_row(0, vec![red, bold])];

    // Then the row is not highlighted
    assert!(highlighted_rows(&styles, &lines(&["status: error and warning found"])).is_empty());
}

#[test]
fn a_colour_covering_the_whole_row_without_a_background_or_inverse_is_not_a_bar() {
    let styles = [in_row(
        0,
        vec![StyleRun {
            fg: Some(TerminalColour::Indexed(2)),
            ..run(0, 9)
        }],
    )];

    assert!(highlighted_rows(&styles, &lines(&["all green"])).is_empty());
}

#[test]
fn the_highlighted_rows_come_back_ascending() {
    let styles = [
        in_row(1, vec![inverse(0, 3)]),
        in_row(3, vec![on(4, 0, 3)]),
        in_row(4, vec![inverse(0, 1)]),
    ];

    assert_eq!(
        highlighted_rows(&styles, &lines(&["", "abc", "", "def", "x"])),
        [1, 3, 4]
    );
}
