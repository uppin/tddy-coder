//! The test modules a cross-crate move takes along from **beside** the modules it moves.
//!
//! A `#[cfg(test)] mod t;` declared in the file that declares a moved module, when that file stays
//! in the origin, is a sibling of the moved code. Its `super::` paths name that code, so once the
//! code leaves, a test module left behind no longer compiles — `#carve` 21/21 R9 moved three such
//! files by hand. This module decides which of them **follow** the move (everything they name in the
//! origin is moving, and they name at least one moving thing), writes the edits that take them
//! along, and says which stay and why (`#reshape` 13/24).
//!
//! A test declaration inside a file that moves is a directory child, and [`super::carried`] carries
//! it; this module only tells the header pass that such a file is test code throughout
//! ([`test_gated`]).

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::Path;

use crate::crate_move::header::{FileInTree, Gate};
use crate::crate_move::survey::SurveyedPath;
use crate::crate_move::{header, manifest_edits, module_files, survey};
use crate::edit::{FileEdit, Position};
use crate::plan::RefactorOp;
use crate::registry::Workspace;
use crate::RestructureError;

use super::carried::fold_carried_members;
use super::destination::Destination;
use super::module_files::MovedFile;
use super::preconditions::{home_of_anchor, moved_by_earlier_operations};
use super::source_scan::{test_declarations, TestDeclaration};
use super::{malformed, ModuleReferences, Move, MovingCluster, Result};

/// A test module that follows the move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FollowingTest {
    /// The file that declares it, which stays in the origin.
    pub(crate) declared_in: String,
    /// Its declaration there, with the attributes and doc comments that leave with it.
    pub(crate) declaration: TestDeclaration,
    /// Its module path in the origin, outermost first: the declaring file's path, then its name.
    pub(crate) module_path: Vec<String>,
    /// Every file it spans, its own first, each with where it lands in the destination.
    pub(crate) files: Vec<MovedFile>,
    /// The moved module it lands beside: the first member, in plan order, that it names.
    pub(crate) follows: String,
}

/// What the move decided about the test modules beside its members.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TestModules {
    pub(crate) following: Vec<FollowingTest>,
    /// One per follower, and one per staying test module that names something the move takes,
    /// naming the path that keeps it.
    pub(crate) notes: Vec<String>,
}

/// Sort every `#[cfg(test)] mod t;` declared beside a member into those that follow and those that
/// stay, from the text of the tree and the origin's re-exports alone, so a plain `check` reaches the
/// same verdict `apply` does.
///
/// `earlier` holds the module paths operations before this one already moved, which count as
/// moving.
///
/// # Errors
///
/// Refuses when a file the survey must read cannot be, or a path climbs above the crate root.
pub(crate) fn sorted(
    workspace: &Workspace<'_>,
    members: &[Move],
    earlier: &BTreeSet<String>,
) -> Result<TestModules> {
    let mut modules = TestModules::default();
    let Some(first) = members.first() else {
        return Ok(modules);
    };
    let origin = &first.origin;
    let co_moving: BTreeSet<String> = members
        .iter()
        .map(|member| member.home.path.join("::"))
        .chain(earlier.iter().cloned())
        .collect();

    let mut travelling_files = BTreeSet::new();
    for member in members {
        travelling_files.extend(member.carried(workspace)?.into_iter().map(|file| file.from));
    }

    // Each declaring file is read once, however many members it declares.
    let mut declaring: Vec<&Move> = Vec::new();
    for member in members {
        if declaring
            .iter()
            .all(|known| known.home.declared_in != member.home.declared_in)
        {
            declaring.push(member);
        }
    }

    for member in declaring {
        let declared_in = member.home.declared_in.as_str();
        if travelling_files.contains(declared_in) {
            continue;
        }
        let text = workspace.read(declared_in)?;
        let outer = &member.home.path[..member.home.path.len() - 1];
        for declaration in test_declarations(&text) {
            let module_path: Vec<String> = outer
                .iter()
                .cloned()
                .chain([declaration.name.clone()])
                .collect();
            if co_moving.contains(&module_path.join("::")) {
                continue;
            }
            let Some(file) = declared_file(workspace, declared_in, &text, &declaration) else {
                continue;
            };
            if travelling_files.contains(&file) {
                continue;
            }

            let destination_src = Path::new(&member.destination.dir).join("src");
            let files = spanned_files(workspace, &file, &destination_src)?;
            let (reached, outside) =
                read_paths(workspace, origin, &co_moving, &module_path, &files)?;
            let Some(follows) = members
                .iter()
                .map(|member| member.home.path.join("::"))
                .find(|path| reached.contains(path))
            else {
                continue;
            };

            let name = declaration.name.clone();
            if let Some(outside) = outside {
                modules.notes.push(format!(
                    "test module `{name}` stays in `{declared_in}`: it names `{written}` (line {line}), \
                     which stays in `{origin}`",
                    written = outside.written,
                    line = outside.line,
                    origin = origin.package,
                ));
                continue;
            }
            if declaration.placed_by_path {
                modules.notes.push(format!(
                    "test module `{name}` stays in `{declared_in}`: it is placed with `#[path]`, \
                     which a move cannot follow"
                ));
                continue;
            }
            modules.notes.push(format!(
                "test module `{name}` ({count} file(s)) follows `{follows}`: everything it names in \
                 `{origin}` moves",
                count = files.len(),
                origin = origin.package,
            ));
            modules.following.push(FollowingTest {
                declared_in: declared_in.to_string(),
                declaration,
                module_path,
                files,
                follows,
            });
        }
    }
    Ok(modules)
}

