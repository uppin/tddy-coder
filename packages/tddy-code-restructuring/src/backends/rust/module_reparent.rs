//! `reparent_module`: moving a module, with the directory of its children, under a different parent
//! of the same crate.
//!
//! The sibling of `move_item`, and built on the same pieces: rust-analyzer has no "move module"
//! assist, so the operation is authored here and informed by the server. The server answers one
//! question — who names the module (`textDocument/references` on the name in its `mod` declaration)
//! — and the callers are re-pointed by `item_move`'s own re-pointing, so a `use`, an inline path and
//! a relative `self::`/`super::` form are all read the way a moved item's are.
//!
//! What is lexical is answered before a server exists ([`survey`]): a destination that is not there,
//! a name the new parent already declares, a destination inside the module itself, and a module
//! placed with `#[path]`. The moved files are moved, not printed again: only the paths that must
//! change are edited, so comments and formatting arrive as they were.

mod assemble;
mod declaration;
mod reading;
mod relocation;
mod survey;
mod visibility;

use serde_json::Value;

use super::item_move::sites::Site;
use super::{failure, uri_of, RustBackend};
use crate::edit::{FileEdit, Resolution, WorkspaceEdit};
use crate::plan::{Reexport, RefactorOp};
use crate::registry::Workspace;
use crate::Result;
use assemble::{assemble, Reparenting};
use survey::Reading;

/// What a static check finds wrong with a `reparent_module`, from the text alone.
pub(super) fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    let request = reading::read(workspace, op)?;
    Ok(match survey::survey(workspace, &request)? {
        Reading::Obstructed(found) => found,
        Reading::Clear(_) => Vec::new(),
    })
}

impl RustBackend {
    /// Resolve a `reparent_module` into the edits to every file it changes and the files it moves.
    pub(super) fn reparent_module(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> Result<Resolution> {
        let request = reading::read(workspace, op)?;
        let survey = match survey::survey(workspace, &request)? {
            Reading::Obstructed(found) => return Err(failure(found.join("; "))),
            Reading::Clear(survey) => *survey,
        };

        let sites = self.callers_of_the_module(workspace, &request, &survey)?;
        let assembled = assemble(&Reparenting {
            workspace,
            request: &request,
            survey: &survey,
            sites: &sites,
            reexport: op.reexport.unwrap_or(Reexport::None),
        })?;

        let mut changes: Vec<FileEdit> = assembled
            .files
            .into_iter()
            .map(|(path, (old, new))| FileEdit::Change {
                path,
                edits: super::seam_survey::minimal_edits(&old, &new),
            })
            .collect();
        changes.extend(
            assembled
                .renames
                .into_iter()
                .map(|(from, to)| FileEdit::Rename { from, to }),
        );
        Ok(Resolution {
            edit: WorkspaceEdit { changes },
            report: assembled.report,
            notes: assembled.notes,
        })
    }

    /// Every place that names the module, in any file the server knows.
    fn callers_of_the_module(
        &mut self,
        workspace: &Workspace<'_>,
        request: &reading::Reparent,
        survey: &survey::Survey,
    ) -> Result<Vec<Site>> {
        let file = &survey.old_parent.file;
        let uri = uri_of(&workspace.root.join(file));
        self.start(workspace.root)?;
        self.did_open(&uri, &survey.parent_text)?;
        self.ensure_indexed(&uri)?;
        (self.progress)("reparent_module: surveying the callers of the module");
        let position: Value = survey.declaration.name_position();
        let references = self.sites_of(
            &uri,
            workspace,
            file,
            &survey.parent_text,
            &[(request.name.as_str(), &position)],
        )?;
        spelling_the_name(workspace, references, &request.name)
    }
}

/// The references that write the module's name.
///
/// The server reports every path step that resolves to the module, and that includes the keywords:
/// a `super` in a child, a `self` in the module itself. Neither is a name to re-point — each means
/// "the module around me" and goes on meaning it where the module goes — so only the sites whose
/// text is the name are kept. An alias (`use a::module as other`) is a site too and is left alone
/// for the same reason: the name in its path is the one that is written.
fn spelling_the_name(workspace: &Workspace<'_>, sites: Vec<Site>, name: &str) -> Result<Vec<Site>> {
    let mut named = Vec::new();
    for site in sites {
        let text = workspace.read(&site.path)?;
        let spelled = text[site.offset..].strip_prefix(name).is_some_and(|rest| {
            !rest
                .bytes()
                .next()
                .is_some_and(super::item_move::text::is_identifier_byte)
        });
        if spelled {
            named.push(site);
        }
    }
    Ok(named)
}
