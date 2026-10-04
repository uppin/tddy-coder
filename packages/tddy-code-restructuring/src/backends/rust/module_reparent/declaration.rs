//! Reading a `mod name;` declaration out of its parent's text, and finding where one belongs.
//!
//! The declaration is read as whole lines — the attributes and doc comments above it, its own
//! visibility and the `;` — because that is what travels: the same bytes are written into the new
//! parent, with only the visibility respelled when the move changes what it must say.

use std::ops::Range;

use serde_json::{json, Value};

use super::super::early_return::masked_to_code;
use super::super::item_move::text::{depth_at, is_identifier_byte, line_start, next_line_start};
use super::super::{attached_trivia_starts_at, seam_refusal, visibility_in};
use crate::crate_move::source_scan::items_of_module;
use crate::Result;

/// A module declaration of a parent module.
#[derive(Debug, Clone)]
pub(super) struct Declaration {
    /// The whole lines holding it: attributes and doc comments above, the `;` and its line end. For
    /// a module with a body, the lines up to the one it opens on.
    pub(super) lines: Range<usize>,
    /// The visibility as written, empty for private.
    pub(super) visibility: String,
    /// From the visibility (or, with none, the keyword) up to the keyword: what a respelled
    /// visibility replaces.
    pub(super) visibility_span: Range<usize>,
    /// Whether the module has a body in braces rather than a file of its own.
    pub(super) inline: bool,
    /// Whether an attribute above it places its file with `#[path]`.
    pub(super) placed_by_path: bool,
    /// Zero-based line and the byte offset in it where the name starts, as a server counts them.
    line: usize,
    character: usize,
}

impl Declaration {
    /// Where the server is asked about the module: the name in the declaration.
    pub(super) fn name_position(&self) -> Value {
        json!({ "line": self.line, "character": self.character })
    }

    /// The declaration's lines with its visibility written as `visibility`.
    pub(super) fn written_with(&self, text: &str, visibility: &str) -> String {
        let lines = &text[self.lines.clone()];
        let span = self.visibility_span.start - self.lines.start
            ..self.visibility_span.end - self.lines.start;
        let respelled = if visibility.is_empty() {
            String::new()
        } else {
            format!("{visibility} ")
        };
        format!("{}{respelled}{}", &lines[..span.start], &lines[span.end..])
    }
}

/// Where the `mod` keyword of a declaration stands, and its name and its terminator.
struct Keyword {
    at: usize,
    name_at: usize,
    /// The `;` of `mod name;` or the `{` of `mod name {`.
    terminator_at: usize,
}

/// The declaration of the module `name` among the items of the module spanning `scope` of `text`.
///
/// `None` when the module declares no such child. Refused when it is declared on a line shared with
/// other code, because moving whole lines would take that code along.
pub(super) fn find(text: &str, scope: &Range<usize>, name: &str) -> Result<Option<Declaration>> {
    let masked = masked_to_code(text);
    let Some(keyword) = keyword_of(&masked, scope, name) else {
        return Ok(None);
    };
    let inline = masked.as_bytes()[keyword.terminator_at] == b'{';

    let line_number = text[..keyword.at].matches('\n').count() as u32 + 1;
    let keyword_line = line_start(text, line_number);
    let prefix = &text[keyword_line..keyword.at];
    let visibility = visibility_in(prefix);
    let lead = prefix.len() - prefix.trim_start().len();
    let others_on_the_line = |written: &str| {
        seam_refusal(format!(
            "`mod {name}` shares its line with {written}, so its lines cannot move on their own: \
             put the declaration on a line of its own and plan again"
        ))
    };
    if !prefix.trim_start()[visibility.len()..].trim().is_empty() {
        return Err(others_on_the_line("code before it"));
    }
    let end = next_line_start(text, keyword.terminator_at);
    if !inline && !text[keyword.terminator_at + 1..end].trim().is_empty() {
        return Err(others_on_the_line("code after it"));
    }

    let start = line_start(text, attached_trivia_starts_at(text, line_number));
    Ok(Some(Declaration {
        lines: start..end,
        visibility_span: if visibility.is_empty() {
            keyword.at..keyword.at
        } else {
            keyword_line + lead..keyword.at
        },
        visibility,
        inline,
        placed_by_path: has_path_attribute(&masked[start..keyword_line]),
        line: line_number as usize - 1,
        character: keyword.name_at - keyword_line,
    }))
}

/// The `mod` keyword that declares `name` at the top level of `scope`.
fn keyword_of(masked: &str, scope: &Range<usize>, name: &str) -> Option<Keyword> {
    let base = depth_at(masked, scope.start);
    masked[scope.clone()]
        .match_indices("mod")
        .map(|(offset, _)| scope.start + offset)
        .find_map(|at| {
            if at > 0 && is_identifier_byte(masked.as_bytes()[at - 1]) {
                return None;
            }
            let after = &masked[at + "mod".len()..scope.end];
            let rest = after.trim_start();
            let tail = rest.strip_prefix(name)?;
            let whole = !tail.bytes().next().is_some_and(is_identifier_byte);
            let terminator = tail.trim_start();
            let ends_it = terminator.starts_with(';') || terminator.starts_with('{');
            let declares = rest.len() < after.len() && whole && ends_it;
            (declares && depth_at(masked, at) == base).then(|| Keyword {
                at,
                name_at: scope.end - rest.len(),
                terminator_at: scope.end - terminator.len(),
            })
        })
}

