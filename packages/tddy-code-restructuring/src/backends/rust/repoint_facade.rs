//! `repoint_facade_imports`: naming what a file uses by the crate that defines it.
//!
//! A module that is to move into another crate must name what it uses by its defining crate, or
//! the move presents an edge back to the crate it leaves. This operation applies the path survey's
//! answer (`crate_move::survey`) to one file or one module and writes nothing else: no facade is
//! removed, no manifest is edited, and no server is asked anything. See
//! `docs/dev/1-WIP/2026-10-05-sharpen-repoint-facade.md` for the rules.
//!
//! The work is split: [`scope`] says which files an anchor names, [`rewrite`] says which surveyed
//! paths go through a facade of another crate and holds each to its preconditions, and [`group`]
//! rewrites one `use` statement — in place when every member agrees, by splitting it when they do
//! not. What is here is the walk over the files, the text edits built from those answers, and the
//! account `check --deep` and `apply` print.

mod group;
mod refusals;
mod rewrite;
mod scope;

use super::early_return::masked_to_code;
use super::item_move::text::{line_end, line_start, split_use, use_statements};
use super::RustBackend;
use crate::crate_move::{manifest_edits, survey, Destination};
use crate::edit::{FileEdit, Resolution, TextEdit, WorkspaceEdit};
use crate::item_anchor::{module_path_of, owning_package};
use crate::plan::{Anchor, RefactorOp};
use crate::registry::Workspace;
use crate::Result;

use rewrite::Rewrite;

/// What a static check finds wrong with a `repoint_facade_imports`, from the plan and the text of
/// the file a `symbol` anchor names alone: the refusals `resolve` gives for paths it cannot
/// rewrite. A module anchor needs `--deep` (it must be lowered), and says so.
pub(super) fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    if matches!(op.anchor, Anchor::Items { .. } | Anchor::Item { .. }) {
        return Ok(Vec::new());
    }
    match resolve_files(op, workspace) {
        Ok(_) => Ok(Vec::new()),
        Err(refusal) => Ok(vec![refusal.to_string()]),
    }
}

