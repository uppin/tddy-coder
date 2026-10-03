use super::IMPORT_TITLE;

use super::module_bounds;

use crate::{backends::rust::names_bound, Result};

use serde_json::Value;

use super::ModuleBlock;

/// The one import to apply out of everything the server offered for a name.
///
/// A single offer is the answer. Several mean the name is worn by several items, and the one the
/// moved code meant is the one the file was already importing before the move carried it out of
/// that declaration's scope. Reading those declarations back narrows the choice without inventing a
/// path — rust-analyzer still writes every character of the `use` it inserts. A name the file's own
/// imports do not settle has no answer here either, and is reported rather than picked.
pub(crate) fn choose_import<'a>(text: &str, offered: &[&'a str]) -> Option<&'a str> {
    if let [only] = offered {
        return Some(only);
    }

    let in_scope = imported_paths(text);
    let mut narrowed = offered
        .iter()
        .filter(|title| import_path(title).is_some_and(|path| in_scope.contains(&path)));

    if let Some(only) = narrowed.next() {
        if narrowed.next().is_none() {
            return Some(only);
        }
        return None;
    }

    // The name itself is bound nowhere — routinely true after a seam has moved the code that used
    // it and the import pass pruned the parent's now-unused binding. The module it came from is
    // still evidence: a file importing twenty-six names from `tddy_service::proto::connection` and
    // one contested `Signal` meant that one, not `sysinfo::Signal`.
    //
    // Only decisive where exactly one candidate's module is already imported from. Two candidates
    // from two imported modules is the ambiguity this function exists to refuse.
    let modules: Vec<String> = in_scope
        .iter()
        .filter_map(|path| parent_module(path))
        .collect();
    let mut by_module = offered.iter().filter(|title| {
        import_path(title)
            .and_then(|path| parent_module(&path))
            .is_some_and(|module| modules.contains(&module))
    });

    if let Some(only) = by_module.next() {
        if by_module.next().is_none() {
            return Some(only);
        }
        return None;
    }

    // Neither tier can see through a re-export. A crate publishing an item at its root gives one
    // item two paths, and rust-analyzer offers the shortest: `tddy-core` carries
    // `pub use error::{BackendError, ParseError, WorkflowError};`, so a file writing the canonical
    // `tddy_core::error::ParseError` is offered `tddy_core::ParseError` — a different string and a
    // different parent module for the same type. One live extraction was refused three candidates
    // deep over exactly that.
    //
    // The crate the file already binds *this name* from is the evidence that settles it, and it is
    // keyed on a binding of the contested name rather than on any binding from the crate. Keyed on
    // the crate alone it would be worthless: almost every file imports something from `std`, so
    // `std::string::ParseError` would match as readily as the one the file means.
    //
    // Third, and strictly weaker than the two above — a crate root is a coarser claim than a path,
    // and must never outrank one. Two candidates rooted in the crate the name is bound from is
    // still the ambiguity this function exists to refuse, because a crate holding both is no
    // narrower. A wrong guess costs a refusal rather than bad source either way: the caller
    // verifies each import it writes against the occurrences it was supposed to resolve.
    let mut by_crate = offered
        .iter()
        .filter(|title| import_path(title).is_some_and(|path| binds_that_name(&in_scope, &path)));

    let only = *by_crate.next()?;
    by_crate.next().is_none().then_some(only)
}

/// Whether one of `in_scope` binds `path`'s own last segment from `path`'s own crate.
fn binds_that_name(in_scope: &[String], path: &str) -> bool {
    let (Some(root), Some(name)) = (crate_root(path), path.rsplit("::").next()) else {
        return false;
    };

    in_scope
        .iter()
        .any(|bound| bound.rsplit("::").next() == Some(name) && crate_root(bound) == Some(root))
}

/// The crate a path is rooted in — `a::b::C` is `a`. `None` for an empty path.
fn crate_root(path: &str) -> Option<&str> {
    path.split("::").next().filter(|root| !root.is_empty())
}

/// The module a path's last segment lives in — `a::b::C` is `a::b`. `None` for a bare name.
pub(crate) fn parent_module(path: &str) -> Option<String> {
    path.rsplit_once("::").map(|(module, _)| module.to_string())
}

/// `source` without the single-name `use` lines inside `block` that no import can be.
///
/// A line goes when the server reports an unresolved name on it — the import binds nothing — or when
/// the name it binds is bound again by another `use` in the same block, which the compiler rejects
/// whichever of the two resolves. A grouped `use` is never dropped: it carries names beyond the one
/// in question.
pub(crate) fn without_dead_imports(
    source: &[String],
    block: &ModuleBlock,
    unresolved: &[UnresolvedName],
) -> Vec<String> {
    let inside = |index: usize| index > block.opened && index < block.closed;

    // Names bound by a group have to be known before the simple lines are judged: the duplicate is
    // as often written above its group as below it.
    let grouped: Vec<String> = source
        .iter()
        .enumerate()
        .filter(|(index, line)| inside(*index) && simple_import(line).is_none())
        .flat_map(|(_, line)| names_bound(line))
        .collect();

    let mut kept = Vec::with_capacity(source.len());
    let mut standing: Vec<String> = Vec::new();

    for (index, line) in source.iter().enumerate() {
        if let Some(name) = simple_import(line).filter(|_| inside(index)) {
            if unresolved_on_line(unresolved, index)
                || grouped.contains(&name)
                || standing.contains(&name)
            {
                continue;
            }
            standing.push(name);
        }
        kept.push(line.clone());
    }

    kept
}