/// The file a test declaration names: the one `#[path]` places it at, relative to the declaring
/// file's directory, or the one the module's name leads to beside it. `None` when no file exists,
/// which a declaration under `cfg(test)` may leave.
fn declared_file(
    workspace: &Workspace<'_>,
    declared_in: &str,
    text: &str,
    declaration: &TestDeclaration,
) -> Option<String> {
    if declaration.placed_by_path {
        let placed = placed_by_attribute(&text[declaration.span.clone()])?;
        let file = Path::new(declared_in)
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(placed)
            .to_string_lossy()
            .to_string();
        return workspace.read(&file).ok().map(|_| file);
    }
    module_files::file_of_child(
        workspace,
        &module_files::children_directory(declared_in),
        &declaration.name,
    )
    .map(|(file, _)| file)
}

/// The value of the `#[path = "…"]` attribute in a declaration's span.
fn placed_by_attribute(declaration: &str) -> Option<&str> {
    declaration.split("#[path").nth(1)?.split('"').nth(1)
}

/// `file` and the files of the modules below it, each with where it lands under `destination_src`
/// (`file` first). A file that is `mod.rs` lands in its own directory, as a moved member's does.
fn spanned_files(
    workspace: &Workspace<'_>,
    file: &str,
    destination_src: &Path,
) -> Result<Vec<MovedFile>> {
    let files = module_files::files_of(workspace, file)?;
    let path = Path::new(file);
    let (parent, name) = if path.file_name() == Some(OsStr::new("mod.rs")) {
        let directory = path.parent().unwrap_or_else(|| Path::new(""));
        (
            directory.parent().unwrap_or_else(|| Path::new("")),
            directory.file_name(),
        )
    } else {
        (
            path.parent().unwrap_or_else(|| Path::new("")),
            path.file_stem(),
        )
    };
    let name = name.and_then(OsStr::to_str).unwrap_or_default();
    Ok(module_files::relocated(
        parent,
        destination_src,
        name,
        &files,
    ))
}

/// What the paths of a test module's files name in the origin: the moving modules they reach, and
/// the first origin path that reaches none of them.
///
/// A path inside the test module itself names nothing outside it, so it is no reach either way.
fn read_paths(
    workspace: &Workspace<'_>,
    origin: &Destination,
    co_moving: &BTreeSet<String>,
    module_path: &[String],
    files: &[MovedFile],
) -> Result<(BTreeSet<String>, Option<Outside>)> {
    let name = origin.extern_name.as_str();
    let mut reached = BTreeSet::new();
    let mut outside = None;
    for file in files {
        let text = workspace.read(&file.from)?;
        let file_module: Vec<String> = module_path.iter().chain(&file.below).cloned().collect();
        let surveyed = survey::survey_moved_file(workspace, &text, origin, &file_module)?;
        for path in &surveyed.paths {
            if path.defining_crate != name || inside_the_module(path, name, module_path) {
                continue;
            }
            match reaching(path, name, co_moving) {
                Some(member) => {
                    reached.insert(member.clone());
                }
                None if outside.is_none() => {
                    outside = Some(Outside {
                        line: path.site.line,
                        written: spelled_at(&text, path.site)?,
                    });
                }
                None => {}
            }
        }
    }
    Ok((reached, outside))
}

