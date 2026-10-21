//! The cut a member move makes in the origin (R2), and the split of the widenings between the moved
//! lines and the rest of the origin.
//!
//! A function of the origin's text and the run: the lines the members occupy, one blank line that
//! goes with them so no double blank line remains, or the whole block when the run is all of it.

use std::ops::Range;

use super::super::crate_move::source_scan::attached_trivia_starts_at;
use super::super::item_move::text::{line_end, line_start, next_line_start, Edit};
use super::super::retarget_impl::outline::Run;
use super::super::seam_refusal;
use super::landing::line_start_of;
use crate::Result;

/// The one-based first and last lines of the moved members (attached trivia included).
fn member_lines(run: &Run) -> Result<(u32, u32)> {
    let (Some(first), Some(last)) = (run.moved().first(), run.moved().last()) else {
        return Err(seam_refusal("the run moves no member"));
    };
    Ok((first.first_line, last.last_line))
}

/// The bytes of the moved members, from the first one's first line to the last one's end.
pub(super) fn members_region(text: &str, run: &Run) -> Result<Range<usize>> {
    let (first, last) = member_lines(run)?;
    Ok(line_start(text, first)..line_end(text, last))
}

/// The span the origin loses (R2): the moved lines, or the whole block, and one blank line.
pub(super) fn cut_span(text: &str, run: &Run) -> Result<Range<usize>> {
    let (first, last) = if run.whole_block() {
        (
            attached_trivia_starts_at(text, run.header_line),
            run.close_line,
        )
    } else {
        member_lines(run)?
    };
    let mut span = line_start(text, first)..line_end(text, last);
    if is_blank_line_at(text, span.end) {
        span.end = next_line_start(text, span.end);
    } else if span.start > 0 && is_blank_line_at(text, line_start_of(text, span.start - 1)) {
        span.start = line_start_of(text, span.start - 1);
    }
    Ok(span)
}

fn is_blank_line_at(text: &str, offset: usize) -> bool {
    text.get(offset..)
        .and_then(|rest| rest.lines().next())
        .is_some_and(|line| line.trim().is_empty())
}

/// The widenings that touch the moved lines, and the rest (which stay in the origin file).
pub(super) fn split_widening(edits: &[Edit], members: &Range<usize>) -> (Vec<Edit>, Vec<Edit>) {
    edits
        .iter()
        .cloned()
        .partition(|edit| members.start <= edit.start && edit.end <= members.end)
}
