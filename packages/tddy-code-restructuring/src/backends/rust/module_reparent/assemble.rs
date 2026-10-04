//! Putting a module move together: the new text of every file it changes, and the files it moves.
//!
//! A function of the original texts and of what the server said — the reference set of the module's
//! name — so the assembly can be read, and tested, without a server.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use super::super::item_move::assemble::written_from_the_root;
use super::super::item_move::facade;
use super::super::item_move::outside;
use super::super::item_move::placement::vacated;
use super::super::item_move::rebase::{self, Modules};
use super::super::item_move::sites::{edits_for_file, Context, Qualifiers, Site};
use super::super::item_move::text::{applied, qualifier_start, Edit};
use super::declaration::insertion;
use super::reading::Reparent;
use super::survey::Survey;
use super::visibility::{landing, Landing};
use crate::edit::VisibilityChange;
use crate::plan::Reexport;
use crate::registry::Workspace;
use crate::Result;

/// What a module move is made of.
pub(super) struct Reparenting<'a> {
    pub(super) workspace: &'a Workspace<'a>,
    pub(super) request: &'a Reparent,
    pub(super) survey: &'a Survey,
    /// The callers the move re-points: under `outside`, only the ones in the module's own crate.
    pub(super) sites: &'a [Site],
    /// The module's name when something outside the crate reaches it under `outside`.
    pub(super) outside: &'a BTreeSet<String>,
    pub(super) reexport: Reexport,
}

/// The result of a move: the new text of each file it changes, and each file it moves.
pub(super) struct Assembled {
    /// By path before the move: the text before and after.
    pub(super) files: BTreeMap<String, (String, String)>,
    pub(super) renames: Vec<(String, String)>,
    pub(super) report: Vec<VisibilityChange>,
    pub(super) notes: Vec<String>,
}

pub(super) fn assemble(moving: &Reparenting<'_>) -> Result<Assembled> {
    let survey = moving.survey;
    let texts = original_texts(moving)?;
    let landing = landing(
        moving.workspace,
        moving.request,
        survey,
        moving.sites,
        moving.reexport,
    )?;

    let mut edits: BTreeMap<String, Vec<Edit>> = BTreeMap::new();
    repoint_callers(moving, &texts, &mut edits)?;
    rebase_the_moved_files(moving, &texts, &mut edits);
    leave_behind(moving, &landing, &mut edits);
    arrive(moving, &landing, &mut edits);

    let mut files = BTreeMap::new();
    for (path, mut list) in edits {
        list.sort_by(|one, other| {
            (one.start, one.end, &one.text).cmp(&(other.start, other.end, &other.text))
        });
        list.dedup();
        let old = texts[&path].clone();
        let new = applied(&old, &list)?;
        if new != old {
            files.insert(path, (old, new));
        }
    }
    Ok(Assembled {
        files,
        renames: survey
            .files
            .iter()
            .map(|file| (file.from.clone(), file.to.clone()))
            .collect(),
        report: landing.report,
        notes: vec![format!(
            "{} file(s) move with the module, with the directory of its children",
            survey.files.len()
        )],
    })
}

fn original_texts(moving: &Reparenting<'_>) -> Result<BTreeMap<String, String>> {
    let survey = moving.survey;
    let mut texts = BTreeMap::new();
    texts.insert(survey.old_parent.file.clone(), survey.parent_text.clone());
    texts.insert(
        survey.new_parent.file.clone(),
        survey.new_parent_text.clone(),
    );
    let wanted = survey
        .files
        .iter()
        .map(|file| &file.from)
        .chain(moving.sites.iter().map(|site| &site.path));
    for path in wanted {
        if !texts.contains_key(path) {
            texts.insert(path.clone(), moving.workspace.read(path)?);
        }
    }
    Ok(texts)
}

