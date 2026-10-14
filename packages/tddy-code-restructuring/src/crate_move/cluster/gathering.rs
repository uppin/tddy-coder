use super::super::Result;
use super::MergedChanges;
use super::ModuleReferences;
use crate::crate_move::{carried, header, module_files, moving, refusals};
use crate::edit::FileEdit;
use crate::registry::Workspace;
use crate::Reexport;

/// What gathering a cluster's members accumulates: the set they are read against, the merged edit
/// and the totals that belong to the set.
pub(super) struct Gathering<'a> {
    pub(super) the_set: &'a carried::TheSet,
    pub(super) merged: MergedChanges,
    pub(super) totals: carried::SetTotals,
}

impl Gathering<'_> {
    /// What one member adds to the edit: a rename and a re-pointed header for each file it carries,
    /// the callers of everything in its tree that something outside the set names, and the refusals
    /// that are about the member as a whole.
    ///
    /// A reference from one carried file to another is no caller: both move, and the header pass
    /// re-points it.
    pub(super) fn member_changes(
        &mut self,
        engine: &mut dyn ModuleReferences,
        workspace: &Workspace<'_>,
        member: &moving::Move,
        files: &[module_files::MovedFile],
    ) -> Result<()> {
        let Gathering {
            the_set,
            merged,
            totals,
        } = self;
        let mut rewrites = Vec::new();
        let mut origin_paths = Vec::new();
        for file in files {
            let (survey, file_rewrites) = crate::crate_move::surveyed_in(
                engine,
                workspace,
                member,
                &file.from,
                &the_set.travelling,
            )?;
            let text = workspace.read(&file.from)?;
            let header = header::repointed_header_at(
                workspace,
                &text,
                member,
                &file.below,
                &the_set.co_moving,
            )?;

            merged.absorb(FileEdit::Rename {
                from: file.from.clone(),
                to: file.to.clone(),
            });
            merged.add(file.from.clone(), header.edits);
            totals.crates_named.extend(header.crates_named);
            totals.dev_crates_named.extend(header.dev_crates_named);
            header::extend_once(&mut origin_paths, header.origin_paths);
            if file.below.is_empty() {
                totals.surveys.push(survey);
            }
            rewrites.extend(file_rewrites);
        }

        let naming_it = refusals::crates_still_naming_the_module(workspace, member, &rewrites)?;
        refusals::refuse_a_dependency_cycle(member, &origin_paths, &naming_it)?;
        carried::restricted_children_reached(workspace, member, &rewrites)?;
        if member.reexport == Reexport::None {
            for change in moving::caller_changes(workspace, rewrites)? {
                merged.absorb(change);
            }
        }
        totals.keeps_naming_it.extend(naming_it);
        Ok(())
    }
}
