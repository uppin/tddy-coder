//! A `super::` or `self::` path in moved code keeps its meaning one module deeper.
//!
//! `extract_module` relocates items into a new child of the module that held them. The imports
//! pass rebases the `use` declarations; the paths written inline in the moved code — calls, types,
//! struct literals, patterns — are rooted at the module the code was written in, and one level
//! down `super::f()` names that module rather than its parent. rust-analyzer's assist rewrites
//! none of them.
//!
//! Pure text, over the body of the new module. Only code is read: strings, characters and
//! comments are masked, and a `use` item is left to the imports pass.

use super::early_return::masked_to_code;
use super::{module_bounds, Result};

/// `text` with the inline paths of its `module` block re-rooted for the module being one level
/// deeper than the code was written, and how many paths that was.
pub(super) fn rerooted_module(text: &str, module: &str) -> Result<(String, usize)> {
    let mut source: Vec<String> = text.split('\n').map(str::to_string).collect();
    let block = module_bounds(&source, module)?;
    let body = source[block.opened + 1..block.closed].join("\n");

    let (rerooted, count) = rerooted_for_child(&body);
    source.splice(
        block.opened + 1..block.closed,
        rerooted.split('\n').map(str::to_string),
    );
    Ok((source.join("\n"), count))
}

/// What the operator is told when paths were rewritten, and nothing when none were.
pub(super) fn note(count: usize) -> Option<String> {
    (count > 0).then(|| format!("paths: {count} super:: path(s) re-rooted for the new module"))
}

/// `body` with every path that leaves the moved module re-rooted for it, and how many were.
///
/// A path that stays inside a module the body declares is untouched: that module moves with it, so
/// `super::` there still names the same place.
pub(super) fn rerooted_for_child(body: &str) -> (String, usize) {
    let masked = masked_to_code(body);
    let mut skipped = use_items(&masked);
    skipped.extend(visibility_scopes(&masked));
    let depths = module_depths(&masked);

    let mut edits: Vec<Edit> = Vec::new();
    let mut at = 0;
    while at < masked.len() {
        let edit = path_start(&masked, at)
            .filter(|_| !skipped.iter().any(|&(from, to)| (from..to).contains(&at)))
            .and_then(|root| edit_for(&masked, at, root, depths[at]));
        at += 1;
        edits.extend(edit);
    }

    let mut rerooted = body.to_string();
    for edit in edits.iter().rev() {
        rerooted.replace_range(edit.at..edit.at + edit.replaces, edit.with);
    }
    (rerooted, edits.len())
}

