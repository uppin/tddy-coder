//! What a cross-crate move carries below a module: its directory children.
//!
//! A Rust 2018 module `a` is `a.rs`, and the files of its children sit in `a/`. A move of `a`
//! that renames `a.rs` alone strands them (`E0583`), so the move carries every file
//! [`files_of`](super::module_files::files_of) finds below the module, each to the same place relative to it under
//! the destination's `src/`. This module holds what the set of carried files decides on its own:
//! which plan members are already carried by another, which children cannot be carried, the note
//! naming what moved, and the refusal for a restricted child something outside the tree reaches.

use crate::plan::RefactorOp;
use crate::registry::Workspace;

use super::preconditions::{
    body_path_left_behind, home_of_anchor, moved_by_earlier_operations, travelling_modules,
};
use super::{malformed, manifest_edits, ModuleHome, Move, MovingCluster, PlannedRewrite, Result};
use crate::RestructureError;

/// The members of a cluster with every member a co-moving member carries taken out.
///
/// A member whose module path lies under another member's (`a::b` beside `a`) is one of that
/// member's directory children: it moves with it, at its nested position, so it is neither
/// renamed to the destination's root nor declared there, and its parent's `mod b;` line stays.
/// Order is kept for the members that remain.
pub(crate) fn fold_carried_members(members: Vec<ModuleHome>) -> Vec<ModuleHome> {
    let carried_by_another: Vec<bool> = members
        .iter()
        .map(|member| {
            members.iter().any(|other| {
                other.crate_dir == member.crate_dir
                    && other.path.len() < member.path.len()
                    && member.path.starts_with(&other.path)
            })
        })
        .collect();
    members
        .into_iter()
        .zip(carried_by_another)
        .filter_map(|(member, carried)| (!carried).then_some(member))
        .collect()
}

/// Every reason operation `index` of `ops` (`op`) cannot carry one of its modules' children,
/// read statically, so a plain `check` reports what `apply` would refuse.
///
/// One finding per child, naming its file: a `mod x;` that leads to neither `x.rs` nor
/// `x/mod.rs`; a `mod` placed with `#[path]`; a target already present in the destination; a child
/// another operation of the plan moves on its own; a child's body reaching a module that stays
/// behind.
///
/// A member another member of the same operation carries is not read on its own: its parent's
/// tree already holds every one of its files.
///
/// # Errors
///
/// Refuses when a file the check must read cannot be.
pub(crate) fn uncarriable(
    workspace: &Workspace<'_>,
    ops: &[RefactorOp],
    index: usize,
    op: &RefactorOp,
) -> Result<Vec<String>> {
    if carried_by_another_anchor(workspace, op)? {
        return Ok(Vec::new());
    }
    let moving = Move::read(workspace, op)?;
    let files = match moving.carried(workspace) {
        Ok(files) => files,
        // The move would refuse the same tree, so the check says so too.
        Err(RestructureError::MalformedPlan(finding)) => return Ok(vec![finding]),
        Err(other) => return Err(other),
    };

    let mut findings = Vec::new();
    for file in &files {
        if workspace.read(&file.from)?.lines().any(places_a_module) {
            findings.push(format!(
                "`{}` places a module with `#[path]`, which a move cannot follow — the file it \
                 names would be left where it is",
                file.from
            ));
        }
    }

    let earlier = moved_by_earlier_operations(workspace, ops, index, &op.anchor);
    let travelling = travelling_modules(workspace, op, &earlier)?;
    for file in files.iter().skip(1) {
        if workspace.root.join(&file.to).exists() {
            findings.push(format!(
                "`{destination}` already has {target} — carrying `{from}` there with `{module}` \
                 would be a merge, which no operation performs",
                destination = moving.destination.package,
                target = file.to,
                from = file.from,
                module = moving.module,
            ));
        }
        findings.extend(moved_on_its_own(ops, index, &moving, &file.from));

        let module_path: Vec<String> = moving
            .home
            .path
            .iter()
            .chain(&file.below)
            .cloned()
            .collect();
        findings.extend(body_path_left_behind(
            workspace,
            &moving,
            &file.from,
            &module_path,
            &travelling,
        )?);
    }
    Ok(findings)
}

/// Whether `op`'s own module is a directory child of another module its operation moves.
fn carried_by_another_anchor(workspace: &Workspace<'_>, op: &RefactorOp) -> Result<bool> {
    let homes = op
        .anchors()
        .map(|anchor| home_of_anchor(workspace, anchor))
        .collect::<Result<Vec<_>>>()?;
    let own = home_of_anchor(workspace, &op.anchor)?;
    Ok(!fold_carried_members(homes).contains(&own))
}

/// Whether a source line is a `#[path = "…"]` attribute.
fn places_a_module(line: &str) -> bool {
    line.trim_start()
        .strip_prefix("#[path")
        .is_some_and(|rest| rest.trim_start().starts_with('='))
}

