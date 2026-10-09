//! Reading the `impl` an anchor lands in, and the run of members it moves, out of the server's
//! outline.
//!
//! An `impl` is one outline item of kind 19 whose children are its members — a method (kind 6) or an
//! associated function (kind 12) alike. The anchor's lowered range picks the block (the one it
//! overlaps) and the members (those whose attached trivia begins inside it). The three refusals S1,
//! S2 and S3 are made here, before any edit is built.

use serde_json::Value;

use super::super::{attached_trivia_starts_at, seam_refusal, SYMBOL_KIND_IMPL};
use crate::edit::Range;
use crate::Result;

/// One member of an `impl`, with what the operation needs about it.
pub(in crate::backends::rust) struct Member {
    pub(in crate::backends::rust) name: String,
    /// Where the server reports the member's name, which is where a reference query asks about it.
    pub(in crate::backends::rust) position: Value,
    /// The one-based first line of the member, its attached trivia (docs, attributes, comments)
    /// included.
    pub(in crate::backends::rust) first_line: u32,
    /// The one-based last line of the member.
    pub(in crate::backends::rust) last_line: u32,
}

/// The `impl` an anchor lands in, its members, and which run of them moves.
pub(in crate::backends::rust) struct Run {
    pub(in crate::backends::rust) members: Vec<Member>,
    /// The index of the first moved member.
    pub(in crate::backends::rust) first_moved: usize,
    /// One past the index of the last moved member.
    pub(in crate::backends::rust) moved_end: usize,
    /// The `impl`'s self type as the outline names it: `Host` of `impl<T> Host<T>`.
    pub(in crate::backends::rust) self_type: String,
    /// The one-based line the `impl` keyword is on.
    pub(in crate::backends::rust) header_line: u32,
    /// The one-based line the closing `}` is on.
    pub(in crate::backends::rust) close_line: u32,
}

impl Run {
    /// The members the anchor moves.
    pub(in crate::backends::rust) fn moved(&self) -> &[Member] {
        &self.members[self.first_moved..self.moved_end]
    }

    /// Whether the run is every member of the block, so the self type alone changes.
    pub(in crate::backends::rust) fn whole_block(&self) -> bool {
        self.first_moved == 0 && self.moved_end == self.members.len()
    }
}

/// The `impl` block `range` lands in and the run of members it moves, or the refusal for why none
/// can be.
pub(in crate::backends::rust) fn read(symbols: &Value, text: &str, range: Range) -> Result<Run> {
    let overlapping: Vec<&Value> = symbols
        .as_array()
        .into_iter()
        .flatten()
        .filter(|node| node.get("kind").and_then(Value::as_u64) == Some(SYMBOL_KIND_IMPL))
        .filter(|node| overlaps(node, range))
        .collect();

    match overlapping.as_slice() {
        [] => Err(seam_refusal(format!(
            "the anchor names no `impl` of this file: nothing on lines {}–{} is the self type of an \
             `impl`",
            range.start.line, range.end.line
        ))),
        [one] => block(one, text, range),
        many => Err(seam_refusal(format!(
            "the members anchored sit in more than one `impl` block ({} of them): a retarget changes \
             one block's self type, and one operation cannot change {}",
            many.len(),
            many.len()
        ))),
    }
}

/// The members of one `impl` block, and which run of them `range` moves.
fn block(impl_node: &Value, text: &str, range: Range) -> Result<Run> {
    let members: Vec<Member> = impl_node
        .get("children")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|child| member(child, text))
        .collect();

    let moved: Vec<usize> = members
        .iter()
        .enumerate()
        .filter(|(_, member)| (range.start.line..=range.end.line).contains(&member.first_line))
        .map(|(index, _)| index)
        .collect();
    let (Some(&first_moved), Some(&last_moved)) = (moved.first(), moved.last()) else {
        return Err(seam_refusal(format!(
            "the anchor covers no member of `{}`: it spans lines {}–{} and no member's own text \
             begins in it",
            label_of(impl_node),
            range.start.line,
            range.end.line
        )));
    };

    refuse_a_cut_member(&members, &moved, range)?;

    Ok(Run {
        members,
        first_moved,
        moved_end: last_moved + 1,
        self_type: self_type_of(&label_of(impl_node)),
        header_line: line_of(impl_node, "start").unwrap_or(range.start.line),
        close_line: line_of(impl_node, "end").unwrap_or(range.end.line),
    })
}

