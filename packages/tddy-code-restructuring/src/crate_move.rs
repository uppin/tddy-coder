//! Moving a module's file from one crate to another.
//!
//! This is the **third** transformation this package authors itself, after `extract_class` and the
//! facade `use` line — and the only one that crosses a crate boundary. rust-analyzer has no
//! cross-crate move assist, so there is nothing to delegate to: `move_symbol` and `move_file` are
//! absent from the Rust backend's supported set, `RefactorOp::to` is read nowhere on the assist
//! path, and `extract_module_to_file` names and places its own file beside the parent module. The
//! vocabulary deliberately dropped `rewrite_import_path` for having no engine behind it, and this
//! operation is not that one returning: **it is engine-informed rather than engine-performed.**
//!
//! The distinction is the whole justification, so it is worth stating precisely. Everything this
//! module decides comes from the server:
//!
//! | Decision | Source |
//! |---|---|
//! | which callers to rewrite | `textDocument/references` on the moved module's public items |
//! | what each caller's new path is | the destination crate's name from its own `Cargo.toml` |
//! | which crate a copied dependency line belongs to | the crate the moved code names, and the manifest that already declares it |
//! | whether an item is reached from outside at all | the reference set, not a text search |
//!
//! The reference set arrives through [`ModuleReferences`], which the Rust backend implements and a
//! test supplies directly — the deciding half is worth exercising against a known set rather than a
//! cold index.
//!
//! What this module authors is the *mechanical* half: a `git mv`, two manifest edits, a `pub use`
//! line, and the qualifier at the head of the moved file's own `use` declarations. None of those is
//! a code transformation an engine could have offered.
//!
//! # What the header pass is, and is not
//!
//! `extract_module` restores imports by asking the server which names went unresolved and which
//! import fixes each — it can, because the items stay in the file it is holding open. A module that
//! has left its crate cannot be typed until it is in the destination, so there is no equivalent
//! answer to ask for here, and none is invented: what this rewrites is the `crate::`/`super::`
//! qualifier at the head of the moved file's own `use` declarations, which changed meaning by
//! definition when the file changed crates. A `crate::` path written inside a function body is a
//! name in code and is left alone; a build after the move is what surfaces one.
//!
//! # The facade
//!
//! With `reexport`, the crate the module left keeps `pub use <new_crate>::…;` and **no caller
//! changes at all** — the same principle as `extract_module`'s in-parent facade, one level up. That
//! is what made a 23,099-line intra-package split invisible to 93 referring files, and it is what
//! makes a staged cross-crate move affordable: move first with a facade, remove the facade later
//! when the callers are ready.

use std::collections::BTreeSet;
use std::path::Path;

use crate::apply::byte_offset;
use crate::edit::{FileEdit, Position, Range, TextEdit, WorkspaceEdit};
use crate::plan::{Reexport, RefactorOp};
use crate::registry::Workspace;
use crate::RestructureError;

type Result<T> = std::result::Result<T, RestructureError>;

/// A refusal this operation makes on its own, before anything is written.
fn malformed(reason: impl Into<String>) -> RestructureError {
    RestructureError::MalformedPlan(reason.into())
}

/// The half of a cross-crate move only a language server can answer.
///
/// Nothing here decides *which* callers exist — it asks, which is what keeps the operation
/// engine-*informed* rather than a text search wearing an engine's clothes. The implementation that
/// matters is the Rust backend's `textDocument/references`; the seam exists because the deciding
/// half is worth testing against a known reference set rather than a cold index.
pub trait ModuleReferences {
    /// Every item in `file` a module path can name, each with the places outside `file` that reach
    /// it.
    ///
    /// An item nothing outside reaches comes back with an empty list rather than being omitted, so
    /// a caller can tell "reached by nobody" from "not an item".
    fn outside_references(
        &mut self,
        workspace: &Workspace<'_>,
        file: &str,
    ) -> Result<Vec<ItemReferences>>;
}

/// One item of the moving module, and everywhere outside its file that names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemReferences {
    /// The item's own name, as `mod`-level code would write it.
    pub item: String,
    /// Where it is reached from, outside the module's own file.
    pub referenced_at: Vec<Reference>,
}

/// One place a moving item is named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// The referring file, relative to the repository root.
    pub path: String,
    /// Where the item's own identifier starts — the position `textDocument/references` reports.
    pub at: Position,
}

/// Where a module is going, resolved from the plan's `to` and the destination's own manifest.
///
/// The crate *name* is never taken from the directory name: `packages/tddy-daemon-kernel` could
/// declare any `[package] name`, and a caller's `use` path needs the declared one with its hyphens
/// turned into underscores. Reading the manifest is the only correct source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    /// The destination crate's directory, relative to the repository root, as the plan gave it.
    pub dir: String,
    /// `[package] name` from the destination's `Cargo.toml` — e.g. `tddy-daemon-kernel`.
    pub package: String,
    /// The identifier a `use` path needs — `package` with `-` replaced by `_`.
    pub extern_name: String,
}

impl Destination {
    /// Read a destination from its `Cargo.toml`.
    ///
    /// Refuses a directory with no manifest rather than creating one: a plan that names a crate
    /// which does not exist is a plan defect, and scaffolding a crate is authoring, not moving.
    pub fn read(root: &Path, dir: &str) -> Result<Destination> {
        let manifest = root.join(dir).join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest).map_err(|error| {
            RestructureError::MalformedPlan(format!(
                "`{dir}` is not a crate: {} could not be read ({error})",
                manifest.display()
            ))
        })?;
        let package = declared_package_name(&text).ok_or_else(|| {
            RestructureError::MalformedPlan(format!(
                "{} declares no `[package] name`",
                manifest.display()
            ))
        })?;

        Ok(Destination {
            dir: dir.to_string(),
            package: package.to_string(),
            extern_name: package.replace('-', "_"),
        })
    }
}

/// The `[package] name` a manifest declares, read without a TOML parser.
///
/// One key of one table is all this needs, and the shape it has to survive is a workspace manifest
/// where `[dependencies]` and `[[bin]]` also carry a `name`. Scoping the search to the lines between
/// `[package]` and the next table header is what keeps those out; a dependency's name being returned
/// as the crate's would produce a `use` path that compiles nowhere.
fn declared_package_name(manifest: &str) -> Option<&str> {
    manifest
        .lines()
        .map(str::trim)
        .skip_while(|line| *line != "[package]")
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "name")
        .and_then(|(_, value)| quoted(value))
}

/// The contents of the first double-quoted string in `value`, or `None` if it is not one.
fn quoted(value: &str) -> Option<&str> {
    let opened = value.trim_start().strip_prefix('"')?;
    opened.find('"').map(|end| &opened[..end])
}

