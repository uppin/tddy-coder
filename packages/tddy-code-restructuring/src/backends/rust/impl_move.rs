//! `move_impl_members`: moving a run of members of one inherent `impl T` into another module of the
//! same crate.
//!
//! rust-analyzer has no assist for this, and `extract_module`'s inherent-member path can only write
//! a new child module of the file and widens every private member to `pub(crate)`. Like
//! `move_item`, this operation is authored here and informed by the server: where the members are
//! comes from the outline, who calls them from `textDocument/references`, and everything else is a
//! function of the files' text, in [`assemble`]. No caller is edited — a method resolves through its
//! type wherever its block is written. See `packages/tddy-code-restructuring/docs/impl-move.md`.

pub(super) mod assemble;
mod cut;
pub(super) mod landing;
mod preflight;
mod survey;

use std::collections::BTreeMap;

use serde_json::Value;

use super::item_move::creation::{self, Destination};
use super::item_move::destination::{find_module, package_of, Lookup};
use super::item_move::preflight::{anchored, does_not_exist, Named};
use super::retarget_impl::outline::{self, Operation, Run};
use super::{failure, seam_survey, uri_of, RustBackend};
use crate::edit::{FileEdit, Range, Resolution, WorkspaceEdit};
use crate::item_anchor::{module_path_of, span_of, unlowered_item_anchor};
use crate::plan::{Anchor, RefactorOp};
use crate::registry::Workspace;
use crate::Result;
use assemble::{Assembled, MovingMembers};
use survey::{Reach, Survey};

/// What a static check finds wrong with a `move_impl_members`, from the plan and the text alone:
/// P2 (the destination is in another crate), then P3–P5 (the destination, or the parent of the
/// module it creates, and whether the members already live there).
pub(super) fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    let origin = origin_module(op, workspace)?;
    let named = named_of(op, workspace)?;
    let crate_of_to = named
        .to
        .split("::")
        .next()
        .unwrap_or_default()
        .trim()
        .replace('-', "_");
    if crate_of_to != named.package.crate_name {
        return Ok(vec![format!(
            "`{}` is in `{crate_of_to}`, and the members are in `{}`: an inherent `impl` outside \
             the type's crate is `E0116`",
            named.to, named.package.crate_name
        )]);
    }
    destination_findings(workspace, &named, &origin)
}

/// The destination a `move_impl_members` names, read against the package its origin is in.
fn named_of(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Named> {
    let to = op.to.as_deref().unwrap_or_default();
    Ok(Named {
        package: package_of(workspace.root, op.anchor.file())?,
        to: to.to_string(),
        module: to
            .split("::")
            .skip(1)
            .map(|piece| piece.trim().to_string())
            .collect(),
        creates: op.name.clone(),
    })
}

/// The module the members are declared in, below the crate root. An item path names the `impl`'s
/// type before the member, so the module is the path's module with that type taken off its end. A
/// lowered anchor (a range, which run open leaves every item anchor as) names no path: the file's
/// module is the origin, as `move_item` reads it.
fn origin_module(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    match anchored(op) {
        Some((mut module, _)) => {
            module.pop();
            Ok(module)
        }
        None => Ok(module_path_of(workspace.root, op.anchor.file())?.split_off(1)),
    }
}

/// P3–P5 for a destination read against its package: a module that does not exist, a parent that
/// does not exist or already declares the name, and a destination that is the origin module.
fn destination_findings(
    workspace: &Workspace<'_>,
    named: &Named,
    origin: &[String],
) -> Result<Vec<String>> {
    let parent = match find_module(workspace, &named.package, &named.module)? {
        Lookup::Found(module) => module,
        Lookup::Missing { at } => return Ok(vec![does_not_exist(named, at)]),
    };
    match &named.creates {
        Some(name) => Ok(creation::within(workspace, named, name, parent)?.0),
        None if parent.path == origin => Ok(vec![format!(
            "the members are already in `{}`: the destination is the module they are declared in",
            named.to
        )]),
        None => Ok(Vec::new()),
    }
}

/// The module the members land in: the one `to` names, or the module `name` the line creates in it.
fn destination_of(workspace: &Workspace<'_>, named: &Named) -> Result<Destination> {
    let parent = match find_module(workspace, &named.package, &named.module)? {
        Lookup::Found(module) => module,
        Lookup::Missing { at } => return Err(failure(does_not_exist(named, at))),
    };
    match &named.creates {
        Some(name) => {
            let (_, created) = creation::within(workspace, named, name, parent)?;
            created.ok_or_else(|| failure(format!("`{name}` names no module to create")))
        }
        None => Ok(Destination {
            module: parent,
            parent: None,
        }),
    }
}

/// What a member move has read before it surveys: the origin's text, its symbols and its run, and
/// the destination's module and text.
struct Reading {
    file: String,
    text: String,
    uri: String,
    symbols: Value,
    run: Run,
    origin: Vec<String>,
    named: Named,
    destination: Destination,
    destination_text: String,
}

impl Reading {
    /// The survey's view of the origin and the destination.
    fn survey(&self) -> Survey<'_> {
        Survey {
            package: &self.named.package,
            uri: &self.uri,
            file: &self.file,
            text: &self.text,
            symbols: &self.symbols,
            run: &self.run,
            source: &self.origin,
            destination: &self.destination.module,
        }
    }

    /// The move as assembly takes it, with what the survey found.
    fn moving<'a>(&'a self, reach: &'a Reach) -> MovingMembers<'a> {
        MovingMembers {
            source_file: &self.file,
            source_text: &self.text,
            source: &self.origin,
            run: &self.run,
            destination: &self.destination.module,
            destination_text: &self.destination_text,
            created: self
                .destination
                .parent
                .as_ref()
                .zip(self.named.creates.as_deref()),
            texts: &reach.texts,
            reached: &reach.items,
            widening: &reach.widening,
        }
    }
}

