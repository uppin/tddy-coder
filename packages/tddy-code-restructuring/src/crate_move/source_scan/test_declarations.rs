//! The `#[cfg(test)] mod t;` declarations a module's text holds at its top level.
//!
//! A cross-crate move takes along a test module declared *beside* the module it moves when that
//! test module tests only moved code (`#reshape` 13/24). Finding the candidates is lexical: a
//! file-backed `mod` (not an inline block) whose attributes include the `cfg(test)` marker every
//! other `in_test` decision reads — `#[cfg(test)]` or `#[cfg(all(test, …))]`. The span covers the
//! declaration together with the attributes and doc comments directly above it, because that is
//! what leaves the origin and what the destination root receives.

use std::ops::Range;

use super::{attached_trivia_starts_at, Kind, Scan};

/// One top-level `#[cfg(test)] mod name;` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TestDeclaration {
    /// The identifier `mod` declares.
    pub(crate) name: String,
    /// The bytes of the declaration with the attributes and doc comments directly above it, from
    /// the start of the first of their lines to the end of the `mod` line, its newline included.
    pub(crate) span: Range<usize>,
    /// Whether one of its attributes is `#[path …]`, which places the file somewhere a move cannot
    /// follow.
    pub(crate) placed_by_path: bool,
}

/// Every top-level `#[cfg(test)]` / `#[cfg(all(test, …))]` declaration of a file-backed module in
/// `text`, in source order. `#[cfg(any(test, …))]`, a `cfg_attr` and an inline
/// `mod tests { … }` are not test declarations here.
pub(crate) fn test_declarations(text: &str) -> Vec<TestDeclaration> {
    let scan = Scan::of(text);
    let mut found = Vec::new();
    let mut pending = Pending::default();
    let mut depth = 0usize;
    let mut at = 0usize;

    while at < scan.tokens.len() {
        match scan.tokens[at].kind {
            Kind::Punct(b'#') if depth == 0 && scan.is_punct(at + 1, b'[') => {
                if let Some(length) = scan.cfg_test_attribute(at) {
                    pending.test = true;
                    at += length;
                    continue;
                }
                pending.placed_by_path |= scan.ident(at + 2) == Some("path");
                at = after_attribute(&scan, at + 1);
                continue;
            }
            Kind::Punct(b'{') => depth += 1,
            Kind::Punct(b'}') => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    pending = Pending::default();
                }
            }
            Kind::Punct(b';') if depth == 0 => pending = Pending::default(),
            Kind::Ident if depth == 0 && scan.text(scan.tokens[at]) == "mod" => {
                if let (Some(name), true) = (scan.ident(at + 1), scan.is_punct(at + 2, b';')) {
                    if pending.test {
                        found.push(declaration_at(
                            text,
                            &scan,
                            at,
                            name,
                            pending.placed_by_path,
                        ));
                    }
                    pending = Pending::default();
                    at += 3;
                    continue;
                }
            }
            _ => {}
        }
        at += 1;
    }
    found
}

/// The attributes read so far that belong to the next item at the top level.
#[derive(Debug, Default)]
struct Pending {
    test: bool,
    placed_by_path: bool,
}

/// The index after the `]` closing the attribute whose `[` is at token `open`.
fn after_attribute(scan: &Scan<'_>, open: usize) -> usize {
    let mut depth = 0usize;
    for index in open..scan.tokens.len() {
        match scan.kind(index) {
            Some(Kind::Punct(b'[')) => depth += 1,
            Some(Kind::Punct(b']')) => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
    }
    scan.tokens.len()
}

/// The declaration whose `mod` keyword is at token `keyword`, with the lines above it that leave
/// with it, and its `;` two tokens on.
fn declaration_at(
    text: &str,
    scan: &Scan<'_>,
    keyword: usize,
    name: &str,
    placed_by_path: bool,
) -> TestDeclaration {
    let line = text[..scan.tokens[keyword].start].matches('\n').count() as u32 + 1;
    let start = line_start(text, attached_trivia_starts_at(text, line));
    let semicolon_end = scan.tokens[keyword + 2].end;
    let end = text[semicolon_end..]
        .find('\n')
        .map_or(text.len(), |newline| semicolon_end + newline + 1);
    TestDeclaration {
        name: name.to_string(),
        span: start..end,
        placed_by_path,
    }
}

/// The byte offset where the one-based `line` begins.
fn line_start(text: &str, line: u32) -> usize {
    text.split_inclusive('\n')
        .take(line as usize - 1)
        .map(str::len)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_PARENT: &str = "pub mod a;\n\n/// Tests `a` alone.\n/// PRD: none.\n#[cfg(test)]\n#[allow(unused)]\nmod a_tests;\n\n#[cfg(all(test, unix))]\nmod unix_tests;\n";

    #[test]
    fn reads_a_cfg_test_declaration_with_its_doc_comment_and_attributes_as_one_span() {
        let found = test_declarations(A_PARENT);

        let spans: Vec<(&str, &str, bool)> = found
            .iter()
            .map(|declaration| {
                (
                    declaration.name.as_str(),
                    &A_PARENT[declaration.span.clone()],
                    declaration.placed_by_path,
                )
            })
            .collect();
        assert_eq!(
            spans,
            [
                (
                    "a_tests",
                    "/// Tests `a` alone.\n/// PRD: none.\n#[cfg(test)]\n#[allow(unused)]\nmod a_tests;\n",
                    false
                ),
                ("unix_tests", "#[cfg(all(test, unix))]\nmod unix_tests;\n", false),
            ]
        );
    }

    #[test]
    fn a_cfg_any_test_or_an_inline_test_module_is_not_a_test_declaration() {
        let text = "#[cfg(any(test, unix))]\nmod either_tests;\n\n#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n\nmod plain_tests;\n";

        assert_eq!(test_declarations(text), Vec::new());
    }

    #[test]
    fn a_test_declaration_placed_with_a_path_attribute_says_so() {
        let text = "#[cfg(test)]\n#[path = \"elsewhere.rs\"]\nmod placed_tests;\n";

        assert_eq!(
            test_declarations(text)
                .iter()
                .map(|declaration| (declaration.name.as_str(), declaration.placed_by_path))
                .collect::<Vec<_>>(),
            [("placed_tests", true)]
        );
    }
}