/// Refuse a member the range cuts in half: it begins inside the range and goes on past it, or it
/// began above the range and reaches into it.
fn refuse_a_cut_member(members: &[Member], moved: &[usize], range: Range) -> Result<()> {
    for (index, member) in members.iter().enumerate() {
        if moved.contains(&index) {
            continue;
        }
        let overlaps = member.first_line <= range.end.line && member.last_line >= range.start.line;
        if overlaps {
            return Err(seam_refusal(format!(
                "the range cuts `{}` in half: it ends at line {} and the member goes on",
                member.name, range.end.line
            )));
        }
    }
    Ok(())
}

/// One outline child as a member, when it carries the name and range a member needs.
fn member(child: &Value, text: &str) -> Option<Member> {
    let name = child.get("name")?.as_str()?.to_string();
    let position = child.pointer("/selectionRange/start")?.clone();
    let start_line = line_of(child, "start")?;
    let last_line = line_of(child, "end")?;
    Some(Member {
        name,
        position,
        first_line: attached_trivia_starts_at(text, start_line),
        last_line,
    })
}

/// The `impl`'s outline label, `impl Host` or `impl<T> Wrapper<T>`.
fn label_of(node: &Value) -> String {
    node.get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The bare self type of an `impl`'s outline label: `Host` of `impl Host` or `impl<T> Host<T>`.
fn self_type_of(label: &str) -> String {
    let rest = label.strip_prefix("impl").unwrap_or(label).trim_start();
    let rest = if rest.starts_with('<') {
        after_generics(rest).unwrap_or(rest)
    } else {
        rest
    };
    let rest = rest.trim_start();
    // A trait impl reads `impl Trait for Self`, so the self type is what follows ` for `.
    let written = rest.split(" for ").last().unwrap_or(rest).trim();
    written
        .split('<')
        .next()
        .unwrap_or(written)
        .rsplit("::")
        .next()
        .unwrap_or(written)
        .trim()
        .to_string()
}

/// `text` without its leading `<…>` list.
fn after_generics(text: &str) -> Option<&str> {
    let mut depth = 0usize;
    for (at, character) in text.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&text[at + 1..]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether an outline node's lines overlap `range`, both one-based.
fn overlaps(node: &Value, range: Range) -> bool {
    let (Some(start), Some(end)) = (line_of(node, "start"), line_of(node, "end")) else {
        return false;
    };
    start <= range.end.line && range.start.line <= end
}

/// The one-based line of a node's range end `which` (`start` or `end`).
fn line_of(node: &Value, which: &str) -> Option<u32> {
    Some(node.pointer(&format!("/range/{which}/line"))?.as_u64()? as u32 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_bare_self_type_of_an_impl_label() {
        assert_eq!(self_type_of("impl Host"), "Host");
        assert_eq!(self_type_of("impl<T> Wrapper<T>"), "Wrapper");
        assert_eq!(self_type_of("impl Display for Host"), "Host");
    }

    #[test]
    fn refuses_two_impls_the_range_spans() {
        // Given two `impl Host` blocks, lines 1–3 and 5–7
        let symbols = json!([
            { "name": "impl Host", "kind": 19,
              "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 2, "character": 1 } },
              "children": [] },
            { "name": "impl Host", "kind": 19,
              "range": { "start": { "line": 4, "character": 0 }, "end": { "line": 6, "character": 1 } },
              "children": [] },
        ]);
        let range = Range {
            start: crate::edit::Position { line: 1, col: 1 },
            end: crate::edit::Position { line: 7, col: 1 },
        };

        let refused = read(&symbols, "", range);

        assert!(refused
            .map(|_| ())
            .map_err(|error| error.to_string())
            .unwrap_err()
            .contains("sit in more than one `impl` block"));
    }
}
