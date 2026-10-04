//! What the text of the tree says about a module move, before any server exists.
//!
//! The new parent that is not there, the name it already declares, the destination that is inside
//! the module being moved, and a `#[path]` are all lexical. Reading them here lets `check` report
//! them without an index, and lets `apply` refuse a plan that cannot be honoured before it spawns
//! one. Every finding is reported rather than the first, because a plan is checked to be fixed in
//! one pass.

use super::super::early_return::masked_to_code;
use super::super::item_move::destination::{find_module, Lookup, Module};
use super::super::item_move::preflight::{does_not_exist, names_declared_in};
use crate::crate_move::module_files::{file_of_child, files_of};
use crate::registry::Workspace;
use crate::Result;

use super::declaration::{self, has_path_attribute, Declaration};
use super::reading::Reparent;
use super::relocation::{self, MovedFile};

/// What a move needs to know about the tree it is made in.
pub(super) struct Survey {
    pub(super) old_parent: Module,
    pub(super) parent_text: String,
    pub(super) declaration: Declaration,
    pub(super) new_parent: Module,
    pub(super) new_parent_text: String,
    /// The module's own file first, then every file of the modules below it.
    pub(super) files: Vec<MovedFile>,
}

/// A survey, or everything that obstructs the move.
pub(super) enum Reading {
    Obstructed(Vec<String>),
    Clear(Box<Survey>),
}

/// The old parent, its text, and the declaration of the module in it.
struct Old {
    module: Module,
    text: String,
    declaration: Declaration,
}

/// The new parent and its text.
struct New {
    module: Module,
    text: String,
}

pub(super) fn survey(workspace: &Workspace<'_>, request: &Reparent) -> Result<Reading> {
    let mut findings: Vec<String> = Vec::new();
    findings.extend(inside_itself(request));
    let old = old_parent(workspace, request, &mut findings)?;
    let new = new_parent(workspace, request, &mut findings)?;
    if let Some(new) = &new {
        findings.extend(name_taken(request, new));
    }
    let files = match (&old, &new) {
        (Some(old), Some(new)) => files_of_the_module(workspace, request, old, new, &mut findings)?,
        _ => Vec::new(),
    };

    Ok(match (old, new) {
        (Some(old), Some(new)) if findings.is_empty() => Reading::Clear(Box::new(Survey {
            old_parent: old.module,
            parent_text: old.text,
            declaration: old.declaration,
            new_parent: new.module,
            new_parent_text: new.text,
            files,
        })),
        _ => Reading::Obstructed(findings),
    })
}

/// A module cannot become its own descendant, nor its own parent's child twice.
fn inside_itself(request: &Reparent) -> Option<String> {
    let moved = request.module_path();
    request.named.module.starts_with(&moved).then(|| {
        format!(
            "`{}` is inside `{}`, the module being moved: a module cannot become its own \
             descendant",
            request.named.to,
            written(request, &moved)
        )
    })
}

fn written(request: &Reparent, module: &[String]) -> String {
    std::iter::once(request.named.package.crate_name.as_str())
        .chain(module.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join("::")
}

fn old_parent(
    workspace: &Workspace<'_>,
    request: &Reparent,
    findings: &mut Vec<String>,
) -> Result<Option<Old>> {
    let parent = written(request, &request.parent);
    let module = match find_module(workspace, &request.named.package, &request.parent)? {
        Lookup::Found(module) => module,
        Lookup::Missing { .. } => {
            findings.push(format!(
                "`{parent}` does not exist, so the module `{}` the anchor names is declared \
                 nowhere",
                request.name
            ));
            return Ok(None);
        }
    };
    if module.file != request.file {
        findings.push(format!(
            "`{parent}` is in {}, and the anchor is in {}: the anchor must be on the `mod` \
             declaration, in the file of the module that declares it",
            module.file, request.file
        ));
    }
    let text = workspace.read(&module.file)?;
    let Some(declaration) = declaration::find(&text, &module.scope, &request.name)? else {
        findings.push(format!(
            "`{parent}` declares no `mod {};`: `reparent_module` is anchored on the module's \
             declaration",
            request.name
        ));
        return Ok(None);
    };
    if let Some(finding) = cannot_be_followed(&declaration, &request.name, &parent) {
        findings.push(finding);
        return Ok(None);
    }
    Ok(Some(Old {
        module,
        text,
        declaration,
    }))
}

/// Why the declaration cannot be moved as a module with a file of its own, if it cannot.
fn cannot_be_followed(declaration: &Declaration, name: &str, parent: &str) -> Option<String> {
    if declaration.inline {
        return Some(format!(
            "`{name}` is an inline module of `{parent}`, with no file of its own to move: \
             `reparent_module` moves a module declared as `mod {name};`"
        ));
    }
    declaration
        .placed_by_path
        .then(|| path_attribute(name, parent))
}

fn path_attribute(name: &str, place: &str) -> String {
    format!(
        "`{name}` is placed with `#[path]` in `{place}`: its file is wherever the attribute says, \
         and this move cannot follow the attribute — remove it, or move the file by hand"
    )
}

fn new_parent(
    workspace: &Workspace<'_>,
    request: &Reparent,
    findings: &mut Vec<String>,
) -> Result<Option<New>> {
    let named = &request.named;
    let module = match find_module(workspace, &named.package, &named.module)? {
        Lookup::Found(module) => module,
        Lookup::Missing { at } => {
            findings.push(does_not_exist(named, at));
            return Ok(None);
        }
    };
    if module.scope.start != 0 {
        findings.push(format!(
            "`{}` is an inline module: the new parent must be a module with a file of its own, \
             which is where the declaration is written",
            named.to
        ));
        return Ok(None);
    }
    let text = workspace.read(&module.file)?;
    Ok(Some(New { module, text }))
}

fn name_taken(request: &Reparent, new: &New) -> Option<String> {
    names_declared_in(&new.text[new.module.scope.clone()])
        .contains(&request.name)
        .then(|| {
            format!(
                "`{}` is already declared in `{}`, so moving a second one there would be `E0428`",
                request.name, request.named.to
            )
        })
}

/// The files the module spans and where each goes, with a finding for each file that cannot move.
fn files_of_the_module(
    workspace: &Workspace<'_>,
    request: &Reparent,
    old: &Old,
    new: &New,
    findings: &mut Vec<String>,
) -> Result<Vec<MovedFile>> {
    let root = workspace.root;
    let old_directory = relocation::directory_of(root, &old.module)?;
    let Some((module_file, _)) = file_of_child(workspace, &old_directory, &request.name) else {
        findings.push(format!(
            "`mod {};` leads to neither `{}.rs` nor `{}/mod.rs` in {}",
            request.name,
            request.name,
            request.name,
            old_directory.display()
        ));
        return Ok(Vec::new());
    };
    let files = files_of(workspace, &module_file)?;
    let new_directory = relocation::directory_of(root, &new.module)?;
    let moved = relocation::plan(&old_directory, &new_directory, &request.name, &files);

    for file in &moved {
        if workspace.read(&file.to).is_ok() {
            findings.push(format!(
                "{} already exists, where {} would move to",
                file.to, file.from
            ));
        }
        if has_path_attribute(&masked_to_code(&workspace.read(&file.from)?)) {
            findings.push(format!(
                "{} places a module with `#[path]`, which this move cannot follow: its file would \
                 stay where the attribute says",
                file.from
            ));
        }
    }
    Ok(moved)
}
