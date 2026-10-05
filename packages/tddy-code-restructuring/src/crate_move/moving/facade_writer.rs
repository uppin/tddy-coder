use super::super::malformed;

use crate::{
    crate_move::{destination, facade_line, facade_lines_for_plan, header, manifest_edits},
    edit::TextEdit,
    Reexport,
};

use crate::edit::FileEdit;

use super::super::Result;

use super::super::Survey;

use super::Move;

use crate::registry::Workspace;

/// What a set of moves leaves in the files that declared them: the `mod` lines go, and in their
/// place a facade, when the plan asked for one.
///
/// The set is read together because its facade is: [`Reexport::Glob`] leaves **one** `pub use
/// <dest>::{a, b};` naming every module that moved there. That line takes the place of the first
/// `mod` line it replaces, and a line an earlier operation of the plan already wrote for the same
/// destination is extended in place instead, so a plan of three moves ends with one line, not three.
/// `surveys` pairs with `members`, in order.
///
/// A module moved with no facade leaves its parent's `use <module>::*;` and `use <module>::{…};`
/// dangling: no caller reaches them by name, so nothing else re-points them. Both are rewritten to
/// name the destination.
pub(crate) fn left_behind(
    workspace: &Workspace<'_>,
    members: &[Move],
    surveys: &[Survey],
) -> Result<Vec<FileEdit>> {
    let mut declaring_files: Vec<&str> = Vec::new();
    for member in members {
        if !declaring_files.contains(&member.home.declared_in.as_str()) {
            declaring_files.push(&member.home.declared_in);
        }
    }

    let mut changes = Vec::new();
    for path in declaring_files {
        let text = workspace.read(path)?;
        let here: Vec<(&Move, &Survey)> = members
            .iter()
            .zip(surveys)
            .filter(|(member, _)| member.home.declared_in == path)
            .collect();
        changes.push(FileEdit::Change {
            path: path.to_string(),
            edits: leaving(workspace, path, &text, &here)?,
        });
    }
    Ok(changes)
}

/// The edits one declaring file needs for the members it declares.
fn leaving(
    workspace: &Workspace<'_>,
    path: &str,
    text: &str,
    members: &[(&Move, &Survey)],
) -> Result<Vec<TextEdit>> {
    let facade_members: Vec<&Move> = members
        .iter()
        .map(|(member, _)| *member)
        .filter(|member| member.reexport == Reexport::Glob)
        .collect();
    let moved: Vec<(destination::Destination, String)> = facade_members
        .iter()
        .map(|member| (member.destination.clone(), member.module.clone()))
        .collect();

    let mut edits = Vec::new();
    let mut facade_written = false;
    if let Some(earlier) = earlier_facade(workspace, text, &facade_members) {
        edits.push(extended_facade(text, &earlier, &facade_members, &moved));
        facade_written = true;
    }

    for (member, survey) in members {
        let span = declaration_of(path, text, member)?;
        let facade = match member.reexport {
            Reexport::Glob if facade_written => None,
            Reexport::Glob => {
                facade_written = true;
                Some(facade_lines_for_plan(&moved).join("\n"))
            }
            // `outside` is refused for a cross-crate move when the plan is read.
            Reexport::Named | Reexport::None | Reexport::Outside => facade_line(
                &member.destination,
                member.reexport,
                &survey.reached_from_outside,
            ),
        };
        // The declaration's line goes entirely, newline included, when nothing replaces it.
        let line = facade.map_or_else(String::new, |facade| format!("{facade}\n"));
        edits.push(manifest_edits::replacement(text, span, &line));
        edits.extend(parent_reexport_edits(text, member));
    }
    Ok(edits)
}

/// The facade an earlier operation of the plan wrote for the first facade member's destination.
fn earlier_facade(
    workspace: &Workspace<'_>,
    text: &str,
    facade_members: &[&Move],
) -> Option<WrittenFacade> {
    let first = facade_members.first()?;
    let root = workspace.read(&first.destination_root()).ok()?;
    written_facade(text, &first.destination.extern_name, &root)
}

/// The edit that rewrites an earlier facade line to also name the modules moving now.
fn extended_facade(
    text: &str,
    earlier: &WrittenFacade,
    facade_members: &[&Move],
    moved: &[(destination::Destination, String)],
) -> TextEdit {
    let destination = &facade_members[0].destination;
    let all: Vec<_> = earlier
        .modules
        .iter()
        .map(|module| (destination.clone(), module.clone()))
        .chain(moved.iter().cloned())
        .collect();
    let line = facade_lines_for_plan(&all).join("\n");
    manifest_edits::replacement(text, earlier.span.clone(), &format!("{line}\n"))
}

/// Where the file declares `member`'s module, or the refusal when it does not.
fn declaration_of(path: &str, text: &str, member: &Move) -> Result<std::ops::Range<usize>> {
    manifest_edits::module_declaration(text, &member.module).ok_or_else(|| {
        let where_declared = if member.home.is_top_level() {
            "crate root"
        } else {
            "parent module"
        };
        malformed(format!(
            "{path} declares no `mod {}` — a module this {where_declared} does not declare is \
             not this crate's to move",
            member.module
        ))
    })
}

