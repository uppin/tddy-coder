use crate::crate_move::destination;
use crate::plan::RefactorOp;

use crate::edit::WorkspaceEdit;

use super::Result;

use crate::registry::Workspace;

use super::ModuleReferences;

use std::collections::{BTreeMap, BTreeSet};

use crate::plan::Reexport;

use super::module_home;

use super::malformed;
use crate::crate_move::{header, moving, refusals};
use crate::edit::{FileEdit, TextEdit};

use std::path::Path;

/// A set of modules that move to one destination **as a single unit**.
///
/// `move_module_to_crate` models one module, and that is why a mutually-referencing subsystem
/// cannot move: the header pass re-points every `crate::` path at the *origin*, so a reference to a
/// sibling that is also moving becomes a `destination → origin` edge; and the reference survey runs
/// against a tree where the siblings have not moved, so moving one rewrites the others' callers
/// before they are correct. Between the first operation and the last the tree does not compile,
/// which is why there is no intermediate state to verify against.
///
/// `#unbundle` node 3 moved **0 of 4** entangled modules for exactly this reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovingCluster {
    /// Every module in the set, in the order the plan named them.
    pub members: Vec<module_home::ModuleHome>,
    /// Where they are all going.
    pub destination: destination::Destination,
    /// What each leaves behind in the crate it left.
    pub reexport: Reexport,
}

impl MovingCluster {
    /// The module paths moving together, as a `crate::`-relative path would write them.
    ///
    /// This is what the header pass needs in order to tell a sibling that is coming along from one
    /// that is staying behind — the distinction the single-module model cannot express.
    #[must_use]
    pub fn co_moving(&self) -> BTreeSet<String> {
        self.members
            .iter()
            .map(|member| member.path.join("::"))
            .collect()
    }
}

/// The cluster a single module makes on its own.
///
/// A module travelling alone still has to answer the co-moving question — with "nothing else is
/// coming" — so [`resolve`](super::resolve) is this cluster resolved rather than a second
/// implementation of the same operation beside it.
pub(crate) fn travelling_alone(moving: &moving::Move) -> MovingCluster {
    MovingCluster {
        members: vec![moving.home.clone()],
        destination: moving.destination.clone(),
        reexport: moving.reexport,
    }
}

/// The cluster a `move_cluster_to_crate` operation names: its anchor is the first member and
/// `also` the rest, so every member is addressed the way a single move addresses its one module.
///
/// Read here rather than in the backend, so what a plan means stays where the plan's own
/// vocabulary is honoured. Refuses for every reason resolving one member's home does, and for a
/// destination the operation does not name.
pub(crate) fn named_by(workspace: &Workspace<'_>, op: &RefactorOp) -> Result<MovingCluster> {
    let mut members = Vec::new();
    for anchor in op.anchors() {
        let source = anchor.file();
        let module = module_home::module_name(source)?;
        members.push(module_home::module_home(workspace, source, &module)?);
    }
    let to = op.to.as_deref().ok_or_else(|| {
        malformed("`move_cluster_to_crate` needs `to`: the destination crate's directory")
    })?;

    Ok(MovingCluster {
        members,
        destination: destination::Destination::read(workspace.root, to)?,
        reexport: op.reexport.unwrap_or(Reexport::None),
    })
}

