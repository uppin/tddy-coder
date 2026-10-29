//! Cropping a [`TextGrid`] to a box of screen cells (`#home-inbox` 9/9).
//!
//! The box is in screen cells, the units of the grid's `cursor`, `cols` and `rows`. A row's text is
//! characters, and a wide character spans two cells, so the cut needs each character's cell span:
//! [`TextGrid::widths`] carries it, from the same walk over the screen model's cells that produces
//! [`TextGrid::styles`]. A wide character cut by either edge of the box becomes a space.
//!
//! Feature doc: docs/ft/daemon/1-WIP/PRD-2026-10-29-terminal-styles.md § Crop box.

use super::{RowStyles, TextGrid};

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
    // TODO(terminal-styles): implement. Fails closed: refuses every box.
    let _ = (grid, region);
    Err("not implemented".to_string())
}
