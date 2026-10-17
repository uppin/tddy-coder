use crate::crate_move::PlannedRewrite;
use crate::crate_move::Result;
use crate::registry::Workspace;
use crate::{
    crate_move::{
        manifest_edits,
        use_group::{self, Leaf},
    },
    edit::{FileEdit, TextEdit},
};
use std::collections::BTreeSet;

/// One `FileEdit::Change` per caller, carrying every path in that file at once.
///
/// `rewrites` are every path the whole move re-points, so a `use` group naming two moved modules is
/// written once — see [`caller_edits`].
///
/// # Errors
///
/// Refuses for every reason [`caller_edits`] does.
pub(crate) fn caller_changes(
    workspace: &Workspace<'_>,
    rewrites: Vec<PlannedRewrite>,
) -> Result<Vec<FileEdit>> {
    let paths: BTreeSet<String> = rewrites
        .iter()
        .map(|rewrite| rewrite.path.clone())
        .collect();

    let mut changes = Vec::new();
    for path in paths {
        let text = workspace.read(&path)?;
        let in_the_file: Vec<&PlannedRewrite> = rewrites
            .iter()
            .filter(|rewrite| rewrite.path == path)
            .collect();
        let edits = caller_edits(&path, &text, &in_the_file)?;
        changes.push(FileEdit::Change { path, edits });
    }
    Ok(changes)
}

/// The edits one caller file needs. A path in a plain `use` or in a body is replaced where it
/// stands; paths inside one grouped `use` are one [`use_group`] edit of the whole statement, so the
/// group keeps what stays and the re-pointed members leave it, rather than a new crate name being
/// spliced into someone else's group.
///
/// # Errors
///
/// Refuses a group that must split while an attribute or doc comment sits above it, and one the
/// group rule cannot read.
fn caller_edits(file: &str, text: &str, rewrites: &[&PlannedRewrite]) -> Result<Vec<TextEdit>> {
    let mut edits = Vec::new();
    let mut groups: Vec<(std::ops::Range<usize>, Vec<Leaf>)> = Vec::new();

    for rewrite in rewrites {
        let group = use_group::statement_containing(text, rewrite.span.start)
            .filter(|span| use_group::is_a_group(&text[span.clone()]));
        let Some(span) = group else {
            edits.push(manifest_edits::replacement(
                text,
                rewrite.span.clone(),
                &rewrite.to,
            ));
            continue;
        };
        let leaf = Leaf {
            written: use_group::leaf_path(
                &text[span.clone()],
                rewrite.span.start - span.start,
                &text[rewrite.span.clone()],
            ),
            rewritten: rewrite.to.clone(),
        };
        match groups.iter_mut().find(|(known, _)| *known == span) {
            Some((_, leaves)) => leaves.push(leaf),
            None => groups.push((span, vec![leaf])),
        }
    }

    for (span, leaves) in groups {
        edits.push(use_group::statement_edit(file, text, span, &leaves)?);
    }
    Ok(edits)
}