/// Resolve a whole cluster into one edit, applied all or not at all.
///
/// Every member's callers are surveyed against the **post-move** shape of the set, so a reference to
/// a co-moving sibling is re-pointed at the destination rather than at the crate it left. The
/// returned edit is the union: there is no ordering in which the tree is half-moved.
///
/// Each member contributes what [`resolve`](super::resolve) describes for one module — a rename, its
/// own re-pointed header, the declaration it leaves behind and the one it gains. The three edits
/// that belong to the *set* rather than to a member are made once over the union: the destination's
/// manifest gains every crate the whole set names, each crate that goes on naming any member gains
/// a dependency on the destination, and the workspace `members` list gains the destination at most
/// once. Computing them per member would write a dependency line, and a members entry, once each.
///
/// # Errors
///
/// Refuses for every reason a single move does, plus: an empty set, a member named twice, and a set
/// whose members do not all leave the same crate — `crate::<module>` is written relative to one
/// crate root, so a set spanning two of them has no single co-moving vocabulary to be read in.
pub fn resolve_cluster(
    engine: &mut dyn ModuleReferences,
    workspace: &Workspace<'_>,
    cluster: &MovingCluster,
) -> Result<WorkspaceEdit> {
    let members = read_members(workspace, cluster)?;
    let co_moving = cluster.co_moving();
    let travelling: BTreeSet<String> = members.iter().map(|member| member.source.clone()).collect();

    let mut merged = MergedChanges::default();
    let mut crates_named = BTreeSet::new();
    let mut keeps_naming_it = BTreeSet::new();

    for member in &members {
        let (survey, rewrites) = super::surveyed(engine, workspace, member, &travelling)?;
        let moved = workspace.read(&member.source)?;
        let header = header::repointed_header(workspace, &moved, &member.origin, &co_moving)?;
        let names = refusals::crates_the_moved_code_names(workspace, &member.origin, &header)?;
        let naming_it = refusals::crates_still_naming_the_module(workspace, member, &rewrites)?;
        refusals::refuse_a_dependency_cycle(workspace, member, &header, &naming_it)?;

        merged.absorb(FileEdit::Rename {
            from: member.source.clone(),
            to: member.moved_to(),
        });
        merged.add(member.source.clone(), header.edits);
        merged.absorb(member.left_behind(workspace, &survey)?);
        merged.absorb(member.declared_in_destination(workspace)?);
        if member.reexport == Reexport::None {
            for change in moving::caller_changes(workspace, rewrites)? {
                merged.absorb(change);
            }
        }

        crates_named.extend(names);
        keeps_naming_it.extend(naming_it);
    }

    // Read off the first member because every member shares them: `read_members` refuses a set
    // whose members leave different crates, and the destination is the set's own field.
    let across_the_set = &members[0];
    merged.absorb(across_the_set.destination_manifest(workspace, &crates_named)?);
    for change in across_the_set.dependents_on_the_destination(workspace, &keeps_naming_it)? {
        merged.absorb(change);
    }
    for change in across_the_set.workspace_members(workspace)? {
        merged.absorb(change);
    }

    Ok(WorkspaceEdit {
        changes: merged.changes(),
    })
}

/// Every member as the move it is, refusing a set that cannot be resolved as one.
fn read_members(workspace: &Workspace<'_>, cluster: &MovingCluster) -> Result<Vec<moving::Move>> {
    let Some(first) = cluster.members.first() else {
        return Err(malformed(
            "a moving cluster names no modules — there is nothing to move",
        ));
    };
    if cluster.co_moving().len() != cluster.members.len() {
        return Err(malformed(
            "a moving cluster names the same module twice — it would be moved, declared and \
             depended on twice over",
        ));
    }

    let mut members = Vec::new();
    for home in &cluster.members {
        if home.crate_dir != first.crate_dir {
            return Err(malformed(format!(
                "`{}` and `{}` are in different crates, so the set has no one crate root for its \
                 `crate::` paths to be read against — move one crate's modules at a time",
                first.crate_dir, home.crate_dir
            )));
        }
        members.push(moving::Move::of(
            workspace,
            home,
            &cluster.destination,
            cluster.reexport,
        )?);
    }
    Ok(members)
}

/// The edits a cluster makes, with each file changed exactly once.
///
/// Every member addresses its edits in the coordinates of the tree as it stands, so the whole set
/// shares one coordinate space per file — and [`crate::apply::edited`] folds one file's edits
/// last-first within that space. Two `FileEdit::Change`s for one path would instead be applied in
/// sequence, the second against text the first had already shifted.
#[derive(Default)]
struct MergedChanges {
    /// The paths in the order they were first changed, which is the order they are reported in.
    order: Vec<String>,
    edits: BTreeMap<String, Vec<TextEdit>>,
    /// Renames and creations, which carry no text to merge.
    resources: Vec<FileEdit>,
}

impl MergedChanges {
    fn add(&mut self, path: String, edits: Vec<TextEdit>) {
        if !self.edits.contains_key(&path) {
            self.order.push(path.clone());
        }
        self.edits.entry(path).or_default().extend(edits);
    }

    fn absorb(&mut self, change: FileEdit) {
        match change {
            FileEdit::Change { path, edits } => self.add(path, edits),
            resource => self.resources.push(resource),
        }
    }

