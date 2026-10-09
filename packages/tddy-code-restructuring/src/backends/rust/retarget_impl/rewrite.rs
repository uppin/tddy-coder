//! The text a retarget writes: the header's self type, the split into up to three blocks, and the
//! re-pointed `Old::` paths inside the moved members.
//!
//! Nothing but the header's self type, the repeated `impl` headers and the `use` is authored here;
//! every member, attribute and comment is a byte range of the source. The edits are handed to
//! `seam_survey::minimal_edits` by the caller, so the ledger sees insertions at the two cuts rather
//! than one rewrite over the block.

use std::ops::Range;

use super::super::item_move::text::{line_start, next_line_start, qualifier_start};
use super::outline::Run;

/// Where a retarget's pieces fall in the file: the span it replaces, the span of the moved members,
/// and the header texts the blocks are built from.
pub(super) struct Layout<'a> {
    text: &'a str,
    /// The byte span the retarget replaces: the `impl` line's start through the closing `}`.
    pub(super) replaced: Range<usize>,
    /// The byte span of the moved members, where paths are re-pointed.
    pub(super) moved: Range<usize>,
    body_start: usize,
    cut_before: usize,
    cut_after: usize,
    close_start: usize,
    old_header: String,
    new_header: String,
}

impl<'a> Layout<'a> {
    /// The pieces of the retarget of `run` to the self type `new_self`.
    pub(super) fn of(text: &'a str, run: &Run, new_self: &str) -> Layout<'a> {
        let header_start = line_start(text, run.header_line);
        let body_open = text[header_start..]
            .find('{')
            .map(|at| header_start + at)
            .unwrap_or(header_start);
        let body_start = next_line_start(text, body_open);
        let close_start = close_brace(text, run.close_line);

        // A whole-block retarget is the same geometry with the cuts at the block's own ends, so
        // there is nothing to omit on either side.
        let (cut_before, cut_after) = if run.whole_block() {
            (body_start, close_start)
        } else {
            let moved = run.moved();
            let first = moved.first().expect("a run moves at least one member");
            let last = moved.last().expect("a run moves at least one member");
            (
                line_start(text, first.first_line),
                line_start(text, last.last_line + 1),
            )
        };

        let header_plain = &text[header_start..body_open];
        Layout {
            text,
            replaced: header_start..close_start + 1,
            moved: cut_before..cut_after,
            body_start,
            cut_before,
            cut_after,
            close_start,
            old_header: format!("{header_plain}{{\n"),
            new_header: format!("{}{{\n", rewrite_self_type(header_plain, new_self)),
        }
    }

    /// The text that replaces [`Layout::replaced`], with `moved` (the re-pointed moved members) in
    /// the `New` block.
    pub(super) fn assemble(&self, moved: &str) -> String {
        let mut out = String::new();
        if self.cut_before > self.body_start {
            out.push_str(&self.old_header);
            out.push_str(&self.text[self.body_start..self.cut_before]);
            out.push_str("}\n");
        }
        out.push_str(&self.new_header);
        out.push_str(moved);
        out.push_str("}\n");
        if self.cut_after < self.close_start {
            out.push_str(&self.old_header);
            out.push_str(&self.text[self.cut_after..self.close_start]);
            out.push_str("}\n");
        }
        // The replaced span ends at the closing `}` itself, so the last block's own `}` is the one
        // left, and the newline after it comes from the text the span did not cover.
        out.pop();
        out
    }

    /// The text that replaces [`Layout::replaced`] when the plan asks for delegators: the old block
    /// keeps its shape with each moved member's slot holding its delegator (in `delegators`, in
    /// member order), and `impl <New> { <moved> }` follows it after one blank line.
    pub(super) fn with_delegators(&self, moved: &str, delegators: &[String]) -> String {
        // TODO(reshape-methods-leave-type): implement
        let _ = (moved, delegators);
        todo!("TODO(reshape-methods-leave-type): implement Layout::with_delegators")
    }
}

/// The offset of the `}` that closes the `impl`, on the line the outline says it ends on.
fn close_brace(text: &str, close_line: u32) -> usize {
    let start = line_start(text, close_line);
    text[start..]
        .find('}')
        .map(|at| start + at)
        .unwrap_or(start)
}

/// `header` with its self type replaced by `new_self`, everything else as bytes.
///
/// `header` is the `impl` keyword and its generics, self type and `where` clause, without the body's
/// `{`. The self type is the path between the generics and `where` (or the end).
fn rewrite_self_type(header: &str, new_self: &str) -> String {
    let Some(impl_at) = header.find("impl") else {
        return header.to_string();
    };
    let mut at = skip_whitespace(header, impl_at + "impl".len());
    if header.as_bytes().get(at) == Some(&b'<') {
        at = skip_angle(header, at);
    }
    at = skip_whitespace(header, at);
    let end = at + self_type_len(&header[at..]);
    format!("{}{new_self}{}", &header[..at], &header[end..])
}

/// How long the self type at the start of `rest` is: up to a whole-word `where`, trailing whitespace
/// trimmed, or the end.
fn self_type_len(rest: &str) -> usize {
    let bytes = rest.as_bytes();
    let mut at = 0;
    while at + 5 <= bytes.len() {
        let is_where = &bytes[at..at + 5] == b"where"
            && (at == 0 || bytes[at - 1].is_ascii_whitespace())
            && (at + 5 == bytes.len() || bytes[at + 5].is_ascii_whitespace());
        if is_where {
            return rest[..at].trim_end().len();
        }
        at += 1;
    }
    rest.trim_end().len()
}

fn skip_whitespace(text: &str, at: usize) -> usize {
    let bytes = text.as_bytes();
    let mut at = at;
    while at < bytes.len() && bytes[at].is_ascii_whitespace() {
        at += 1;
    }
    at
}

/// The offset just past the `<…>` list opening at `at`.
fn skip_angle(text: &str, at: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut cursor = at;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'<' => depth += 1,
            b'>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return cursor + 1;
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    at
}

/// `text` with every `[chain::]Old::` in front of a moved name replaced by `New::`.
///
/// `sites` are `(offset, name)` pairs into `text`, the offsets of the names the server reports as
/// references to the moved members. Only a site written through the old type is touched: a method
/// call (`self.m()`) has nothing in front of it, and a site outside the moved members is not passed
/// here at all.
pub(super) fn repointed(text: &str, sites: &[(usize, String)], old: &str, new: &str) -> String {
    let mut edits: Vec<Range<usize>> = Vec::new();
    for (offset, _name) in sites {
        let start = qualifier_start(text, *offset);
        if text[start..*offset].ends_with(&format!("{old}::")) {
            edits.push(start..*offset);
        }
    }
    edits.sort_by_key(|span| span.start);
    edits.dedup();

    let mut out = text.to_string();
    for span in edits.into_iter().rev() {
        out.replace_range(span, &format!("{new}::"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_the_self_type_of_a_plain_header() {
        assert_eq!(rewrite_self_type("impl Host ", "Roster"), "impl Roster ");
    }

    #[test]
    fn keeps_the_parameter_list_and_where_clause_of_a_generic_header() {
        let header = "impl<T> Wrapper<T>\nwhere\n    T: Copy,\n";

        assert_eq!(
            rewrite_self_type(header, "Pair<T>"),
            "impl<T> Pair<T>\nwhere\n    T: Copy,\n"
        );
    }

    #[test]
    fn re_points_a_path_through_the_old_type() {
        let text = "        Host::build(self.n)\n";

        assert_eq!(
            repointed(text, &[(14, "build".to_string())], "Host", "Roster"),
            "        Roster::build(self.n)\n"
        );
    }

    #[test]
    fn leaves_a_method_call_alone() {
        let text = "        self.get()\n";

        assert_eq!(
            repointed(text, &[(13, "get".to_string())], "Host", "Roster"),
            "        self.get()\n"
        );
    }
}
