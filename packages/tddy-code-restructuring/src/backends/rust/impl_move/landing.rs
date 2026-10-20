//! Where moved members land in the destination: the one block of the same header they join, or a
//! new block.
//!
//! Rule R3: the destination's scope (inline modules included) is read for inherent `impl` blocks
//! whose header — attributes, generics, self type and `where` clause — is token-equal to the
//! origin's. Exactly one such block is joined; none, or more than one, opens a new block. A block
//! written on one line has no line to insert into, so it is never joined (a new block is opened).

use std::ops::Range;

use super::super::early_return::masked_to_code;
use super::super::item_move::placement;
use super::super::item_move::text::{depth_at, Edit};
use crate::crate_move::readable_spans;
use crate::Result;

/// Where the members go in the destination's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::backends::rust) enum Landing {
    /// Into the one block of the same header: inserted at byte `before`, the start of the line
    /// holding its closing `}`.
    Join { before: usize },
    /// Into a new block, placed by `item_move::placement::insertion`.
    New,
}

/// An inherent `impl` block of the destination module, as far as landing needs it.
struct Block {
    /// The attributes and the header, token by token.
    header: Vec<String>,
    /// Whether the block's `{` and `}` share a line.
    one_line: bool,
    /// The byte offset of the start of the line holding the closing `}`.
    close_line_start: usize,
}

/// The origin block the members leave, as the landing needs it: its attributes and header as written,
/// and the indentation of its header line.
pub(super) struct Origin<'a> {
    pub(super) header: &'a str,
    pub(super) indent: &'a str,
}

/// The edit that places the moved `members` in the module spanning `scope` of `text`, and which
/// landing it is.
pub(super) fn placed(
    text: &str,
    scope: &Range<usize>,
    origin: &Origin<'_>,
    members: &str,
) -> Result<(Edit, Landing)> {
    match block_for(text, scope, origin.header) {
        Landing::Join { before } => Ok((
            joined(text, before, members, origin.indent),
            Landing::Join { before },
        )),
        Landing::New => {
            let block = opened(origin, members);
            Ok((placement::insertion(text, scope, &block)?, Landing::New))
        }
    }
}

/// The destination's edits for the members that land, with the `use` items they need: where both
/// land at one offset (the top of a file with no items), one insert, in the order they were made.
pub(super) fn destination_edits(imports: Option<Edit>, landed: Edit) -> Vec<Edit> {
    match imports {
        Some(imports) if imports.start == landed.start => {
            vec![Edit::insert(landed.start, imports.text + &landed.text)]
        }
        Some(imports) => vec![imports, landed],
        None => vec![landed],
    }
}

/// The block joins above `before`, the line holding its closing brace: after a blank line unless
/// the block is empty. The members are indented to the block's own header.
fn joined(text: &str, before: usize, members: &str, origin_indent: &str) -> Edit {
    let closing_indent: String = text[before..]
        .chars()
        .take_while(|character| *character == ' ' || *character == '\t')
        .collect();
    let previous = match before {
        0 => "",
        _ => &text[line_start_of(text, before - 1)..before],
    };
    let lead = if previous.trim().is_empty() || previous.trim_end().ends_with('{') {
        ""
    } else {
        "\n"
    };
    let body = reindent(members, origin_indent, &closing_indent);
    Edit::insert(before, format!("{lead}{}", with_newline(&body)))
}

/// A new block: the origin's attributes and header as written, the members, and the closing brace,
/// at column zero for `placement` to indent.
fn opened(origin: &Origin<'_>, members: &str) -> String {
    let header = if origin.header.ends_with('\n') {
        origin.header.to_string()
    } else {
        format!("{} ", origin.header.trim_end())
    };
    let block = format!("{header}{{\n{}}}\n", with_newline(members));
    reindent(&block, origin.indent, "")
}

/// `text` with its indentation `from` replaced by `to` on every line that carries it. A line that
/// does not carry `from` is left as it is, and so is a line that begins inside a multi-line string
/// literal, whose bytes are data.
fn reindent(text: &str, from: &str, to: &str) -> String {
    if from == to {
        return text.to_string();
    }
    let literals = literal_gaps(text);
    let mut at = 0usize;
    text.split_inclusive('\n')
        .map(|line| {
            let start = at;
            at += line.len();
            let inside_a_literal = literals
                .iter()
                .any(|gap| gap.start < start && start < gap.end);
            match line.strip_prefix(from) {
                Some(rest) if !inside_a_literal && !line.trim().is_empty() => {
                    format!("{to}{rest}")
                }
                _ => line.to_string(),
            }
        })
        .collect()
}

/// The stretches of `text` that are neither code nor a comment: its string literals.
fn literal_gaps(text: &str) -> Vec<Range<usize>> {
    let mut gaps = Vec::new();
    let mut from = 0usize;
    for (span, _) in readable_spans(text) {
        if span.start > from {
            gaps.push(from..span.start);
        }
        from = from.max(span.end);
    }
    if from < text.len() {
        gaps.push(from..text.len());
    }
    gaps
}

fn with_newline(text: &str) -> String {
    if text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{text}\n")
    }
}