/// One caller this move has to re-point, and the path it needs afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerRewrite {
    /// The referring file, relative to the repository root.
    pub path: String,
    /// The path as the caller writes it today — e.g. `crate::host_registry::HostRegistry`.
    pub from: String,
    /// The path it needs after the move — e.g. `tddy_host_service::host_registry::HostRegistry`.
    ///
    /// The module keeps its name in the crate it arrives in, so the rewrite replaces everything
    /// *before* that name and nothing after it. That is also what makes the facade a glob rather
    /// than a per-item re-export: `pub use <new_crate>::*;` brings the module itself back into the
    /// crate root the callers already write, so with a facade this path is never needed.
    pub to: String,
}

/// Everything the operation established before it wrote anything, so a refusal can name it.
///
/// A survey is worth having as a value rather than an internal step for the same reason
/// `refuse_stranded` names each item and each referring file: when a cross-crate move is going to be
/// wrong, the useful output is *what it found*, not that it declined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Survey {
    /// The module file being moved, relative to the repository root.
    pub source: String,
    /// Where it is going.
    pub destination: Destination,
    /// Public items in the module that something outside it reaches.
    pub reached_from_outside: Vec<String>,
    /// Every caller found by `textDocument/references`, with its new path.
    pub callers: Vec<CallerRewrite>,
}

/// Survey a cross-crate move without writing anything.
///
/// This is what `restructure check --deep` rehearses, and it is deliberately separable from the
/// move: the reference set *is* the blast radius, and a plan author wants it before paying for a
/// cold index and an apply.
pub fn survey(
    engine: &mut dyn ModuleReferences,
    workspace: &Workspace<'_>,
    op: &RefactorOp,
) -> Result<Survey> {
    Ok(planned(engine, workspace, op)?.1)
}

/// Resolve a cross-crate move into the multi-file edit that performs it.
///
/// The returned [`WorkspaceEdit`] carries, in this order:
///
/// 1. `FileEdit::Rename` for the module file — applied with `git mv`, so blame survives. This is the
///    first time the Rust path emits a rename; `convert_change` refused every resource operation
///    but `create` until this operation existed.
/// 2. `FileEdit::Change` for the moved file's own `use` header.
/// 3. `FileEdit::Change` for the source crate's `lib.rs`: the `mod` declaration goes, and with
///    [`Reexport::Glob`] or [`Reexport::Named`] a `pub use <new_crate>::…;` takes its place.
/// 4. `FileEdit::Change` for the destination crate's `lib.rs`: the new `mod` declaration.
/// 5. `FileEdit::Change` per caller, when no facade was asked for.
/// 6. `FileEdit::Change` per `Cargo.toml` the new shape needs: the destination's `[dependencies]`
///    gains what the moved code names; every crate that goes on naming the module — the one holding
///    the facade, and each one whose callers were re-pointed — gains a dependency on the
///    destination; and the workspace `members` list gains the destination if it is new.
///
/// With a facade, (5) is empty by construction. That is the difference between a move a reviewer can
/// read and one that touches ninety files.
///
/// Edits are addressed in the coordinates of the tree as it stands, including (2), which names the
/// module at the path it is moving *from*: [`crate::apply`] applies creations, then changes, then
/// renames, so the file is still where the plan found it when its own text is rewritten.
pub fn resolve(
    engine: &mut dyn ModuleReferences,
    workspace: &Workspace<'_>,
    op: &RefactorOp,
) -> Result<WorkspaceEdit> {
    let (moving, survey, rewrites) = planned(engine, workspace, op)?;

    let moved = workspace.read(&moving.source)?;
    let header = repointed_header(&moved, &moving.origin.extern_name);
    let keeps_naming_it = crates_still_naming_the_module(workspace, &moving, &rewrites)?;
    refuse_a_dependency_cycle(&moving, &header, &keeps_naming_it)?;

    let mut changes = vec![FileEdit::Rename {
        from: moving.source.clone(),
        to: moving.moved_to(),
    }];

    changes.push(FileEdit::Change {
        path: moving.source.clone(),
        edits: header.edits,
    });
    changes.push(moving.left_behind(workspace, &survey)?);
    changes.push(moving.declared_in_destination(workspace)?);

    if moving.reexport == Reexport::None {
        changes.extend(caller_changes(workspace, rewrites)?);
    }

    changes.push(moving.destination_manifest(workspace, &header.crates_named)?);
    changes.extend(moving.dependents_on_the_destination(workspace, &keeps_naming_it)?);
    changes.extend(moving.workspace_members(workspace)?);

    // A change with no edits names a file the operation did not touch, and the journal would hash
    // it as one it did.
    changes.retain(|change| !matches!(change, FileEdit::Change { edits, .. } if edits.is_empty()));

    Ok(WorkspaceEdit { changes })
}

/// The survey, plus the exact spans each caller rewrite replaces.
///
/// One pass answers both questions, because they are the same question: a caller is only in the
/// survey because a path in it names the module, and that path *is* the span to replace.
fn planned(
    engine: &mut dyn ModuleReferences,
    workspace: &Workspace<'_>,
    op: &RefactorOp,
) -> Result<(Move, Survey, Vec<PlannedRewrite>)> {
    let moving = Move::read(workspace, op)?;

    let mut reached = Vec::new();
    let mut callers = Vec::new();
    let mut rewrites = Vec::new();

    for item in engine.outside_references(workspace, &moving.source)? {
        if item.referenced_at.is_empty() {
            continue;
        }
        reached.push(item.item.clone());

        for reference in item.referenced_at {
            let text = workspace.read(&reference.path)?;
            let written = written_path_at(&text, reference.at)?;

            // A reference reached through a name the file bound earlier writes no path to rewrite:
            // its own `use` declaration is a reference too, and re-pointing that one is what moves
            // the binding. Rewriting the bare name here would rewrite an identifier, not a path.
            let Some(to) = repointed(&written.text, &moving) else {
                continue;
            };

            callers.push(CallerRewrite {
                path: reference.path.clone(),
                from: written.text.clone(),
                to: to.clone(),
            });
            rewrites.push(PlannedRewrite {
                path: reference.path,
                span: written.span,
                to,
            });
        }
    }

    reached.sort();
    reached.dedup();

    let survey = Survey {
        source: moving.source.clone(),
        destination: moving.destination.clone(),
        reached_from_outside: reached,
        callers,
    };

    Ok((moving, survey, rewrites))
}

/// One path a caller writes, and the span of it to replace.
struct PlannedRewrite {
    path: String,
    span: std::ops::Range<usize>,
    to: String,
}

/// Everything the two entry points read out of the plan and the two manifests.
struct Move {
    /// The module file, relative to the repository root.
    source: String,
    /// The identifier the crate root declares — `host_registry`.
    module: String,
    /// The crate the module is leaving, read from its own manifest for the same reason the
    /// destination is: a caller's `use` path needs the declared name, not the directory's.
    origin: Destination,
    /// The crate it is arriving in.
    destination: Destination,
    /// What to leave behind in the crate it left.
    reexport: Reexport,
}

