//! Keeping the meaning of the relative spellings in code that changes module.
//!
//! `super::f()` and `pub(super)` say "the parent of the module I am written in". Moved to another
//! module they say something else, and nothing in the text changes to warn of it. These are the two
//! spellings the code being moved carries along: the paths it writes, and the visibilities of what
//! it declares inside itself (fields, methods). Each is read as what it meant in the module the code
//! leaves, and written again for the module it arrives in; one that already means the same thing
//! there is not touched.

use std::collections::BTreeSet;
use std::ops::Range;

use super::super::early_return::masked_to_code;
use super::scope::Scope;
use super::text::{enclosing_modules, is_identifier_byte, Edit};

/// The module the code leaves and the one it arrives in, below the crate root.
pub(in crate::backends::rust) struct Modules<'a> {
    pub(in crate::backends::rust) from: &'a [String],
    pub(in crate::backends::rust) to: &'a [String],
    /// A module that moves whole with the code, when it does: a relative path that reaches into it
    /// still reaches the same place, and the visibilities written in the code keep their meaning.
    pub(in crate::backends::rust) travelling: Option<&'a [String]>,
    /// Where a name a module imports really lives: the module and the name, and the path below the
    /// crate root the import brings in. `super::Name` is respelled through it when the module the
    /// path named only imports `Name`, which nothing outside that module may name.
    pub(in crate::backends::rust) imports: Option<&'a Imports<'a>>,
}

/// The lookup behind [`Modules::imports`].
pub(in crate::backends::rust) type Imports<'a> =
    dyn Fn(&[String], &str) -> Option<Vec<String>> + 'a;

/// The edits over `region` of `text` that keep every relative path and visibility meaning what it
/// meant. `claimed` are spans another edit owns — an item's own visibility — and are left alone.
pub(in crate::backends::rust) fn edits(
    text: &str,
    region: Range<usize>,
    modules: &Modules<'_>,
    moved_names: &BTreeSet<String>,
    claimed: &[Range<usize>],
) -> Vec<Edit> {
    let masked = masked_to_code(text);
    let visibilities = visibility_spans(&masked, &region);
    let mut found = Vec::new();

    for span in &visibilities {
        if modules.travelling.is_some() || claimed.iter().any(|other| overlaps(other, span)) {
            continue;
        }
        found.extend(visibility_edit(text, span.clone(), modules));
    }

    let mut at = region.start;
    while at < region.end {
        let in_visibility = visibilities.iter().any(|span| span.contains(&at));
        let claimed_by_another = claimed.iter().any(|span| span.contains(&at));
        if !in_visibility && !claimed_by_another {
            if let Some(edit) = path_edit(&masked, at, modules, moved_names) {
                at = edit.end.max(at + 1);
                found.push(edit);
                continue;
            }
        }
        at += 1;
    }
    found
}

fn overlaps(one: &Range<usize>, other: &Range<usize>) -> bool {
    one.start < other.end && other.start < one.end
}

/// The spans of the `pub(…)` qualifiers in `region`.
fn visibility_spans(masked: &str, region: &Range<usize>) -> Vec<Range<usize>> {
    let bytes = masked.as_bytes();
    masked[region.clone()]
        .match_indices("pub(")
        .filter_map(|(offset, _)| {
            let start = region.start + offset;
            let whole = start == 0 || !is_identifier_byte(bytes[start - 1]);
            let close = masked[start..].find(')')?;
            whole.then_some(start..start + close + 1)
        })
        .collect()
}

fn visibility_edit(text: &str, span: Range<usize>, modules: &Modules<'_>) -> Option<Edit> {
    let written = &text[span.clone()];
    if written == "pub(crate)" {
        return None;
    }
    let respelled = Scope::parse(written, modules.from)?.spelled_in(modules.to);
    (respelled != written).then(|| Edit::replace(span, respelled))
}