impl RustBackend {
    /// Resolve a `move_impl_members` into the edits to every file it changes.
    pub(super) fn move_impl_members(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> Result<Resolution> {
        // Re-run here, before any server, so `apply` refuses a plan a plain `check` would report.
        if let Some(finding) = findings(op, workspace)?.into_iter().next() {
            return Err(failure(finding));
        }
        let reading = self.read_members(op, workspace)?;
        let mut reach = self.survey_members(workspace, &reading.survey())?;
        preflight::refuse_a_name_bound_elsewhere(
            &reading.destination_text,
            &reading.destination.module.scope,
            &reading.origin,
            &reach.items,
        )?;
        with_parent_text(
            workspace,
            &reading.destination,
            &reading.file,
            &mut reach.texts,
        )?;
        let assembled = assemble::assemble(&reading.moving(&reach))?;
        Ok(resolution(assembled, &reading.destination))
    }

    /// The origin's text, its outline and its run, read from the server, and the destination.
    fn read_members(&mut self, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Reading> {
        let file = op.anchor.file();
        let origin = origin_module(op, workspace)?;
        let named = named_of(op, workspace)?;
        let destination = destination_of(workspace, &named)?;
        let destination_text = match destination.parent {
            Some(_) => String::new(),
            None => workspace.read(&destination.module.file)?,
        };
        let range = self.members_range_of(op, workspace)?;
        let text = workspace.read(file)?;
        let uri = uri_of(&workspace.root.join(file));
        self.start(workspace.root)?;
        self.did_open(&uri, &text)?;
        self.ensure_indexed(&uri)?;
        let symbols = self.settled_outline(&uri)?;
        let run = outline::read(&symbols, &text, range, Operation::MoveImplMembers)?;
        preflight::refuse_a_non_member(&text, &run)?;
        Ok(Reading {
            file: file.to_string(),
            text,
            uri,
            symbols,
            run,
            origin,
            named,
            destination,
            destination_text,
        })
    }

    /// The lines the anchor covers: as written when a run lowered it already, otherwise resolved the
    /// way an item anchor would.
    fn members_range_of(&mut self, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Range> {
        match &op.anchor {
            Anchor::Range { start, end, .. } => Ok(Range {
                start: *start,
                end: *end,
            }),
            Anchor::Item { .. } | Anchor::Items { .. } => span_of(&op.anchor, workspace.root, self),
            Anchor::Symbol { .. } => Err(unlowered_item_anchor(&op.anchor, "`move_impl_members`")),
        }
    }
}

/// The text of the parent that declares a created module, read when it is not the origin, which
/// the assembly already holds.
fn with_parent_text(
    workspace: &Workspace<'_>,
    destination: &Destination,
    origin: &str,
    texts: &mut BTreeMap<String, String>,
) -> Result<()> {
    if let Some(parent) = &destination.parent {
        if parent.file != origin && !texts.contains_key(&parent.file) {
            texts.insert(parent.file.clone(), workspace.read(&parent.file)?);
        }
    }
    Ok(())
}

/// The edits of an assembled move as a resolution: the file a created module lands in is created
/// first.
fn resolution(assembled: Assembled, destination: &Destination) -> Resolution {
    let mut changes = Vec::new();
    for (path, (old, new)) in assembled.files {
        if destination.parent.is_some() && path == destination.module.file {
            changes.push(FileEdit::Create { path: path.clone() });
        }
        changes.push(FileEdit::Change {
            path,
            edits: seam_survey::minimal_edits(&old, &new),
        });
    }
    Resolution {
        edit: WorkspaceEdit { changes },
        report: assembled.report,
        notes: assembled.notes,
    }
}
