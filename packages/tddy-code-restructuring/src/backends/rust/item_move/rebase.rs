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
use super::bindings::Imported;
use super::scope::Scope;
use super::text::{enclosing_modules, is_identifier_byte, Edit};
use crate::Result;

/// The module the code leaves and the one it arrives in, below the crate root.
pub(in crate::backends::rust) struct Modules<'a> {
    /// The file the code is written in, as a refusal names it.
    #[allow(
        dead_code,
        reason = "TODO(reshape-move-item-paths): named by the R8 refusal at green"
    )]
    pub(in crate::backends::rust) file: &'a str,
    pub(in crate::backends::rust) from: &'a [String],
    pub(in crate::backends::rust) to: &'a [String],
    /// A module that moves whole with the code, when it does: a relative path that reaches into it
    /// still reaches the same place, and the visibilities written in the code keep their meaning.
    pub(in crate::backends::rust) travelling: Option<&'a [String]>,
    /// How a module binds a name it only imports, asked with the module, the name and the module the
    /// code arrives in. `super::Name` is respelled through it when the module the path named only
    /// imports `Name`, which nothing outside that module may name.
    pub(in crate::backends::rust) imports: Option<&'a Imports<'a>>,
}

/// The lookup behind [`Modules::imports`].
pub(in crate::backends::rust) type Imports<'a> =
    dyn Fn(&[String], &str, &[String]) -> Option<Imported> + 'a;

/// The edits over `region` of `text` that keep every relative path and visibility meaning what it
/// meant. `claimed` are spans another edit owns — an item's own visibility — and are left alone.
///
/// # Errors
///
/// Refuses a path through a glob import that cannot be followed (R8): naming it as written would name
/// a private import from outside the module that holds it.
pub(in crate::backends::rust) fn edits(
    text: &str,
    region: Range<usize>,
    modules: &Modules<'_>,
    moved_names: &BTreeSet<String>,
    claimed: &[Range<usize>],
) -> Result<Vec<Edit>> {
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
            if let Some(edit) = path_edit(&masked, at, modules, moved_names).transpose()? {
                at = edit.end.max(at + 1);
                found.push(edit);
                continue;
            }
        }
        at += 1;
    }
    Ok(found)
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
) -> Option<Result<Edit>> {
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
    respelled(masked, at..cursor, &named, &arrives_at, &to, modules).transpose()
}