/// The edits that point a moved-without-facade module's parent re-exports at the destination.
fn parent_reexport_edits(text: &str, member: &Move) -> Vec<TextEdit> {
    if member.reexport != Reexport::None {
        return Vec::new();
    }
    parent_reexports_of(text, &member.module)
        .into_iter()
        .map(|at| {
            manifest_edits::replacement(
                text,
                at..at + member.module.len(),
                &format!("{}::{}", member.destination.extern_name, member.module),
            )
        })
        .collect()
}

/// A grouped facade an earlier operation wrote for one destination.
pub(crate) struct WrittenFacade {
    /// The line, newline included.
    pub(crate) span: std::ops::Range<usize>,
    pub(crate) modules: Vec<String>,
}

/// The `pub use <crate>::{a, b};` (or `pub use <crate>::a;`) line this tool wrote, if the file has
/// one.
///
/// Provenance is read off the destination: the tool declares every module it moves as a `pub mod`
/// in the destination root (`destination_root`), so a line is extended only when **every** module it
/// names is declared there. A `pub use <crate>::a;` the user wrote by hand for an item or a module
/// the destination does not declare is left alone. A hand-written line naming only modules the
/// destination does declare is indistinguishable from ours and is extended; that limit is pinned by
/// a test.
pub(crate) fn written_facade(
    text: &str,
    extern_name: &str,
    destination_root: &str,
) -> Option<WrittenFacade> {
    let prefix = format!("pub use {extern_name}::");
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();

        let Some(named) = line
            .trim_end()
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix(';'))
        else {
            continue;
        };
        let named = named
            .strip_prefix('{')
            .and_then(|group| group.strip_suffix('}'))
            .unwrap_or(named);
        let modules: Vec<String> = named
            .split(',')
            .map(|name| name.trim().to_string())
            .collect();
        let plain_and_declared = modules.iter().all(|name| {
            !name.is_empty()
                && name.chars().all(header::is_path_character)
                && is_declared_pub(destination_root, name)
        });
        if plain_and_declared {
            return Some(WrittenFacade {
                span: start..offset,
                modules,
            });
        }
    }
    None
}

/// Whether the root declares `module` as `pub mod`, which is how the tool declares what it moves.
fn is_declared_pub(root: &str, module: &str) -> bool {
    manifest_edits::module_declaration(root, module)
        .is_some_and(|span| root[span].trim_start().starts_with("pub mod "))
}

/// Where `module` starts in each top-level `use module::*;` / `use module::{…};` of a file, whatever
/// the visibility. A path to one item is not here: the caller survey sees it and re-points it.
///
/// Only declarations at column 0 and outside any brace nesting count: a `use` inside an inline
/// module or a function body names that scope's own `module`, not the parent's.
pub(crate) fn parent_reexports_of(text: &str, module: &str) -> Vec<usize> {
    let mut found = Vec::new();
    let mut offset = 0usize;
    let mut depth = 0i32;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();

        let code = line.split("//").next().unwrap_or(line);
        let top_level = depth == 0 && !line.starts_with(char::is_whitespace);
        depth += code.matches('{').count() as i32 - code.matches('}').count() as i32;
        if !top_level {
            continue;
        }

        let Some(tree) = after_visibility(line)
            .strip_prefix("use ")
            .map(str::trim_start)
        else {
            continue;
        };
        let Some(rest) = tree
            .strip_prefix(module)
            .and_then(|rest| rest.strip_prefix("::"))
        else {
            continue;
        };
        if rest.starts_with('*') || rest.starts_with('{') {
            found.push(start + (line.len() - tree.len()));
        }
    }
    found
}

/// A declaration without its `pub`, `pub(crate)` or `pub(in …)` prefix.
fn after_visibility(declaration: &str) -> &str {
    let Some(rest) = declaration.strip_prefix("pub") else {
        return declaration;
    };
    let rest = match rest.strip_prefix('(') {
        Some(restriction) => restriction.split_once(')').map_or("", |(_, after)| after),
        None => rest,
    };
    if rest.starts_with(char::is_whitespace) {
        rest.trim_start()
    } else {
        declaration
    }
}

/// The destination crate root, declaring every module it is about to receive.
///
/// `pub mod`, not `mod`: a module keeps its name and its callers keep writing it, which only
/// resolves from another crate if the module is public. That is also what a facade needs to
/// re-export. Each lands in sorted position among the root's existing declarations.
pub(crate) fn declared_in_destination(
    workspace: &Workspace<'_>,
    members: &[Move],
) -> Result<Vec<FileEdit>> {
    let Some(first) = members.first() else {
        return Ok(Vec::new());
    };
    let path = first.destination_root();
    let text = workspace.read(&path)?;

    // Each is placed against the root as it stands, so two that land in the same gap stay in the
    // order the plan named them.
    let edits = members
        .iter()
        .map(|member| {
            manifest_edits::insert_module_declaration_sorted(
                &text,
                &format!("pub mod {};", member.module),
            )
        })
        .collect();

    Ok(vec![FileEdit::Change { path, edits }])
}