impl Move {
    fn read(workspace: &Workspace<'_>, op: &RefactorOp) -> Result<Move> {
        let source = op.anchor.file().to_string();
        let module = module_name(&source)?;
        let origin_dir = source_crate_of(&source, &module)?;
        let destination = op.to.as_deref().ok_or_else(|| {
            malformed(
                "`move_module_to_crate` needs `to`: the destination crate's directory, relative to \
                 the repository root",
            )
        })?;

        Ok(Move {
            origin: Destination::read(workspace.root, &origin_dir)?,
            destination: Destination::read(workspace.root, destination)?,
            reexport: op.reexport.unwrap_or(Reexport::None),
            source,
            module,
        })
    }

    /// Where the module file lands.
    fn moved_to(&self) -> String {
        format!("{}/src/{}.rs", self.destination.dir, self.module)
    }

    /// The crate root that declares the module today.
    fn origin_root(&self) -> String {
        format!("{}/src/lib.rs", self.origin.dir)
    }

    /// The crate root that has to declare it afterwards.
    fn destination_root(&self) -> String {
        format!("{}/src/lib.rs", self.destination.dir)
    }

    /// The crate root the module left: its `mod` declaration replaced by the facade, or removed.
    fn left_behind(&self, workspace: &Workspace<'_>, survey: &Survey) -> Result<FileEdit> {
        let path = self.origin_root();
        let text = workspace.read(&path)?;
        let span = module_declaration(&text, &self.module).ok_or_else(|| {
            malformed(format!(
                "{path} declares no `mod {}` — a module this crate root does not declare is not \
                 this crate's to move",
                self.module
            ))
        })?;

        let facade = facade_line(
            &self.destination,
            self.reexport,
            &survey.reached_from_outside,
        );
        let line = match facade {
            // The declaration's line goes entirely, newline included, when nothing replaces it.
            None => String::new(),
            Some(facade) => format!("{facade}\n"),
        };

        Ok(FileEdit::Change {
            path,
            edits: vec![replacement(&text, span, &line)],
        })
    }

    /// The destination crate root, declaring the module it is about to receive.
    ///
    /// `pub mod`, not `mod`: the module keeps its name and its callers keep writing it, which only
    /// resolves from another crate if the module is public. That is also what a glob facade needs
    /// to re-export.
    fn declared_in_destination(&self, workspace: &Workspace<'_>) -> Result<FileEdit> {
        let path = self.destination_root();
        let text = workspace.read(&path)?;
        let declaration = format!("pub mod {};\n", self.module);
        let at = after_last_module_declaration(&text);

        Ok(FileEdit::Change {
            path,
            edits: vec![replacement(&text, at..at, &declaration)],
        })
    }

    /// The destination's manifest, gaining every crate the moved code names.
    ///
    /// Each dependency is copied from the manifest that already declares it rather than written
    /// here: a version this operation invented would be a fact about the world it has no way to
    /// know. The one it does author is the path back to the crate the module left, which is a fact
    /// about this repository's own layout.
    fn destination_manifest(
        &self,
        workspace: &Workspace<'_>,
        named: &BTreeSet<String>,
    ) -> Result<FileEdit> {
        let path = format!("{}/Cargo.toml", self.destination.dir);
        let text = workspace.read(&path)?;
        let origin = workspace.read(&format!("{}/Cargo.toml", self.origin.dir))?;

        let mut lines = Vec::new();
        for extern_name in named {
            if declares_dependency(&text, extern_name) {
                continue;
            }
            if *extern_name == self.origin.extern_name {
                lines.push(format!(
                    "{} = {{ path = \"{}\" }}",
                    self.origin.package,
                    relative_from(&self.destination.dir, &self.origin.dir)
                ));
                continue;
            }
            let declared = dependency_line(&origin, extern_name).ok_or_else(|| {
                malformed(format!(
                    "the moved module names `{extern_name}`, which {}/Cargo.toml does not declare \
                     — there is nothing to carry across",
                    self.origin.dir
                ))
            })?;
            lines.push(re_anchored(
                &declared,
                &self.origin.dir,
                &self.destination.dir,
            ));
        }

        Ok(FileEdit::Change {
            path,
            edits: with_dependencies(&text, &lines),
        })
    }

    /// Every manifest that has to gain a dependency on the destination.
    ///
    /// Without this a move produces a tree that reads correctly and does not build: the facade, or
    /// the caller this operation re-pointed, names a crate the manifest never heard of. It is the
    /// one edit no unit test caught and the first `cargo check` did.
    fn dependents_on_the_destination(
        &self,
        workspace: &Workspace<'_>,
        crates: &BTreeSet<String>,
    ) -> Result<Vec<FileEdit>> {
        let mut changes = Vec::new();
        for directory in crates {
            let path = format!("{directory}/Cargo.toml");
            let text = workspace.read(&path)?;
            if declares_dependency(&text, &self.destination.extern_name) {
                continue;
            }

            let line = format!(
                "{} = {{ path = \"{}\" }}",
                self.destination.package,
                relative_from(directory, &self.destination.dir)
            );
            changes.push(FileEdit::Change {
                path,
                edits: with_dependencies(&text, &[line]),
            });
        }
        Ok(changes)
    }

    /// The workspace root's `members`, gaining the destination when it is not already listed.
    ///
    /// Nothing is emitted when the root manifest declares no `members` array: there is no list for
    /// the crate to be missing from, and inventing one would be authoring a workspace rather than
    /// moving a module.
    fn workspace_members(&self, workspace: &Workspace<'_>) -> Result<Vec<FileEdit>> {
        let path = "Cargo.toml".to_string();
        if !workspace.root.join(&path).exists() {
            return Ok(Vec::new());
        }

        let text = workspace.read(&path)?;
        let Some(members) = members_list(&text) else {
            return Ok(Vec::new());
        };
        if text[members.clone()].contains(&format!("\"{}\"", self.destination.dir)) {
            return Ok(Vec::new());
        }

        let entry = format!("    \"{}\",\n", self.destination.dir);
        Ok(vec![FileEdit::Change {
            path,
            edits: vec![replacement(&text, members.end..members.end, &entry)],
        }])
    }
}

/// One `FileEdit::Change` per caller, carrying every path in that file at once.
fn caller_changes(
    workspace: &Workspace<'_>,
    rewrites: Vec<PlannedRewrite>,
) -> Result<Vec<FileEdit>> {
    let paths: BTreeSet<String> = rewrites
        .iter()
        .map(|rewrite| rewrite.path.clone())
        .collect();

    let mut changes = Vec::new();
    for path in paths {
        let text = workspace.read(&path)?;
        let edits = rewrites
            .iter()
            .filter(|rewrite| rewrite.path == path)
            .map(|rewrite| replacement(&text, rewrite.span.clone(), &rewrite.to))
            .collect();
        changes.push(FileEdit::Change { path, edits });
    }
    Ok(changes)
}