/// The one name a `use` line binds, when it is a single-line declaration binding exactly one.
fn simple_import(line: &str) -> Option<String> {
    if line.contains('{') || !line.trim_end().ends_with(';') {
        return None;
    }

    let names = names_bound(line);
    let [only] = names.as_slice() else {
        return None;
    };
    Some(only.clone())
}

/// Whether the server reports any unresolved name on the given zero-based line.
fn unresolved_on_line(unresolved: &[UnresolvedName], index: usize) -> bool {
    unresolved
        .iter()
        .any(|found| found.position.get("line").and_then(Value::as_u64) == Some(index as u64))
}

/// How many of the names the server could not resolve are `name`.
pub(crate) fn occurrences_of(unresolved: &[UnresolvedName], name: &str) -> usize {
    unresolved.iter().filter(|found| found.text == name).count()
}

/// The order to try the offered imports in: the one this file's own imports point at first, then
/// every other path the server offered, as it offered them.
///
/// `choose_import` still decides which path the moved code *meant*, and still refuses when neither
/// the server nor the file settles it. What is new is that its answer is a first guess rather than
/// the only one — rust-analyzer offers paths that resolve nothing, and the caller verifies each in
/// turn instead of trusting the title.
pub(crate) fn import_order<'a>(text: &str, offered: &'a [String]) -> Option<Vec<&'a str>> {
    let borrowed: Vec<&str> = offered.iter().map(String::as_str).collect();
    let first = choose_import(text, &borrowed)?;

    let mut ordered = vec![first];
    ordered.extend(borrowed.iter().copied().filter(|title| *title != first));
    Some(ordered)
}

/// Whether the identifier at `position` is reached through a qualifier — `Type::assoc`, `value.field`.
///
/// Such a name is an associated item or a field, resolved through what precedes it rather than
/// through a path, so no `use` can bind it. A range's `..` is excluded: the name after it is an
/// ordinary expression, and a constant there is importable like any other.
pub(crate) fn reached_through_qualifier(text: &str, position: &Value) -> bool {
    let (Some(line), Some(character)) = (
        position.get("line").and_then(Value::as_u64),
        position.get("character").and_then(Value::as_u64),
    ) else {
        return false;
    };

    let Some(before) = text
        .split('\n')
        .nth(line as usize)
        .and_then(|source| source.get(..character as usize))
        .map(str::trim_end)
    else {
        return false;
    };

    before.ends_with("::") || (before.ends_with('.') && !before.ends_with(".."))
}

/// Whether `module`'s own `use` declarations already bind `name`.
///
/// Scoped to the block being repaired rather than read over the whole file: the same name is
/// routinely imported by a sibling module, and treating that as already bound here would skip an
/// import the moved code genuinely lost.
pub(crate) fn already_bound(text: &str, module: &str, name: &str) -> Result<bool> {
    let source: Vec<String> = text.split('\n').map(str::to_string).collect();
    let block = module_bounds(&source, module)?;
    let body = source[block.opened..block.closed].join("\n");

    Ok(names_bound(&body).iter().any(|bound| bound == name))
}

/// The path an `Import` quickfix names, read out of its title.
fn import_path(title: &str) -> Option<String> {
    let named = title.strip_prefix(IMPORT_TITLE)?;
    Some(named.trim().trim_matches('`').to_string())
}

/// Every path the text's own `use` declarations bind, with their `{…}` groups expanded.
///
/// This is a lexical read, not a resolution: a glob contributes nothing because there is no telling
/// what it brings in, and a `use` written inside a function counts the same as one at the top. Both
/// only ever cost a refusal, never a wrong import.
pub(crate) fn imported_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();

    for statement in text.split(';') {
        if let Some(tree) = use_tree(statement) {
            expand_use(tree, "", &mut paths);
        }
    }

    paths
}

/// The `use` tree a semicolon-terminated statement declares, if it declares one.
///
/// The statement is read from its last line-initial `use`, which keeps a `{…}` group spanning
/// several lines whole while leaving whatever precedes the declaration — an earlier statement, a
/// comment quoting an import — out of it.
pub(crate) fn use_tree(statement: &str) -> Option<&str> {
    let mut start = None;
    let mut offset = 0;

    for line in statement.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("use ") || trimmed.starts_with("pub use ") {
            start = Some(offset + line.len() - trimmed.len());
        }
        offset += line.len();
    }

    let tree = &statement[start?..];
    tree.strip_prefix("use ")
        .or_else(|| tree.strip_prefix("pub use "))
}

/// Push every path a `use` tree binds, expanding each group onto the prefix that leads to it.
fn expand_use(tree: &str, prefix: &str, paths: &mut Vec<String>) {
    let tree = tree.trim();

    let Some(open) = tree.find('{') else {
        let bound = tree.split(" as ").next().unwrap_or(tree).trim();
        if bound == "self" {
            paths.push(prefix.trim_end_matches("::").to_string());
        } else if !bound.is_empty() && !bound.ends_with('*') {
            paths.push(format!("{prefix}{bound}"));
        }
        return;
    };

    let head = format!("{prefix}{}", &tree[..open]);
    let close = tree.rfind('}').unwrap_or(tree.len());

    for member in group_members(&tree[open + 1..close]) {
        expand_use(member, &head, paths);
    }
}

/// A group's members, split on the commas that are not inside a group of their own.
pub(crate) fn group_members(inner: &str) -> Vec<&str> {
    let mut members = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;

    for (offset, character) in inner.char_indices() {
        match character {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                members.push(&inner[start..offset]);
                start = offset + 1;
            }
            _ => {}
        }
    }
    members.push(&inner[start..]);

    members
}

/// An identifier the server could not resolve, with the position an assist is asked for at.
pub(crate) struct UnresolvedName {
    pub(crate) text: String,
    pub(crate) position: Value,
}