    /// The resource operations, then one change per file.
    ///
    /// A change with no edits names a file the operation did not touch, and the journal would hash
    /// it as one it did.
    fn changes(mut self) -> Vec<FileEdit> {
        let mut changes = self.resources;
        for path in self.order {
            let edits = self.edits.remove(&path).unwrap_or_default();
            if !edits.is_empty() {
                changes.push(FileEdit::Change { path, edits });
            }
        }
        changes
    }
}

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
/// Empty when no moved module names a sibling staying behind, or nothing left behind names it back.
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

        findings.push((
            module.op,
            format!(
                "`{source}`, which operation {op} moves to `{destination}`, names `{paths}`, which \
                 stays behind in `{origin}` — so the destination would depend on the crate it \
                 left, while {origin_names_it_back}: a cycle `apply` refuses. Move what those \
                 paths reach with the set, or leave `{named}` where it is",
                source = module.source,
                op = module.op,
                destination = module.moving.destination.dir,
                paths = names_the_origin.join("`, `"),
                origin = module.crate_dir(),
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
/// refusal agree about which paths count: one reaching a module the plan also moves out of this
/// crate is not an edge, nor is one the origin only re-exports from a third crate.
fn paths_naming_the_origin(
    workspace: &Workspace<'_>,
    module: &MovingModule,
    moving: &[MovingModule],
) -> Result<Vec<String>> {
    let co_moving: BTreeSet<String> = moving
        .iter()
        .filter(|other| other.crate_dir() == module.crate_dir())
        .map(|other| other.path().join("::"))
        .collect();
    let text = workspace.read(&module.source)?;
    let header = header::repointed_header(workspace, &text, &module.moving.origin, &co_moving)?;
    refusals::origin_named_dependencies(workspace, &module.moving, &header)
}

/// One module a plan moves out of the crate that holds it.
struct MovingModule {
    /// Which operation moves it, by index in the plan.
    op: usize,
    /// The module file the plan names, relative to the repository root.
    source: String,
    /// The move as `apply` reads it: both crates, the module's home and the facade it leaves.
    moving: moving::Move,
}

impl MovingModule {
    /// The crate it is leaving.
    fn crate_dir(&self) -> &str {
        &self.moving.home.crate_dir
    }

    /// Its module path inside that crate, outermost first.
    fn path(&self) -> &[String] {
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
            super::move_preconditions(workspace, &op.with_anchor(anchor.clone())).ok()?;
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
    path: String,
    /// The module path the file carries inside its crate — `[]` for the crate root.
    home: Vec<String>,
    text: String,
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

/// Whether the file at module path `home` is moving with the set — itself, or as part of a member.
fn travelling(home: &[String], module: &MovingModule, moving: &[MovingModule]) -> bool {
    moving
        .iter()
        .filter(|other| other.crate_dir() == module.crate_dir())
        .any(|other| home.starts_with(other.path()))
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
fn names_the_module(text: &str, target: &[String], home: &[String]) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crate_move::{manifest_edits, ItemReferences, Reference};
    use crate::plan::{Anchor, RefactorKind};

    const ORIGIN: &str = "crates/origin";
    const ORIGIN_ROOT: &str = "crates/origin/src/lib.rs";
    const SPAWNER: &str = "crates/origin/src/spawner.rs";
    const WORKER: &str = "crates/origin/src/spawn_worker.rs";
    const DESTINATION: &str = "crates/destination";
    const DESTINATION_ROOT: &str = "crates/destination/src/lib.rs";
    const DESTINATION_MANIFEST: &str = "crates/destination/Cargo.toml";

    /// A crate whose two modules name each other, the crate they are going to, and a workspace
    /// root that lists neither.
    ///
    /// Every file is real because everything the operation decides, it reads: the declared package
    /// names, the `mod` lines it replaces, the headers it re-points and both manifests. The pair
    /// referencing each other is the shape that defeated `#unbundle` node 3.
    struct AWorkspace {
        root: tempfile::TempDir,
        overlay: crate::Overlay,
    }

    fn a_workspace_with_an_entangled_pair() -> AWorkspace {
        AWorkspace {
            root: tempfile::tempdir().expect("a temporary directory"),
            overlay: crate::Overlay::default(),
        }
        .with(
            "Cargo.toml",
            "[workspace]\nmembers = [\n    \"crates/origin\",\n]\n",
        )
        .with(
            "crates/origin/Cargo.toml",
            "[package]\nname = \"origin\"\n\n[dependencies]\nshared = { path = \"../shared\" }\n",
        )
        .with(
            ORIGIN_ROOT,
            "//! The origin.\n\nmod spawner;\nmod spawn_worker;\nmod runtime;\n",
        )
        .with(
            SPAWNER,
            "use crate::spawn_worker::Worker;\n\npub struct Spawner;\n",
        )
        .with(
            WORKER,
            "use crate::spawner::Spawner;\n\npub struct Worker;\n",
        )
        .with("crates/origin/src/runtime.rs", "pub struct Clock;\n")
        .with(
            "crates/destination/Cargo.toml",
            "[package]\nname = \"destination\"\n\n[dependencies]\n",
        )
        .with(DESTINATION_ROOT, "//! The destination.\n\n")
    }

    impl AWorkspace {
        fn with(self, path: &str, text: &str) -> Self {
            let absolute = self.root.path().join(path);
            std::fs::create_dir_all(absolute.parent().expect("a parent")).expect("directories");
            std::fs::write(absolute, text).expect("the file is written");
            self
        }

        fn read(&self, path: &str) -> String {
            std::fs::read_to_string(self.root.path().join(path)).expect("the file is read")
        }

        fn workspace(&self) -> Workspace<'_> {
            Workspace {
                root: self.root.path(),
                overlay: &self.overlay,
            }
        }
    }

    /// A reference set standing in for `textDocument/references`.
    ///
    /// A fake rather than a mock: it answers the one question the engine answers — which places
    /// outside a module's own file name each of its items — and the deciding half under test cannot
    /// tell it from the Rust backend's own implementation.
    #[derive(Default)]
    struct AKnownReferenceSet {
        by_module: BTreeMap<String, Vec<ItemReferences>>,
    }

    fn nothing_reaches_the_set() -> AKnownReferenceSet {
        AKnownReferenceSet::default()
    }

    impl AKnownReferenceSet {
        /// Every place each of `referring` names `item`, which `module` declares — the import that
        /// binds the name and each use of it, as the server reports them.
        fn reaching(
            mut self,
            item: &str,
            in_module: &str,
            from: &[&str],
            workspace: &AWorkspace,
        ) -> Self {
            let referenced_at = from
                .iter()
                .flat_map(|file| {
                    let text = workspace.read(file);
                    text.match_indices(item)
                        .map(|(offset, _)| Reference {
                            path: (*file).to_string(),
                            at: manifest_edits::position_of(&text, offset),
                        })
                        .collect::<Vec<_>>()
                })
                .collect();

            self.by_module
                .entry(in_module.to_string())
                .or_default()
                .push(ItemReferences {
                    item: item.to_string(),
                    referenced_at,
                });
            self
        }
    }

    impl ModuleReferences for AKnownReferenceSet {
        fn outside_references(
            &mut self,
            _workspace: &Workspace<'_>,
            file: &str,
        ) -> Result<Vec<ItemReferences>> {
            Ok(self.by_module.get(file).cloned().unwrap_or_default())
        }
    }

    fn a_member(module: &str) -> module_home::ModuleHome {
        module_home::ModuleHome {
            crate_dir: ORIGIN.to_string(),
            declared_in: ORIGIN_ROOT.to_string(),
            path: vec![module.to_string()],
        }
    }

    fn a_cluster_of(members: &[&str], reexport: Reexport) -> MovingCluster {
        MovingCluster {
            members: members.iter().map(|module| a_member(module)).collect(),
            destination: destination::Destination {
                dir: DESTINATION.to_string(),
                package: "destination".to_string(),
                extern_name: "destination".to_string(),
            },
            reexport,
        }
    }

    /// What a file reads as once the cluster's edits for it are applied — its own text where the
    /// cluster authored none.
    ///
    /// Authoring nothing is a real outcome, not an absent one: a path that already says what it
    /// has to say after the move is left alone, and an identity edit would make the journal hash a
    /// file the operation did not touch. The assertion the caller makes is on the text either way.
    fn applied(edit: &WorkspaceEdit, path: &str, workspace: &AWorkspace) -> String {
        let edits = edit
            .changes
            .iter()
            .find_map(|change| match change {
                FileEdit::Change {
                    path: changed,
                    edits,
                } if changed == path => Some(edits.clone()),
                _ => None,
            })
            .unwrap_or_default();

        crate::apply::edited(workspace.read(path), &edits).expect("the edits apply")
    }

    fn renames(edit: &WorkspaceEdit) -> Vec<(&str, &str)> {
        edit.changes
            .iter()
            .filter_map(|change| match change {
                FileEdit::Rename { from, to } => Some((from.as_str(), to.as_str())),
                _ => None,
            })
            .collect()
    }

    fn changed(edit: &WorkspaceEdit) -> Vec<&str> {
        edit.changes
            .iter()
            .filter_map(|change| match change {
                FileEdit::Change { path, .. } => Some(path.as_str()),
                _ => None,
            })
            .collect()
    }

    /// AC1 — the whole set moves in one edit, so the tree is never left half-moved.
    #[test]
    fn moves_every_member_of_a_mutually_referencing_set_in_one_edit() {
        // Given a pair of modules that name each other, moving together
        let workspace = a_workspace_with_an_entangled_pair();
        let mut engine = nothing_reaches_the_set();
        let cluster = a_cluster_of(&["spawner", "spawn_worker"], Reexport::Glob);

        // When the set is resolved
        let edit = resolve_cluster(&mut engine, &workspace.workspace(), &cluster)
            .expect("a cluster resolves");

        // Then both files move, in the one edit
        assert_eq!(
            renames(&edit),
            vec![
                (SPAWNER, "crates/destination/src/spawner.rs"),
                (WORKER, "crates/destination/src/spawn_worker.rs"),
            ]
        );
    }

    /// AC2 — a path reaching a sibling that is coming along resolves in the destination, because by
    /// the time the edit lands that sibling is there.
    ///
    /// Which is `crate::`, not the destination's package name: the destination *is* `crate` for a
    /// file that has arrived in it, and `use destination::…` written inside crate `destination` is
    /// `E0433`. The live suite settles that with `cargo check`; this pins the text it produces.
    #[test]
    fn points_a_path_reaching_a_co_moving_sibling_at_the_destination() {
        // Given a member whose header names a sibling in the same set
        let workspace = a_workspace_with_an_entangled_pair();
        let mut engine = nothing_reaches_the_set();
        let cluster = a_cluster_of(&["spawner", "spawn_worker"], Reexport::Glob);

        // When
        let edit = resolve_cluster(&mut engine, &workspace.workspace(), &cluster)
            .expect("a cluster resolves");

        // Then
        assert_eq!(
            applied(&edit, SPAWNER, &workspace),
            "use crate::spawn_worker::Worker;\n\npub struct Spawner;\n"
        );
    }

    /// AC3 — a path reaching a module staying behind names the origin, exactly as a single-module
    /// move leaves it.
    #[test]
    fn points_a_path_reaching_a_module_staying_behind_at_the_origin() {
        // Given a member whose header names a module the set does not include
        let workspace = a_workspace_with_an_entangled_pair().with(
            SPAWNER,
            "use crate::runtime::Clock;\nuse crate::spawn_worker::Worker;\n\npub struct Spawner;\n",
        );
        let mut engine = nothing_reaches_the_set();
        let cluster = a_cluster_of(&["spawner", "spawn_worker"], Reexport::None);

        // When
        let edit = resolve_cluster(&mut engine, &workspace.workspace(), &cluster)
            .expect("a cluster with no facade resolves");

        // Then only the path reaching a module staying behind was sent to the origin
        assert_eq!(
            applied(&edit, SPAWNER, &workspace),
            "use origin::runtime::Clock;\nuse crate::spawn_worker::Worker;\n\n\
             pub struct Spawner;\n"
        );
    }

    /// AC4 — a co-moving reference is not an outbound dependency, so the destination is never made
    /// to depend on itself. Cargo's own error for that names neither the module nor the operation.
    #[test]
    fn does_not_make_the_destination_depend_on_itself() {
        // Given a set whose every `crate::` path reaches another member
        let workspace = a_workspace_with_an_entangled_pair();
        let mut engine = nothing_reaches_the_set();
        let cluster = a_cluster_of(&["spawner", "spawn_worker"], Reexport::Glob);

        // When
        let edit = resolve_cluster(&mut engine, &workspace.workspace(), &cluster)
            .expect("a cluster resolves");

        // Then the destination's manifest gained nothing at all
        assert!(
            !changed(&edit).contains(&DESTINATION_MANIFEST),
            "the destination's manifest was edited for a set that names no other crate: {:?}",
            changed(&edit)
        );
    }

    /// AC1 — the destination declares every member, so the set arrives whole.
    #[test]
    fn declares_every_member_in_the_crate_they_arrive_in() {
        // Given
        let workspace = a_workspace_with_an_entangled_pair();
        let mut engine = nothing_reaches_the_set();
        let cluster = a_cluster_of(&["spawner", "spawn_worker"], Reexport::Glob);

        // When
        let edit = resolve_cluster(&mut engine, &workspace.workspace(), &cluster)
            .expect("a cluster resolves");

        // Then
        assert_eq!(
            applied(&edit, DESTINATION_ROOT, &workspace),
            "//! The destination.\n\npub mod spawner;\npub mod spawn_worker;\n"
        );
    }

    /// The workspace `members` list gains the destination **once**, however many modules moved:
    /// a list with the same crate in it twice is a manifest cargo refuses to read.
    #[test]
    fn adds_the_destination_to_the_workspace_members_once_for_the_whole_set() {
        // Given a set of two moving into a crate the workspace does not list
        let workspace = a_workspace_with_an_entangled_pair();
        let mut engine = nothing_reaches_the_set();
        let cluster = a_cluster_of(&["spawner", "spawn_worker"], Reexport::Glob);

        // When
        let edit = resolve_cluster(&mut engine, &workspace.workspace(), &cluster)
            .expect("a cluster resolves");

        // Then
        assert_eq!(
            applied(&edit, "Cargo.toml", &workspace),
            "[workspace]\nmembers = [\n    \"crates/origin\",\n    \"crates/destination\",\n]\n"
        );
    }

    /// A caller outside the set is re-pointed once, from the file it sits in — and a reference
    /// inside the set is left to the member's own header pass, which is already moving it.
    #[test]
    fn re_points_a_caller_outside_the_set_without_touching_one_inside_it() {
        // Given a module staying behind that names a member, and members that name each other
        let workspace = a_workspace_with_an_entangled_pair().with(
            "crates/origin/src/runtime.rs",
            "use crate::spawner::Spawner;\n\npub fn boot(spawner: &Spawner) {}\n",
        );
        let mut engine = nothing_reaches_the_set()
            .reaching(
                "Spawner",
                SPAWNER,
                &["crates/origin/src/runtime.rs", WORKER],
                &workspace,
            )
            .reaching("Worker", WORKER, &[SPAWNER], &workspace);
        let cluster = a_cluster_of(&["spawner", "spawn_worker"], Reexport::None);

        // When
        let edit = resolve_cluster(&mut engine, &workspace.workspace(), &cluster)
            .expect("a cluster with no facade resolves");

        // Then the module outside the set names the destination, and the member's own header was
        // left to say `crate::` — re-pointing it here as well would author two edits over one span
        assert_eq!(
            applied(&edit, "crates/origin/src/runtime.rs", &workspace),
            "use destination::spawner::Spawner;\n\npub fn boot(spawner: &Spawner) {}\n"
        );
        assert_eq!(
            applied(&edit, WORKER, &workspace),
            "use crate::spawner::Spawner;\n\npub struct Worker;\n"
        );
    }

    /// `crate::<module>` is written against one crate root, so a set spanning two crates has no
    /// single vocabulary to read its co-moving paths in.
    #[test]
    fn refuses_a_set_whose_members_leave_different_crates() {
        // Given a set naming a module in another crate
        let workspace = a_workspace_with_an_entangled_pair();
        let mut engine = nothing_reaches_the_set();
        let mut cluster = a_cluster_of(&["spawner"], Reexport::Glob);
        cluster.members.push(module_home::ModuleHome {
            crate_dir: "crates/elsewhere".to_string(),
            declared_in: "crates/elsewhere/src/lib.rs".to_string(),
            path: vec!["spawn_worker".to_string()],
        });

        // When
        let outcome = resolve_cluster(&mut engine, &workspace.workspace(), &cluster);

        // Then
        assert_refusal(outcome).naming("different crates");
    }

    /// A member named twice would be moved, declared and depended on twice over.
    #[test]
    fn refuses_a_set_naming_the_same_module_twice() {
        // Given
        let workspace = a_workspace_with_an_entangled_pair();
        let mut engine = nothing_reaches_the_set();
        let cluster = a_cluster_of(&["spawner", "spawner"], Reexport::Glob);

        // When
        let outcome = resolve_cluster(&mut engine, &workspace.workspace(), &cluster);

        // Then
        assert_refusal(outcome).naming("same module twice");
    }

    /// An empty set is a plan defect rather than a move of nothing: every edit the operation makes
    /// beyond a member's own is read off the set, and there would be nothing to read.
    #[test]
    fn refuses_a_set_with_no_members() {
        // Given
        let workspace = a_workspace_with_an_entangled_pair();
        let mut engine = nothing_reaches_the_set();
        let cluster = a_cluster_of(&[], Reexport::Glob);

        // When
        let outcome = resolve_cluster(&mut engine, &workspace.workspace(), &cluster);

        // Then
        assert_refusal(outcome).naming("no modules");
    }

    /// The scan reads paths, not substrings: `spawner_pool` is a module of its own, and reporting
    /// it as a reference to `spawner` is the kind of finding that teaches a reader to skip them.
    #[test]
    fn does_not_read_a_longer_module_name_as_a_reference() {
        // Given text naming a module whose name begins with the moving one's
        let text = "use crate::spawner_pool::Pool;\n";

        // When
        let names = names_the_module(text, &["spawner".to_string()], &["other".to_string()]);

        // Then
        assert!(
            !names,
            "`spawner_pool` was read as a reference to `spawner`"
        );
    }

    /// A `super::` path is read against the module holding the file, so a nested sibling's
    /// reference is attributed to the module it actually reaches.
    #[test]
    fn reads_a_super_path_against_the_module_holding_the_file() {
        // Given a file in `supervisor/`, naming its own sibling
        let text = "use super::spawner::Spawner;\n";

        // When it is read for a reference to `supervisor::spawner`, and to the crate-root one
        let nested = names_the_module(
            text,
            &["supervisor".to_string(), "spawner".to_string()],
            &["supervisor".to_string(), "client".to_string()],
        );
        let at_the_root = names_the_module(
            text,
            &["spawner".to_string()],
            &["supervisor".to_string(), "client".to_string()],
        );

        // Then
        assert!(
            nested,
            "`super::spawner` did not reach `supervisor::spawner`"
        );
        assert!(
            !at_the_root,
            "`super::spawner` was read as reaching the crate-root `spawner`"
        );
    }

    /// AC7 — the finding names the operation it is about, so a reader can fix the plan by index.
    #[test]
    fn ties_a_stranded_sibling_to_the_operation_that_would_strand_it() {
        // Given a plan whose second operation moves half of a mutually-referencing pair
        let workspace = a_workspace_with_an_entangled_pair();
        let plan = [a_move_of("runtime"), a_move_of("spawner")];

        // When
        let findings = stranded_siblings(&workspace.workspace(), &plan)
            .expect("the check reads the workspace");

        // Then
        assert_eq!(
            findings.iter().map(|(op, _)| *op).collect::<Vec<_>>(),
            vec![1],
            "the findings are not the second operation's: {findings:?}"
        );
    }

    /// A module staying behind that names the one moving is what a facade serves: the path it
    /// writes, `crate::spawner::…`, resolves through `pub use destination::spawner::*;` exactly as
    /// before. Flagging it made every move of a module anything still calls look like a defect.
    #[test]
    fn does_not_report_a_module_staying_behind_that_names_one_leaving_behind_a_facade() {
        // Given a module that names nothing staying, called by two modules that are staying
        let workspace = a_workspace_with_an_entangled_pair()
            .with(SPAWNER, "pub struct Spawner;\n")
            .with(
                "crates/origin/src/runtime.rs",
                "use crate::spawner::Spawner;\n\npub fn boot(spawner: &Spawner) {}\n",
            );
        let plan = [a_move_of("spawner")];

        // When
        let findings = stranded_siblings(&workspace.workspace(), &plan)
            .expect("the check reads the workspace");

        // Then
        assert_eq!(findings, Vec::<(usize, String)>::new());
    }

    /// The protection the finding exists for: a moved module naming a sibling that stays behind
    /// makes the destination depend on the crate it left, while the facade keeps that crate naming
    /// the destination — a cycle `apply` refuses.
    #[test]
    fn reports_a_moving_module_that_names_a_sibling_staying_behind_a_facade() {
        // Given a module that names a sibling the plan does not move
        let workspace = a_workspace_with_an_entangled_pair().with(WORKER, "pub struct Worker;\n");
        let plan = [a_move_of("spawner")];

        // When
        let findings = stranded_siblings(&workspace.workspace(), &plan)
            .expect("the check reads the workspace");

        // Then
        assert_findings(findings)
            .are_about_operations(&[0])
            .naming("origin::spawn_worker::Worker");
    }

    /// With no facade, `apply` re-points every caller it finds at the destination, so a module
    /// staying behind that names the one leaving is rewritten rather than broken.
    #[test]
    fn does_not_report_a_module_staying_behind_that_names_one_leaving_without_a_facade() {
        // Given a module that names nothing staying, called by a module that is staying
        let workspace = a_workspace_with_an_entangled_pair().with(SPAWNER, "pub struct Spawner;\n");
        let plan = [a_move_without_a_facade_of("spawner")];

        // When
        let findings = stranded_siblings(&workspace.workspace(), &plan)
            .expect("the check reads the workspace");

        // Then
        assert_eq!(findings, Vec::<(usize, String)>::new());
    }

    /// Without a facade the crate left behind still names the destination wherever a caller of the
    /// moved module is re-pointed — so a moved module naming that crate back is the same cycle.
    #[test]
    fn reports_a_mutually_referencing_pair_split_by_a_move_without_a_facade() {
        // Given a pair that name each other, one of which moves
        let workspace = a_workspace_with_an_entangled_pair();
        let plan = [a_move_without_a_facade_of("spawner")];

        // When
        let findings = stranded_siblings(&workspace.workspace(), &plan)
            .expect("the check reads the workspace");

        // Then
        assert_findings(findings)
            .are_about_operations(&[0])
            .naming("origin::spawn_worker::Worker")
            .naming(WORKER);
    }

    /// Without a facade and with nothing left behind naming it, the moved module depending on the
    /// crate it left is one edge, not a cycle — `points_a_path_reaching_a_module_staying_behind_at_the_origin`
    /// is `apply` resolving exactly that move.
    #[test]
    fn does_not_report_a_module_leaving_without_a_facade_that_nothing_left_behind_names() {
        // Given a module naming a sibling staying behind, which nothing staying names back
        let workspace = a_workspace_with_an_entangled_pair()
            .with(
                SPAWNER,
                "use crate::runtime::Clock;\n\npub struct Spawner;\n",
            )
            .with(WORKER, "pub struct Worker;\n");
        let plan = [a_move_without_a_facade_of("spawner")];

        // When
        let findings = stranded_siblings(&workspace.workspace(), &plan)
            .expect("the check reads the workspace");

        // Then
        assert_eq!(findings, Vec::<(usize, String)>::new());
    }

    fn a_move_without_a_facade_of(module: &str) -> RefactorOp {
        RefactorOp {
            reexport: Some(Reexport::None),
            ..a_move_of(module)
        }
    }

    /// What a check found, read as the plan's reader reads it: by operation, then by what it says.
    struct TheFindings(Vec<(usize, String)>);

    fn assert_findings(findings: Vec<(usize, String)>) -> TheFindings {
        TheFindings(findings)
    }

    impl TheFindings {
        fn are_about_operations(self, expected: &[usize]) -> Self {
            assert_eq!(
                self.0.iter().map(|(op, _)| *op).collect::<Vec<_>>(),
                expected,
                "the findings are not about the expected operations: {:?}",
                self.0
            );
            self
        }

        fn naming(self, fragment: &str) -> Self {
            assert!(
                self.0.iter().all(|(_, said)| said.contains(fragment)),
                "expected every finding to name `{fragment}`: {:?}",
                self.0
            );
            self
        }
    }

    fn a_move_of(module: &str) -> RefactorOp {
        RefactorOp {
            op: RefactorKind::MoveModuleToCrate,
            anchor: Anchor::Symbol {
                file: format!("{ORIGIN}/src/{module}.rs"),
                path: module.to_string(),
            },
            name: None,
            to: Some(DESTINATION.to_string()),
            variant: None,
            with_private_deps: false,
            reexport: Some(Reexport::Glob),
            to_file: false,
            also: Vec::new(),
        }
    }

    /// A refusal is only useful if it names what made it refuse.
    struct ARefusal(crate::RestructureError);

    fn assert_refusal<T: std::fmt::Debug>(outcome: Result<T>) -> ARefusal {
        match outcome {
            Err(error) => ARefusal(error),
            Ok(value) => panic!("expected a refusal but the cluster resolved: {value:?}"),
        }
    }

    impl ARefusal {
        fn naming(self, fragment: &str) -> Self {
            let said = self.0.to_string();
            assert!(
                said.contains(fragment),
                "expected the refusal to name `{fragment}`, it said: {said}"
            );
            self
        }
    }
}
