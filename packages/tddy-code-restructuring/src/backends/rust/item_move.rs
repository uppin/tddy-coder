//! `move_item`: moving a run of items into another module of the same crate.
//!
//! rust-analyzer has no "move item to another module" assist, so — like the cross-crate move — this
//! operation is authored here and informed by the server. The server answers two things: where each
//! item is (its outline) and who names it (`textDocument/references`). Everything else is a function
//! of the files' text, in [`assemble`]: the items' bytes are copied from the source range rather than
//! printed again, so the comments and attributes on them arrive untouched.
//!
//! What is lexical is answered before a server exists ([`preflight`]): a destination module that is
//! not there, a name it already declares, and a destination that is the items' own module.
//!
//! The destination must exist, unless the plan line carries `name`: then `to` is the parent and the
//! move declares a new, empty module of that name in it first ([`creation`]).

pub(super) mod assemble;
pub(super) mod bindings;
mod canonical_paths;
pub(super) mod creation;
pub(super) mod destination;
pub(super) mod doc_links;
pub(super) mod facade;
pub(super) mod imports;
pub(crate) mod members;
pub(crate) mod outline;
pub(super) mod outside;
pub(super) mod placement;
pub(super) mod preflight;
mod reach;
pub(super) mod rebase;
pub(super) mod scope;
pub(super) mod sites;
pub(super) mod text;
pub(super) mod use_path;

use std::collections::BTreeMap;

use serde_json::Value;

use super::{lsp_edits, relative_to, seam_survey, server_defect, uri_of, LspPoint, RustBackend};
use crate::crate_move::source_scan::items_of_module;
use crate::edit::{FileEdit, Range, Resolution, WorkspaceEdit};
use crate::item_anchor::{module_path_of, span_of, unlowered_item_anchor};
use crate::plan::{Anchor, Reexport, RefactorOp};
use crate::registry::Workspace;
use crate::Result;
use assemble::{assemble, Moving};
use creation::Destination;
use members::ReachedMember;
use outline::{Item, Run};
use sites::Site;

/// What a static check finds wrong with a `move_item`, from the text alone.
pub(super) fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    preflight::findings(workspace, op)
}