/// A move that would make the two crates depend on each other is refused rather than written.
///
/// The crate the module left goes on naming it either way — through a facade, or through the
/// callers this operation re-points — so it gains a dependency on the destination. If the moved
/// code still names the crate it left, the destination depends on it back, and cargo refuses that
/// pair with an error naming neither the module nor the operation that produced it. Refusing here
/// names both, and names every path that forced it.
fn refuse_a_dependency_cycle(
    moving: &Move,
    header: &Header,
    keeps_naming_it: &BTreeSet<String>,
) -> Result<()> {
    if !header.crates_named.contains(&moving.origin.extern_name)
        || !keeps_naming_it.contains(&moving.origin.dir)
    {
        return Ok(());
    }

    Err(malformed(format!(
        "`{}` still names `{}` ({}), so the destination would depend on the crate it left while \
         that crate goes on naming the module it lost — move what those paths reach, or move the \
         module's own dependencies with it",
        moving.source,
        moving.origin.package,
        header.origin_paths.join(", ")
    )))
}

/// Every crate directory that will name the destination once the move has been applied.
///
/// A facade keeps the crate the module left naming it. A re-pointed caller does the same from
/// whichever crate it sits in, which need not be that one — a workspace-wide move re-points callers
/// in crates the plan never mentioned, and each of them needs the dependency or stops compiling.
fn crates_still_naming_the_module(
    workspace: &Workspace<'_>,
    moving: &Move,
    rewrites: &[PlannedRewrite],
) -> Result<BTreeSet<String>> {
    let mut crates = BTreeSet::new();
    if moving.reexport != Reexport::None {
        crates.insert(moving.origin.dir.clone());
        return Ok(crates);
    }

    for rewrite in rewrites {
        crates.insert(crate_holding(workspace, &rewrite.path)?);
    }
    Ok(crates)
}

/// The crate directory a file belongs to — the nearest ancestor with a `Cargo.toml`.
fn crate_holding(workspace: &Workspace<'_>, file: &str) -> Result<String> {
    let mut directory = Path::new(file).parent();
    while let Some(candidate) = directory {
        if workspace.root.join(candidate).join("Cargo.toml").exists() {
            return Ok(candidate.display().to_string());
        }
        directory = candidate.parent();
    }

    Err(malformed(format!(
        "`{file}` is in no crate — no `Cargo.toml` stands above it, so there is no manifest to \
         give the dependency its re-pointed path needs"
    )))
}

/// What the moved file's `use` header says about the crates it needs.
struct Header {
    /// The edits that re-point it, empty when it names nothing that moved.
    edits: Vec<TextEdit>,
    /// Every crate the header names once re-pointed, by extern name.
    crates_named: BTreeSet<String>,
    /// The paths that now name the crate the module left, for a refusal that has to list them.
    origin_paths: Vec<String>,
}

/// The moved file's own `use` header, re-pointed at the crate it left.
///
/// Inside the module, `crate::` and a top-level `super::` both named the crate it is leaving; in the
/// destination they would name the destination. Only the qualifier is rewritten, and only in a `use`
/// declaration — see this module's own documentation for why that is the whole of the header pass.
fn repointed_header(text: &str, origin_extern: &str) -> Header {
    let mut header = Header {
        edits: Vec::new(),
        crates_named: BTreeSet::new(),
        origin_paths: Vec::new(),
    };

    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();

        let Some((at, path)) = use_path(line) else {
            continue;
        };
        let (qualifier, rest) = match path.split_once("::") {
            Some(split) => split,
            None => (path, ""),
        };

        if matches!(qualifier, "crate" | "super") {
            let at = start + at;
            header
                .edits
                .push(replacement(text, at..at + qualifier.len(), origin_extern));
            header.crates_named.insert(origin_extern.to_string());
            header.origin_paths.push(format!("{origin_extern}::{rest}"));
            continue;
        }
        if !matches!(qualifier, "self" | "std" | "core" | "alloc") {
            header.crates_named.insert(qualifier.to_string());
        }
    }

    header
}

/// The path a top-level `use` declaration names, and where on the line it starts.
///
/// An indented `use` belongs to a nested module or a function body; only the file's own header is
/// this operation's to rewrite.
fn use_path(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_end();
    if trimmed.starts_with(char::is_whitespace) {
        return None;
    }
    for keyword in ["pub use ", "use "] {
        if let Some(path) = trimmed.strip_prefix(keyword) {
            return Some((keyword.len(), path.trim_end_matches(';')));
        }
    }
    None
}

/// The path this caller writes, ending at the identifier the server reported.
struct WrittenPath {
    text: String,
    span: std::ops::Range<usize>,
}

/// Read the whole `a::b::C` a reference sits at the end of.
///
/// The server reports where the item's own name is; what has to be replaced is everything leading
/// to it, which is only readable from the caller's text.
fn written_path_at(text: &str, at: Position) -> Result<WrittenPath> {
    let start = byte_offset(text, at.line, at.col)?;
    let end = start
        + text[start..]
            .find(|character: char| !is_path_character(character))
            .unwrap_or(text.len() - start);

    let mut first = start;
    while let Some(head) = text[..first].strip_suffix("::") {
        let segment = head.trim_end_matches(is_path_character);
        if segment.len() == head.len() {
            break;
        }
        first = segment.len();
    }

    Ok(WrittenPath {
        text: text[first..end].to_string(),
        span: first..end,
    })
}

fn is_path_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// The same path, written from outside the crate the module is leaving.
///
/// `None` when the path never names the module: the caller reaches the item through a name bound
/// somewhere else in its own file, and that binding is a reference of its own.
fn repointed(written: &str, moving: &Move) -> Option<String> {
    let segments: Vec<&str> = written.split("::").collect();
    let at = segments
        .iter()
        .position(|segment| *segment == moving.module)?;
    Some(format!(
        "{}::{}",
        moving.destination.extern_name,
        segments[at..].join("::")
    ))
}

/// The module file's own name — the identifier `mod` declares.
fn module_name(source: &str) -> Result<String> {
    let stem = Path::new(source)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| malformed(format!("`{source}` is not a Rust module file")))?;

    if matches!(stem, "lib" | "main" | "mod") {
        return Err(malformed(format!(
            "`{source}` is a crate or module root, not a module — moving one moves everything it \
             declares, which is a plan of its own"
        )));
    }
    Ok(stem.to_string())
}

/// The crate directory a module file belongs to.
///
/// Only a module the crate root declares can move: a nested module's `mod` line lives in another
/// module's file, whose own path this operation would have to guess at. Refused rather than guessed.
fn source_crate_of(source: &str, module: &str) -> Result<String> {
    source
        .strip_suffix(&format!("/src/{module}.rs"))
        .map(str::to_string)
        .ok_or_else(|| {
            malformed(format!(
                "`{source}` is not `<crate>/src/{module}.rs` — `move_module_to_crate` moves a \
                 module the crate root itself declares"
            ))
        })
}