/// The rewrite of the `self::`/`super::` prefix at `prefix`, which reaches the module `arrives_at`
/// and names `named` there, for code arriving in `to` — rules R1–R8 of the changeset.
fn respelled(
    masked: &str,
    prefix: Range<usize>,
    named: &str,
    arrives_at: &[String],
    to: &[String],
    modules: &Modules<'_>,
) -> Result<Option<Edit>> {
    // TODO(reshape-move-item-paths): implement R2 (destination inside the module), R3 (visible
    // import), R5 (alias: the name is rewritten too), R6 (confirmed glob), R7 (extern path) and R8
    // (refusal); today an import is followed only when it keeps the written name (R4).
    let written = &masked[prefix.clone()];
    let through = modules
        .imports
        .and_then(|imports| imports(arrives_at, named, to))
        .and_then(|imported| match imported {
            Imported::InCrate { module, name } => (name == named).then_some(module),
            _ => None,
        });
    let respelled = relative_to(through.as_deref().unwrap_or(arrives_at), to);
    Ok((respelled != written).then(|| Edit::replace(prefix, respelled)))
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
        rebased_through(text, from, to, None).expect("the rebase is not refused")
    }

    /// `text`, written in `src/host/worker.rs` as the module `from`, rebased for `to`, with the module
    /// `host` answering `imported` for every name it is asked about.
    fn rebased_through(
        text: &str,
        from: &str,
        to: &str,
        imported: Option<Imported>,
    ) -> Result<String> {
        let (from, to) = (module(from), module(to));
        let answer = |at: &[String], _: &str, _: &[String]| {
            (at == module("host").as_slice())
                .then(|| imported.clone())
                .flatten()
        };
        let found = edits(
            text,
            0..text.len(),
            &Modules {
                file: "src/host/worker.rs",
                from: &from,
                to: &to,
                travelling: None,
                imports: Some(&answer),
            },
            &BTreeSet::new(),
            &[],
        )?;
        Ok(applied(text, &found).unwrap())
    }

    fn in_crate(module_path: &str, name: &str) -> Option<Imported> {
        Some(Imported::InCrate {
            module: module(module_path),
            name: name.to_string(),
        })
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
        // Given `host` binding `Config` by a private `use crate::types::Config;` (R4)
        let text = "use super::Config;\n";

        // When the code leaves `host::worker` for `split::worker`
        let moved = rebased_through(
            text,
            "host::worker",
            "split::worker",
            in_crate("types", "Config"),
        );

        // Then the path reaches `Config` where the import brings it from
        assert_eq!(moved.unwrap(), "use super::super::types::Config;\n");
    }

    #[test]
    fn keeps_a_super_path_when_the_destination_is_inside_the_module_it_reaches() {
        // Given `host` binding `service_util` by a facade of another crate, the `#carve` 21/21 shape,
        // and a lookup that would follow the import somewhere else entirely
        let text = "fn f() { super::service_util::find(); }\n";
        let facade = in_crate("types", "service_util");

        // When the code leaves `host::observer` for its sibling `host::wiring` (R2)
        let moved = rebased_through(text, "host::observer", "host::wiring", facade);

        // Then the path is byte-identical: `host`'s imports are visible below it
        assert_eq!(moved.unwrap(), text);
    }

    #[test]
    fn keeps_a_super_path_through_an_import_visible_at_the_destination() {
        // Given `host` binding `Config` by a `pub(crate) use` (R3)
        let text = "fn f() -> super::Config { todo!() }\n";

        // When the code leaves `host::worker` for `split::worker`
        let moved = rebased_through(
            text,
            "host::worker",
            "split::worker",
            Some(Imported::Visible),
        );

        // Then the path reaches `host` and names `Config` there
        assert_eq!(
            moved.unwrap(),
            "fn f() -> super::super::host::Config { todo!() }\n"
        );
    }

    #[test]
    fn follows_an_aliased_import_and_writes_the_name_it_brings_in() {
        // Given `host` binding `Settings` by `use crate::types::Config as Settings;` (R5)
        let text = "fn f(settings: &super::Settings) {}\n";

        // When the code leaves `host::worker` for `split::worker`
        let moved = rebased_through(
            text,
            "host::worker",
            "split::worker",
            in_crate("types", "Config"),
        );

        // Then the path names `Config` in `types`, prefix and name both rewritten
        assert_eq!(
            moved.unwrap(),
            "fn f(settings: &super::super::types::Config) {}\n"
        );
    }

    #[test]
    fn follows_a_confirmed_glob_import_to_the_module_that_binds_the_name() {
        // Given `host` binding `Config` by `use crate::types::*;`, which `types` confirms (R6)
        let text = "fn f(config: super::Config) {}\n";

        // When the code leaves `host::worker` for `split`
        let moved = rebased_through(text, "host::worker", "split", in_crate("types", "Config"));

        // Then the path reaches `types`
        assert_eq!(moved.unwrap(), "fn f(config: super::types::Config) {}\n");
    }

    #[test]
    fn writes_an_extern_import_as_the_crates_own_path() {
        // Given `host` binding `util` by a private `use kernel::util;` (R7)
        let text = "fn f() -> u32 { super::util::answer() }\n";
        let kernel = Some(Imported::Extern {
            path: module("kernel::util"),
            shadowed: false,
        });

        // When the code leaves `host::worker` for `split`
        let moved = rebased_through(text, "host::worker", "split", kernel);

        // Then the path is the crate's own, never `super::host::kernel::`
        assert_eq!(moved.unwrap(), "fn f() -> u32 { kernel::util::answer() }\n");
    }

    #[test]
    fn roots_an_extern_path_when_the_destination_shadows_the_crate_name() {
        // Given the same import, and a destination that binds a name `kernel` of its own (R7)
        let text = "fn f() -> u32 { super::util::answer() }\n";
        let kernel = Some(Imported::Extern {
            path: module("kernel::util"),
            shadowed: true,
        });

        // When the code leaves `host::worker` for `split`
        let moved = rebased_through(text, "host::worker", "split", kernel);

        // Then the path is rooted at the extern prelude
        assert_eq!(
            moved.unwrap(),
            "fn f() -> u32 { ::kernel::util::answer() }\n"
        );
    }

    #[test]
    fn refuses_a_path_through_an_unconfirmed_glob_naming_the_file_and_the_line() {
        // Given `host` binding `Config` only through globs that cannot confirm it (R8)
        let text = "fn g() {}\nfn f(config: super::Config) {}\n";
        let unconfirmed = Some(Imported::Unconfirmed(
            "no glob of `host` binds `Config`".to_string(),
        ));

        // When the code leaves `host::worker` for `split`
        let refusal = rebased_through(text, "host::worker", "split", unconfirmed)
            .expect_err("the rebase refuses")
            .to_string();

        // Then the refusal names the file, the line and the path as written
        assert!(
            refusal.contains("src/host/worker.rs:2") && refusal.contains("`super::Config`"),
            "unexpected refusal: {refusal}"
        );
    }
}
