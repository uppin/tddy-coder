use crate::crate_move::destination;
use std::path::Path;

use std::collections::BTreeSet;

use std::collections::BTreeMap;

use super::super::Result;

use crate::{
    crate_move::{header, module_home, moving},
    registry::Workspace,
    Reexport, RefactorOp,
};

/// The moves in a plan that would leave the crate they left and the destination depending on
/// each other — each named with the paths that make it so.
///
/// The cluster defect's worse half is that `check` cannot see it: a four-operation plan reported
/// `no findings` and was then rejected by `apply`. This is that rejection read statically: a moved
/// module whose header still names a module staying behind makes the destination depend on the
/// crate it left, and that crate goes on naming the destination — through its facade, or, with
/// `reexport: none`, from every caller `apply` re-points. Two edges, one cycle, which
/// [`refusals::refuse_a_dependency_cycle`] refuses.
///
/// A module staying behind that names one leaving is **not** a finding on its own. A facade keeps
/// its `crate::…` path resolving, and without one `apply` re-points it at the destination; either
/// way it compiles, and reporting it made every move of a module anything still calls look broken.
///
/// **Staying behind is read at each operation's point in the plan**, because `apply` runs one
/// operation at a time. A sibling moved by the same operation, or by an earlier one, is already gone
/// from the origin. A sibling moved by a *later* operation is still there when this one runs, so a
/// mutually-referencing set spread over several `move_module_to_crate` operations is refused at its
/// first. That is what `move_cluster_to_crate` exists for, and the finding names it.
///
/// Empty when no moved module names a sibling still in the origin at that point, or nothing there
/// names it back.
///
/// # Errors
///
/// Refuses when a file the check must read cannot be.
pub fn siblings_left_behind(workspace: &Workspace<'_>, ops: &[RefactorOp]) -> Result<Vec<String>> {
    Ok(stranded_siblings(workspace, ops)?
        .into_iter()
        .map(|(_, finding)| finding)
        .collect())
}