/// The rewrite of the `self::`/`super::` path starting at `at`, if it means something else in the
/// module the code arrives in.
fn path_edit(
    masked: &str,
    at: usize,
    modules: &Modules<'_>,
    moved_names: &BTreeSet<String>,
) -> Option<Edit> {
    let bytes = masked.as_bytes();
    let continues = (at >= 2 && &bytes[at - 2..at] == b"::")
        || (at > 0 && (is_identifier_byte(bytes[at - 1]) || bytes[at - 1] == b'.'));
    if continues {
        return None;
    }

    let (from, to) = modules_at(masked, at, modules);
    let mut cursor = at;
    let mut arrives_at = from.clone();
    let mut chain = 0usize;
    loop {
        let rest = &masked[cursor..];
        if rest.starts_with("super::") {
            arrives_at.pop()?;
            cursor += "super::".len();
        } else if rest.starts_with("self::") {
            cursor += "self::".len();
        } else {
            break;
        }
        chain += 1;
    }
    if chain == 0 {
        return None;
    }

    if modules
        .travelling
        .is_some_and(|inside| arrives_at.starts_with(inside))
    {
        return None;
    }

    // `self::name` for a name that travels with the code still names its own module.
    let named: String = masked[cursor..]
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect();
    if arrives_at == from && moved_names.contains(&named) {
        return None;
    }

    let written = &masked[at..cursor];
    let through = modules
        .imports
        .and_then(|imports| imports(&arrives_at, &named))
        .and_then(|target| {
            let (last, module) = target.split_last()?;
            (*last == named).then(|| module.to_vec())
        });
    let respelled = relative_to(through.as_ref().unwrap_or(&arrives_at), &to);
    (respelled != written).then(|| Edit::replace(at..cursor, respelled))
}

/// The module the code at `at` is written in, before and after the move.
///
/// Code that travels whole carries its inline modules along, and a `super::` inside one of them
/// names the module around it, not the one the file is.
fn modules_at(masked: &str, at: usize, modules: &Modules<'_>) -> (Vec<String>, Vec<String>) {
    let mut from = modules.from.to_vec();
    let mut to = modules.to.to_vec();
    if modules.travelling.is_some() {
        let inline = enclosing_modules(masked, at);
        from.extend(inline.iter().cloned());
        to.extend(inline);
    }
    (from, to)
}

/// The path prefix, ending in `::`, that reaches `target` from the module `from`.
fn relative_to(target: &[String], from: &[String]) -> String {
    let shared = target
        .iter()
        .zip(from)
        .take_while(|(one, other)| one == other)
        .count();
    let ups = from.len() - shared;
    let mut parts: Vec<&str> = if ups == 0 {
        vec!["self"]
    } else {
        vec!["super"; ups]
    };
    parts.extend(target[shared..].iter().map(String::as_str));
    format!("{}::", parts.join("::"))
}

#[cfg(test)]
mod tests {
    use super::super::text::applied;
    use super::*;

    fn module(path: &str) -> Vec<String> {
        path.split("::")
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn rebased(text: &str, from: &str, to: &str) -> String {
        let (from, to) = (module(from), module(to));
        let found = edits(
            text,
            0..text.len(),
            &Modules {
                from: &from,
                to: &to,
                travelling: None,
                imports: None,
            },
            &BTreeSet::new(),
            &[],
        );
        applied(text, &found).unwrap()
    }

    #[test]
    fn leaves_a_super_path_alone_when_the_new_module_has_the_same_parent() {
        let text = "fn f() { super::helper(); }";

        assert_eq!(rebased(text, "a::pairing", "a::answers"), text);
    }

    #[test]
    fn respells_a_super_path_for_a_module_one_level_deeper() {
        assert_eq!(
            rebased("fn f() { super::helper(); }", "pairing", "pairing::deep"),
            "fn f() { super::super::helper(); }"
        );
    }

    #[test]
    fn respells_a_self_path_as_the_module_it_left() {
        assert_eq!(
            rebased("fn f() { self::helper(); }", "pairing", "answers"),
            "fn f() { super::pairing::helper(); }"
        );
    }

    #[test]
    fn keeps_a_pub_super_field_meaning_the_same_module() {
        assert_eq!(
            rebased("struct S { pub(super) x: u32 }", "a::b", "a::c::d"),
            "struct S { pub(in crate::a) x: u32 }"
        );
    }

    #[test]
    fn does_not_read_a_path_in_a_comment() {
        let text = "// super::helper()\nfn f() {}";

        assert_eq!(rebased(text, "pairing", "answers"), text);
    }

    #[test]
    fn follows_an_import_of_the_module_a_super_path_names() {
        let (from, to) = (module("host::worker"), module("split::worker"));
        let imported = |at: &[String], name: &str| {
            (at == module("host").as_slice() && name == "Config").then(|| module("types::Config"))
        };
        let text = "use super::Config;\n";
        let found = edits(
            text,
            0..text.len(),
            &Modules {
                from: &from,
                to: &to,
                travelling: None,
                imports: Some(&imported),
            },
            &BTreeSet::new(),
            &[],
        );

        assert_eq!(
            applied(text, &found).unwrap(),
            "use super::super::types::Config;\n"
        );
    }
}
