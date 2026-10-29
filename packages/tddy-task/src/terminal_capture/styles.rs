//! The styles of a terminal screen's text: the runs of cells whose pen differs from the terminal
//! default, per row, and the rows a single run highlights (`#home-inbox` 9/9).
//!
//! The runs are read from the screen model's cells ([`avt::Line::cells`]) in the same iteration
//! [`avt::Line::text`] uses, which skips the tail cell of a wide character. A run's columns are
//! therefore character positions in the row's [`TextGrid::lines`](super::TextGrid::lines) entry,
//! and a run slices that string directly: `from` is inclusive, `to` is exclusive. A run may reach
//! past the trimmed text into the blank cells a highlight bar paints.
//!
//! Feature doc: docs/ft/daemon/1-WIP/PRD-2026-10-29-terminal-styles.md.

use std::fmt;

/// A colour a cell is drawn in: a palette index or a true colour.
///
/// Named by [`fmt::Display`]: the 16 standard indices as `black`, `red`, `green`, `yellow`,
/// `blue`, `magenta`, `cyan`, `white` and their `bright-` forms; any other index as `idx:N`; a true
/// colour as `#rrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalColour {
    /// A palette index, 0 to 255.
    Indexed(u8),
    /// A true colour.
    Rgb(u8, u8, u8),
}

impl fmt::Display for TerminalColour {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO(terminal-styles): implement. Fails closed: names nothing.
        Ok(())
    }
}

/// A run of adjacent cells of one row that share one non-default pen.
///
/// Only a pen that differs from the terminal default makes a run, and the attributes that are set
/// are the ones that differ: a `false` flag and a `None` colour are the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleRun {
    /// The first column of the run: a character position in the row's text.
    pub from: u16,
    /// One past the last column of the run. At most the row's character width.
    pub to: u16,
    pub fg: Option<TerminalColour>,
    pub bg: Option<TerminalColour>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub strikethrough: bool,
    pub blink: bool,
}

/// The non-default runs of one row of the grid. A row with no such run has no entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowStyles {
    /// The zero-based row of the grid.
    pub row: u16,
    /// The runs, left to right, never overlapping.
    pub runs: Vec<StyleRun>,
}

/// The rows of `lines` that are painted as a bar; ascending.
///
/// A row is highlighted when it has at least one non-blank character, and the runs that cover the
/// span from its first to its last non-blank character leave no gap of default-styled cells (each
/// run starts where the one before ended), and either all of those runs are `inverse` or all of
/// them carry one identical `bg`. A coloured or bold word inside an inverse bar therefore still
/// counts, and so does a bar whose text is split over several pens; a bar that stops short of a
/// non-blank character, or has a plain gap in it, does not. Blank cells inside the span count as
/// covered only when a run covers them.
///
/// A row with no non-blank character is never highlighted, however it is painted. Status bars
/// qualify as well as a selection: the rule measures how a row is painted, not what it means.
///
/// `styles` and `lines` are those of one [`TextGrid`](super::TextGrid).
pub fn highlighted_rows(styles: &[RowStyles], lines: &[String]) -> Vec<u16> {
    // TODO(terminal-styles): implement. Fails closed: no row is highlighted.
    let _ = (styles, lines);
    Vec::new()
}

/// What one walk over the screen's cells reads: the style runs, and each character's cell span.
pub(super) struct ScreenCells {
    pub(super) styles: Vec<RowStyles>,
    pub(super) widths: Vec<Vec<u8>>,
}

/// Walk the cells of every row of `lines` once, in row order, over characters with the tail cell of
/// a wide character skipped, as [`avt::Line::text`] does.
///
/// Both results come from this one walk, so the columns of the runs and the cell spans the crop
/// uses cannot disagree. Rows without a run are left out of `styles`; `widths` has one entry per
/// row, holding one width (1 or 2) per character of the row, trailing blanks included.
pub(super) fn walk_cells<'a>(lines: impl Iterator<Item = &'a avt::Line>) -> ScreenCells {
    // TODO(terminal-styles): implement. Fails closed: no row has a run, and no width is known (a
    // grid without widths is read as all-narrow).
    let _ = lines;
    ScreenCells {
        styles: Vec::new(),
        widths: Vec::new(),
    }
}
