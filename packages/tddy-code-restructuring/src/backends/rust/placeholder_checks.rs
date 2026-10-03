use super::is_identifier_char;

use super::server_defect;

use crate::Result;

/// Refuse an extraction whose signature carries an inferred-type placeholder.
///
/// rust-analyzer writes the types in an extracted item's signature from its own inference. Asked
/// before inference is ready it can still produce the assist — the *shape* comes from the syntax tree
/// — and fills what it does not yet know with `_`:
///
/// ```ignore
/// fn compute_spread(sample: &Sample) -> (_, _) {
/// ```
///
/// `_` is not legal in an item signature (`E0121`), so the result compiles nowhere and the operation
/// would otherwise report success. Waiting for the anchor to resolve is what avoids this; this is the
/// post-condition that keeps a recurrence loud instead of writing a tree that cannot build.
pub(crate) fn refuse_inferred_placeholder(text: &str, declaration: &str) -> Result<()> {
    let Some(line) = text
        .split('\n')
        .find(|line| line.contains(declaration) && carries_placeholder_type(line))
    else {
        return Ok(());
    };

    Err(server_defect(format!(
        "rust-analyzer wrote `{}` — it produced the extraction before it could infer the types the \
         signature needs, and `_` is not legal there (E0121). The crate graph was most likely still \
         loading; retrying the operation against a warm server resolves it.",
        line.trim()
    )))
}

/// Whether a declaration line carries `_` where a type belongs.
///
/// Tokenised on identifier boundaries, so `fun_name` and `var_name` — which contain an underscore but
/// are not one — do not register. A `_` straight after `'` is the elided lifetime `'_`, which is legal
/// in a signature and is what rust-analyzer writes for a borrowed view (`state: RosterState<'_>`).
pub(crate) fn carries_placeholder_type(line: &str) -> bool {
    let mut previous = None;
    let mut token = String::new();
    for character in line.chars().chain(std::iter::once(' ')) {
        if is_identifier_char(character) {
            token.push(character);
            continue;
        }
        if token == "_" && previous != Some('\'') {
            return true;
        }
        previous = Some(character);
        token.clear();
    }
    false
}

/// Refuse a result the placeholder survived into.
///
/// The assist rewrites references to the items it moved as `modname::Item`, and the rename that
/// follows is asked of the server — so every reference it can resolve is renamed along with the
/// declaration. One it *cannot* resolve is left exactly as it was: from inside a different,
/// already-extracted module `modname::Item` never named anything, and rust-analyzer does not rename
/// an unresolved path. What lands is source that compiles nowhere, from an operation that reported
/// success, which is the worst outcome this tool has.
///
/// The count is compared against the text as it stood before the assist ran rather than against
/// zero, so a file that legitimately contains the identifier is not refused for containing it.
pub(crate) fn refuse_residual_placeholder(
    original: &str,
    produced: &str,
    name: &str,
) -> Result<()> {
    let before = placeholder_sites(original, name).len();
    let sites = placeholder_sites(produced, name);
    if sites.len() <= before {
        return Ok(());
    }

    let mut lines: Vec<String> = Vec::new();
    for site in &sites {
        let line = site.to_string();
        if !lines.contains(&line) {
            lines.push(line);
        }
    }

    // Two causes, and they want opposite advice. A leftover inside an already-extracted *module* is
    // an ordering mistake: extract the definition first and no reference to it is sitting in a scope
    // the rewritten path cannot reach. A module the file already had (its `mod tests`) reads the
    // same lexically and wants no reordering, so that wording names it too; the one such leftover
    // known, a call beside the module's own import of the item, is repaired before this runs
    // (`with_nested_references_restored`). A leftover inside an `impl` is not an ordering mistake
    // either, and reordering the plan provably does not help — one real split was reordered in full
    // and produced byte-identical refusals at identical offsets. An `impl` body cannot hold a
    // `mod`, so the sibling can be moved neither first nor second; only a wider seam removes the
    // reference.
    //
    // Where both occur the `impl` wording wins, because it is the one no ordering can satisfy.
    let inside_an_impl = sites
        .iter()
        .any(|site| enclosing_block(produced, *site) == Some(Block::Impl));

    let remedy = if inside_an_impl {
        "That call sits inside an `impl`, and the module was written outside it, so no ordering of \
         this plan makes the path resolve — an `impl` body cannot hold a `mod`. Either grow the \
         seam to carry the whole `impl`, or cut it where nothing crosses."
    } else {
        "That reference sits inside another module of this file. If an earlier operation of this \
         plan extracted that module, extract a definition before the items that reference it, so \
         no reference to it is sitting inside an already-extracted module when it moves. If the \
         file already had that module, such as its `mod tests`, no ordering helps: reach the item \
         there through `use super::*;`, or cut the seam where that module does not name it."
    };

    Err(server_defect(format!(
        "rust-analyzer left `{name}` behind in {} place(s) its rename could not reach, so the \
         extraction would report success over source that resolves nowhere — line(s) {}. {remedy}",
        sites.len() - before,
        lines.join(", ")
    )))
}

/// The kind of block a line sits inside, where the difference changes what a refusal should advise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Block {
    Impl,
    Module,
}

/// The innermost `impl` or `mod` a one-based line sits inside.
///
/// Lexical, and deliberately allowed to be approximate: a brace inside a string literal would mislead
/// it. That is affordable because this only ever chooses the *wording* of a refusal that has already
/// been decided — never whether to refuse. Anything finer would mean parsing, for no change in
/// outcome.
fn enclosing_block(text: &str, line: u32) -> Option<Block> {
    let mut stack: Vec<Option<Block>> = Vec::new();

    for (index, raw) in text.split('\n').enumerate() {
        if index as u32 + 1 == line {
            return stack.iter().rev().copied().flatten().next();
        }

        let code = raw.split("//").next().unwrap_or(raw);
        let mut declaration = 0usize;

        for (at, character) in code.char_indices() {
            match character {
                '{' => {
                    stack.push(declares(&code[declaration..at]));
                    declaration = at + character.len_utf8();
                }
                '}' => {
                    stack.pop();
                    declaration = at + character.len_utf8();
                }
                ';' => declaration = at + character.len_utf8(),
                _ => {}
            }
        }
    }

    None
}

/// Which block, if either, a declaration introduces. Whole tokens, so `implement` is not `impl`.
pub(crate) fn declares(head: &str) -> Option<Block> {
    head.split(|character: char| !is_identifier_char(character))
        .find_map(|token| match token {
            "impl" => Some(Block::Impl),
            "mod" => Some(Block::Module),
            _ => None,
        })
}

/// Every one-based line on which `name` occurs as a whole identifier, once per occurrence.
///
/// Whole-word, because `modname` inside `modnamed` is a different name and refusing on it would make
/// perfectly good code unrefactorable.
pub(crate) fn placeholder_sites(text: &str, name: &str) -> Vec<u32> {
    let mut sites = Vec::new();

    for (index, line) in text.split('\n').enumerate() {
        let bytes = line.as_bytes();
        let mut from = 0;
        while let Some(found) = line[from..].find(name) {
            let start = from + found;
            let end = start + name.len();
            let opens = start == 0 || !is_identifier_byte(bytes[start - 1]);
            let closes = end >= bytes.len() || !is_identifier_byte(bytes[end]);
            if opens && closes {
                sites.push(index as u32 + 1);
            }
            from = end;
        }
    }

    sites
}

pub(crate) fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
