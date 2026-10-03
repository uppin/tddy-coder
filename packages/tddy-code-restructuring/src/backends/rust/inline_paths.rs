//! A `super::` or `self::` path in moved code keeps its meaning one module deeper.
//!
//! `extract_module` relocates items into a new child of the module that held them. The imports
//! pass rebases the `use` declarations; the paths written inline in the moved code — calls, types,
//! struct literals, patterns — are rooted at the module the code was written in, and one level
//! down `super::f()` names that module rather than its parent. rust-analyzer's assist rewrites
//! none of them.
//!
//! A third case rides the same pass. A path whose first segment is a module the *parent* declares
//! (`visibility::WIDENED`, written in the parent next to `mod visibility;`) does not resolve from
//! the new module, a sibling of it: it is written `super::visibility::WIDENED`. Whether the body
//! binds that name itself cannot be told from text, so the rule is conservative — any occurrence of
//! the name that could be a binding (not a field or method after `.`, not a path segment after `::`,
//! not followed by `::`, or anything at all inside a `use` item) leaves that module's paths alone.
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

    let siblings = declared_modules(text, module);
    let (rerooted, count) = rerooted_for_child(&body, &siblings);
    source.splice(
        block.opened + 1..block.closed,
        rerooted.split('\n').map(str::to_string),
    );
    Ok((source.join("\n"), count))
}

/// What the operator is told when paths were rewritten, and nothing when none were.
pub(super) fn note(count: usize) -> Option<String> {
    (count > 0).then(|| format!("paths: {count} path(s) re-rooted for the new module"))
}

/// `body` with every path that leaves the moved module re-rooted for it, and how many were.
///
/// A path that stays inside a module the body declares is untouched: that module moves with it, so
/// `super::` there still names the same place. `siblings` are the modules the parent declares
/// beside the moved one.
pub(super) fn rerooted_for_child(body: &str, siblings: &[String]) -> (String, usize) {
    let masked = masked_to_code(body);
    let mut skipped = use_items(&masked);
    let siblings: Vec<&str> = siblings
        .iter()
        .map(String::as_str)
        .filter(|name| !may_be_bound(&masked, &skipped, name))
        .collect();
    skipped.extend(visibility_scopes(&masked));
    let depths = module_depths(&masked);

    let mut edits: Vec<Edit> = Vec::new();
    let mut at = 0;
    while at < masked.len() {
        let edit = path_start(&masked, at, &siblings)
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
    /// The first segment is a module the parent declares.
    Sibling,
}

/// Which relative root a path begins with at `at`: a whole `super` or `self` identifier, not the
/// tail of a longer path (`foo::super`) or of a longer name, and followed by `::`.
///
/// A path into one of the `siblings` starts the same way: the whole module name at a path start.
fn path_start(masked: &str, at: usize, siblings: &[&str]) -> Option<Root> {
    let bytes = masked.as_bytes();
    let continues_a_path = at >= 2 && &bytes[at - 2..at] == b"::";
    let continues_a_name = at > 0 && is_ident_byte(bytes[at - 1]);
    let is_a_member = at > 0 && bytes[at - 1] == b'.';
    if continues_a_path || continues_a_name || is_a_member {
        return None;
    }

    let rest = &bytes[at..];
    if rest.starts_with(b"super::") {
        Some(Root::Super)
    } else if rest.starts_with(b"self::") {
        Some(Root::SelfModule)
    } else if siblings.iter().any(|name| starts_a_path_in(rest, name)) {
        Some(Root::Sibling)
    } else {
        None
    }
}

fn starts_a_path_in(rest: &[u8], module: &str) -> bool {
    rest.starts_with(module.as_bytes()) && rest[module.len()..].starts_with(b"::")
}

/// A non-ASCII byte belongs to an identifier: it can only be one in code, which is all this reads.
fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || !byte.is_ascii()
}

/// Whether `name` occurs in the code of `masked` in a way that could bind it: anywhere in a `use`
/// item, or as a bare name that is neither a member (`.name`), nor a path segment (`a::name`), nor
/// the first segment of a path (`name::`).
fn may_be_bound(masked: &str, uses: &[(usize, usize)], name: &str) -> bool {
    masked.match_indices(name).any(|(at, _)| {
        let end = at + name.len();
        let bytes = masked.as_bytes();
        let whole = (at == 0 || !is_ident_byte(bytes[at - 1]))
            && bytes.get(end).is_none_or(|byte| !is_ident_byte(*byte));
        whole
            && (uses.iter().any(|&(from, to)| (from..to).contains(&at)) || is_bare(masked, at, end))
    })
}

/// Whether the name at `at..end` stands alone, as a binding or a use of one would.
fn is_bare(masked: &str, at: usize, end: usize) -> bool {
    let before = masked[..at].trim_end();
    let after = masked[end..].trim_start();
    !(before.ends_with('.') || before.ends_with("::") || after.starts_with("::"))
}