/// The span of the `mod <module>;` line in a crate root, newline included.
fn module_declaration(text: &str, module: &str) -> Option<std::ops::Range<usize>> {
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();

        let declaration = line.trim();
        let declaration = declaration.strip_prefix("pub ").unwrap_or(declaration);
        if declaration == format!("mod {module};") {
            return Some(start..offset);
        }
    }
    None
}

/// Where a new `mod` declaration goes in a crate root: after the last one already there, and after
/// the file's own header when it declares none.
///
/// Placed rather than sorted in, because a crate root's `mod` order is the author's and nothing
/// here knows what it means.
fn after_last_module_declaration(text: &str) -> usize {
    let mut offset = 0usize;
    let mut header_ends = 0usize;
    let mut in_header = true;
    let mut last_declaration = None;

    for line in text.split_inclusive('\n') {
        offset += line.len();
        let trimmed = line.trim();

        if in_header
            && (trimmed.is_empty() || trimmed.starts_with("//!") || trimmed.starts_with("#!"))
        {
            header_ends = offset;
        } else {
            in_header = false;
        }

        let declaration = trimmed.strip_prefix("pub ").unwrap_or(trimmed);
        if declaration.starts_with("mod ") && declaration.ends_with(';') {
            last_declaration = Some(offset);
        }
    }

    last_declaration.unwrap_or(header_ends)
}

/// Whether a manifest already declares a dependency on the crate with this extern name.
fn declares_dependency(manifest: &str, extern_name: &str) -> bool {
    dependency_line(manifest, extern_name).is_some()
}

/// The line declaring one dependency, verbatim, out of a manifest that has it.
fn dependency_line(manifest: &str, extern_name: &str) -> Option<String> {
    dependencies_of(manifest)
        .into_iter()
        .find(|line| {
            line.split('=')
                .next()
                .map(|key| key.trim().replace('-', "_") == extern_name)
                .unwrap_or_default()
        })
        .map(str::to_string)
}