struct Edit {
    at: usize,
    replaces: usize,
    with: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
enum Root {
    Super,
    SelfModule,
}

/// Which relative root a path begins with at `at`: a whole `super` or `self` identifier, not the
/// tail of a longer path (`foo::super`) or of a longer name, and followed by `::`.
fn path_start(masked: &str, at: usize) -> Option<Root> {
    let bytes = masked.as_bytes();
    let continues_a_path = at >= 2 && &bytes[at - 2..at] == b"::";
    let continues_a_name = at > 0 && is_ident_byte(bytes[at - 1]);
    if continues_a_path || continues_a_name {
        return None;
    }

    let rest = &bytes[at..];
    if rest.starts_with(b"super::") {
        Some(Root::Super)
    } else if rest.starts_with(b"self::") {
        Some(Root::SelfModule)
    } else {
        None
    }
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The rewrite of the path at `at`, written inside `depth` modules the body itself declares.
fn edit_for(masked: &str, at: usize, root: Root, depth: usize) -> Option<Edit> {
    match root {
        Root::SelfModule => (depth == 0).then_some(Edit {
            at,
            replaces: "self".len(),
            with: "super",
        }),
        Root::Super => {
            let leading = masked[at..]
                .split("::")
                .take_while(|segment| *segment == "super")
                .count();
            (leading > depth).then_some(Edit {
                at,
                replaces: 0,
                with: "super::",
            })
        }
    }
}

/// The byte ranges of `pub(in path)` qualifiers, which the visibility pass owns.
fn visibility_scopes(masked: &str) -> Vec<(usize, usize)> {
    masked
        .match_indices("pub(in ")
        .map(|(from, _)| {
            (
                from,
                masked[from..]
                    .find(')')
                    .map_or(masked.len(), |end| from + end),
            )
        })
        .collect()
}

/// The byte ranges of `use` items, from the keyword to the `;` that ends it.
fn use_items(masked: &str) -> Vec<(usize, usize)> {
    let mut items = Vec::new();
    let mut line_start = 0;
    for line in masked.split('\n') {
        if starts_a_use_item(line) {
            let to = masked[line_start..]
                .find(';')
                .map_or(masked.len(), |end| line_start + end + 1);
            items.push((line_start, to));
        }
        line_start += line.len() + 1;
    }
    items
}

fn starts_a_use_item(line: &str) -> bool {
    let rest = line.trim_start();
    let rest = rest.strip_prefix("pub").map_or(rest, |after| {
        let after = after.trim_start();
        match after
            .strip_prefix('(')
            .and_then(|inner| inner.split_once(')'))
        {
            Some((_, tail)) => tail.trim_start(),
            None => after,
        }
    });
    rest.starts_with("use ") || rest.starts_with("use{")
}

/// For each byte, how many `mod name { … }` blocks of the body enclose it.
fn module_depths(masked: &str) -> Vec<usize> {
    let mut stack: Vec<bool> = Vec::new();
    let mut depths = Vec::with_capacity(masked.len());
    for (at, byte) in masked.bytes().enumerate() {
        match byte {
            b'{' => stack.push(opens_a_module(&masked.as_bytes()[..at])),
            b'}' => {
                stack.pop();
            }
            _ => {}
        }
        depths.push(stack.iter().filter(|module| **module).count());
    }
    depths
}

/// Whether the brace that follows `before` opens `mod name {`.
fn opens_a_module(before: &[u8]) -> bool {
    let name_end = before.trim_ascii_end();
    let name_start = name_end
        .iter()
        .rposition(|byte| !is_ident_byte(*byte))
        .map_or(0, |at| at + 1);
    let keyword_end = name_end[..name_start].trim_ascii_end();
    let is_whole_word = keyword_end.len() < name_start;
    let keyword_start = keyword_end.len().saturating_sub("mod".len());

    is_whole_word
        && keyword_end.ends_with(b"mod")
        && keyword_start
            .checked_sub(1)
            .is_none_or(|at| !is_ident_byte(keyword_end[at]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rerooted(text: &str) -> String {
        rerooted_for_child(text).0
    }

    #[test]
    fn puts_one_more_super_in_front_of_a_super_path() {
        assert_eq!(
            rerooted("let a = super::f(x);"),
            "let a = super::super::f(x);"
        );
        assert_eq!(rerooted("super::super::g"), "super::super::super::g");
    }

    #[test]
    fn turns_a_self_path_into_a_super_path() {
        assert_eq!(rerooted("self::h()"), "super::h()");
    }

    #[test]
    fn leaves_a_receiver_and_the_other_roots_alone() {
        for text in [
            "self.field",
            "fn f(&self) {}",
            "g(self, 1)",
            "Self::new()",
            "crate::a::b",
            "::std::fmt::Display",
            "std::fmt::Display",
        ] {
            assert_eq!(rerooted(text), text);
        }
    }

    #[test]
    fn leaves_strings_and_comments_alone() {
        for text in [
            "let s = \"super::x\";",
            "// super::x",
            "/// [`f`](super::f)",
            "/* super::x */ let a = 1;",
            "#[path = \"super::x\"]",
        ] {
            assert_eq!(rerooted(text), text);
        }
    }

    #[test]
    fn leaves_use_items_to_the_imports_pass() {
        for text in [
            "use super::x;",
            "pub use super::x;",
            "pub(crate) use super::x;",
            "use super::{a,\n    b};",
            "    use self::x;",
        ] {
            assert_eq!(rerooted(text), text);
        }
    }

    #[test]
    fn leaves_a_visibility_scope_to_the_visibility_pass() {
        let text = "pub(in super::super) fn f() { super::g(); }\n    pub(in self::x) b: u32,";

        assert_eq!(
            rerooted(text),
            "pub(in super::super) fn f() { super::super::g(); }\n    pub(in self::x) b: u32,"
        );
    }

    #[test]
    fn matches_a_whole_identifier_at_the_start_of_a_path_only() {
        for text in [
            "a_super::x",
            "my::super_thing",
            "foo::super::x",
            "supers::x",
        ] {
            assert_eq!(rerooted(text), text);
        }
    }

    #[test]
    fn rewrites_a_struct_literal_a_type_a_turbofish_and_a_pattern() {
        assert_eq!(
            rerooted("let c = super::Config { a: 1 };"),
            "let c = super::super::Config { a: 1 };"
        );
        assert_eq!(
            rerooted("let v: Vec<super::Item> = x;"),
            "let v: Vec<super::super::Item> = x;"
        );
        assert_eq!(
            rerooted("super::parse::<u8>(s)"),
            "super::super::parse::<u8>(s)"
        );
        assert_eq!(
            rerooted("match k { super::Kind::A => 1, _ => 2 }"),
            "match k { super::super::Kind::A => 1, _ => 2 }"
        );
    }

    #[test]
    fn reads_code_around_non_ascii_text_and_a_module_named_like_a_keyword() {
        let text = "fn e() { let m = \"é\"; super::f(); } // é\nfn modem() { super::g(); }\n";

        assert_eq!(
            rerooted(text),
            "fn e() { let m = \"é\"; super::super::f(); } // é\nfn modem() { super::super::g(); }\n"
        );
    }

    #[test]
    fn counts_what_it_rewrote() {
        assert_eq!(
            rerooted_for_child("super::a(); self::b(); crate::c();").1,
            2
        );
    }

    #[test]
    fn leaves_what_stays_inside_a_module_the_body_declares() {
        let text = "mod deep {\n    fn f() { super::g(); self::h(); super::super::k(); }\n}\n";

        assert_eq!(
            rerooted(text),
            "mod deep {\n    fn f() { super::g(); self::h(); super::super::super::k(); }\n}\n"
        );
    }

    #[test]
    fn keeps_everything_else_byte_identical() {
        let text = "// head super::a\npub(crate) fn go(&self) -> u32 {\n    let s = \"self::x\";\n\n    super::run(self.n) + self::own()  // tail\n}\n";

        assert_eq!(
            rerooted(text),
            "// head super::a\npub(crate) fn go(&self) -> u32 {\n    let s = \"self::x\";\n\n    super::super::run(self.n) + super::own()  // tail\n}\n"
        );
    }
}