/// A path of a test module that names something outside it and reaches no moving module.
struct Outside {
    line: u32,
    /// The path as the file writes it, glob `*` included.
    written: String,
}

/// Whether `path` resolves inside the module at `module_path` itself.
fn inside_the_module(path: &SurveyedPath, origin: &str, module_path: &[String]) -> bool {
    let root = format!("{origin}::{}", module_path.join("::"));
    path.resolved == root || path.resolved.starts_with(&format!("{root}::"))
}

/// The moving module `path` reaches, read from what it resolves to or what its re-exports lead to.
fn reaching<'a>(
    path: &SurveyedPath,
    origin: &str,
    co_moving: &'a BTreeSet<String>,
) -> Option<&'a String> {
    [&path.resolved, &path.defined_at]
        .into_iter()
        .find_map(|full| {
            let rest = full.strip_prefix(&format!("{origin}::"))?;
            header::travels_with(rest, co_moving)
        })
}

/// The path a file writes at `at`, read from the text: its segments and any glob, without the
/// `::` a group would leave behind.
fn spelled_at(text: &str, at: Position) -> Result<String> {
    let start = crate::apply::byte_offset(text, at.line, at.col)?;
    let spelled: String = text[start..]
        .chars()
        .take_while(|character| {
            header::is_path_character(*character) || *character == ':' || *character == '*'
        })
        .collect();
    Ok(spelled.trim_end_matches("::").to_string())
}

/// `modules`, with every follower whose items a file outside `travelling` names moved back to the
/// staying side, with a note naming that file: taking it along would break that file.
///
/// # Errors
///
/// Refuses for every reason the reference engine does.
pub(crate) fn kept_by_outside_references(
    engine: &mut dyn ModuleReferences,
    workspace: &Workspace<'_>,
    modules: TestModules,
    travelling: &BTreeSet<String>,
) -> Result<TestModules> {
    let TestModules {
        following,
        mut notes,
    } = modules;
    let mut kept = Vec::new();
    for follower in following {
        let mut named_from_outside = None;
        for file in &follower.files {
            named_from_outside = engine
                .outside_references(workspace, &file.from)?
                .into_iter()
                .flat_map(|item| {
                    let name = item.item;
                    item.referenced_at
                        .into_iter()
                        .map(move |reference| (name.clone(), reference.path))
                })
                .find(|(_, path)| !travelling.contains(path));
            if named_from_outside.is_some() {
                break;
            }
        }
        match named_from_outside {
            Some((item, path)) => notes.push(format!(
                "test module `{name}` stays in `{declared_in}`: `{path}` names its `{item}`",
                name = follower.declaration.name,
                declared_in = follower.declared_in,
            )),
            None => kept.push(follower),
        }
    }
    Ok(TestModules {
        following: kept,
        notes,
    })
}

/// The test modules `members` takes along: [`sorted`]'s verdicts, with the references from outside
/// the set read through `engine` for each one that would follow.
///
/// No earlier operation is passed to [`sorted`]: an operation resolved in `apply` reads the workspace
/// its earlier operations have already edited, so their moves are already in the text it reads.
///
/// # Errors
///
/// Refuses for every reason [`sorted`] and the reference engine do.
pub(crate) fn following_of(
    engine: &mut dyn ModuleReferences,
    workspace: &Workspace<'_>,
    members: &[Move],
) -> Result<TestModules> {
    let candidates = sorted(workspace, members, &BTreeSet::new())?;
    let mut carried = Vec::new();
    for member in members {
        carried.extend(member.carried(workspace)?);
    }
    let moving = travelling(members, &carried, &candidates.following);
    kept_by_outside_references(engine, workspace, candidates, &moving)
}

