//! `repoint_facade_imports`: naming what a file uses by the crate that defines it.
//!
//! A module that is to move into another crate must name what it uses by its defining crate, or
//! the move presents an edge back to the crate it leaves. This operation applies the path survey's
//! answer (`crate_move::survey`) to one file or one module and writes nothing else: no facade is
//! removed, no manifest is edited, and no server is asked anything. See
//! `docs/dev/1-WIP/2026-10-05-sharpen-repoint-facade.md` for the rules.
//!
//! The work is split: [`scope`] says which files an anchor names, [`rewrite`] says which surveyed
//! paths go through a facade of another crate and holds each to its preconditions, and
//! [`use_group`](crate::crate_move::use_group) rewrites one `use` statement — in place when every
//! member agrees, by splitting it when they do not: the one rule the cross-crate moves read as well.
//! What is here is the walk over the files, the text edits built from those answers, and the
//! account `check --deep` and `apply` print.

mod refusals;
mod rewrite;
mod scope;

use super::early_return::masked_to_code;
use super::item_move::text::{line_end, line_start};
use super::RustBackend;
use crate::crate_move::use_group::use_statements;
use crate::crate_move::{manifest_edits, survey, Destination};
use crate::edit::{FileEdit, Resolution, TextEdit, WorkspaceEdit};
use crate::item_anchor::{module_path_of, owning_package};
use crate::plan::{Anchor, RefactorOp};
use crate::registry::Workspace;
use crate::Result;

use crate::crate_move::use_group::refusals::unreadable_use;
use crate::crate_move::use_group::{group_rewrite, is_a_group, last_segment, Leaf};
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
    let mut taken = vec![false; rewrites.len()];
    let mut edits = use_statement_edits(file, text, &statements, rewrites, &mut taken)?;
    edits.extend(body_path_edits(text, rewrites, &taken)?);
    Ok(edits)
}

/// A replacement per `use` statement that holds a path to rewrite: the statement's own tree when it
/// is a group, and its path when it is a plain import. Marks each path it handled in `taken`, so the
/// body pass leaves it alone.
fn use_statement_edits(
    file: &str,
    text: &str,
    statements: &[std::ops::Range<usize>],
    rewrites: &mut [Rewrite],
    taken: &mut [bool],
) -> Result<Vec<TextEdit>> {
    let mut edits = Vec::new();
    for span in statements {
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
            // TODO(reshape-8-hand-fix): the shared group rule reads `Leaf`s, not this module's `Rewrite`.
            let leaves: Vec<Leaf> = inside
                .iter()
                .map(|index| Leaf {
                    written: rewrites[*index].written.clone(),
                    rewritten: rewrites[*index].defined_at.clone(),
                })
                .collect();
            let rewritten = group_rewrite(file, text, span, &leaves)?;
            if rewritten.contains('\n') {
                mark_split_from_group(rewrites, &inside);
            }
            edits.push(manifest_edits::replacement(text, span.clone(), &rewritten));
        } else {
            let index = inside[0];
            let written = &rewrites[index].written;
            let at = span.start
                + statement
                    .find(written)
                    .ok_or_else(|| unreadable_use(statement))?;
            let new = plain_path(text, at, &rewrites[index]);
            edits.push(manifest_edits::replacement(
                text,
                at..at + written.len(),
                &new,
            ));
        }
    }
    Ok(edits)
}

/// A replacement of the path itself, per rewrite no `use` statement handled — the paths in bodies.
fn body_path_edits(text: &str, rewrites: &[Rewrite], taken: &[bool]) -> Result<Vec<TextEdit>> {
    let mut edits = Vec::new();
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

/// Mark each of `inside` as a member of a grouped `use` that left the group (Rule S), so the note
/// says so.
fn mark_split_from_group(rewrites: &mut [Rewrite], inside: &[usize]) {
    for index in inside {
        rewrites[*index].split_from_group = true;
    }
}

/// The replacement of a plain `use`'s path, keeping the name the import bound when the rewrite
/// would change its last segment.
fn plain_path(text: &str, at: usize, rewrite: &Rewrite) -> String {
    let after = at + rewrite.written.len();
    let has_alias = text[after..].trim_start().starts_with("as ");
    // TODO(reshape-8-hand-fix): `last_segment` is `use_group`'s now; `rewrite::last_segment` is gone.
    let was = last_segment(&rewrite.written);
    if !has_alias && last_segment(&rewrite.defined_at) != was {
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
        .ok_or_else(|| unreadable_use(written))
}

/// The one-based first and last line a byte span covers.
fn span_lines(text: &str, span: &std::ops::Range<usize>) -> (u32, u32) {
    let first = manifest_edits::position_of(text, span.start).line;
    let last = manifest_edits::position_of(text, span.end.saturating_sub(1)).line;
    (first, last)
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
