//! Cropping a [`TextGrid`] to a box of screen cells (`#home-inbox` 9/9).
//!
//! The box is in screen cells, the units of the grid's `cursor`, `cols` and `rows`. A row's text is
//! characters, and a wide character spans two cells, so the cut needs each character's cell span:
//! [`TextGrid::widths`] carries it, from the same walk over the screen model's cells that produces
//! [`TextGrid::styles`]. A wide character cut by either edge of the box becomes a space.
//!
//! Feature doc: docs/ft/daemon/1-WIP/PRD-2026-10-29-terminal-styles.md § Crop box.

use super::{highlighted_rows, RowStyles, StyleRun, TextGrid};

/// A box on the screen: its top-left cell, zero-based, and its size, in screen cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridRegion {
    pub row: u16,
    pub col: u16,
    pub rows: u16,
    pub cols: u16,
}

/// Where the cursor is relative to a box.
///
/// `row` and `col` are signed: they are negative when the cursor is above or left of the box, and
/// at least `rows` / `cols` of the region when it is below or right of it. `inside` is whether it
/// is within the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CroppedCursor {
    pub row: i32,
    pub col: i32,
    pub visible: bool,
    pub inside: bool,
}

/// What a box of the screen shows: [`crop`]'s answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CroppedGrid {
    /// The box's rows, each cut to the box's columns and trimmed at the end.
    pub lines: Vec<String>,
    /// The style runs clipped to the box and re-based: row 0 and column 0 are the box's top-left,
    /// and columns are character positions in the cropped `lines`.
    pub styles: Vec<RowStyles>,
    /// [`highlighted_rows`](super::highlighted_rows) computed on the cropped rows.
    pub highlighted_rows: Vec<u16>,
    pub cursor: CroppedCursor,
    /// The box actually read: the requested one clipped to the screen.
    pub region: GridRegion,
}

/// The part of `grid` inside `region`.
///
/// A box running past the screen is clipped to it, and [`CroppedGrid::region`] shows the clip. A box
/// with no rows or no columns, or wholly off the screen, is an error naming the screen's size
/// (`<cols> columns by <rows> rows`).
pub fn crop(grid: &TextGrid, region: GridRegion) -> Result<CroppedGrid, String> {
    let (top, left) = (usize::from(region.row), usize::from(region.col));
    let (screen_rows, screen_cols) = (usize::from(grid.rows), usize::from(grid.cols));
    if region.rows == 0 || region.cols == 0 || top >= screen_rows || left >= screen_cols {
        return Err(format!(
            "the box at row {}, column {}, {} rows by {} columns, is empty or lies off the screen; \
             the screen is {} columns by {} rows",
            region.row, region.col, region.rows, region.cols, grid.cols, grid.rows
        ));
    }
    let bottom = (top + usize::from(region.rows)).min(screen_rows);
    let right = (left + usize::from(region.cols)).min(screen_cols);

    let mut lines = Vec::with_capacity(bottom - top);
    let mut styles = Vec::new();
    for row in top..bottom {
        let entries = entries_of(grid, row);
        let visible: Vec<&Entry> = entries
            .iter()
            .filter(|entry| entry.start < right && entry.start + entry.width > left)
            .collect();
        // The visible entries are contiguous, so the first one's index shifts every run into the box.
        let offset = entries
            .iter()
            .position(|entry| entry.start + entry.width > left)
            .unwrap_or(0);
        let text: String = visible
            .iter()
            .map(|entry| {
                let cut = entry.start < left || entry.start + entry.width > right;
                if cut {
                    ' '
                } else {
                    entry.ch
                }
            })
            .collect();
        lines.push(text.trim_end().to_owned());

        let runs = grid
            .styles
            .iter()
            .find(|styled| usize::from(styled.row) == row)
            .map(|styled| {
                let end = offset + visible.len();
                styled
                    .runs
                    .iter()
                    .filter_map(|run| {
                        let from = usize::from(run.from).max(offset);
                        let to = usize::from(run.to).min(end);
                        (from < to).then(|| StyleRun {
                            from: (from - offset) as u16,
                            to: (to - offset) as u16,
                            ..*run
                        })
                    })
                    .collect::<Vec<StyleRun>>()
            })
            .unwrap_or_default();
        if !runs.is_empty() {
            styles.push(RowStyles {
                row: (row - top) as u16,
                runs,
            });
        }
    }

    let cursor_row = i32::from(grid.cursor.row) - top as i32;
    let cursor_col = i32::from(grid.cursor.col) - left as i32;
    let inside = (0..(bottom - top) as i32).contains(&cursor_row)
        && (0..(right - left) as i32).contains(&cursor_col);
    let highlighted = highlighted_rows(&styles, &lines);
    Ok(CroppedGrid {
        lines,
        styles,
        highlighted_rows: highlighted,
        cursor: CroppedCursor {
            row: cursor_row,
            col: cursor_col,
            visible: grid.cursor.visible,
            inside,
        },
        region: GridRegion {
            row: region.row,
            col: region.col,
            rows: (bottom - top) as u16,
            cols: (right - left) as u16,
        },
    })
}

/// One character of a row and the cells it spans, counted from the row's left edge.
struct Entry {
    ch: char,
    start: usize,
    width: usize,
}

/// The characters of `grid`'s row `row`, each with its cell span. The row's text is a prefix of
/// them: the characters past the trimmed text are blanks, and the list reaches the screen's width,
/// so a run that paints a bar past the text has cells to fall on. A row with no widths is read as
/// all narrow.
fn entries_of(grid: &TextGrid, row: usize) -> Vec<Entry> {
    let text = grid.lines.get(row).map_or("", String::as_str);
    let widths = grid.widths.get(row).map_or(&[][..], Vec::as_slice);
    let count = text.chars().count().max(widths.len());
    let mut chars = text.chars();
    let mut entries = Vec::with_capacity(count);
    let mut cells = 0;
    for index in 0..count {
        let width = usize::from(widths.get(index).copied().unwrap_or(1));
        entries.push(Entry {
            ch: chars.next().unwrap_or(' '),
            start: cells,
            width,
        });
        cells += width;
    }
    while cells < usize::from(grid.cols) {
        entries.push(Entry {
            ch: ' ',
            start: cells,
            width: 1,
        });
        cells += 1;
    }
    entries
}