/// The rewrite of the path at `at`, written inside `depth` modules the body itself declares.
fn edit_for(masked: &str, at: usize, root: Root, depth: usize) -> Option<Edit> {
    match root {
        Root::SelfModule => (depth == 0).then_some(Edit {
            at,
            replaces: "self".len(),
            with: "super",
        }),
        Root::Sibling => (depth == 0).then_some(Edit {
            at,
            replaces: 0,
            with: "super::",
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
    let rest = without_visibility(line);
    rest.starts_with("use ") || rest.starts_with("use{")
}

/// `line` from its first token, past an optional `pub`, `pub(crate)` or `pub(in path)`.
fn without_visibility(line: &str) -> &str {
    let rest = line.trim_start();
    rest.strip_prefix("pub").map_or(rest, |after| {
        let after = after.trim_start();
        match after
            .strip_prefix('(')
            .and_then(|inner| inner.split_once(')'))
        {
            Some((_, tail)) => tail.trim_start(),
            None => after,
        }
    })
}

/// The modules `text` declares at its top level, bar `module`, the one being moved: `mod name;` and
/// `mod name {`, whatever their visibility.
fn declared_modules(text: &str, module: &str) -> Vec<String> {
    let masked = masked_to_code(text);
    let mut depth = 0usize;
    let mut declared = Vec::new();
    for line in masked.split('\n') {
        if depth == 0 {
            declared.extend(declared_module(line).filter(|name| *name != module));
        }
        depth = (depth + line.matches('{').count()).saturating_sub(line.matches('}').count());
    }
    declared.into_iter().map(str::to_string).collect()
}

/// The name `line` declares if it begins `mod name;` or `mod name {`.
fn declared_module(line: &str) -> Option<&str> {
    let named = without_visibility(line).strip_prefix("mod ")?.trim_start();
    let length = named
        .bytes()
        .take_while(|byte| is_ident_byte(*byte))
        .count();
    let after = named[length..].trim_start();
    (length > 0 && (after.starts_with(';') || after.starts_with('{'))).then(|| &named[..length])
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
        rerooted_for_child(text, &[]).0
    }

    /// `text` as the body of a new module whose parent declares `siblings`.
    fn rerooted_beside(text: &str, siblings: &[&str]) -> String {
        let declared: Vec<String> = siblings.iter().map(|name| name.to_string()).collect();
        rerooted_for_child(text, &declared).0
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
            rerooted_for_child("super::a(); self::b(); crate::c();", &[]).1,
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

    #[test]
    fn puts_super_in_front_of_a_path_into_a_module_the_parent_declares() {
        assert_eq!(
            rerooted_beside("let w = visibility::WIDENED;", &["visibility"]),
            "let w = super::visibility::WIDENED;"
        );
        assert_eq!(
            rerooted_beside("visibility::f(x) + 1", &["visibility"]),
            "super::visibility::f(x) + 1"
        );
        assert_eq!(
            rerooted_beside("let v: Vec<visibility::Item> = x;", &["visibility"]),
            "let v: Vec<super::visibility::Item> = x;"
        );
    }

    #[test]
    fn reads_a_field_named_like_the_module_as_no_binding_of_it() {
        let text = "a.filter(|m| m.visibility != visibility::WIDENED.trim())";

        assert_eq!(
            rerooted_beside(text, &["visibility"]),
            "a.filter(|m| m.visibility != super::visibility::WIDENED.trim())"
        );
    }

    #[test]
    fn leaves_the_module_alone_when_the_body_may_bind_its_name() {
        for text in [
            "use super::visibility;\nfn f() { visibility::g(); }",
            "use crate::x as visibility;\nfn f() { visibility::g(); }",
            "mod visibility { pub fn g() {} }\nfn f() { visibility::g(); }",
            "fn f() { let visibility = 1; visibility::g(); }",
            "fn f(visibility: u32) { visibility::g(); }",
        ] {
            assert_eq!(rerooted_beside(text, &["visibility"]), text);
        }
    }

    #[test]
    fn leaves_what_is_not_a_path_that_starts_with_the_module() {
        for text in [
            "x.visibility::<T>()",
            "m.visibility",
            "a::visibility::b",
            "my_visibility::b",
            "visibility_of::b",
            "visibility.len()",
        ] {
            assert_eq!(rerooted_beside(text, &["visibility"]), text);
        }
    }

    #[test]
    fn leaves_strings_comments_and_use_items_beside_a_declared_module() {
        for text in [
            "let s = \"visibility::x\";",
            "// visibility::x",
            "/* visibility::x */ let a = 1;",
            "use visibility::x;",
            "pub(crate) use visibility::{a,\n    b};",
        ] {
            assert_eq!(rerooted_beside(text, &["visibility"]), text);
        }
    }

    #[test]
    fn leaves_a_module_the_parent_does_not_declare() {
        for text in ["std::fmt::Display", "crate::a::b", "visibility::x"] {
            assert_eq!(rerooted_beside(text, &["facade"]), text);
        }
    }

    #[test]
    fn rewrites_two_sibling_modules_and_counts_them_with_the_super_paths() {
        let text = "visibility::a(); seam_survey::b(); super::c(); visibility::d();";
        let siblings = ["visibility".to_string(), "seam_survey".to_string()];

        assert_eq!(
            rerooted_for_child(text, &siblings),
            (
                "super::visibility::a(); super::seam_survey::b(); super::super::c(); \
                 super::visibility::d();"
                    .to_string(),
                4
            )
        );
    }

    #[test]
    fn leaves_a_path_inside_a_module_the_body_declares_alone() {
        let text = "mod deep {\n    fn f() { visibility::g(); }\n}\nfn h() { visibility::g(); }";

        assert_eq!(
            rerooted_beside(text, &["visibility"]),
            "mod deep {\n    fn f() { visibility::g(); }\n}\nfn h() { super::visibility::g(); }"
        );
    }

    #[test]
    fn reads_the_modules_the_parent_declares_at_its_top_level() {
        let parent = "mod a;\npub mod b;\npub(crate) mod c;\nmod d {\n    mod nested;\n}\n\
                      // mod commented;\nfn f() { let s = \"mod quoted;\"; }\nmod facade {\n}\n";

        assert_eq!(
            declared_modules(parent, "facade"),
            ["a", "b", "c", "d"].map(String::from)
        );
    }
}