impl RustBackend {
    /// Resolve a `repoint_facade_imports` into the edits to the files its anchor names, and the
    /// notes listing every path rewritten (`file:line: written -> defined`).
    pub(super) fn repoint_facade_imports(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> Result<Resolution> {
        resolve_files(op, workspace)
    }
}

/// The edits and the account for every file the anchor names, or the first refusal.
fn resolve_files(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Resolution> {
    let files = scope::files_of(op, workspace)?;
    let mut changes = Vec::new();
    let mut accounted: Vec<(String, Vec<Rewrite>)> = Vec::new();

    for file in &files {
        let text = workspace.read(file)?;
        let (origin, module_path) = origin_of(workspace, file)?;
        let survey = survey::survey_moved_file(workspace, &text, &origin, &module_path)?;
        let manifest = workspace.read(&format!("{}/Cargo.toml", origin.dir))?;

        let mut rewrites = rewrite::path_edits(&text, &survey, &manifest, file)?;
        if rewrites.is_empty() {
            continue;
        }
        let edits = edits_for(file, &text, &mut rewrites)?;
        if edits.is_empty() {
            continue;
        }
        changes.push(FileEdit::Change {
            path: file.clone(),
            edits,
        });
        accounted.push((file.clone(), rewrites));
    }

    Ok(Resolution {
        edit: WorkspaceEdit { changes },
        report: Vec::new(),
        notes: notes_of(&files, &accounted),
    })
}

/// The file's own crate, and its module path inside that crate — the two things the survey reads a
/// path against.
fn origin_of(workspace: &Workspace<'_>, file: &str) -> Result<(Destination, Vec<String>)> {
    let (dir, _package) = owning_package(workspace.root, file)?;
    let origin = Destination::read(workspace.root, &dir.to_string_lossy())?;
    let mut module_path = module_path_of(workspace.root, file)?;
    if !module_path.is_empty() {
        module_path.remove(0);
    }
    Ok((origin, module_path))
}

/// The text edits for one file: a `use` statement per group or plain import it rewrites, and a
/// replacement per path in a body.
fn edits_for(file: &str, text: &str, rewrites: &mut [Rewrite]) -> Result<Vec<TextEdit>> {
    let masked = masked_to_code(text);
    let statements = use_statements(&masked);
    let mut edits = Vec::new();
    let mut taken = vec![false; rewrites.len()];

    for span in &statements {
        let (first, last) = span_lines(text, span);
        let inside: Vec<usize> = (0..rewrites.len())
            .filter(|index| {
                !taken[*index] && rewrites[*index].line >= first && rewrites[*index].line <= last
            })
            .collect();
        if inside.is_empty() {
            continue;
        }
        for index in &inside {
            taken[*index] = true;
        }

        let statement = &text[span.clone()];
        if is_a_group(statement) {
            let leaves: Vec<Rewrite> = inside
                .iter()
                .map(|index| rewrites[*index].clone())
                .collect();
            let rewritten = group_rewrite(file, text, span, statement, &leaves)?;
            if rewritten.contains('\n') {
                for index in &inside {
                    rewrites[*index].split_from_group = true;
                }
            }
            edits.push(manifest_edits::replacement(text, span.clone(), &rewritten));
        } else {
            let index = inside[0];
            let written = &rewrites[index].written;
            let at = span.start
                + statement
                    .find(written)
                    .ok_or_else(|| refusals::unreadable_use(statement))?;
            let new = plain_path(text, at, &rewrites[index]);
            edits.push(manifest_edits::replacement(
                text,
                at..at + written.len(),
                &new,
            ));
        }
    }

    for (index, rewrite) in rewrites.iter().enumerate() {
        if taken[index] {
            continue;
        }
        let at = offset_on_line(text, rewrite.line, &rewrite.written)?;
        edits.push(manifest_edits::replacement(
            text,
            at..at + rewrite.written.len(),
            &rewrite.defined_at,
        ));
    }

    Ok(edits)
}

/// One whole `use` item's replacement, with the indentation of the statement it replaced added to
/// every line after the first, and a refusal when a group that must split carries an attribute.
fn group_rewrite(
    file: &str,
    text: &str,
    span: &std::ops::Range<usize>,
    statement: &str,
    leaves: &[Rewrite],
) -> Result<String> {
    let line = manifest_edits::position_of(text, span.start).line;
    let rewritten = group::split_or_reprefix(statement, leaves)?;
    if rewritten.contains('\n') && attribute_above(text, line) {
        return Err(refusals::attribute_above_a_split(file, line));
    }

    let indent = indentation(text, span.start);
    if indent.is_empty() {
        Ok(rewritten)
    } else {
        Ok(rewritten.replace('\n', &format!("\n{indent}")))
    }
}

/// The replacement of a plain `use`'s path, keeping the name the import bound when the rewrite
/// would change its last segment.
fn plain_path(text: &str, at: usize, rewrite: &Rewrite) -> String {
    let after = at + rewrite.written.len();
    let has_alias = text[after..].trim_start().starts_with("as ");
    let was = rewrite::last_segment(&rewrite.written);
    if !has_alias && rewrite::last_segment(&rewrite.defined_at) != was {
        format!("{} as {was}", rewrite.defined_at)
    } else {
        rewrite.defined_at.clone()
    }
}

/// The byte offset of `written` on one-based `line`, refused when it is not there.
fn offset_on_line(text: &str, line: u32, written: &str) -> Result<usize> {
    let start = line_start(text, line);
    let end = line_end(text, line);
    text[start..end]
        .find(written)
        .map(|at| start + at)
        .ok_or_else(|| refusals::unreadable_use(written))
}

/// Whether a `use` item is a group — its tree carries a `{`.
fn is_a_group(statement: &str) -> bool {
    split_use(statement).is_some_and(|(_, tree)| tree.contains('{'))
}

/// The one-based first and last line a byte span covers.
fn span_lines(text: &str, span: &std::ops::Range<usize>) -> (u32, u32) {
    let first = manifest_edits::position_of(text, span.start).line;
    let last = manifest_edits::position_of(text, span.end.saturating_sub(1)).line;
    (first, last)
}

/// The whitespace that stands between the start of `offset`'s line and `offset`.
fn indentation(text: &str, offset: usize) -> String {
    let start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    text[start..offset]
        .chars()
        .take_while(|character| *character == ' ' || *character == '\t')
        .collect()
}

/// Whether the line above one-based `line` is an attribute or a doc comment.
fn attribute_above(text: &str, line: u32) -> bool {
    if line <= 1 {
        return false;
    }
    let previous = text
        .lines()
        .nth(line as usize - 2)
        .unwrap_or_default()
        .trim_start();
    previous.starts_with("#[")
        || previous.starts_with("///")
        || previous.starts_with("//!")
        || previous.starts_with("/**")
}

/// The account of what the operation would write, in the format rule 6 of the changeset names.
fn notes_of(files: &[String], accounted: &[(String, Vec<Rewrite>)]) -> Vec<String> {
    let total: usize = accounted.iter().map(|(_, rewrites)| rewrites.len()).sum();
    if total == 0 {
        return files
            .iter()
            .map(|file| {
                format!(
                    "repoint_facade_imports: nothing in {file} goes through a facade of another \
                     crate"
                )
            })
            .collect();
    }

    let mut notes = vec![format!(
        "repoint_facade_imports: {total} path(s) in {} file(s) go through a facade of another \
         crate",
        accounted.len()
    )];
    for (file, rewrites) in accounted {
        for rewrite in rewrites {
            let split = if rewrite.split_from_group {
                " (split out of a grouped `use`)"
            } else {
                ""
            };
            notes.push(format!(
                "  {file}:{}: {} -> {}{split}",
                rewrite.line, rewrite.written, rewrite.defined_at
            ));
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_indentation_of_the_statement_it_replaces() {
        assert_eq!(indentation("mod a {\n    use x::y;\n}\n", 12), "    ");
        assert_eq!(indentation("use x::y;\n", 0), "");
    }

    #[test]
    fn sees_an_attribute_or_doc_comment_above_a_statement() {
        assert!(attribute_above("#[cfg(unix)]\nuse a;\n", 2));
        assert!(attribute_above("/// The limits.\nuse a;\n", 2));
        assert!(!attribute_above("use a;\nuse b;\n", 2));
        assert!(!attribute_above("use a;\n", 1));
    }
}
