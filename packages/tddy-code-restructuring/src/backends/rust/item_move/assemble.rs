//! Putting a move together: the new text of every file it touches.
//!
//! Everything here is a function of the original texts and of what the server said — the outline of
//! the source file and the reference set of each moved item — so the whole assembly can be read, and
//! tested, without a server. The bytes of the moved items are copied from the source range; only the
//! tokens a move has to change inside them (a visibility, a relative path, a qualifier) are edited.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use super::super::seam_refusal;
use super::bindings;
use super::canonical_paths;
use super::destination::{Module, Package};
use super::doc_links;
use super::facade;
use super::imports::{self, Source};
use super::outline::{visibility_edit, Item, Run};
use super::outside;
use super::placement;
use super::preflight::names_declared_in;
use super::reach;
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
    /// Spell a facade path in the moved text as its defining path (`canonical_paths` on the plan line).
    pub(super) canonical_paths: bool,
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
    let mut texts = original_texts(moving)?;
    let region = line_start(moving.source_text, moving.run.first_line)
        ..line_end(moving.source_text, moving.run.last_line);
    let landing = visibilities(moving, &texts, &region)?;

    let mut edits: BTreeMap<String, Vec<Edit>> = BTreeMap::new();
    edits
        .entry(moving.source_file.to_string())
        .or_default()
        .extend(landing.kept_edits.clone());
    let facade_names: BTreeSet<String> = if moving.reexport == Reexport::Outside {
        outside::facade_items(moving.reexport, &landing.written, moving.outside)
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    } else {
        BTreeSet::new()
    };
    let mut notes = Vec::new();
    repoint_callers(
        moving,
        &texts,
        &region,
        &facade_names,
        &mut edits,
        &mut notes,
    )?;

    let moved_names: BTreeSet<String> = moving
        .run
        .items
        .iter()
        .map(|item| item.name.clone())
        .collect();
    let moved = moved_text(
        moving,
        &region,
        &moved_names,
        &landing,
        &mut edits,
        &mut notes,
    )?;
    let destination_edits = into_destination(moving, &mut texts, &moved_names, &moved, &mut notes)?;
    for (path, edit) in destination_edits {
        edits.entry(path).or_default().push(edit);
    }
    let pairs = moved_paths(moving);
    for (path, list) in doc_links::across_the_crate(
        moving.workspace,
        moving.package,
        &pairs,
        &mut texts,
        Some((moving.source_file, region.clone())),
    )? {
        edits.entry(path).or_default().extend(list);
    }
    edits
        .entry(moving.source_file.to_string())
        .or_default()
        .push(leave_behind(moving, &region, &landing));
    let facade = !moving.reexport.repoints_callers() || !facade_names.is_empty();
    reach::reach_the_destination(moving, &mut texts, &region, facade, &mut edits)?;

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
pub(super) fn users_of(
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
    facade_names: &BTreeSet<String>,
    edits: &mut BTreeMap<String, Vec<Edit>>,
    notes: &mut Vec<String>,
) -> Result<()> {
    let to = &moving.destination.path;
    let qualifiers = Qualifiers {
        same_crate: written_from_the_root(to),
        extern_crate: std::iter::once(moving.package.crate_name.as_str())
            .chain(to.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("::"),
    };
    let sink = RefCell::new(Vec::new());
    let context = Context {
        root: moving.workspace.root,
        destination: moving.destination,
        crate_name: &moving.package.crate_name,
        qualifiers: &qualifiers,
        repoint: moving.reexport.repoints_callers(),
        region: (moving.source_file, region.clone()),
        moved_files: &[],
        bound_by_the_facade: facade_names,
        notes: &sink,
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
    notes.extend(sink.into_inner());
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
    notes: &mut Vec<String>,
) -> Result<String> {
    let imported = |module: &[String], name: &str| {
        bindings::import_target(moving.workspace, moving.package, module, name)
    };
    let modules = Modules {
        from: moving.source,
        to: &moving.destination.path,
        travelling: None,
        imports: Some(&imported),
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
    let mut shifted = shifted;
    // In the coordinates of the lines themselves, as the edits just above are.
    let text = &moving.source_text[region.clone()];
    shifted.extend(doc_links::edits(text, &moved_paths(moving)));
    if moving.canonical_paths {
        let claimed: Vec<Range<usize>> = landing
            .claimed
            .iter()
            .filter(|claim| claim.start >= region.start && claim.end <= region.end)
            .map(|claim| claim.start - region.start..claim.end - region.start)
            .collect();
        let rewritten = canonical_paths::defining_paths(moving, text, &claimed)?;
        shifted.extend(rewritten.edits);
        notes.extend(rewritten.notes);
    }
    applied(text, &shifted)
}

/// The old and new paths of every moved item, each in the two spellings a doc link may use: from the
/// crate root (`crate::a::f`) and by the crate's extern name (`app::a::f`).
fn moved_paths(moving: &Moving<'_>) -> Vec<(String, String)> {
    let crate_name = moving.package.crate_name.as_str();
    let mut pairs = Vec::new();
    for item in &moving.run.items {
        let crate_old = format!("{}::{}", written_from_the_root(moving.source), item.name);
        let crate_new = format!(
            "{}::{}",
            written_from_the_root(&moving.destination.path),
            item.name
        );
        pairs.push((crate_old, crate_new));
        pairs.push((
            named_from_the_root(crate_name, moving.source, &item.name),
            named_from_the_root(crate_name, &moving.destination.path, &item.name),
        ));
    }
    pairs
}

/// `crate_name::a::b::name`, the spelling a file outside the crate writes.
fn named_from_the_root(crate_name: &str, module: &[String], name: &str) -> String {
    std::iter::once(crate_name)
        .chain(module.iter().map(String::as_str))
        .chain(std::iter::once(name))
        .collect::<Vec<_>>()
        .join("::")
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
    texts: &mut BTreeMap<String, String>,
    moved_names: &BTreeSet<String>,
    moved: &str,
    notes: &mut Vec<String>,
) -> Result<Vec<(String, Edit)>> {
    let text = texts[&moving.destination.file].clone();
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
    let (lines, widenings) = reach::reachable_imports(moving, texts, moved, lines)?;

    let mut edits: Vec<(String, Edit)> = widenings;
    let mut imports = None;
    if !lines.is_empty() {
        notes.push(format!(
            "imports: {} `use` item(s) copied from the source module for the moved code; the ones \
             it does not use are removed by the unused-import tidy at the end of a complete run",
            lines.len()
        ));
        let (at, blank) = use_insertion(&text, scope.clone());
        let mut block: String = lines.iter().map(|line| format!("{line}\n")).collect();
        if blank {
            block.push('\n');
        }
        imports = Some(Edit::insert(at, block));
    }
    let landed = placement::insertion(&text, &scope, moved)?;
    // Both land at one offset in a file with no items yet; as two edits their order would be the
    // order of their text, which writes the imports below the items.
    let file = moving.destination.file.clone();
    match imports {
        Some(imports) if imports.start == landed.start && imports.end == landed.end => {
            edits.push((
                file,
                Edit::insert(landed.start, format!("{}{}", imports.text, landed.text)),
            ))
        }
        Some(imports) => {
            edits.push((file.clone(), imports));
            edits.push((file, landed));
        }
        None => edits.push((file, landed)),
    }
    Ok(edits)
}