/// Every path that named the module, pointed at the module's new home.
fn repoint_callers(
    moving: &Reparenting<'_>,
    texts: &BTreeMap<String, String>,
    edits: &mut BTreeMap<String, Vec<Edit>>,
) -> Result<()> {
    let to = &moving.survey.new_parent.path;
    let crate_name = &moving.request.named.package.crate_name;
    let qualifiers = Qualifiers {
        same_crate: written_from_the_root(to),
        extern_crate: std::iter::once(crate_name.as_str())
            .chain(to.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("::"),
    };
    let moved: Vec<String> = moving
        .survey
        .files
        .iter()
        .map(|file| file.from.clone())
        .collect();
    let context = Context {
        root: moving.workspace.root,
        destination: &moving.survey.new_parent,
        crate_name,
        qualifiers: &qualifiers,
        repoint: moving.reexport.repoints_callers(),
        region: ("", 0..0),
        moved_files: &moved,
        bound_by_the_facade: &BTreeSet::new(),
    };
    let files: BTreeSet<&String> = moving.sites.iter().map(|site| &site.path).collect();
    for path in files {
        let here: Vec<&Site> = moving
            .sites
            .iter()
            .filter(|site| &site.path == path)
            .collect();
        edits
            .entry(path.clone())
            .or_default()
            .extend(edits_for_file(&context, path, &texts[path], &here)?);
    }
    Ok(())
}

/// The relative paths in each moved file, kept meaning what they meant.
///
/// A path in front of the module's own name is the re-pointing's, and is claimed so the rebase
/// leaves it alone.
fn rebase_the_moved_files(
    moving: &Reparenting<'_>,
    texts: &BTreeMap<String, String>,
    edits: &mut BTreeMap<String, Vec<Edit>>,
) {
    let survey = moving.survey;
    let old = moving.request.module_path();
    let mut new = survey.new_parent.path.clone();
    new.push(moving.request.name.clone());
    for file in &survey.files {
        let text = &texts[&file.from];
        let claimed = qualifiers_in(text, &file.from, moving.sites);
        let (from, to) = (
            [old.as_slice(), &file.below].concat(),
            [new.as_slice(), &file.below].concat(),
        );
        let modules = Modules {
            from: &from,
            to: &to,
            travelling: Some(&old),
        };
        edits
            .entry(file.from.clone())
            .or_default()
            .extend(rebase::edits(
                text,
                0..text.len(),
                &modules,
                &BTreeSet::new(),
                &claimed,
            ));
    }
}

/// The spans of the qualifiers written in front of the module's name in `path`.
fn qualifiers_in(text: &str, path: &str, sites: &[Site]) -> Vec<Range<usize>> {
    sites
        .iter()
        .filter(|site| site.path == path)
        .map(|site| qualifier_start(text, site.offset)..site.offset)
        .filter(|span| !span.is_empty())
        .collect()
}

/// What stands where the declaration was: nothing, or the facade that keeps the old path resolving.
fn leave_behind(
    moving: &Reparenting<'_>,
    landing: &Landing,
    edits: &mut BTreeMap<String, Vec<Edit>>,
) {
    let survey = moving.survey;
    let qualifier = written_from_the_root(&survey.new_parent.path);
    let module = [(moving.request.name.clone(), landing.written.clone())];
    let items = outside::facade_items(moving.reexport, &module, moving.outside);
    let facade = facade::lines(moving.reexport, &qualifier, &moving.request.parent, &items);
    edits
        .entry(survey.old_parent.file.clone())
        .or_default()
        .push(vacated(
            &survey.parent_text,
            &survey.declaration.lines,
            &facade,
        ));
}

/// The declaration, written into the new parent with the visibility it lands with.
fn arrive(moving: &Reparenting<'_>, landing: &Landing, edits: &mut BTreeMap<String, Vec<Edit>>) {
    let survey = moving.survey;
    let declaration = survey
        .declaration
        .written_with(&survey.parent_text, &landing.spelled);
    let (at, written) = insertion(
        &survey.new_parent_text,
        &survey.new_parent.scope,
        &declaration,
    );
    edits
        .entry(survey.new_parent.file.clone())
        .or_default()
        .push(Edit::insert(at, written));
}