impl RustBackend {
    /// Resolve a `move_item` into the edits to every file it changes.
    pub(super) fn move_items(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> Result<Resolution> {
        let file = op.anchor.file();
        let named = preflight::named_by(workspace, op, "move_item")?;
        let source_text = workspace.read(file)?;

        // Answered by the text, so answered before a server exists.
        let (source, names) = match preflight::anchored(op) {
            Some(read) => read,
            None => written_in(workspace, op, &source_text)?,
        };
        preflight::destination_for(workspace, &named, &source, &names)?;
        let range = self.range_of(op, workspace)?;

        let uri = uri_of(&workspace.root.join(file));
        self.start(workspace.root)?;
        self.did_open(&uri, &source_text)?;
        self.ensure_indexed(&uri)?;
        let symbols = self.settled_outline(&uri)?;

        let run = outline::run_covering(&symbols, &source_text, range)?;
        let names: Vec<String> = run.items.iter().map(|item| item.name.clone()).collect();
        let destination = preflight::destination_for(workspace, &named, &source, &names)?;
        (self.progress)(&format!(
            "move_item: surveying the callers of {} item(s)",
            run.items.len()
        ));
        let moved: Vec<(&str, &Value)> = run
            .items
            .iter()
            .map(|item| (item.name.as_str(), &item.position))
            .collect();
        let reexport = op.reexport.unwrap_or(Reexport::None);
        let sites = self.sites_of(&uri, workspace, file, &source_text, &moved)?;
        let reach = outside::Reach::of(reexport, workspace.root, &named.package, sites)?;
        let reached =
            self.move_reach(&uri, workspace, &source_text, &symbols, &run, &destination)?;

        let assembled = assemble(&Moving {
            workspace,
            package: &named.package,
            source_file: file,
            source_text: &source_text,
            source: &source,
            run: &run,
            reached: &reached,
            destination: &destination.module,
            created: destination.parent.as_ref().zip(named.creates.as_deref()),
            sites: &reach.sites,
            outside: &reach.outside,
            reexport,
            canonical_paths: op.canonical_paths,
        })?;

        let mut changes = Vec::new();
        for (path, (old, new)) in assembled.files {
            if path == destination.module.file && destination.parent.is_some() {
                changes.push(FileEdit::Create { path: path.clone() });
            }
            changes.push(FileEdit::Change {
                path,
                edits: seam_survey::minimal_edits(&old, &new),
            });
        }
        Ok(Resolution {
            edit: WorkspaceEdit { changes },
            report: assembled.report,
            notes: assembled.notes,
        })
    }

    /// The lines the anchor covers: as written when a run lowered it already, otherwise resolved
    /// the way a run would.
    fn range_of(&mut self, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Range> {
        match &op.anchor {
            Anchor::Range { start, end, .. } => Ok(Range {
                start: *start,
                end: *end,
            }),
            Anchor::Item { .. } | Anchor::Items { .. } => span_of(&op.anchor, workspace.root, self),
            Anchor::Symbol { .. } => Err(unlowered_item_anchor(&op.anchor, "`move_item`")),
        }
    }

    /// Every place that names one of `named` (a name and where the server reports it, in the file at
    /// `uri`), in any file the server knows.
    pub(super) fn sites_of(
        &mut self,
        uri: &str,
        workspace: &Workspace<'_>,
        file: &str,
        source_text: &str,
        named: &[(&str, &Value)],
    ) -> Result<Vec<Site>> {
        let mut texts: BTreeMap<String, String> = BTreeMap::new();
        texts.insert(file.to_string(), source_text.to_string());
        let mut sites: Vec<Site> = Vec::new();

        for (name, position) in named {
            let references = self.references_at(uri, position)?;
            for reference in references.as_array().into_iter().flatten() {
                let referrer = reference
                    .get("uri")
                    .and_then(Value::as_str)
                    .ok_or_else(|| server_defect("a reference carries no uri"))?;
                let path = if referrer == uri {
                    file.to_string()
                } else {
                    relative_to(referrer, workspace.root)?
                };
                if !texts.contains_key(&path) {
                    texts.insert(path.clone(), workspace.read(&path)?);
                }
                let at = LspPoint::read(reference.pointer("/range/start"))?;
                let offset = lsp_edits::offset_of(&texts[&path], at);
                if !sites
                    .iter()
                    .any(|site| site.path == path && site.offset == offset)
                {
                    sites.push(Site {
                        path,
                        offset,
                        name: (*name).to_string(),
                    });
                }
            }
        }
        Ok(sites)
    }

    /// What the move connects across its two sides: the items the run leaves behind that the moved
    /// code names, and the private members of the types it splits, with who uses each from the other
    /// side.
    fn move_reach(
        &mut self,
        uri: &str,
        workspace: &Workspace<'_>,
        source_text: &str,
        symbols: &Value,
        run: &Run,
        destination: &Destination,
    ) -> Result<Reach> {
        let left = outline::left_behind(symbols, source_text, run);
        let items = self.reached_by_the_moved_code(uri, source_text, &left, run)?;
        let lines = run.first_line..=run.last_line;
        let moved = &source_text[text::line_start(source_text, run.first_line)
            ..text::line_end(source_text, run.last_line)];
        let mentioned = text::identifiers_in(moved);
        let moved_members = members::members_of(symbols, source_text, lines.clone(), true);
        let mut kept_members = members::members_of(symbols, source_text, lines, false);
        kept_members.retain(|member| mentioned.contains(&member.name));
        let surveyed = moved_members.len() + kept_members.len();
        if surveyed > 0 {
            (self.progress)(&format!(
                "move_item: surveying {surveyed} member(s) of the types the move splits"
            ));
        }
        let file = relative_to(uri, workspace.root)?;
        let source = module_path_of(workspace.root, &file)?;
        let survey = MemberSurvey {
            uri,
            file: &file,
            source_text,
            run,
            source: &source[1..],
            destination: &destination.module.path,
        };
        Ok(Reach {
            items,
            moved_members: self.reached_members(workspace, &survey, moved_members, true)?,
            kept_members: self.reached_members(workspace, &survey, kept_members, false)?,
        })
    }

    /// Each of `members` with the modules on the other side of the move that use it: for a member
    /// that moves (`moves`), the modules outside the run; for one that stays, the destination when
    /// the run uses it.
    fn reached_members(
        &mut self,
        workspace: &Workspace<'_>,
        survey: &MemberSurvey<'_>,
        members: Vec<members::Member>,
        moves: bool,
    ) -> Result<Vec<ReachedMember>> {
        let region = text::line_start(survey.source_text, survey.run.first_line)
            ..text::line_end(survey.source_text, survey.run.last_line);
        let crate_name = module_path_of(workspace.root, survey.file)?.swap_remove(0);
        let mut reached = Vec::new();
        for member in members {
            let named = [(member.name.as_str(), &member.position)];
            let sites = self.sites_of(
                survey.uri,
                workspace,
                survey.file,
                survey.source_text,
                &named,
            )?;
            let inside = |site: &Site| site.path == survey.file && region.contains(&site.offset);
            let users = if moves {
                Self::users_outside(workspace, &crate_name, &sites, inside)?
            } else if sites.iter().any(inside) {
                vec![survey.destination.to_vec()]
            } else {
                Vec::new()
            };
            reached.push(ReachedMember {
                member,
                written_in: survey.source.to_vec(),
                lands_in: if moves {
                    survey.destination
                } else {
                    survey.source
                }
                .to_vec(),
                users,
            });
        }
        Ok(reached)
    }

    /// The modules, other than the run itself, that hold one of `sites`.
    fn users_outside(
        workspace: &Workspace<'_>,
        crate_name: &str,
        sites: &[Site],
        inside: impl Fn(&Site) -> bool,
    ) -> Result<Vec<Vec<String>>> {
        let mut users = Vec::new();
        for site in sites.iter().filter(|site| !inside(site)) {
            let text = workspace.read(&site.path)?;
            let base = sites::module_of_file(workspace.root, crate_name, &site.path);
            users.extend(base.map(|base| {
                base.into_iter()
                    .chain(text::enclosing_modules(&text, site.offset))
                    .collect()
            }));
        }
        Ok(users)
    }

    /// The items the run leaves behind that the moved code names.
    ///
    /// Asked of the server for the items whose names the moved lines mention, because a name that
    /// merely shares its spelling with a field or a local is not a reference to the item.
    fn reached_by_the_moved_code(
        &mut self,
        uri: &str,
        source_text: &str,
        left: &[Item],
        run: &Run,
    ) -> Result<Vec<Item>> {
        let moved = &source_text[text::line_start(source_text, run.first_line)
            ..text::line_end(source_text, run.last_line)];
        let mentioned = text::identifiers_in(moved);
        let mut reached = Vec::new();
        for item in left.iter().filter(|item| mentioned.contains(&item.name)) {
            let references = self.references_at(uri, &item.position)?;
            let from_the_moved_lines =
                references
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|reference| {
                        let line = reference
                            .pointer("/range/start/line")
                            .and_then(Value::as_u64)
                            .map(|line| line as u32 + 1);
                        reference.get("uri").and_then(Value::as_str) == Some(uri)
                            && line.is_some_and(|line| {
                                (run.first_line..=run.last_line).contains(&line)
                            })
                    });
            if from_the_moved_lines {
                reached.push(item.clone());
            }
        }
        Ok(reached)
    }
}

/// Where a move's members are surveyed from.
struct MemberSurvey<'a> {
    uri: &'a str,
    file: &'a str,
    source_text: &'a str,
    run: &'a Run,
    source: &'a [String],
    destination: &'a [String],
}

/// What a move connects across its two sides.
pub(super) struct Reach {
    /// The items the source module keeps that the moved code names.
    pub(super) items: Vec<Item>,
    /// The private members of the moved structs and `impl`s that code left behind uses.
    pub(super) moved_members: Vec<ReachedMember>,
    /// The private members of the kept structs and `impl`s that the moved code uses.
    pub(super) kept_members: Vec<ReachedMember>,
}

/// The module and names a lowered anchor holds, read from the lines it covers.
fn written_in(
    workspace: &Workspace<'_>,
    op: &RefactorOp,
    source_text: &str,
) -> Result<(Vec<String>, Vec<String>)> {
    let Anchor::Range { file, start, end } = &op.anchor else {
        return Err(unlowered_item_anchor(&op.anchor, "`move_item`"));
    };
    let region = text::line_start(source_text, start.line)..text::line_end(source_text, end.line);
    let module = module_path_of(workspace.root, file)?.split_off(1);
    Ok((module, items_of_module(&source_text[region]).defined))
}