/// The lines of a manifest's `[dependencies]` table.
fn dependencies_of(manifest: &str) -> Vec<&str> {
    manifest
        .lines()
        .map(str::trim_end)
        .skip_while(|line| line.trim() != "[dependencies]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter(|line| line.contains('='))
        .collect()
}

/// The manifest with `lines` added to its `[dependencies]` table.
fn with_dependencies(manifest: &str, lines: &[String]) -> Vec<TextEdit> {
    if lines.is_empty() {
        return Vec::new();
    }

    let added = lines.join("\n") + "\n";
    match end_of_dependencies(manifest) {
        Some(at) => vec![replacement(manifest, at..at, &added)],
        // A manifest with no `[dependencies]` gains the table the moved code needs, at the end
        // where a table cannot land inside another one.
        None => vec![replacement(
            manifest,
            manifest.len()..manifest.len(),
            &format!("\n[dependencies]\n{added}"),
        )],
    }
}

/// Where a manifest's `[dependencies]` table ends, or `None` when it declares none.
fn end_of_dependencies(manifest: &str) -> Option<usize> {
    let mut offset = 0usize;
    let mut found = None;
    let mut inside = false;

    for line in manifest.split_inclusive('\n') {
        offset += line.len();

        let trimmed = line.trim();
        if trimmed == "[dependencies]" {
            inside = true;
            found = Some(offset);
            continue;
        }
        if inside {
            if trimmed.starts_with('[') {
                return found;
            }
            if !trimmed.is_empty() {
                found = Some(offset);
            }
        }
    }
    found
}

/// The span of a workspace manifest's `members` array, between its brackets.
fn members_list(manifest: &str) -> Option<std::ops::Range<usize>> {
    let opened = manifest.find("members")?;
    let start = manifest[opened..].find('[')? + opened + 1;
    let end = manifest[start..].find(']')? + start;
    Some(start..end)
}

/// A copied dependency line, with a relative `path` re-anchored on the crate receiving it.
///
/// `path = "../tddy-lsp"` means different crates read from different directories. Copying the line
/// verbatim is how a manifest ends up pointing at a crate that is not the one it was copied from —
/// or at nothing, which at least fails loudly.
fn re_anchored(declared: &str, from: &str, to: &str) -> String {
    let Some((head, rest)) = declared.split_once("path = \"") else {
        return declared.to_string();
    };
    let Some((path, tail)) = rest.split_once('"') else {
        return declared.to_string();
    };
    if path.starts_with('/') {
        return declared.to_string();
    }

    let target = normalized(&format!("{from}/{path}"));
    format!("{head}path = \"{}\"{tail}", relative_from(to, &target))
}

/// A slash-separated path with its `.` and `..` components resolved.
fn normalized(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}

/// The path from one directory to another, both relative to the repository root.
fn relative_from(from: &str, to: &str) -> String {
    let from: Vec<&str> = from.split('/').filter(|part| !part.is_empty()).collect();
    let to: Vec<&str> = to.split('/').filter(|part| !part.is_empty()).collect();
    let shared = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();

    let up = std::iter::repeat_n("..", from.len() - shared);
    up.chain(to[shared..].iter().copied())
        .collect::<Vec<_>>()
        .join("/")
}

/// A replacement of one byte span, in the coordinates [`crate::apply`] reads edits back in.
fn replacement(text: &str, span: std::ops::Range<usize>, new_text: &str) -> TextEdit {
    TextEdit {
        range: Range {
            start: position_of(text, span.start),
            end: position_of(text, span.end),
        },
        new_text: new_text.to_string(),
    }
}

/// The one-based line/column a byte offset sits at.
fn position_of(text: &str, offset: usize) -> Position {
    let before = &text[..offset];
    Position {
        line: before.matches('\n').count() as u32 + 1,
        col: before
            .rsplit('\n')
            .next()
            .map_or(0, |line| line.chars().count()) as u32
            + 1,
    }
}

/// The `pub use` line a facade leaves in the crate the module left.
///
/// [`Reexport::Glob`] is one line and legal whatever moved, for the same reason it is inside a
/// parent module: a glob re-export caps at each item's own visibility rather than failing on a
/// member less visible than itself. [`Reexport::Named`] names only the items something outside
/// reaches, which the survey already knows. [`Reexport::None`] leaves nothing, and then every caller
/// in the survey is rewritten instead.
pub fn facade_line(
    destination: &Destination,
    reexport: Reexport,
    reached: &[String],
) -> Option<String> {
    let crate_name = &destination.extern_name;
    match reexport {
        Reexport::Glob => Some(format!("pub use {crate_name}::*;")),
        // A group is ordered and de-duplicated so the same survey always writes the same line: the
        // reference set arrives in whatever order the server listed it, and a facade that reordered
        // itself between runs would show up as a diff nobody asked for.
        Reexport::Named => {
            let named: BTreeSet<&str> = reached.iter().map(String::as_str).collect();
            if named.is_empty() {
                return None;
            }
            Some(format!(
                "pub use {crate_name}::{{{}}};",
                named.into_iter().collect::<Vec<_>>().join(", ")
            ))
        }
        Reexport::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::WorkspaceEdit;
    use crate::plan::{Anchor, RefactorKind};

    const ROOT_MANIFEST: &str = "Cargo.toml";
    const ORIGIN_MANIFEST: &str = "packages/tddy-daemon/Cargo.toml";
    const ORIGIN_ROOT: &str = "packages/tddy-daemon/src/lib.rs";
    const MODULE: &str = "packages/tddy-daemon/src/host_registry.rs";
    const CALLER: &str = "packages/tddy-daemon/src/runtime.rs";
    const DESTINATION_MANIFEST: &str = "packages/tddy-host-service/Cargo.toml";
    const DESTINATION_ROOT: &str = "packages/tddy-host-service/src/lib.rs";
    const MOVED_TO: &str = "packages/tddy-host-service/src/host_registry.rs";

    fn a_move_of(module: &str, to: &str) -> RefactorOp {
        RefactorOp {
            op: RefactorKind::MoveModuleToCrate,
            anchor: Anchor::Symbol {
                file: format!("packages/tddy-daemon/src/{module}.rs"),
                path: module.to_string(),
            },
            name: None,
            to: Some(to.to_string()),
            variant: None,
            with_private_deps: false,
            reexport: None,
            to_file: false,
        }
    }

    fn a_destination_named(package: &str) -> Destination {
        Destination {
            dir: format!("packages/{package}"),
            package: package.to_string(),
            extern_name: package.replace('-', "_"),
        }
    }

    /// A repository holding the crate a module is leaving, the crate it is going to, and a caller.
    ///
    /// Every file is real because everything this operation decides, it reads: the two declared
    /// package names, the `mod` line it replaces, the `use` header it re-points, the caller's own
    /// text and both manifests. A fixture that left any of them out would be testing a different
    /// operation.
    struct AWorkspace {
        root: tempfile::TempDir,
        overlay: crate::Overlay,
    }

    fn a_workspace_with_two_crates() -> AWorkspace {
        AWorkspace {
            root: tempfile::tempdir().unwrap(),
            overlay: crate::Overlay::default(),
        }
        .with(
            ROOT_MANIFEST,
            "[workspace]\nmembers = [\n    \"packages/tddy-daemon\",\n]\n",
        )
        .with(
            ORIGIN_MANIFEST,
            "[package]\nname = \"tddy-daemon\"\n\n[dependencies]\ntddy-lsp = { path = \"../tddy-lsp\" }\n",
        )
        .with(ORIGIN_ROOT, "//! The daemon.\n\nmod host_registry;\nmod runtime;\n")
        .with(
            MODULE,
            "use tddy_lsp::Client;\n\npub struct HostRegistry {\n    client: Client,\n}\n",
        )
        .with(
            CALLER,
            "use crate::host_registry::HostRegistry;\n\npub fn boot(registry: &HostRegistry) {}\n",
        )
        .with(
            DESTINATION_MANIFEST,
            "[package]\nname = \"tddy-host-service\"\n\n[dependencies]\nserde = \"1\"\n",
        )
        .with(DESTINATION_ROOT, "//! The host service.\n\n")
    }

    impl AWorkspace {
        fn with(self, path: &str, text: &str) -> Self {
            let absolute = self.root.path().join(path);
            std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
            std::fs::write(absolute, text).unwrap();
            self
        }

        fn read(&self, path: &str) -> String {
            std::fs::read_to_string(self.root.path().join(path)).unwrap()
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
    /// A fake rather than a mock: it answers the one question the engine answers, and the deciding
    /// half under test cannot tell it from the Rust backend's own implementation.
    struct AKnownReferenceSet {
        items: Vec<ItemReferences>,
    }

    fn a_reference_set(items: Vec<ItemReferences>) -> AKnownReferenceSet {
        AKnownReferenceSet { items }
    }

    impl ModuleReferences for AKnownReferenceSet {
        fn outside_references(
            &mut self,
            _workspace: &Workspace<'_>,
            _file: &str,
        ) -> Result<Vec<ItemReferences>> {
            Ok(self.items.clone())
        }
    }

    /// Every place `file` names `item`, as the server would report them — the import that binds the
    /// name and each use of it.
    fn references_to(item: &str, file: &str, workspace: &AWorkspace) -> ItemReferences {
        let text = workspace.read(file);
        ItemReferences {
            item: item.to_string(),
            referenced_at: text
                .match_indices(item)
                .map(|(offset, _)| Reference {
                    path: file.to_string(),
                    at: position_of(&text, offset),
                })
                .collect(),
        }
    }

    /// What a file contains once the move's edits for it are applied.
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
            .unwrap_or_else(|| panic!("the move changed nothing in {path}"));

        crate::apply::edited(workspace.read(path), &edits).expect("the edits apply")
    }

    /// Every file the move changes, in the order it reports them.
    fn changed(edit: &WorkspaceEdit) -> Vec<&str> {
        edit.changes
            .iter()
            .filter_map(|change| match change {
                FileEdit::Change { path, .. } => Some(path.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Every file the move renames, as `from` → `to`.
    fn renames(edit: &WorkspaceEdit) -> Vec<(&str, &str)> {
        edit.changes
            .iter()
            .filter_map(|change| match change {
                FileEdit::Rename { from, to } => Some((from.as_str(), to.as_str())),
                _ => None,
            })
            .collect()
    }

    /// A directory name is not a crate name: `packages/tddy-host-service` may declare any
    /// `[package] name`, and a caller's `use` path needs the declared one. Reading the manifest is
    /// the only correct source, so a destination with no manifest is a plan defect rather than a
    /// crate to scaffold.
    #[test]
    fn refuses_a_destination_with_no_manifest() {
        // Given
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("packages/tddy-host-service")).unwrap();

        // When
        let outcome = Destination::read(root.path(), "packages/tddy-host-service");

        // Then
        assert!(
            outcome.is_err(),
            "a directory with no Cargo.toml is not a crate"
        );
    }

    /// The `use` path a caller needs is the declared package name with hyphens turned into
    /// underscores — not the directory name, and not the package name verbatim.
    #[test]
    fn reads_the_extern_name_from_the_declared_package_name() {
        // Given
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("packages/host-svc");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"tddy-host-service\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();

        // When
        let destination = Destination::read(root.path(), "packages/host-svc").unwrap();

        // Then
        assert_eq!(destination.package, "tddy-host-service");
        assert_eq!(destination.extern_name, "tddy_host_service");
    }

    /// A glob facade is one line and legal whatever moved, for the same reason it is inside a parent
    /// module: it caps at each item's own visibility rather than failing on a member less visible
    /// than itself.
    #[test]
    fn writes_a_glob_facade_naming_only_the_crate() {
        // Given
        let destination = a_destination_named("tddy-host-service");

        // When
        let line = facade_line(&destination, Reexport::Glob, &["HostRegistry".to_string()]);

        // Then
        assert_eq!(line.as_deref(), Some("pub use tddy_host_service::*;"));
    }

    /// A named facade re-exports only what something outside actually reaches, which the survey
    /// already knows — naming an item nothing reaches would force it public for no caller.
    #[test]
    fn writes_a_named_facade_covering_only_what_is_reached() {
        // Given
        let destination = a_destination_named("tddy-host-service");
        let reached = vec!["HostRegistry".to_string(), "FileHostRegistry".to_string()];

        // When
        let line = facade_line(&destination, Reexport::Named, &reached);

        // Then
        assert_eq!(
            line.as_deref(),
            Some("pub use tddy_host_service::{FileHostRegistry, HostRegistry};")
        );
    }

    /// Without a facade there is nothing to leave behind, and every caller is rewritten instead.
    #[test]
    fn writes_no_facade_when_none_was_asked_for() {
        // Given
        let destination = a_destination_named("tddy-host-service");

        // When
        let line = facade_line(&destination, Reexport::None, &["HostRegistry".to_string()]);

        // Then
        assert_eq!(line, None);
    }

    /// The survey is the blast radius, and it is worth having before paying for a cold index: a plan
    /// author wants to know which callers a move touches while the plan is still editable.
    ///
    /// The caller names the item twice — once in the `use` that binds it and once where it is used —
    /// and only the path is a rewrite. Re-pointing the import is what moves the bare name with it.
    #[test]
    fn surveys_the_callers_a_move_would_rewrite() {
        // Given
        let workspace = a_workspace_with_two_crates();
        let mut engine = a_reference_set(vec![references_to("HostRegistry", CALLER, &workspace)]);
        let op = a_move_of("host_registry", "packages/tddy-host-service");

        // When
        let outcome = survey(&mut engine, &workspace.workspace(), &op);

        // Then
        let found = outcome.expect("a survey reports what it found");
        assert_eq!(found.source, MODULE);
        assert_eq!(found.destination.package, "tddy-host-service");
        assert_eq!(found.reached_from_outside, vec!["HostRegistry"]);
        assert_eq!(
            found.callers,
            vec![CallerRewrite {
                path: CALLER.to_string(),
                from: "crate::host_registry::HostRegistry".to_string(),
                to: "tddy_host_service::host_registry::HostRegistry".to_string(),
            }]
        );
    }

    /// With a facade the caller list is empty by construction — that is the difference between a
    /// move a reviewer can read and one that touches ninety files.
    #[test]
    fn resolves_a_faceded_move_without_touching_a_single_caller() {
        // Given
        let workspace = a_workspace_with_two_crates();
        let mut engine = a_reference_set(vec![references_to("HostRegistry", CALLER, &workspace)]);
        let mut op = a_move_of("host_registry", "packages/tddy-host-service");
        op.reexport = Some(Reexport::Glob);

        // When
        let outcome = resolve(&mut engine, &workspace.workspace(), &op);

        // Then
        let edit = outcome.expect("a faceded move resolves");
        assert_eq!(
            renames(&edit),
            vec![(MODULE, MOVED_TO)],
            "the module's file is moved with git mv"
        );
        assert_eq!(
            changed(&edit),
            vec![
                ORIGIN_ROOT,
                DESTINATION_ROOT,
                DESTINATION_MANIFEST,
                ORIGIN_MANIFEST,
                ROOT_MANIFEST
            ],
            "a faceded move re-points no caller"
        );
    }

    /// Without a facade the callers are the move: each path that named the module through the crate
    /// it left names the crate it arrived in instead, and the name bound by the import moves with it
    /// untouched.
    #[test]
    fn re_points_every_caller_when_no_facade_was_asked_for() {
        // Given
        let workspace = a_workspace_with_two_crates();
        let mut engine = a_reference_set(vec![references_to("HostRegistry", CALLER, &workspace)]);
        let op = a_move_of("host_registry", "packages/tddy-host-service");

        // When
        let edit = resolve(&mut engine, &workspace.workspace(), &op).expect("the move resolves");

        // Then
        assert_eq!(
            applied(&edit, CALLER, &workspace),
            "use tddy_host_service::host_registry::HostRegistry;\n\n\
             pub fn boot(registry: &HostRegistry) {}\n"
        );
    }

    /// A glob facade takes the place of the `mod` line, so every path that reached the module
    /// through the crate root still resolves and no caller is rewritten at all.
    #[test]
    fn leaves_a_glob_facade_where_the_module_was_declared() {
        // Given
        let workspace = a_workspace_with_two_crates();
        let mut engine = a_reference_set(vec![references_to("HostRegistry", CALLER, &workspace)]);
        let mut op = a_move_of("host_registry", "packages/tddy-host-service");
        op.reexport = Some(Reexport::Glob);

        // When
        let edit = resolve(&mut engine, &workspace.workspace(), &op).expect("the move resolves");

        // Then
        assert_eq!(
            applied(&edit, ORIGIN_ROOT, &workspace),
            "//! The daemon.\n\npub use tddy_host_service::*;\nmod runtime;\n"
        );
    }

    /// Nothing is left behind without a facade: the declaration goes, and the callers carry the
    /// move instead.
    #[test]
    fn takes_the_module_declaration_out_of_the_crate_it_left() {
        // Given
        let workspace = a_workspace_with_two_crates();
        let mut engine = a_reference_set(vec![references_to("HostRegistry", CALLER, &workspace)]);
        let op = a_move_of("host_registry", "packages/tddy-host-service");

        // When
        let edit = resolve(&mut engine, &workspace.workspace(), &op).expect("the move resolves");

        // Then
        assert_eq!(
            applied(&edit, ORIGIN_ROOT, &workspace),
            "//! The daemon.\n\nmod runtime;\n"
        );
    }

    /// `pub mod`, because the module keeps its name and its callers keep writing it — which only
    /// resolves from another crate if the module is public, and is also what a glob facade needs to
    /// have something to re-export.
    #[test]
    fn declares_the_module_in_the_crate_it_arrives_in() {
        // Given
        let workspace = a_workspace_with_two_crates();
        let mut engine = a_reference_set(vec![references_to("HostRegistry", CALLER, &workspace)]);
        let op = a_move_of("host_registry", "packages/tddy-host-service");

        // When
        let edit = resolve(&mut engine, &workspace.workspace(), &op).expect("the move resolves");

        // Then
        assert_eq!(
            applied(&edit, DESTINATION_ROOT, &workspace),
            "//! The host service.\n\npub mod host_registry;\n"
        );
    }

    /// The moved code's `crate::` paths named the crate it left; in the destination they would name
    /// the destination. Only the qualifier changes, and only in the file's own `use` header.
    #[test]
    fn re_points_the_moved_header_at_the_crate_the_module_left() {
        // Given
        let workspace = a_workspace_with_two_crates().with(
            MODULE,
            "use crate::runtime::Clock;\n\npub struct HostRegistry {\n    clock: Clock,\n}\n",
        );
        let mut engine = a_reference_set(Vec::new());
        let op = a_move_of("host_registry", "packages/tddy-host-service");

        // When
        let edit = resolve(&mut engine, &workspace.workspace(), &op).expect("the move resolves");

        // Then
        assert_eq!(
            applied(&edit, MODULE, &workspace),
            "use tddy_daemon::runtime::Clock;\n\npub struct HostRegistry {\n    clock: Clock,\n}\n"
        );
    }

    /// A dependency travels with the code that names it, copied from the manifest that already
    /// declares it — a version this operation invented would be a fact about the world it has no way
    /// to know.
    #[test]
    fn carries_the_dependencies_the_moved_code_names_into_the_destination() {
        // Given
        let workspace = a_workspace_with_two_crates();
        let mut engine = a_reference_set(Vec::new());
        let op = a_move_of("host_registry", "packages/tddy-host-service");

        // When
        let edit = resolve(&mut engine, &workspace.workspace(), &op).expect("the move resolves");

        // Then
        assert_eq!(
            applied(&edit, DESTINATION_MANIFEST, &workspace),
            "[package]\nname = \"tddy-host-service\"\n\n[dependencies]\nserde = \"1\"\n\
             tddy-lsp = { path = \"../tddy-lsp\" }\n"
        );
    }

    /// A crate the workspace does not list is a crate cargo does not build, so a move into one that
    /// is new to the members list adds it.
    #[test]
    fn adds_the_destination_crate_to_the_workspace_members() {
        // Given
        let workspace = a_workspace_with_two_crates();
        let mut engine = a_reference_set(Vec::new());
        let op = a_move_of("host_registry", "packages/tddy-host-service");

        // When
        let edit = resolve(&mut engine, &workspace.workspace(), &op).expect("the move resolves");

        // Then
        assert_eq!(
            applied(&edit, ROOT_MANIFEST, &workspace),
            "[workspace]\nmembers = [\n    \"packages/tddy-daemon\",\n    \
             \"packages/tddy-host-service\",\n]\n"
        );
    }

    /// The facade names the crate the module moved to, so the crate holding it has to depend on
    /// that crate. A tree that reads correctly and does not build is the worst outcome this
    /// operation can produce, and this is the manifest that decides which one it is.
    #[test]
    fn makes_the_crate_keeping_the_facade_depend_on_the_one_it_moved_to() {
        // Given
        let workspace = a_workspace_with_two_crates();
        let mut engine = a_reference_set(vec![references_to("HostRegistry", CALLER, &workspace)]);
        let mut op = a_move_of("host_registry", "packages/tddy-host-service");
        op.reexport = Some(Reexport::Glob);

        // When
        let edit = resolve(&mut engine, &workspace.workspace(), &op).expect("the move resolves");

        // Then
        assert_eq!(
            applied(&edit, ORIGIN_MANIFEST, &workspace),
            "[package]\nname = \"tddy-daemon\"\n\n[dependencies]\n\
             tddy-lsp = { path = \"../tddy-lsp\" }\n\
             tddy-host-service = { path = \"../tddy-host-service\" }\n"
        );
    }

    /// The same cycle without a facade: the caller this operation re-points makes its own crate
    /// depend on the destination, and the moved code naming the crate it left points the dependency
    /// straight back.
    #[test]
    fn refuses_a_move_whose_re_pointed_caller_would_close_a_dependency_cycle() {
        // Given
        let workspace = a_workspace_with_two_crates().with(
            MODULE,
            "use crate::runtime::Clock;\n\npub struct HostRegistry {\n    clock: Clock,\n}\n",
        );
        let mut engine = a_reference_set(vec![references_to("HostRegistry", CALLER, &workspace)]);
        let op = a_move_of("host_registry", "packages/tddy-host-service");

        // When
        let outcome = resolve(&mut engine, &workspace.workspace(), &op);

        // Then
        assert_refusal(outcome).naming("tddy_daemon::runtime::Clock");
    }

    /// A facade makes the crate the module left depend on the destination. If the moved code still
    /// names the crate it left, the destination depends on it back — and cargo refuses that pair
    /// with an error naming neither the module nor the operation that produced it.
    #[test]
    fn refuses_a_faceded_move_the_moved_code_would_make_cyclic() {
        // Given
        let workspace = a_workspace_with_two_crates().with(
            MODULE,
            "use crate::runtime::Clock;\n\npub struct HostRegistry {\n    clock: Clock,\n}\n",
        );
        let mut engine = a_reference_set(Vec::new());
        let mut op = a_move_of("host_registry", "packages/tddy-host-service");
        op.reexport = Some(Reexport::Glob);

        // When
        let outcome = resolve(&mut engine, &workspace.workspace(), &op);

        // Then
        assert_refusal(outcome).naming("tddy_daemon::runtime::Clock");
    }

    /// A module the crate root does not declare is not that crate's to move, and finding out at
    /// `git mv` time would leave the tree half-moved.
    #[test]
    fn refuses_a_module_the_crate_root_does_not_declare() {
        // Given
        let workspace =
            a_workspace_with_two_crates().with(ORIGIN_ROOT, "//! The daemon.\n\nmod runtime;\n");
        let mut engine = a_reference_set(Vec::new());
        let op = a_move_of("host_registry", "packages/tddy-host-service");

        // When
        let outcome = resolve(&mut engine, &workspace.workspace(), &op);

        // Then
        assert_refusal(outcome).naming("declares no `mod host_registry`");
    }

    /// `path = "../tddy-lsp"` reads from the directory of the manifest that carries it, so a line
    /// copied into a crate one level deeper has to be re-anchored or it points somewhere else.
    #[test]
    fn re_anchors_a_path_dependency_on_the_crate_that_receives_it() {
        // Given
        let declared = "tddy-lsp = { path = \"../tddy-lsp\" }";

        // When
        let carried = re_anchored(declared, "packages/tddy-daemon", "packages/services/hosts");

        // Then
        assert_eq!(carried, "tddy-lsp = { path = \"../../tddy-lsp\" }");
    }

    /// A refusal is only useful if it names what made it refuse.
    struct ARefusal(RestructureError);

    fn assert_refusal<T: std::fmt::Debug>(outcome: Result<T>) -> ARefusal {
        match outcome {
            Err(error) => ARefusal(error),
            Ok(value) => panic!("expected a refusal but the move resolved: {value:?}"),
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
