use crate::{
    crate_move::{facade_line, facade_lines_for_plan, header, manifest_edits, FacadeEntry},
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
///
/// A facade is written with the visibility its member's own declaration had, so a `pub(crate) mod`
/// leaves a `pub(crate) use` and a private one a private `use`: a `pub use` would publish what the
/// origin kept to itself. One line is kept per visibility.
fn leaving(
    workspace: &Workspace<'_>,
    path: &str,
    text: &str,
    members: &[(&Move, &Survey)],
) -> Result<Vec<TextEdit>> {
    let mut declarations = Vec::new();
    for (member, _) in members {
        declarations.push(member.declaration(workspace)?);
    }
    let moved = glob_entries(members, &declarations);

    // The visibilities whose facade line is already written: extended in place, or at an earlier
    // member's declaration.
    let (mut edits, mut written) = extended_earlier_facades(workspace, text, members[0].0, &moved);

    let removals = removals_with_separators(path, text, members)?;

    for (((member, survey), declaration), (removal, separated)) in
        members.iter().zip(&declarations).zip(removals)
    {
        let facade = facade_for(
            member,
            survey,
            &moved,
            declaration.visibility.as_str(),
            &mut written,
        );
        edits.push(manifest_edits::replacement(
            text,
            removal.span,
            &left_in_place_of(separated, facade),
        ));
        edits.extend(parent_reexport_edits(text, member));
    }
    Ok(edits)
}

/// The modules moving with a glob facade, each with the visibility its declaration had.
fn glob_entries(
    members: &[(&Move, &Survey)],
    declarations: &[manifest_edits::ModuleDeclaration],
) -> Vec<FacadeEntry> {
    members
        .iter()
        .zip(declarations)
        .filter(|((member, _), _)| member.reexport == Reexport::Glob)
        .map(|((member, _), declaration)| FacadeEntry {
            destination: member.destination.clone(),
            visibility: declaration.visibility.clone(),
            module: member.module.clone(),
        })
        .collect()
}

/// The edits that extend a facade line an earlier operation wrote, one per visibility that has
/// one, and the visibilities they cover.
fn extended_earlier_facades<'a>(
    workspace: &Workspace<'_>,
    text: &str,
    first: &Move,
    moved: &'a [FacadeEntry],
) -> (Vec<TextEdit>, Vec<&'a str>) {
    let mut edits = Vec::new();
    let mut written: Vec<&str> = Vec::new();
    for entry in moved {
        let visibility = entry.visibility.as_str();
        if written.contains(&visibility) {
            continue;
        }
        if let Some(earlier) = earlier_facade(workspace, text, first, visibility) {
            edits.push(extended_facade(text, &earlier, moved, visibility));
            written.push(visibility);
        }
    }
    (edits, written)
}

/// The facade that takes the place of `member`'s declaration, if it gets one: a glob member shares
/// one facade per visibility for the plan, written at the first of them (`written` records it).
fn facade_for<'a>(
    member: &Move,
    survey: &Survey,
    moved: &[FacadeEntry],
    visibility: &'a str,
    written: &mut Vec<&'a str>,
) -> Option<String> {
    match member.reexport {
        Reexport::Glob if written.contains(&visibility) => None,
        Reexport::Glob => {
            written.push(visibility);
            let entries: Vec<FacadeEntry> = moved
                .iter()
                .filter(|entry| entry.visibility == visibility)
                .cloned()
                .collect();
            Some(facade_lines_for_plan(&entries).join("\n"))
        }
        // `outside` is refused for a cross-crate move when the plan is read.
        Reexport::Named | Reexport::None | Reexport::Outside => facade_line(
            &member.destination,
            member.reexport,
            &survey.reached_from_outside,
        ),
    }
}

/// Each member's declaration as a move removes it, with whether it carries the blank line that keeps
/// two `use` runs apart (see [`manifest_edits::separator_carriers`]).
fn removals_with_separators(
    path: &str,
    text: &str,
    members: &[(&Move, &Survey)],
) -> Result<Vec<(manifest_edits::RemovedDeclaration, bool)>> {
    let removals = members
        .iter()
        .map(|(member, _)| declaration_of(path, text, member))
        .collect::<Result<Vec<_>>>()?;
    let spans: Vec<_> = removals
        .iter()
        .map(|removal| removal.span.clone())
        .collect();
    let carriers = manifest_edits::separator_carriers(text, &spans);
    Ok(removals.into_iter().zip(carriers).collect())
}

/// What takes the place of a removed declaration: the facade on a line of its own, or nothing.
///
/// Where the declaration (or the run of adjacent ones it ends) is the only thing between two `use`
/// runs, one blank line stays (after the facade, when there is one) so rustfmt keeps sorting the
/// two apart.
fn left_in_place_of(carries_separator: bool, facade: Option<String>) -> String {
    let separator = if carries_separator { "\n" } else { "" };
    match facade {
        Some(facade) => format!("{facade}\n{separator}"),
        None => separator.to_string(),
    }
}

/// The facade of `visibility` an earlier operation of the plan wrote for `first`'s destination,
/// which every member of the set shares.
fn earlier_facade(
    workspace: &Workspace<'_>,
    text: &str,
    first: &Move,
    visibility: &str,
) -> Option<WrittenFacade> {
    let root = workspace.read(&first.destination_root()).ok()?;
    written_facade(text, &first.destination.extern_name, visibility, &root)
}

/// The edit that rewrites an earlier facade line to also name the modules of `visibility` moving
/// now.
fn extended_facade(
    text: &str,
    earlier: &WrittenFacade,
    moved: &[FacadeEntry],
    visibility: &str,
) -> TextEdit {
    let now: Vec<&FacadeEntry> = moved
        .iter()
        .filter(|entry| entry.visibility == visibility)
        .collect();
    let destination = &now[0].destination;
    let all: Vec<_> = earlier
        .modules
        .iter()
        .map(|module| FacadeEntry {
            destination: destination.clone(),
            visibility: visibility.to_string(),
            module: module.clone(),
        })
        .chain(now.into_iter().cloned())
        .collect();
    let line = facade_lines_for_plan(&all).join("\n");
    manifest_edits::replacement(text, earlier.span.clone(), &format!("{line}\n"))
}

/// The declaration of `member`'s module as a move removes it, or the refusal when the file declares
/// none or it carries an attribute.
fn declaration_of(
    path: &str,
    text: &str,
    member: &Move,
) -> Result<manifest_edits::RemovedDeclaration> {
    manifest_edits::removed_declaration(text, path, &member.module)?
        .ok_or_else(|| member.declared_nowhere())
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
    visibility: &str,
    destination_root: &str,
) -> Option<WrittenFacade> {
    let prefix = if visibility.is_empty() {
        format!("use {extern_name}::")
    } else {
        format!("{visibility} use {extern_name}::")
    };
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

        let Some(tree) = manifest_edits::after_visibility(line)
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
            let comments = comments_of_declaration(workspace, member)?;
            Ok(manifest_edits::insert_module_declaration_sorted(
                &text,
                &format!("pub mod {};", member.module),
                &comments,
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(vec![FileEdit::Change { path, edits }])
}

/// The comment lines directly above `member`'s declaration in the file that declares it, which
/// travel to the destination's declaration.
fn comments_of_declaration(workspace: &Workspace<'_>, member: &Move) -> Result<String> {
    let declared_in = &member.home.declared_in;
    let text = workspace.read(declared_in)?;
    Ok(declaration_of(declared_in, &text, member)?.comments)
}
