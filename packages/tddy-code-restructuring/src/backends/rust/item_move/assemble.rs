//! Putting a move together: the new text of every file it touches.
//!
//! Everything here is a function of the original texts and of what the server said — the outline of
//! the source file and the reference set of each moved item — so the whole assembly can be read, and
//! tested, without a server. The bytes of the moved items are copied from the source range; only the
//! tokens a move has to change inside them (a visibility, a relative path, a qualifier) are edited.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use super::super::seam_refusal;
use super::creation;
use super::destination::{Module, Package};
use super::facade;
use super::imports::{self, Source};
use super::outline::{visibility_edit, Item, Run};
use super::outside;
use super::placement;
use super::preflight::names_declared_in;
use super::rebase::{self, Modules};
use super::scope::Scope;
use super::sites::{edits_for_file, module_of_file, Context, Qualifiers, Site};
use super::text::{applied, enclosing_modules, line_end, line_start, use_insertion, Edit};
use crate::edit::VisibilityChange;
use crate::plan::Reexport;
use crate::registry::Workspace;
use crate::Result;

/// What a move is made of.
pub(super) struct Moving<'a> {
    pub(super) workspace: &'a Workspace<'a>,
    pub(super) package: &'a Package,
    pub(super) source_file: &'a str,
    pub(super) source_text: &'a str,
    /// The source module, below the crate root.
    pub(super) source: &'a [String],
    pub(super) run: &'a Run,
    /// The items the source module keeps that the moved code names.
    pub(super) reached: &'a [Item],
    pub(super) destination: &'a Module,
    /// The parent that declares the destination, and its name, when the move creates it.
    pub(super) created: Option<(&'a Module, &'a str)>,
    /// The callers the move re-points: under `outside`, only the ones in the item's own crate.
    pub(super) sites: &'a [Site],
    /// The moved names a facade is left for under `outside`.
    pub(super) outside: &'a BTreeSet<String>,
    pub(super) reexport: Reexport,
}

/// The result of a move: the new text of each file it changes.
pub(super) struct Assembled {
    pub(super) files: BTreeMap<String, (String, String)>,
    pub(super) report: Vec<VisibilityChange>,
    pub(super) notes: Vec<String>,
}

/// The path of a module from the crate root, as a file inside the crate writes it.
pub(in crate::backends::rust) fn written_from_the_root(module: &[String]) -> String {
    std::iter::once("crate")
        .chain(module.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join("::")
}

pub(super) fn assemble(moving: &Moving<'_>) -> Result<Assembled> {
    let texts = original_texts(moving)?;
    let region = line_start(moving.source_text, moving.run.first_line)
        ..line_end(moving.source_text, moving.run.last_line);
    let landing = visibilities(moving, &texts, &region)?;

    let mut edits: BTreeMap<String, Vec<Edit>> = BTreeMap::new();
    edits
        .entry(moving.source_file.to_string())
        .or_default()
        .extend(landing.kept_edits.clone());
    repoint_callers(moving, &texts, &region, &mut edits)?;

    let moved_names: BTreeSet<String> = moving
        .run
        .items
        .iter()
        .map(|item| item.name.clone())
        .collect();
    let moved = moved_text(moving, &region, &moved_names, &landing, &mut edits)?;
    let mut notes = Vec::new();
    let destination_edits = into_destination(moving, &texts, &moved_names, &moved, &mut notes)?;
    edits
        .entry(moving.destination.file.clone())
        .or_default()
        .extend(destination_edits);
    edits
        .entry(moving.source_file.to_string())
        .or_default()
        .push(leave_behind(moving, &region, &landing));
    if let Some((parent, name)) = moving.created {
        let visibility = declared_visibility(moving, parent, &texts, &region, &landing);
        let (at, written) =
            creation::declaration(&texts[&parent.file], &parent.scope, &visibility, name);
        edits
            .entry(parent.file.clone())
            .or_default()
            .push(Edit::insert(at, written));
    }

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
        report: landing.report,
        notes,
    })
}

/// The visibility the created module is declared with: the narrowest that lets every caller the move
/// re-points, and the facade it leaves behind, reach it.
fn declared_visibility(
    moving: &Moving<'_>,
    parent: &Module,
    texts: &BTreeMap<String, String>,
    region: &Range<usize>,
    landing: &Landing,
) -> String {
    let mut scope = Scope::Within(parent.path.clone());
    let facade = outside::facade_items(moving.reexport, &landing.written, moving.outside);
    if !moving.reexport.repoints_callers() || !facade.is_empty() {
        scope = scope.widened_to(moving.source);
    }
    if moving.reexport.repoints_callers() {
        for item in &moving.run.items {
            scope = match users_of(moving, texts, region, &item.name) {
                Some(users) => users
                    .iter()
                    .fold(scope, |scope, user| scope.widened_to(user)),
                None => Scope::Public,
            };
        }
    }
    scope.spelled_in(&parent.path)
}

