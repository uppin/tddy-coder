//! What a cross-crate move has to widen so the code it leaves behind still compiles.
//!
//! Across a crate boundary there is no visibility narrower than `pub`, so "as little as needed" is a
//! question of **which** declarations are widened, never of how far: a declaration of a moving file
//! is widened when a file the move does not carry, outside the destination crate, still reaches it
//! (`reach`); when the module's parent re-exports it by glob and it is not private (`glob`); or when
//! the signature of a declaration widened for either reason names it (`escaping`). Each edit is
//! addressed by the position the engine reported for the declaration (`declaration`), never by its
//! name, and each is reported as a [`VisibilityChange`].

use std::collections::{BTreeMap, BTreeSet};

use super::manifest_edits::replacement;
use super::{ItemReferences, MovingCluster, Result, Survey};
use crate::edit::{FileEdit, Position, VisibilityChange, WorkspaceEdit};
use crate::registry::Workspace;

pub(crate) mod declaration;
pub(crate) mod escaping;
pub(crate) mod glob;
pub(crate) mod reach;

/// One declaration the move widens to `pub`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Widened {
    /// The moving file that declares it, relative to the repository root.
    pub(crate) file: String,
    /// Its name as the report states it: `AgentRoster`, `AgentRoster::broadcast`, `inner::Probe`.
    pub(crate) name: String,
    /// Where its name is written in `file`, one-based, in characters.
    pub(crate) declared_at: Position,
    /// Why it is widened when no reference of its own says so: through a parent's glob, or named by
    /// a widened signature. `None` when an outside reference reaches it.
    pub(crate) reason: Option<String>,
}

impl Widened {
    /// `declaration`, as the report names it, in the moving file `file`.
    fn of(file: &str, declaration: &ItemReferences, reason: Option<String>) -> Self {
        let containers = declaration
            .within
            .iter()
            .map(|within| container_name(within));
        Widened {
            file: file.to_string(),
            name: containers
                .chain([declaration.item.clone()])
                .collect::<Vec<_>>()
                .join("::"),
            declared_at: declaration.declared_at,
            reason,
        }
    }

    fn is_the_declaration_of(&self, other: &Widened) -> bool {
        self.file == other.file && self.declared_at == other.declared_at
    }
}

/// A container as the report names it: an `impl` block by the type it is for, anything else as the
/// server calls it — `impl AgentRoster` is `AgentRoster`, `impl<T> Roster<T>` is `Roster`.
fn container_name(container: &str) -> String {
    let Some(rest) = container
        .strip_prefix("impl")
        .filter(|rest| rest.starts_with([' ', '<']))
    else {
        return container.to_string();
    };
    let mut depth = 0usize;
    let after_generics = rest.trim_start().trim_start_matches(|c: char| {
        depth += usize::from(c == '<');
        let inside = depth > 0;
        depth -= usize::from(c == '>' && depth > 0);
        inside
    });
    after_generics
        .trim_start()
        .split(['<', ' '])
        .next()
        .unwrap_or_default()
        .to_string()
}

/// The visibility `declaration` is written with in `text`, the text of the moving file `file`:
/// `""` for a private one, `None` when nothing is declared where the server said.
///
/// # Errors
///
/// Refuses a declaration whose keyword is on a line above its name.
fn written_visibility(
    file: &str,
    text: &str,
    declaration: &ItemReferences,
) -> Result<Option<String>> {
    let found = declaration::visibility_in_file(file, text, declaration.declared_at)?;
    Ok(found.map(|(_, written)| written))
}

/// `edit`, with every widening the move needs merged into the change of the moving file it lands
/// in, and the report of them in moving-file and then source order.
///
/// # Errors
///
/// Refuses a reached declaration whose visibility keyword is on a line above its name.
pub(crate) fn widened(
    workspace: &Workspace<'_>,
    cluster: &MovingCluster,
    edit: WorkspaceEdit,
    surveys: &[Survey],
) -> Result<(WorkspaceEdit, Vec<VisibilityChange>)> {
    let moving = moving_files(&edit);
    let mut chosen = reach::reached(workspace, cluster, &moving, surveys)?;
    for survey in surveys {
        chosen.extend(glob::glob_visible(workspace, survey)?);
    }
    let texts = texts_of(workspace, surveys)?;
    chosen = once_each(chosen);
    let candidates: Vec<Widened> = escaping::type_declarations(&texts, surveys)
        .into_iter()
        .filter(|candidate| {
            !chosen
                .iter()
                .any(|one| one.is_the_declaration_of(candidate))
        })
        .collect();
    let escaping = escaping::escaping(&texts, &chosen, &candidates);
    chosen.extend(escaping);

    let order: Vec<&str> = surveys
        .iter()
        .map(|survey| survey.source.as_str())
        .collect();
    chosen.sort_by_key(|one| {
        let file = order.iter().position(|source| *source == one.file);
        (file, one.declared_at.line, one.declared_at.col)
    });
    widen_in(edit, &texts, &chosen)
}

/// Every file the edit moves, which is every file the move carries.
fn moving_files(edit: &WorkspaceEdit) -> BTreeSet<String> {
    edit.changes
        .iter()
        .filter_map(|change| match change {
            FileEdit::Rename { from, .. } => Some(from.clone()),
            FileEdit::Change { .. } | FileEdit::Create { .. } => None,
        })
        .collect()
}

fn texts_of(workspace: &Workspace<'_>, surveys: &[Survey]) -> Result<BTreeMap<String, String>> {
    surveys
        .iter()
        .map(|survey| Ok((survey.source.clone(), workspace.read(&survey.source)?)))
        .collect()
}

/// `found` with a declaration reached more than one way kept for the first, which is the reference
/// before the glob before a signature.
fn once_each(found: Vec<Widened>) -> Vec<Widened> {
    let mut once: Vec<Widened> = Vec::new();
    for one in found {
        if !once.iter().any(|kept| kept.is_the_declaration_of(&one)) {
            once.push(one);
        }
    }
    once
}

/// `edit` with `pub` written where each of `chosen` is declared, and one report line each. A
/// declaration already `pub` needs and gets nothing.
fn widen_in(
    mut edit: WorkspaceEdit,
    texts: &BTreeMap<String, String>,
    chosen: &[Widened],
) -> Result<(WorkspaceEdit, Vec<VisibilityChange>)> {
    let mut report = Vec::new();
    for one in chosen {
        let text = &texts[&one.file];
        let Some((span, written)) =
            declaration::visibility_in_file(&one.file, text, one.declared_at)?
        else {
            continue;
        };
        if written == "pub" {
            continue;
        }
        merge(&mut edit, &one.file, replacement(text, span, "pub "));
        report.push(VisibilityChange {
            item: one.name.clone(),
            from: if written.is_empty() {
                "private".to_string()
            } else {
                written
            },
            to: "pub".to_string(),
            reason: one.reason.clone(),
        });
    }
    Ok((edit, report))
}

/// `text_edit` joins the change that already addresses `file`, in the same coordinates, or is a
/// change of its own.
fn merge(edit: &mut WorkspaceEdit, file: &str, text_edit: crate::edit::TextEdit) {
    let existing = edit.changes.iter_mut().find_map(|change| match change {
        FileEdit::Change { path, edits } if path == file => Some(edits),
        _ => None,
    });
    match existing {
        Some(edits) => edits.push(text_edit),
        None => edit.changes.push(FileEdit::Change {
            path: file.to_string(),
            edits: vec![text_edit],
        }),
    }
}
