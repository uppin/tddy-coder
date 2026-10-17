//! A crate a cross-crate move creates, because its plan line names one with `name`.
//!
//! Scaffolding a crate used to be refused as authoring rather than moving. The facts a skeleton
//! needs are all in the repository, though: the package name is the plan's, the `version` and
//! `edition` lines are the origin's, copied as written, and the `members` entry is the one a move
//! into an unlisted crate already writes. So `move_module_to_crate` and `move_cluster_to_crate`
//! create the crate when — and only when — their line carries `name`, in the same edit as the move.
//!
//! The skeleton is resolved into first, so every edit the move makes to the new manifest and root
//! reads them as text that exists, and is then folded into one creation and one change per file.

use crate::crate_move::destination::{declared_package_name, Destination};
use crate::crate_move::manifest_edits;
use crate::edit::{FileEdit, TextEdit, WorkspaceEdit};
use crate::overlay::Overlay;
use crate::plan::RefactorOp;
use crate::registry::Workspace;
use crate::RestructureError;

use super::Result;

/// The crate a move creates: where, and under which `[package] name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCrate {
    /// The new crate's directory, relative to the repository root — the plan's `to`.
    pub dir: String,
    /// Its `[package] name` — the plan's `name`.
    pub package: String,
}

impl NewCrate {
    /// The crate `op` creates, when it is a module crate move whose line carries `name`.
    pub(crate) fn named_by(op: &RefactorOp) -> Option<NewCrate> {
        if !op.op.moves_across_crates() {
            return None;
        }
        Some(NewCrate {
            dir: op.to.clone()?,
            package: op.name.clone()?,
        })
    }

    /// The destination this crate will be, without reading a manifest that does not exist yet.
    pub(crate) fn destination(&self) -> Destination {
        Destination {
            dir: self.dir.clone(),
            package: self.package.clone(),
            extern_name: self.package.replace('-', "_"),
        }
    }

    /// Why this crate cannot be created in `workspace`, moving code out of `origin`, if it cannot:
    /// a crate already at `dir`, a directory that is not empty, an extern name the origin or a
    /// listed member already has, or a workspace with no `members` array.
    pub(crate) fn refusal(
        &self,
        workspace: &Workspace<'_>,
        origin: &Destination,
    ) -> Result<Option<String>> {
        let dir = &self.dir;
        if holds(workspace, &format!("{dir}/Cargo.toml"))? {
            return Ok(Some(format!(
                "`{dir}` is already a crate — drop `name` to move into it"
            )));
        }
        if !is_empty_or_missing(workspace, dir)? {
            return Ok(Some(format!("`{dir}` is not empty")));
        }

        let extern_name = self.destination().extern_name;
        if extern_name == origin.extern_name {
            return Ok(Some(already_the_extern_name_of(&extern_name, &origin.dir)));
        }
        let root = workspace.read("Cargo.toml")?;
        let Some(members) = manifest_edits::members_list(&root) else {
            return Ok(Some(format!(
                "the workspace lists no `members`, so cargo would not build `{}`",
                self.package
            )));
        };
        // A glob entry names no one directory, and a listed directory with no manifest holds no
        // crate that could clash: neither is this check's to read.
        for member in root[members].split('"').skip(1).step_by(2) {
            let manifest = format!("{member}/Cargo.toml");
            if !member.contains('*') && holds(workspace, &manifest)? {
                let text = workspace.read(&manifest)?;
                let held = declared_package_name(&text).map(|package| package.replace('-', "_"));
                if held.as_deref() == Some(extern_name.as_str()) {
                    return Ok(Some(already_the_extern_name_of(&extern_name, member)));
                }
            }
        }
        Ok(None)
    }

    /// The skeleton the move resolves into: `[package]` with this name and the origin's `version`
    /// and `edition` lines verbatim, and an empty `src/lib.rs`.
    pub(crate) fn skeleton(
        &self,
        workspace: &Workspace<'_>,
        origin: &Destination,
    ) -> Result<Skeleton> {
        let origin_manifest = workspace.read(&format!("{}/Cargo.toml", origin.dir))?;
        let mut manifest = format!("[package]\nname = \"{}\"\n", self.package);
        for key in ["version", "edition"] {
            if let Some(line) = manifest_edits::package_line(&origin_manifest, key) {
                manifest.push_str(line);
                manifest.push('\n');
            }
        }
        Ok(Skeleton {
            manifest,
            root: String::new(),
            created: [
                format!("{}/Cargo.toml", self.dir),
                format!("{}/src/lib.rs", self.dir),
            ],
        })
    }
}