/// [`siblings_left_behind`], with each finding tied to the operation it is about.
///
/// A reader fixes a plan by operation index, and `check` reports one, so the index is carried here
/// and dropped by the published call — which answers "what would this plan strand" rather than
/// "which operation does it".
pub(crate) fn stranded_siblings(
    workspace: &Workspace<'_>,
    ops: &[RefactorOp],
) -> Result<Vec<(usize, String)>> {
    let moving = modules_the_plan_moves(workspace, ops);

    // Read each crate's sources at most once rather than once per module moving out of it, and only
    // for a move that needs them: a plan that moves five modules out of one crate asks the same
    // question of the same files five times.
    let mut sources = BTreeMap::new();
    let mut findings = Vec::new();
    for module in &moving {
        let names_the_origin = paths_naming_the_origin(workspace, module, &moving)?;
        if names_the_origin.is_empty() {
            continue;
        }

        let origin_names_it_back = if module.moving.reexport == Reexport::None {
            let crate_dir = module.crate_dir();
            if !sources.contains_key(crate_dir) {
                sources.insert(crate_dir.to_string(), sources_of(workspace, crate_dir)?);
            }
            let callers = siblings_naming(&sources[crate_dir], module, &moving);
            if callers.is_empty() {
                continue;
            }
            format!(
                "`apply` re-points its callers there (`{}`) at the destination",
                callers.join("`, `")
            )
        } else {
            "the facade it leaves there names the destination".to_string()
        };

        let whereabouts = match moved_later(&names_the_origin, module, &moving).as_slice() {
            [] => format!("stays behind in `{}`", module.crate_dir()),
            [later] => format!(
                "is still in `{}` at that point (operation {later} moves it only afterwards)",
                module.crate_dir()
            ),
            later => format!(
                "is still in `{}` at that point (operations {} move it only afterwards)",
                module.crate_dir(),
                later
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };

        findings.push((
            module.op,
            format!(
                "`{source}`, which operation {op} moves to `{destination}`, names `{paths}`, which \
                 {whereabouts}. So the destination would depend on the crate it left, while \
                 {origin_names_it_back}: a cycle `apply` refuses. Move them in one \
                 `move_cluster_to_crate`, with this module as its anchor and what those paths \
                 reach in `also`, or leave `{named}` where it is",
                source = module.source,
                op = module.op,
                destination = module.moving.destination.dir,
                paths = names_the_origin.join("`, `"),
                named = module.path().join("::"),
            ),
        ));
    }

    Ok(findings)
}

/// The paths `module`'s header names the crate it left by once it is re-pointed — its
/// `destination → origin` edges.
///
/// Read by the header pass and the re-export resolution `apply` itself uses, so the check and the
/// refusal agree about which paths count: one reaching a module that has left this crate by the
/// time `module`'s operation runs is not an edge, nor is one the origin only re-exports from a
/// third crate.
fn paths_naming_the_origin(
    workspace: &Workspace<'_>,
    module: &MovingModule,
    moving: &[MovingModule],
) -> Result<Vec<String>> {
    let co_moving: BTreeSet<String> = gone_by_then(module, moving)
        .map(|other| other.path().join("::"))
        .collect();
    let text = workspace.read(&module.source)?;
    let header = header::repointed_header(workspace, &text, &module.moving, &co_moving)?;
    Ok(header.header_origin_paths)
}

/// One module a plan moves out of the crate that holds it.
struct MovingModule {
    /// Which operation moves it, by index in the plan.
    pub(crate) op: usize,
    /// The module file the plan names, relative to the repository root.
    pub(crate) source: String,
    /// The move as `apply` reads it: both crates, the module's home and the facade it leaves.
    pub(crate) moving: moving::Move,
}

impl MovingModule {
    /// The crate it is leaving.
    pub(crate) fn crate_dir(&self) -> &str {
        &self.moving.home.crate_dir
    }

    /// Its module path inside that crate, outermost first.
    pub(crate) fn path(&self) -> &[String] {
        &self.moving.home.path
    }
}

/// Every module the plan's cross-crate moves take out of a crate — each member of a cluster
/// operation, not only the one its anchor names, so a set moving together is not read as
/// stranding itself.
///
/// A module [`move_preconditions`](super::move_preconditions) refuses is left out rather than
/// reported: `check` already names it, and naming it again here would report one defect twice
/// under two descriptions. It is not moving, either, so it is not counted as travelling with the
/// rest.
fn modules_the_plan_moves(workspace: &Workspace<'_>, ops: &[RefactorOp]) -> Vec<MovingModule> {
    ops.iter()
        .enumerate()
        .filter(|(_, op)| op.op.moves_across_crates())
        .flat_map(|(index, op)| op.anchors().map(move |anchor| (index, op, anchor)))
        .filter_map(|(index, op, anchor)| {
            let earlier = super::super::moved_by_earlier_operations(workspace, ops, index, anchor);
            super::super::move_preconditions(
                workspace,
                &super::super::member_op(op, anchor),
                &earlier,
            )
            .ok()?;
            let source = anchor.file();
            let module = module_home::module_name(source).ok()?;
            let home = module_home::module_home(workspace, source, &module).ok()?;
            let destination =
                destination::Destination::read(workspace.root, op.to.as_deref()?).ok()?;
            let reexport = op.reexport.unwrap_or(Reexport::None);
            Some(MovingModule {
                op: index,
                source: source.to_string(),
                moving: moving::Move::of(workspace, &home, &destination, reexport).ok()?,
            })
        })
        .collect()
}

/// One file of a crate's `src/` tree: where it is, which module it is, and what it says.
struct CrateSource {
    /// The file, relative to the repository root.
    pub(crate) path: String,
    /// The module path the file carries inside its crate — `[]` for the crate root.
    pub(crate) home: Vec<String>,
    pub(crate) text: String,
}

/// Every source file of a crate, read once, in path order.
///
/// # Errors
///
/// Refuses when a file under the crate's `src/` cannot be read.
fn sources_of(workspace: &Workspace<'_>, crate_dir: &str) -> Result<Vec<CrateSource>> {
    let src = format!("{crate_dir}/src");
    let mut sources = Vec::new();

    for path in rust_files_under(workspace.root, &src) {
        let home = module_path_of(
            path.strip_prefix(&format!("{src}/"))
                .unwrap_or(path.as_str()),
        );
        sources.push(CrateSource {
            text: workspace.read(&path)?,
            path,
            home,
        });
    }

    sources.sort_by(|one, other| one.path.cmp(&other.path));
    Ok(sources)
}

/// The files staying behind in `module`'s crate that name it, in path order.
fn siblings_naming(
    sources: &[CrateSource],
    module: &MovingModule,
    moving: &[MovingModule],
) -> Vec<String> {
    sources
        .iter()
        .filter(|source| !travelling(&source.home, module, moving))
        .filter(|source| names_the_module(&source.text, module.path(), &source.home))
        .map(|source| source.path.clone())
        .collect()
}

/// Whether the file at module path `home` has left the origin by the time `module`'s operation
/// runs, as a module moved by it or by an earlier operation, or as part of one.
///
/// A file a later operation moves is still in the origin then, so `apply` re-points it like any
/// other caller.
fn travelling(home: &[String], module: &MovingModule, moving: &[MovingModule]) -> bool {
    gone_by_then(module, moving).any(|other| home.starts_with(other.path()))
}

/// The modules of `module`'s crate that have left it by the time `module`'s operation runs: its own
/// operation's, which move with it, and every earlier operation's, which are already in their
/// destination.
fn gone_by_then<'a>(
    module: &'a MovingModule,
    moving: &'a [MovingModule],
) -> impl Iterator<Item = &'a MovingModule> {
    moving
        .iter()
        .filter(|other| other.crate_dir() == module.crate_dir() && other.op <= module.op)
}