/// The notes a move of `cluster` says about the test modules beside its members.
///
/// # Errors
///
/// Refuses for every reason [`following_of`] does.
pub(crate) fn notes_of(
    engine: &mut dyn ModuleReferences,
    workspace: &Workspace<'_>,
    cluster: &MovingCluster,
) -> Result<Vec<String>> {
    let members = fold_carried_members(cluster.members.clone())
        .iter()
        .map(|home| Move::of(workspace, home, &cluster.destination, cluster.reexport))
        .collect::<Result<Vec<_>>>()?;
    Ok(following_of(engine, workspace, &members)?.notes)
}

/// Every file that travels: each member's own file, the files it carries, and every file of each
/// following test module — what the caller survey must not count as a caller.
pub(crate) fn travelling(
    members: &[Move],
    carried: &[MovedFile],
    following: &[FollowingTest],
) -> BTreeSet<String> {
    members
        .iter()
        .map(|member| member.source.clone())
        .chain(carried.iter().map(|file| file.from.clone()))
        .chain(
            following
                .iter()
                .flat_map(|test| test.files.iter().map(|file| file.from.clone())),
        )
        .collect()
}

/// The edits that take the followers along — a rename per file, its re-pointed header (read as test
/// code throughout), the declaration removed from the origin and appended to the destination root
/// under `#[cfg(test)]` — and the crates they name, every one of them a `[dev-dependencies]` entry.
///
/// # Errors
///
/// Refuses for every reason the header pass does, and for a follower whose landing is taken.
pub(crate) fn follow_changes(
    workspace: &Workspace<'_>,
    members: &[Move],
    following: &[FollowingTest],
    co_moving: &BTreeSet<String>,
) -> Result<(Vec<FileEdit>, BTreeSet<String>)> {
    let mut edits = Vec::new();
    let mut dev_crates = BTreeSet::new();
    let mut declarations = String::new();
    let Some(first) = members.first() else {
        return Ok((edits, dev_crates));
    };
    let root = first.destination_root();

    for follower in following {
        let member = followed(members, follower)?;
        if let Some(finding) = landing_collision(workspace, member, follower)? {
            return Err(malformed(finding));
        }
        for file in &follower.files {
            let text = workspace.read(&file.from)?;
            let in_tree = FileInTree {
                module_path: follower
                    .module_path
                    .iter()
                    .chain(&file.below)
                    .cloned()
                    .collect(),
                tree_root: follower.module_path.clone(),
                gate: Gate::Test,
            };
            let header =
                header::repointed_header_in(workspace, &text, member, &in_tree, co_moving)?;
            edits.push(FileEdit::Rename {
                from: file.from.clone(),
                to: file.to.clone(),
            });
            edits.push(FileEdit::Change {
                path: file.from.clone(),
                edits: header.edits,
            });
            dev_crates.extend(header.dev_crates_named);
        }

        let declared_in_text = workspace.read(&follower.declared_in)?;
        let span = follower.declaration.span.clone();
        edits.push(FileEdit::Change {
            path: follower.declared_in.clone(),
            edits: vec![manifest_edits::replacement(
                &declared_in_text,
                span.clone(),
                "",
            )],
        });
        declarations.push('\n');
        declarations.push_str(&declared_in_text[span]);
    }

    if !declarations.is_empty() {
        let root_text = workspace.read(&root)?;
        let end = root_text.len();
        edits.push(FileEdit::Change {
            path: root,
            edits: vec![manifest_edits::replacement(
                &root_text,
                end..end,
                &declarations,
            )],
        });
    }
    Ok((edits, dev_crates))
}

/// The member a following test module lands beside.
fn followed<'a>(members: &'a [Move], follower: &FollowingTest) -> Result<&'a Move> {
    members
        .iter()
        .find(|member| member.home.path.join("::") == follower.follows)
        .ok_or_else(|| {
            malformed(format!(
                "test module `{}` follows `{}`, which the move does not take",
                follower.declaration.name, follower.follows
            ))
        })
}