/// The destination an operation moves into: the crate it creates, which has no manifest to read
/// yet, or the one `to` names, read through `workspace`'s overlay.
///
/// # Errors
///
/// Refuses as [`Destination::read_in`] does.
pub(crate) fn destination_of(
    workspace: &Workspace<'_>,
    op: &RefactorOp,
    to: &str,
) -> Result<Destination> {
    match NewCrate::named_by(op) {
        Some(creation) => Ok(creation.destination()),
        None => Destination::read_in(workspace, to),
    }
}

fn already_the_extern_name_of(extern_name: &str, dir: &str) -> String {
    format!("`{extern_name}` is already the extern name of `{dir}`")
}

/// Whether `path` is a file `workspace` reads: on disk, or in its overlay.
fn holds(workspace: &Workspace<'_>, path: &str) -> Result<bool> {
    match workspace.read(path) {
        Ok(_) => Ok(true),
        Err(RestructureError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

/// Whether `dir` is absent or holds nothing, so creating a crate in it writes over no file.
fn is_empty_or_missing(workspace: &Workspace<'_>, dir: &str) -> Result<bool> {
    match std::fs::read_dir(workspace.root.join(dir)) {
        Ok(mut entries) => Ok(entries.next().is_none()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
    }
}

/// The two files a new crate starts as, before the move fills them.
pub(crate) struct Skeleton {
    /// `<dir>/Cargo.toml` as created.
    pub(crate) manifest: String,
    /// `<dir>/src/lib.rs` as created.
    pub(crate) root: String,
    /// The two created paths, manifest first.
    pub(crate) created: [String; 2],
}

impl Skeleton {
    /// Both files as the one edit that creates them: a creation and a change inserting their text.
    pub(crate) fn creation(&self) -> WorkspaceEdit {
        let texts = [&self.manifest, &self.root];
        let mut changes: Vec<FileEdit> = self
            .created
            .iter()
            .map(|path| FileEdit::Create { path: path.clone() })
            .collect();
        for (path, text) in self.created.iter().zip(texts) {
            if !text.is_empty() {
                changes.push(inserted(path, text));
            }
        }
        WorkspaceEdit { changes }
    }
}

/// The change that writes all of `text` into the empty file at `path`.
fn inserted(path: &str, text: &str) -> FileEdit {
    FileEdit::Change {
        path: path.to_string(),
        edits: vec![manifest_edits::replacement("", 0..0, text)],
    }
}

/// The move's `changes` with every edit to a created file folded into its skeleton text: one
/// `FileEdit::Create` and one `FileEdit::Change` per created file, so no path gets two changes.
pub(crate) fn folded(skeleton: &Skeleton, changes: Vec<FileEdit>) -> Result<Vec<FileEdit>> {
    let mut folded: Vec<FileEdit> = skeleton
        .created
        .iter()
        .map(|path| FileEdit::Create { path: path.clone() })
        .collect();
    let mut into_created: [Vec<TextEdit>; 2] = Default::default();

    for change in changes {
        match change {
            FileEdit::Change { path, edits } => {
                match skeleton.created.iter().position(|created| *created == path) {
                    Some(created) => into_created[created].extend(edits),
                    None => folded.push(FileEdit::Change { path, edits }),
                }
            }
            other => folded.push(other),
        }
    }

    let texts = [&skeleton.manifest, &skeleton.root];
    for ((path, text), edits) in skeleton.created.iter().zip(texts).zip(&into_created) {
        let text = crate::apply::edited(text.clone(), edits)?;
        if !text.is_empty() {
            folded.push(inserted(path, &text));
        }
    }
    Ok(folded)
}

/// An overlay over `workspace` holding the skeleton of every crate an operation before `index`
/// creates, so a plain `check` reads operation `index` at its place in the plan.
///
/// A creation that is itself refused is left out: it is reported on its own, and seeding a
/// skeleton over a crate that exists would make every later read disagree with the tree.
pub(crate) fn created_by_earlier_operations(
    workspace: &Workspace<'_>,
    ops: &[RefactorOp],
    index: usize,
) -> Result<Overlay> {
    let mut overlay = workspace.overlay.clone();
    for creation in ops[..index]
        .iter()
        .filter_map(|op| NewCrate::named_by(op).map(|creation| (op, creation)))
    {
        let (op, creation) = creation;
        let seeded = Workspace {
            root: workspace.root,
            overlay: &overlay,
        };
        let home = super::preconditions::home_of_anchor(&seeded, &op.anchor)?;
        let origin = Destination::read_in(&seeded, &home.crate_dir)?;
        if creation.refusal(&seeded, &origin)?.is_some() {
            continue;
        }
        let skeleton = creation.skeleton(&seeded, &origin)?;
        overlay.record(workspace.root, &skeleton.creation())?;
    }
    Ok(overlay)
}