/// The later operations moving a module one of `paths` reaches, in plan order.
///
/// `paths` are the moved header's, re-pointed at the origin, so each reads
/// `<origin>::<module path>::…`.
fn moved_later(paths: &[String], module: &MovingModule, moving: &[MovingModule]) -> Vec<usize> {
    let origin = &module.moving.origin.extern_name;
    let within: Vec<Vec<&str>> = paths
        .iter()
        .filter_map(|path| path.strip_prefix(origin.as_str())?.strip_prefix("::"))
        .map(|path| path.split("::").collect())
        .collect();

    let later: BTreeSet<usize> = moving
        .iter()
        .filter(|other| other.crate_dir() == module.crate_dir() && other.op > module.op)
        .filter(|other| {
            within.iter().any(|path| {
                path.len() >= other.path().len()
                    && path.iter().zip(other.path()).all(|(one, two)| *one == two)
            })
        })
        .map(|other| other.op)
        .collect();
    later.into_iter().collect()
}

/// The module path a file under a crate's `src/` carries — `[]` for the crate root.
fn module_path_of(within_src: &str) -> Vec<String> {
    let mut path: Vec<String> = within_src.split('/').map(str::to_string).collect();
    match path
        .pop()
        .as_deref()
        .and_then(|last| last.strip_suffix(".rs"))
    {
        // `lib.rs`, `main.rs` and a directory's `mod.rs` all name the module their directory is.
        Some("lib" | "main" | "mod") | None => path,
        Some(module) => {
            path.push(module.to_string());
            path
        }
    }
}

/// Whether `text` writes a path that reaches the module at `target`.
///
/// Read from the text rather than from an index, which is the point: the decision is available
/// before rust-analyzer is spawned, and `check` reported `no findings` on a plan `apply` then
/// rejected because it was not read at all. A `crate::` path is absolute within the crate; a
/// `super::` one is read against the module holding the file, which is what makes
/// `super::spawner` in `supervisor/client.rs` a reference to `supervisor::spawner` rather than to
/// the crate-root `spawner`.
pub(crate) fn names_the_module(text: &str, target: &[String], home: &[String]) -> bool {
    let above = home.split_last().map(|(_, above)| above).unwrap_or(&[]);
    for (qualifier, base) in [("crate::", &[] as &[String]), ("super::", above)] {
        for (at, _) in text.match_indices(qualifier) {
            if text[..at]
                .chars()
                .next_back()
                .is_some_and(header::is_path_character)
            {
                continue;
            }
            let mut named = base.to_vec();
            named.extend(segments_at(&text[at + qualifier.len()..]));
            if named.starts_with(target) {
                return true;
            }
        }
    }
    false
}

/// The `a::b::C` a path continues with, from the first segment onwards.
fn segments_at(text: &str) -> Vec<String> {
    let mut rest = text;
    let mut segments = Vec::new();
    loop {
        let end = rest
            .find(|character: char| !header::is_path_character(character))
            .unwrap_or(rest.len());
        if end == 0 {
            return segments;
        }
        segments.push(rest[..end].to_string());
        match rest[end..].strip_prefix("::") {
            Some(next) => rest = next,
            None => return segments,
        }
    }
}

/// Every `.rs` file under `relative`, relative to the repository root, deepest paths included.
///
/// A directory that cannot be read contributes nothing: the scan is over a crate's own `src/`, and
/// a crate whose sources are unreadable is refused by the move itself with the file it could not
/// read named, which is a better report than this one could give.
fn rust_files_under(root: &Path, relative: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root.join(relative)) else {
        return Vec::new();
    };

    let mut files = Vec::new();
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let path = format!("{relative}/{name}");
        if entry.path().is_dir() {
            files.extend(rust_files_under(root, &path));
        } else if name.ends_with(".rs") {
            files.push(path);
        }
    }
    files
}