/// The finding for a carried file that another operation of the plan anchors.
fn moved_on_its_own(ops: &[RefactorOp], index: usize, moving: &Move, file: &str) -> Option<String> {
    let other = ops
        .iter()
        .enumerate()
        .filter(|(other, op)| *other != index && op.op.moves_across_crates())
        .find(|(_, op)| op.anchors().any(|anchor| anchor.file() == file))
        .map(|(other, _)| other)?;
    Some(format!(
        "`{file}` moves with `{module}` in operation {index}, and operation {other} moves it on \
         its own — a child goes where its module goes, so name it in that operation's `also` or \
         drop operation {other}",
        module = moving.module,
    ))
}

/// One note per member that carries more than its own file:
/// "`{n}` file(s) move with `{module}`, with the directory of its children".
///
/// # Errors
///
/// Refuses when a member's files cannot be read.
pub(crate) fn carried_notes(
    workspace: &Workspace<'_>,
    cluster: &MovingCluster,
) -> Result<Vec<String>> {
    let mut notes = Vec::new();
    for home in fold_carried_members(cluster.members.clone()) {
        let member = Move::of(workspace, &home, &cluster.destination, cluster.reexport)?;
        let files = member.carried(workspace)?;
        if files.len() > 1 {
            notes.push(format!(
                "{} file(s) move with `{}`, with the directory of its children",
                files.len(),
                member.module
            ));
        }
    }
    Ok(notes)
}

/// Refuse a move whose callers outside the carried tree reach a child the tree declares
/// `pub(crate)`, `pub(super)`, `pub(in …)` or private, through the child's own path: after the
/// move that path is private to the destination (`E0603`). The refusal names the child and every
/// referring file. Widening the child instead is not this operation's to do.
///
/// # Errors
///
/// The refusal, and anything reading the carried files refuses.
pub(crate) fn restricted_children_reached(
    workspace: &Workspace<'_>,
    member: &Move,
    rewrites: &[PlannedRewrite],
) -> Result<()> {
    let restricted = restricted_children(workspace, member)?;
    if restricted.is_empty() {
        return Ok(());
    }

    let mut reached: Vec<(&RestrictedChild, String, &str)> = Vec::new();
    for rewrite in rewrites {
        let text = workspace.read(&rewrite.path)?;
        let written = text[rewrite.span.clone()].to_string();
        let segments: Vec<&str> = written.split("::").collect();
        let Some(at) = segments
            .iter()
            .position(|segment| *segment == member.module)
        else {
            continue;
        };
        let inside = &segments[at + 1..];
        for child in restricted
            .iter()
            .filter(|child| inside.starts_with(&child.below_strs()))
        {
            reached.push((child, written.clone(), &rewrite.path));
        }
    }
    let Some((child, written, _)) = reached.first() else {
        return Ok(());
    };

    let mut callers: Vec<&str> = reached.iter().map(|(.., path)| *path).collect();
    callers.sort_unstable();
    callers.dedup();
    Err(malformed(format!(
        "`{written}` reaches the child `{declaration}` of `{module}`, which {declared_in} declares \
         that way — after the move that path is private to `{destination}`, and {callers} name(s) \
         it from outside the moved tree. Widen the declaration to `pub mod {name};` before \
         moving, or stop naming the child through its own path",
        declaration = child.declaration(),
        module = member.module,
        declared_in = child.declared_in,
        destination = member.destination.package,
        callers = callers.join(", "),
        name = child.name,
    )))
}

/// A carried child its parent does not declare `pub`.
struct RestrictedChild {
    /// The modules from the moved module down to the child, outermost first.
    below: Vec<String>,
    name: String,
    visibility: String,
    declared_in: String,
}

impl RestrictedChild {
    fn below_strs(&self) -> Vec<&str> {
        self.below.iter().map(String::as_str).collect()
    }

    /// The declaration as the parent wrote it, without its semicolon: `pub(crate) mod c`.
    fn declaration(&self) -> String {
        if self.visibility.is_empty() {
            format!("mod {}", self.name)
        } else {
            format!("{} mod {}", self.visibility, self.name)
        }
    }
}

/// Every file-backed child of the moved module whose declaration is not `pub`.
///
/// A child's parent is the carried file for the nearest module above it that has a file; an inline
/// module between them holds the declaration and has none.
fn restricted_children(workspace: &Workspace<'_>, member: &Move) -> Result<Vec<RestrictedChild>> {
    let files = member.carried(workspace)?;
    let mut restricted = Vec::new();
    for file in files.iter().filter(|file| !file.below.is_empty()) {
        let Some(parent) = files
            .iter()
            .filter(|other| {
                other.below.len() < file.below.len() && file.below.starts_with(&other.below)
            })
            .max_by_key(|other| other.below.len())
        else {
            continue;
        };
        let name = file.below[file.below.len() - 1].clone();
        let text = workspace.read(&parent.from)?;
        let Some(declaration) = manifest_edits::declared_module(&text, &name) else {
            continue;
        };
        if declaration.visibility != "pub" {
            restricted.push(RestrictedChild {
                below: file.below.clone(),
                name,
                visibility: declaration.visibility,
                declared_in: parent.from.clone(),
            });
        }
    }
    Ok(restricted)
}