/// The landing of members whose block is written `header` (its attributes and its header, as
/// written) in the module spanning `scope` of `destination_text`.
pub(in crate::backends::rust) fn block_for(
    destination_text: &str,
    scope: &Range<usize>,
    header: &str,
) -> Landing {
    let wanted = tokens_of(header);
    let mut same = inherent_blocks_in(destination_text, scope)
        .into_iter()
        .filter(|block| block.header == wanted);
    match (same.next(), same.next()) {
        (Some(block), None) if !block.one_line => Landing::Join {
            before: block.close_line_start,
        },
        _ => Landing::New,
    }
}

/// The inherent `impl` blocks that are items of the module spanning `scope` (not of a module or a
/// function nested in it), trait impls left out.
fn inherent_blocks_in(text: &str, scope: &Range<usize>) -> Vec<Block> {
    let masked = masked_to_code(text);
    let base = depth_at(&masked, scope.start);
    let candidates = masked
        .get(scope.clone())
        .into_iter()
        .flat_map(|scoped| scoped.match_indices("impl"))
        .map(|(offset, _)| scope.start + offset);
    candidates
        .filter(|&at| is_keyword_at(&masked, at))
        .filter(|&at| depth_at(&masked, at) == base && starts_an_item(&masked, at))
        .filter_map(|at| block_at(text, &masked, at))
        .collect()
}

/// Whether the `impl` at `at` is the whole word, not part of an identifier.
fn is_keyword_at(masked: &str, at: usize) -> bool {
    let bytes = masked.as_bytes();
    let before_is_word = at > 0 && is_word_byte(bytes[at - 1]);
    let after_is_word = bytes
        .get(at + "impl".len())
        .is_some_and(|&byte| is_word_byte(byte));
    !before_is_word && !after_is_word
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Whether an `impl` at `at` opens an item: what precedes it ends one (a block, a statement or an
/// attribute), so an `impl Trait` in a signature is not taken for one.
fn starts_an_item(masked: &str, at: usize) -> bool {
    masked[..at]
        .trim_end()
        .bytes()
        .last()
        .is_none_or(|byte| matches!(byte, b'}' | b';' | b']'))
}

/// The block whose `impl` keyword is at `at`, or `None` for a trait impl or a declaration without a
/// body.
fn block_at(text: &str, masked: &str, at: usize) -> Option<Block> {
    let open = at + masked[at..].find(['{', ';'])?;
    if masked.as_bytes()[open] == b';' {
        return None;
    }
    let close = matching_brace(masked, open)?;
    let start = attributes_start(text, at);
    let header = tokens_of(&text[start..open]);
    if is_trait_impl(&tokens_of(&text[at..open])) {
        return None;
    }
    Some(Block {
        header,
        one_line: !text[open..close].contains('\n'),
        close_line_start: line_start_of(text, close),
    })
}

/// The offset of the `}` that closes the `{` at `open`.
fn matching_brace(masked: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in masked.bytes().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// The start of the first attribute line directly above the line holding `at`, or of that line.
pub(super) fn attributes_start(text: &str, at: usize) -> usize {
    let mut start = line_start_of(text, at);
    while start > 0 {
        let previous = line_start_of(text, start - 1);
        if !text[previous..start].trim_start().starts_with("#[") {
            break;
        }
        start = previous;
    }
    start
}

/// The offset of the start of the line holding `offset`.
pub(super) fn line_start_of(text: &str, offset: usize) -> usize {
    text[..offset].rfind('\n').map_or(0, |newline| newline + 1)
}

/// Whether a header names a trait: a `for` outside its generic arguments (`impl Display for Host`).
fn is_trait_impl(tokens: &[String]) -> bool {
    let mut angles = 0usize;
    let mut previous = "";
    for token in tokens {
        match token.as_str() {
            "where" => return false,
            "<" => angles += 1,
            // The `>` of `->` closes nothing.
            ">" if previous != "-" => angles = angles.saturating_sub(1),
            "for" if angles == 0 => return true,
            _ => {}
        }
        previous = token;
    }
    false
}

/// The tokens of a header: identifiers and lifetimes whole, every other visible character alone.
/// Two headers that differ only in spacing (`T:Clone` and `T: Clone`) have the same tokens.
fn tokens_of(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    for character in text.chars() {
        if character.is_alphanumeric() || character == '_' || character == '\'' {
            word.push(character);
            continue;
        }
        if !word.is_empty() {
            tokens.push(std::mem::take(&mut word));
        }
        if !character.is_whitespace() {
            tokens.push(character.to_string());
        }
    }
    if !word.is_empty() {
        tokens.push(word);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::reindent;

    #[test]
    fn reindent_leaves_a_line_of_a_multi_line_string_literal_alone() {
        // Given members one level deep, one of which holds a literal whose second line starts with
        // the same indentation
        let members = concat!(
            "    fn banner() -> &'static str {\n",
            "        \"first\n",
            "    second\"\n",
            "    }\n",
        );

        // When the level is taken off
        let reindented = reindent(members, "    ", "");

        // Then every line of code moves out a level and the literal's bytes are unchanged
        assert_eq!(
            reindented,
            concat!(
                "fn banner() -> &'static str {\n",
                "    \"first\n",
                "    second\"\n",
                "}\n",
            )
        );
    }
}