/// The finding for a follower whose landing file exists, or whose name the destination root already
/// declares: taking it along would merge two modules.
///
/// # Errors
///
/// Refuses when the destination root cannot be read.
fn landing_collision(
    workspace: &Workspace<'_>,
    member: &Move,
    follower: &FollowingTest,
) -> Result<Option<String>> {
    let name = &follower.declaration.name;
    let landing = follower
        .files
        .first()
        .map(|file| file.to.clone())
        .unwrap_or_default();
    if workspace.root.join(&landing).exists() {
        return Ok(Some(format!(
            "test module `{name}` follows `{follows}`, but `{landing}` already exists — moving it \
             would merge two modules, which no operation performs",
            follows = follower.follows,
        )));
    }
    let root = member.destination_root();
    if workspace.root.join(&root).exists()
        && manifest_edits::module_declaration(&workspace.read(&root)?, name).is_some()
    {
        return Ok(Some(format!(
            "test module `{name}` follows `{follows}`, but `{root}` already declares `{name}` — \
             moving it would merge two modules, which no operation performs",
            follows = follower.follows,
        )));
    }
    Ok(None)
}

/// The carried files reached through a `#[cfg(test)] mod` declaration, or below one: the files the
/// header pass reads as test code throughout.
///
/// # Errors
///
/// Refuses when a declaring file cannot be read.
pub(crate) fn test_gated(
    workspace: &Workspace<'_>,
    carried: &[MovedFile],
) -> Result<BTreeSet<String>> {
    let mut gated = BTreeSet::new();
    // A member's files begin with its own, which has no `below`, so each run of files that starts
    // at one is one member's tree, and a declaring file is looked up within the run.
    for tree in carried.chunk_by(|_, next| !next.below.is_empty()) {
        for file in tree.iter().filter(|file| !file.below.is_empty()) {
            if declared_under_test(workspace, tree, &file.below)? {
                gated.insert(file.from.clone());
            }
        }
    }
    Ok(gated)
}

/// Whether one of the modules `below` leads through is declared `#[cfg(test)]` in the file that
/// declares it, among the files of its member's `tree`.
///
/// TODO(reshape-tests-follow): only a declaration at the top level of its declaring file is read;
/// a test declaration inside an inline module on the way down is not.
fn declared_under_test(
    workspace: &Workspace<'_>,
    tree: &[MovedFile],
    below: &[String],
) -> Result<bool> {
    for (depth, name) in below.iter().enumerate() {
        let Some(declaring) = tree.iter().find(|file| file.below == below[..depth]) else {
            continue;
        };
        let text = workspace.read(&declaring.from)?;
        if test_declarations(&text)
            .iter()
            .any(|declaration| &declaration.name == name)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Every reason operation `index` of `ops` (`op`) cannot take a following test module along, read
/// statically: its landing file already exists in the destination, or the destination root already
/// declares its name — a merge.
///
/// # Errors
///
/// Refuses when a file the check must read cannot be.
pub(crate) fn unfollowable(
    workspace: &Workspace<'_>,
    ops: &[RefactorOp],
    index: usize,
    op: &RefactorOp,
) -> Result<Vec<String>> {
    let earlier = moved_by_earlier_operations(workspace, ops, index, &op.anchor);
    let first = Move::read(workspace, op)?;
    let homes = op
        .anchors()
        .map(|anchor| home_of_anchor(workspace, anchor))
        .collect::<Result<Vec<_>>>()?;
    let members = fold_carried_members(homes)
        .iter()
        .map(|home| Move::of(workspace, home, &first.destination, first.reexport))
        .collect::<Result<Vec<_>>>()?;
    let Some(member) = members.first() else {
        return Ok(Vec::new());
    };

    let following = match sorted(workspace, &members, &earlier) {
        Ok(modules) => modules.following,
        // A carry the move refuses is named by `uncarriable`, and there is no test module to read
        // beside a module that cannot be carried.
        Err(RestructureError::MalformedPlan(_)) => return Ok(Vec::new()),
        Err(other) => return Err(other),
    };
    let mut findings = Vec::new();
    for follower in following {
        if let Some(finding) = landing_collision(workspace, member, &follower)? {
            findings.push(malformed(finding).to_string());
        }
    }
    Ok(findings)
}