/// Whether the attributes in `block` place a module's file with `#[path = "…"]`.
pub(super) fn has_path_attribute(block: &str) -> bool {
    block.match_indices("#[").any(|(at, _)| {
        let attribute = block[at + 2..].trim_start();
        attribute
            .strip_prefix("path")
            .is_some_and(|rest| !rest.bytes().next().is_some_and(is_identifier_byte))
    })
}

/// Where a new declaration goes in the module spanning `scope` of `text`, and the text to write:
/// below the last declaration of a module with a file of its own, else at the head of the module
/// under its inner docs, with a blank line after it.
pub(super) fn insertion(text: &str, scope: &Range<usize>, declaration: &str) -> (usize, String) {
    let last = items_of_module(&text[scope.clone()])
        .children
        .iter()
        .filter_map(|child| find(text, scope, &child.name).ok().flatten())
        .filter(|found| !found.inline)
        .map(|found| found.lines.end)
        .max();
    if let Some(end) = last {
        return (end, declaration.to_string());
    }

    let mut offset = scope.start;
    for line in text[scope.clone()].split_inclusive('\n') {
        let trimmed = line.trim();
        if !(trimmed.is_empty() || trimmed.starts_with("//!") || trimmed.starts_with("#![")) {
            break;
        }
        offset += line.len();
    }
    let blank = if offset < text.len() { "\n" } else { "" };
    (offset, format!("{declaration}{blank}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(text: &str, name: &str) -> Declaration {
        find(text, &(0..text.len()), name).unwrap().unwrap()
    }

    #[test]
    fn takes_the_attributes_and_the_visibility_with_the_declaration() {
        let text = "mod a;\n\n/// Docs.\n#[allow(dead_code)]\npub(crate) mod b;\n\nfn f() {}\n";

        let found = declared(text, "b");

        assert_eq!(
            &text[found.lines.clone()],
            "/// Docs.\n#[allow(dead_code)]\npub(crate) mod b;\n"
        );
        assert_eq!(found.visibility, "pub(crate)");
    }

    #[test]
    fn respells_only_the_visibility() {
        let text = "#[cfg(unix)]\npub mod b;\n";

        assert_eq!(
            declared(text, "b").written_with(text, "pub(crate)"),
            "#[cfg(unix)]\npub(crate) mod b;\n"
        );
        assert_eq!(
            declared(text, "b").written_with(text, ""),
            "#[cfg(unix)]\nmod b;\n"
        );
    }

    #[test]
    fn writes_a_visibility_in_front_of_a_private_declaration() {
        let text = "mod b;\n";

        assert_eq!(
            declared(text, "b").written_with(text, "pub"),
            "pub mod b;\n"
        );
    }

    #[test]
    fn finds_the_name_as_a_server_counts_it() {
        let text = "mod a;\npub mod b;\n";

        assert_eq!(
            declared(text, "b").name_position(),
            json!({ "line": 1, "character": 8 })
        );
    }

    #[test]
    fn does_not_mistake_a_longer_name_or_a_nested_module_for_the_declaration() {
        let text = "mod ab;\nmod outer {\n    mod b;\n}\n";

        assert!(find(text, &(0..text.len()), "b").unwrap().is_none());
    }

    #[test]
    fn knows_a_module_with_a_body_from_one_with_a_file() {
        let text = "mod a {\n}\nmod b;\n";

        assert!(declared(text, "a").inline);
        assert!(!declared(text, "b").inline);
    }

    #[test]
    fn reads_a_path_attribute_and_not_a_longer_one() {
        let text = "#[path = \"x.rs\"]\nmod a;\n#[pathological]\nmod b;\n";

        assert!(declared(text, "a").placed_by_path);
        assert!(!declared(text, "b").placed_by_path);
    }

    #[test]
    fn refuses_a_declaration_that_shares_its_line() {
        let text = "mod a; fn f() {}\n";

        assert!(find(text, &(0..text.len()), "a").is_err());
    }

    #[test]
    fn places_a_new_declaration_below_the_last_one() {
        let text = "pub mod a;\nmod b;\n\nuse x::y;\n";

        let (at, written) = insertion(text, &(0..text.len()), "mod c;\n");

        assert_eq!(
            (&text[..at], written.as_str()),
            ("pub mod a;\nmod b;\n", "mod c;\n")
        );
    }

    #[test]
    fn places_the_first_declaration_under_the_inner_docs_with_a_blank_line_after() {
        let text = "//! Docs.\n\nuse x::y;\n";

        let (at, written) = insertion(text, &(0..text.len()), "mod c;\n");

        assert_eq!(&text[at..], "use x::y;\n");
        assert_eq!(written, "mod c;\n\n");
    }
}
