//! The text grid of a terminal's screen: [`TextGrid`] and its cursor ([`super::TerminalCapture::text_grid`]).

use super::RowStyles;

/// The visible text of a terminal's active screen buffer at a sequence point
/// ([`super::TerminalCapture::text_grid`]).
///
/// No scrollback. `lines` holds one entry per row of the grid (`rows` of them), each trimmed at the
/// end; `styles` holds the colours and attributes of the rows that have any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextGrid {
    /// The visible rows, top first.
    pub lines: Vec<String>,
    /// The non-default style runs of the rows that have any, in row order. Columns are character
    /// positions in the row's `lines` entry (see [`super::StyleRun`]).
    pub styles: Vec<RowStyles>,
    /// The cell span of each character of each row of `lines` and beyond it, to the grid's width:
    /// 1, or 2 for a wide character. One entry per row; a grid without entries is read as all
    /// narrow. [`crate::crop`] cuts on cell boundaries with it.
    pub widths: Vec<Vec<u8>>,
    pub cursor: GridCursor,
    pub cols: u16,
    pub rows: u16,
    /// The sequence point: every output byte below this absolute offset is reflected in `lines`,
    /// none at or above it is. The same offset a [`super::ScreenSnapshot`] taken at that moment carries.
    pub offset: u64,
    /// Whether the application has bracketed paste (private mode 2004) switched on.
    pub bracketed_paste: bool,
}

/// Where the cursor is on the grid, zero-based from the top-left cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridCursor {
    pub row: u16,
    pub col: u16,
    pub visible: bool,
}
