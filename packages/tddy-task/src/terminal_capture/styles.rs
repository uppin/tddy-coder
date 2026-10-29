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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const STANDARD: [&str; 16] = [
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
        match *self {
            TerminalColour::Indexed(index) => match STANDARD.get(usize::from(index)) {
                Some(name) => f.write_str(name),
                None => write!(f, "idx:{index}"),
            },
            TerminalColour::Rgb(red, green, blue) => write!(f, "#{red:02x}{green:02x}{blue:02x}"),
        }
    }
}

/// The colour of a screen-model colour, as this module names it.
fn colour_of(colour: avt::Color) -> TerminalColour {
    match colour {
        avt::Color::Indexed(index) => TerminalColour::Indexed(index),
        avt::Color::RGB(rgb) => TerminalColour::Rgb(rgb.r, rgb.g, rgb.b),
    }
}

/// The run `[from, to)` painted with `pen`, with only the attributes the pen sets.
fn style_run(from: usize, to: usize, pen: &avt::Pen) -> StyleRun {
    StyleRun {
        from: from as u16,
        to: to as u16,
        fg: pen.foreground().map(colour_of),
        bg: pen.background().map(colour_of),
        bold: pen.is_bold(),
        dim: pen.is_faint(),
        italic: pen.is_italic(),
        underline: pen.is_underline(),
        inverse: pen.is_inverse(),
        strikethrough: pen.is_strikethrough(),
        blink: pen.is_blink(),
    }
}

/// The runs of one row, from the pen of each of its characters, left to right. A default pen makes
/// no run, and two adjacent characters with equal pens make one.
fn runs_of(pens: &[&avt::Pen]) -> Vec<StyleRun> {
    let mut spans: Vec<(usize, usize, &avt::Pen)> = Vec::new();
    for (position, pen) in pens.iter().enumerate() {
        if pen.is_default() {
            continue;
        }
        match spans.last_mut() {
            Some((_, to, open)) if *to == position && **open == **pen => *to += 1,
            _ => spans.push((position, position + 1, *pen)),
        }
    }
    spans
        .into_iter()
        .map(|(from, to, pen)| style_run(from, to, pen))
        .collect()
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
    let mut rows: Vec<u16> = styles
        .iter()
        .filter(|styled| {
            let text = lines
                .get(usize::from(styled.row))
                .map_or("", String::as_str);
            is_bar(&styled.runs, text)
        })
        .map(|styled| styled.row)
        .collect();
    rows.sort_unstable();
    rows
}

/// The first and last character positions of `text` that are not whitespace, inclusive; `None` for
/// a row with no such character.
fn span_of_text(text: &str) -> Option<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let first = chars.iter().position(|ch| !ch.is_whitespace())?;
    let last = chars.iter().rposition(|ch| !ch.is_whitespace())?;
    Some((first, last))
}

/// Whether one row is painted as a bar: see [`highlighted_rows`].
///
/// `runs` are ascending and never overlap, so the runs covering the span are found in one pass:
/// each must start where the one before ended, and the first must cover the span's first character.
fn is_bar(runs: &[StyleRun], text: &str) -> bool {
    let Some((first, last)) = span_of_text(text) else {
        return false;
    };
    let mut covering: Vec<&StyleRun> = Vec::new();
    // The first character position of the span that no run covers yet.
    let mut next = first;
    for run in runs {
        let (from, to) = (usize::from(run.from), usize::from(run.to));
        if to <= next {
            continue;
        }
        if from > next {
            break;
        }
        covering.push(run);
        next = to;
        if next > last {
            break;
        }
    }
    if next <= last {
        return false;
    }
    let all_inverse = covering.iter().all(|run| run.inverse);
    let shared_background =
        covering[0].bg.is_some() && covering.iter().all(|run| run.bg == covering[0].bg);
    all_inverse || shared_background
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
    let mut styles = Vec::new();
    let mut widths = Vec::new();
    for (row, line) in lines.enumerate() {
        let mut row_widths = Vec::new();
        let mut pens: Vec<&avt::Pen> = Vec::new();
        for cell in line.cells() {
            // The tail cell of a wide character has width 0: `avt::Line::text` skips it too, so
            // these positions are the characters the row's text is made of.
            if cell.width() == 0 {
                continue;
            }
            row_widths.push(cell.width());
            pens.push(cell.pen());
        }
        let runs = runs_of(&pens);
        if !runs.is_empty() {
            styles.push(RowStyles {
                row: row as u16,
                runs,
            });
        }
        widths.push(row_widths);
    }
    ScreenCells { styles, widths }
}