fn original_texts(moving: &Moving<'_>) -> Result<BTreeMap<String, String>> {
    let mut texts = BTreeMap::new();
    texts.insert(
        moving.source_file.to_string(),
        moving.source_text.to_string(),
    );
    if let Some((parent, _)) = moving.created {
        texts.insert(parent.file.clone(), moving.workspace.read(&parent.file)?);
        texts.insert(moving.destination.file.clone(), String::new());
    }
    let wanted =
        std::iter::once(&moving.destination.file).chain(moving.sites.iter().map(|site| &site.path));
    for path in wanted {
        if !texts.contains_key(path) {
            texts.insert(path.clone(), moving.workspace.read(path)?);
        }
    }
    Ok(texts)
}

/// The visibilities the move changes, as edits to the source file, and the report of them.
struct Landing {
    /// The visibilities of the moved items, which are edits to the moved text.
    moved_edits: Vec<Edit>,
    /// The visibilities of the items left behind that the moved code reaches.
    kept_edits: Vec<Edit>,
    /// The spans of the moved items' own visibility, which no other edit may touch.
    claimed: Vec<Range<usize>>,
    /// The scope each moved item has where it lands, in the order of the run.
    scopes: Vec<(String, Scope)>,
    /// What each moved item's visibility was written as, as a scope in the source module.
    written: Vec<(String, Scope)>,
    report: Vec<VisibilityChange>,
}

fn spelled(visibility: &str) -> String {
    if visibility.is_empty() {
        "private".to_string()
    } else {
        visibility.to_string()
    }
}

fn read_scope(item: &Item, module: &[String]) -> Result<Scope> {
    Scope::parse(&item.visibility, module).ok_or_else(|| {
        seam_refusal(format!(
            "the visibility `{}` of `{}` is not one this move can read",
            item.visibility, item.name
        ))
    })
}

fn visibilities(
    moving: &Moving<'_>,
    texts: &BTreeMap<String, String>,
    region: &Range<usize>,
) -> Result<Landing> {
    let (source, destination) = (moving.source, moving.destination.path.as_slice());
    let mut landing = Landing {
        moved_edits: Vec::new(),
        kept_edits: Vec::new(),
        claimed: Vec::new(),
        scopes: Vec::new(),
        written: Vec::new(),
        report: Vec::new(),
    };

    for item in &moving.run.items {
        let written = read_scope(item, source)?;
        let starts_as = if written == Scope::Within(source.to_vec()) {
            Scope::Within(destination.to_vec())
        } else {
            written.clone()
        };
        let mut scope = starts_as.clone().widened_to(destination);
        scope = if moving.reexport.repoints_callers() {
            match users_of(moving, texts, region, &item.name) {
                Some(users) => users
                    .iter()
                    .fold(scope, |scope, user| scope.widened_to(user)),
                None => Scope::Public,
            }
        } else {
            scope.widened_to(source)
        };

        let to = scope.spelled_in(destination);
        if to != item.visibility {
            let edit = visibility_edit(moving.source_text, &item.position, &item.name, &to)?;
            landing.claimed.push(edit.start..edit.end);
            landing.moved_edits.push(edit);
        }
        if scope != starts_as {
            landing.report.push(VisibilityChange {
                item: item.name.clone(),
                from: spelled(&item.visibility),
                to: spelled(&to),
            });
        }
        landing.scopes.push((item.name.clone(), scope));
        landing.written.push((item.name.clone(), written));
    }

    for item in moving.reached {
        let written = read_scope(item, source)?;
        let widened = written.clone().widened_to(destination);
        if widened != written {
            let to = widened.spelled_in(source);
            landing.kept_edits.push(visibility_edit(
                moving.source_text,
                &item.position,
                &item.name,
                &to,
            )?);
            landing.report.push(VisibilityChange {
                item: item.name.clone(),
                from: spelled(&item.visibility),
                to: spelled(&to),
            });
        }
    }
    Ok(landing)
}

