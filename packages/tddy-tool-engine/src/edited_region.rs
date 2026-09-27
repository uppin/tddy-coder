//! What a file says where an edit landed.
//!
//! `StrReplace` used to answer `{"replaced": true, "bytes_written": 40926}` — two numbers,
//! neither of which describes the text. An agent editing through the tool was therefore writing
//! blind, and in session `01a0e285` that is what went wrong: a deleted block fused the blank
//! lines around it, the agent guessed at how many there now were (`\n\n`, where the file held
//! `\n\n\n\n`) and spent six calls on `old_string not found in file`. Every one of those was a
//! guess about text the tool had just written and declined to show.
//!
//! Runs of blank lines are the worst case for reading a file back by eye, and the one where an
//! editing agent most needs bytes rather than a count — so the region is the file **as it is
//! now**, quoted verbatim, not a diff and not an echo of what was asked for.
//!
//! The sibling of [`crate::read_window`] and [`crate::search_window`]: the same idea one tool
//! along, that a caller is owed the shape of what it got rather than its size.

/// How many lines of the file on each side of an edit a successful `StrReplace` shows.
///
/// Eight, so the region is seventeen lines — a couple of hundred tokens. The floor is the case
/// that motivated the region at all: a deletion that leaves a run of blank lines has to show the
/// run *and* the code bracketing it on both sides, or the caller is back to counting newlines it
/// cannot see. The ceiling is [`crate::read_window`]'s reason for existing — a region that grew
/// towards a screenful would put the context cost the line window removed straight back, on
/// every edit, for a caller that only asked to change one line.
pub const EDITED_REGION_CONTEXT_LINES: usize = 8;

/// The file around an edit, and where in the file that is.
pub(crate) struct EditedRegion {
    /// The lines around the edit, joined as they appear in the file.
    pub text: String,
    /// The 1-based line the edit starts on — without it the region cannot be located in the file
    /// it came from, so a caller cannot page around it.
    pub line: u64,
}

/// The region of `content` around the edit beginning at byte `edit_offset`.
///
/// `content` is the file **after** the write, so the region describes what a reader would now
/// find there. Both ends are clamped to the file: an edit on the first line has nothing above it
/// and an edit on the last has nothing below, and in either case the region is the lines that
/// exist rather than a padded window or a panic.
pub(crate) fn edited_region(content: &str, edit_offset: usize) -> EditedRegion {
    let lines: Vec<&str> = content.lines().collect();
    let edited = content[..edit_offset].matches('\n').count();
    let first = edited.saturating_sub(EDITED_REGION_CONTEXT_LINES);
    let last = (edited + EDITED_REGION_CONTEXT_LINES).min(lines.len().saturating_sub(1));

    EditedRegion {
        text: lines.get(first..=last).unwrap_or_default().join("\n"),
        line: edited as u64 + 1,
    }
}