/// The modules that name the item from outside the lines that move, or `None` when a file outside
/// the crate does, which only a `pub` item can have.
fn users_of(
    moving: &Moving<'_>,
    texts: &BTreeMap<String, String>,
    region: &Range<usize>,
    name: &str,
) -> Option<Vec<Vec<String>>> {
    let mut users = Vec::new();
    for site in moving.sites.iter().filter(|site| site.name == name) {
        if site.path == moving.source_file && region.contains(&site.offset) {
            continue;
        }
        let base = module_of_file(
            moving.workspace.root,
            &moving.package.crate_name,
            &site.path,
        )?;
        let inside = enclosing_modules(&texts[&site.path], site.offset);
        users.push(base.into_iter().chain(inside).collect());
    }
    Some(users)
}

fn repoint_callers(
    moving: &Moving<'_>,
    texts: &BTreeMap<String, String>,
    region: &Range<usize>,
    edits: &mut BTreeMap<String, Vec<Edit>>,
) -> Result<()> {
    let to = &moving.destination.path;
    let qualifiers = Qualifiers {
        same_crate: written_from_the_root(to),
        extern_crate: std::iter::once(moving.package.crate_name.as_str())
            .chain(to.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("::"),
    };
    let context = Context {
        root: moving.workspace.root,
        destination: moving.destination,
        crate_name: &moving.package.crate_name,
        qualifiers: &qualifiers,
        repoint: moving.reexport.repoints_callers(),
        region: (moving.source_file, region.clone()),
        moved_files: &[],
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

/// The moved lines, with every token the move changes inside them changed. Takes the edits that fall
/// inside the lines out of the source file's list: they have been applied to the copy.
fn moved_text(
    moving: &Moving<'_>,
    region: &Range<usize>,
    moved_names: &BTreeSet<String>,
    landing: &Landing,
    edits: &mut BTreeMap<String, Vec<Edit>>,
) -> Result<String> {
    let modules = Modules {
        from: moving.source,
        to: &moving.destination.path,
        travelling: None,
    };
    let source_edits = edits.entry(moving.source_file.to_string()).or_default();
    source_edits.extend(rebase::edits(
        moving.source_text,
        region.clone(),
        &modules,
        moved_names,
        &landing.claimed,
    ));

    let inside = |edit: &Edit| {
        edit.start >= region.start
            && edit.end <= region.end
            && !(edit.start == edit.end && (edit.start == region.start || edit.start == region.end))
    };
    let (mut own, rest): (Vec<Edit>, Vec<Edit>) = source_edits.drain(..).partition(inside);
    *source_edits = rest;
    own.extend(landing.moved_edits.iter().cloned());

    let shifted: Vec<Edit> = own
        .into_iter()
        .map(|edit| {
            Edit::replace(
                edit.start - region.start..edit.end - region.start,
                edit.text,
            )
        })
        .collect();
    applied(&moving.source_text[region.clone()], &shifted)
}

/// What stands where the lines were: nothing, or the facade that keeps the old path resolving.
fn leave_behind(moving: &Moving<'_>, region: &Range<usize>, landing: &Landing) -> Edit {
    let qualifier = written_from_the_root(&moving.destination.path);
    let items = outside::facade_items(moving.reexport, &landing.written, moving.outside);
    let lines = facade::lines(moving.reexport, &qualifier, moving.source, &items);
    placement::vacated(moving.source_text, region, &lines)
}

/// The edits to the destination: the imports the moved code needs, then the moved text itself.
fn into_destination(
    moving: &Moving<'_>,
    texts: &BTreeMap<String, String>,
    moved_names: &BTreeSet<String>,
    moved: &str,
    notes: &mut Vec<String>,
) -> Result<Vec<Edit>> {
    let text = &texts[&moving.destination.file];
    let scope = moving.destination.scope.clone();
    let mut taken = names_declared_in(&text[scope.clone()]);
    taken.extend(moved_names.iter().cloned());

    let kept: Vec<String> = moving
        .reached
        .iter()
        .map(|item| item.name.clone())
        .collect();
    let source = Source {
        text: moving.source_text,
        scope: 0..moving.source_text.len(),
        module: moving.source,
        qualifier: &written_from_the_root(moving.source),
    };
    let lines = imports::needed(&source, &taken, &kept);

    let mut edits = Vec::new();
    if !lines.is_empty() {
        notes.push(format!(
            "imports: {} `use` item(s) copied from the source module for the moved code; the ones \
             it does not use are removed by the unused-import tidy at the end of a complete run",
            lines.len()
        ));
        let (at, blank) = use_insertion(text, scope.clone());
        let mut block: String = lines.iter().map(|line| format!("{line}\n")).collect();
        if blank {
            block.push('\n');
        }
        edits.push(Edit::insert(at, block));
    }
    edits.push(placement::insertion(text, &scope, moved)?);
    Ok(edits)
}
