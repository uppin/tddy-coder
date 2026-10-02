use super::PlannedRewrite;

use std::collections::BTreeSet;

use super::{facade_line, facade_lines_for_plan};

use crate::{
    crate_move::{destination, header, manifest_edits, module_home},
    edit::{FileEdit, TextEdit},
};

use super::Survey;

use super::malformed;

use manifest_edits::Table;

use super::Result;

use crate::plan::RefactorOp;

use crate::registry::Workspace;

use crate::plan::Reexport;

/// Everything the two entry points read out of the plan and the two manifests.
pub(crate) struct Move {
    /// The module file, relative to the repository root.
    pub(crate) source: String,
    /// The identifier the crate root declares — `host_registry`.
    pub(crate) module: String,
    /// Where the module sits in its crate and which file declares it.
    pub(crate) home: module_home::ModuleHome,
    /// The crate the module is leaving, read from its own manifest for the same reason the
    /// destination is: a caller's `use` path needs the declared name, not the directory's.
    pub(crate) origin: destination::Destination,
    /// The crate it is arriving in.
    pub(crate) destination: destination::Destination,
    /// What to leave behind in the crate it left.
    pub(crate) reexport: Reexport,
}

impl Move {
    pub(crate) fn read(workspace: &Workspace<'_>, op: &RefactorOp) -> Result<Move> {
        let source = op.anchor.file().to_string();
        let module = module_home::module_name(&source)?;
        let home = module_home::module_home(workspace, &source, &module)?;
        let destination = op.to.as_deref().ok_or_else(|| {
            malformed(
                "`move_module_to_crate` needs `to`: the destination crate's directory, relative to \
                 the repository root",
            )
        })?;

        Ok(Move {
            origin: destination::Destination::read(workspace.root, &home.crate_dir)?,
            destination: destination::Destination::read(workspace.root, destination)?,
            reexport: op.reexport.unwrap_or(Reexport::None),
            source,
            module,
            home,
        })
    }

    /// The move of one member of a cluster, whose home the set already carries.
    ///
    /// [`Move::read`] resolves a home out of a plan's anchor. A cluster's members arrive already
    /// resolved, and deriving each one back out of a synthetic anchor would be inventing a plan in
    /// order to read it again.
    pub(crate) fn of(
        workspace: &Workspace<'_>,
        home: &module_home::ModuleHome,
        destination: &destination::Destination,
        reexport: Reexport,
    ) -> Result<Move> {
        let module = home.path.last().cloned().ok_or_else(|| {
            malformed("a cluster member names no module — a module path is at least one identifier")
        })?;

        Ok(Move {
            origin: destination::Destination::read(workspace.root, &home.crate_dir)?,
            destination: destination.clone(),
            reexport,
            source: format!("{}/src/{}.rs", home.crate_dir, home.path.join("/")),
            module,
            home: home.clone(),
        })
    }

    /// Where the module file lands.
    pub(crate) fn moved_to(&self) -> String {
        format!("{}/src/{}.rs", self.destination.dir, self.module)
    }

    /// The crate root that has to declare it afterwards.
    pub(crate) fn destination_root(&self) -> String {
        format!("{}/src/lib.rs", self.destination.dir)
    }

    /// Whether the file declaring the module re-exports it by glob or group, which a move with no
    /// facade rewrites to name the destination — and which makes that file's crate depend on it.
    pub(crate) fn has_parent_reexport(&self, workspace: &Workspace<'_>) -> Result<bool> {
        let text = workspace.read(&self.home.declared_in)?;
        Ok(!parent_reexports_of(&text, &self.module).is_empty())
    }

    /// The destination's manifest, gaining every crate the moved code names.
    ///
    /// `named` goes to `[dependencies]`, and `dev_named` — what only `#[cfg(test)]` code names — to
    /// `[dev-dependencies]`. Each dependency is copied from the manifest that already declares it
    /// rather than written here: a version this operation invented would be a fact about the world
    /// it has no way to know. The one it does author is the path back to the crate the module left,
    /// which is a fact about this repository's own layout.
    ///
    /// # Errors
    ///
    /// Refuses when a crate to carry is declared nowhere it could be copied from — and when the
    /// destination is among them. That one is an assertion, not a filter: the survey reads the
    /// destination's own items as `crate::`, so a destination reaching this far means the survey is
    /// wrong, and dropping it here would hide that.
    pub(crate) fn destination_manifest(
        &self,
        workspace: &Workspace<'_>,
        named: &BTreeSet<String>,
        dev_named: &BTreeSet<String>,
    ) -> Result<FileEdit> {
        if named.contains(&self.destination.extern_name)
            || dev_named.contains(&self.destination.extern_name)
        {
            return Err(malformed(format!(
                "the survey of `{}` reports a dependency of `{}` on itself — its own items are \
                 `crate::` paths, so the survey and the rewrite disagree about what the file names",
                self.source, self.destination.package
            )));
        }

        let path = format!("{}/Cargo.toml", self.destination.dir);
        let text = workspace.read(&path)?;
        let origin = workspace.read(&format!("{}/Cargo.toml", self.origin.dir))?;

        let dependencies = self.dependency_lines(&text, &origin, Table::Dependencies, named)?;
        let dev_dependencies =
            self.dependency_lines(&text, &origin, Table::DevDependencies, dev_named)?;

        let mut edits =
            manifest_edits::with_dependencies(&text, Table::Dependencies, &dependencies);
        edits.extend(manifest_edits::with_dependencies(
            &text,
            Table::DevDependencies,
            &dev_dependencies,
        ));
        Ok(FileEdit::Change { path, edits })
    }

    /// The lines `table` of the destination's manifest has to gain for `named`.
    ///
    /// A dev-dependency is already reachable from the destination's tests when either table declares
    /// it, so it is skipped then; a dependency is only reachable from its code through
    /// `[dependencies]`.
    fn dependency_lines(
        &self,
        destination: &str,
        origin: &str,
        table: Table,
        named: &BTreeSet<String>,
    ) -> Result<Vec<String>> {
        let mut lines = Vec::new();
        for extern_name in named {
            let declared_already = match table {
                Table::Dependencies => {
                    manifest_edits::declares_dependency(destination, table, extern_name)
                }
                Table::DevDependencies => {
                    manifest_edits::declares_dependency_in_either_table(destination, extern_name)
                }
            };
            if declared_already {
                continue;
            }
            if *extern_name == self.origin.extern_name {
                lines.push(format!(
                    "{} = {{ path = \"{}\" }}",
                    self.origin.package,
                    manifest_edits::relative_from(&self.destination.dir, &self.origin.dir)
                ));
                continue;
            }
            let declared = match table {
                Table::Dependencies => manifest_edits::dependency_line(origin, table, extern_name),
                Table::DevDependencies => {
                    manifest_edits::dependency_line_from_either_table(origin, extern_name)
                }
            }
            .ok_or_else(|| {
                malformed(format!(
                    "the moved module names `{extern_name}`, which {}/Cargo.toml does not declare \
                     — there is nothing to carry across",
                    self.origin.dir
                ))
            })?;
            lines.push(manifest_edits::re_anchored(
                &declared,
                &self.origin.dir,
                &self.destination.dir,
            ));
        }
        Ok(lines)
    }

    /// Every manifest that has to gain a dependency on the destination.
    ///
    /// Without this a move produces a tree that reads correctly and does not build: the facade, or
    /// the caller this operation re-pointed, names a crate the manifest never heard of. It is the
    /// one edit no unit test caught and the first `cargo check` did.
    pub(crate) fn dependents_on_the_destination(
        &self,
        workspace: &Workspace<'_>,
        crates: &BTreeSet<String>,
    ) -> Result<Vec<FileEdit>> {
        let mut changes = Vec::new();
        for directory in crates {
            let path = format!("{directory}/Cargo.toml");
            let text = workspace.read(&path)?;
            if manifest_edits::declares_dependency(
                &text,
                manifest_edits::Table::Dependencies,
                &self.destination.extern_name,
            ) {
                continue;
            }

            let line = format!(
                "{} = {{ path = \"{}\" }}",
                self.destination.package,
                manifest_edits::relative_from(directory, &self.destination.dir)
            );
            changes.push(FileEdit::Change {
                path,
                edits: manifest_edits::with_dependencies(
                    &text,
                    manifest_edits::Table::Dependencies,
                    &[line],
                ),
            });
        }
        Ok(changes)
    }

    /// The workspace root's `members`, gaining the destination when it is not already listed.
    ///
    /// Nothing is emitted when the root manifest declares no `members` array: there is no list for
    /// the crate to be missing from, and inventing one would be authoring a workspace rather than
    /// moving a module.
    pub(crate) fn workspace_members(&self, workspace: &Workspace<'_>) -> Result<Vec<FileEdit>> {
        let path = "Cargo.toml".to_string();
        if !workspace.root.join(&path).exists() {
            return Ok(Vec::new());
        }

        let text = workspace.read(&path)?;
        let Some(members) = manifest_edits::members_list(&text) else {
            return Ok(Vec::new());
        };
        if text[members.clone()].contains(&format!("\"{}\"", self.destination.dir)) {
            return Ok(Vec::new());
        }

        let entry = format!("    \"{}\",\n", self.destination.dir);
        Ok(vec![FileEdit::Change {
            path,
            edits: vec![manifest_edits::replacement(
                &text,
                members.end..members.end,
                &entry,
            )],
        }])
    }
}

/// What a set of moves leaves in the files that declared them: the `mod` lines go, and in their
/// place a facade, when the plan asked for one.
///
/// The set is read together because its facade is: [`Reexport::Glob`] leaves **one** `pub use
/// <dest>::{a, b};` naming every module that moved there. That line takes the place of the first
/// `mod` line it replaces, and a line an earlier operation of the plan already wrote for the same
/// destination is extended in place instead, so a plan of three moves ends with one line, not three.
/// `surveys` pairs with `members`, in order.
///
/// A module moved with no facade leaves its parent's `use <module>::*;` and `use <module>::{…};`
/// dangling: no caller reaches them by name, so nothing else re-points them. Both are rewritten to
/// name the destination.
pub(crate) fn left_behind(
    workspace: &Workspace<'_>,
    members: &[Move],
    surveys: &[Survey],
) -> Result<Vec<FileEdit>> {
    let mut declaring_files: Vec<&str> = Vec::new();
    for member in members {
        if !declaring_files.contains(&member.home.declared_in.as_str()) {
            declaring_files.push(&member.home.declared_in);
        }
    }

    let mut changes = Vec::new();
    for path in declaring_files {
        let text = workspace.read(path)?;
        let here: Vec<(&Move, &Survey)> = members
            .iter()
            .zip(surveys)
            .filter(|(member, _)| member.home.declared_in == path)
            .collect();
        changes.push(FileEdit::Change {
            path: path.to_string(),
            edits: leaving(path, &text, &here)?,
        });
    }
    Ok(changes)
}

/// The edits one declaring file needs for the members it declares.
fn leaving(path: &str, text: &str, members: &[(&Move, &Survey)]) -> Result<Vec<TextEdit>> {
    let facade_members: Vec<&Move> = members
        .iter()
        .map(|(member, _)| *member)
        .filter(|member| member.reexport == Reexport::Glob)
        .collect();
    let earlier = facade_members
        .first()
        .and_then(|member| written_facade(text, &member.destination.extern_name));

    let mut edits = Vec::new();
    let mut facade_written = false;
    if let (Some(first), Some(earlier)) = (facade_members.first(), &earlier) {
        let moved: Vec<_> = earlier
            .modules
            .iter()
            .chain(facade_members.iter().map(|member| &member.module))
            .map(|module| (first.destination.clone(), module.clone()))
            .collect();
        let line = facade_lines_for_plan(&moved).join("\n");
        edits.push(manifest_edits::replacement(
            text,
            earlier.span.clone(),
            &format!("{line}\n"),
        ));
        facade_written = true;
    }

    for (member, survey) in members {
        let span = manifest_edits::module_declaration(text, &member.module).ok_or_else(|| {
            let where_declared = if member.home.is_top_level() {
                "crate root"
            } else {
                "parent module"
            };
            malformed(format!(
                "{path} declares no `mod {}` — a module this {where_declared} does not declare is \
                 not this crate's to move",
                member.module
            ))
        })?;

        let facade = match member.reexport {
            Reexport::Glob if facade_written => None,
            Reexport::Glob => {
                facade_written = true;
                let moved: Vec<_> = facade_members
                    .iter()
                    .map(|member| (member.destination.clone(), member.module.clone()))
                    .collect();
                Some(facade_lines_for_plan(&moved).join("\n"))
            }
            Reexport::Named | Reexport::None => facade_line(
                &member.destination,
                member.reexport,
                &survey.reached_from_outside,
            ),
        };
        // The declaration's line goes entirely, newline included, when nothing replaces it.
        let line = facade.map_or_else(String::new, |facade| format!("{facade}\n"));
        edits.push(manifest_edits::replacement(text, span, &line));

        if member.reexport == Reexport::None {
            for at in parent_reexports_of(text, &member.module) {
                edits.push(manifest_edits::replacement(
                    text,
                    at..at + member.module.len(),
                    &format!("{}::{}", member.destination.extern_name, member.module),
                ));
            }
        }
    }
    Ok(edits)
}

/// A grouped facade an earlier operation wrote for one destination.
struct WrittenFacade {
    /// The line, newline included.
    span: std::ops::Range<usize>,
    modules: Vec<String>,
}

/// The `pub use <crate>::{a, b};` (or `pub use <crate>::a;`) line naming plain modules, if the file
/// has one. A line re-exporting a path, a glob or an alias is someone's own and is left alone.
fn written_facade(text: &str, extern_name: &str) -> Option<WrittenFacade> {
    let prefix = format!("pub use {extern_name}::");
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();

        let Some(named) = line
            .trim_end()
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix(';'))
        else {
            continue;
        };
        let named = named
            .strip_prefix('{')
            .and_then(|group| group.strip_suffix('}'))
            .unwrap_or(named);
        let modules: Vec<String> = named
            .split(',')
            .map(|name| name.trim().to_string())
            .collect();
        if modules
            .iter()
            .all(|name| !name.is_empty() && name.chars().all(header::is_path_character))
        {
            return Some(WrittenFacade {
                span: start..offset,
                modules,
            });
        }
    }
    None
}

/// Where `module` starts in each `use module::*;` / `use module::{…};` of a file, whatever the
/// visibility. A path to one item is not here: the caller survey sees it and re-points it.
fn parent_reexports_of(text: &str, module: &str) -> Vec<usize> {
    let mut found = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();

        let line = line.trim_start();
        let Some(tree) = after_visibility(line)
            .strip_prefix("use ")
            .map(str::trim_start)
        else {
            continue;
        };
        let Some(rest) = tree
            .strip_prefix(module)
            .and_then(|rest| rest.strip_prefix("::"))
        else {
            continue;
        };
        if rest.starts_with('*') || rest.starts_with('{') {
            found.push(start + (text[start..offset].len() - tree.len()));
        }
    }
    found
}

/// A declaration without its `pub`, `pub(crate)` or `pub(in …)` prefix.
fn after_visibility(declaration: &str) -> &str {
    let Some(rest) = declaration.strip_prefix("pub") else {
        return declaration;
    };
    let rest = match rest.strip_prefix('(') {
        Some(restriction) => restriction.split_once(')').map_or("", |(_, after)| after),
        None => rest,
    };
    if rest.starts_with(char::is_whitespace) {
        rest.trim_start()
    } else {
        declaration
    }
}

/// The destination crate root, declaring every module it is about to receive.
///
/// `pub mod`, not `mod`: a module keeps its name and its callers keep writing it, which only
/// resolves from another crate if the module is public. That is also what a facade needs to
/// re-export. Each lands in sorted position among the root's existing declarations.
pub(crate) fn declared_in_destination(
    workspace: &Workspace<'_>,
    members: &[Move],
) -> Result<Vec<FileEdit>> {
    let Some(first) = members.first() else {
        return Ok(Vec::new());
    };
    let path = first.destination_root();
    let text = workspace.read(&path)?;

    // Each is placed against the root as it stands, so two that land in the same gap stay in the
    // order the plan named them.
    let edits = members
        .iter()
        .map(|member| {
            manifest_edits::insert_module_declaration_sorted(
                &text,
                &format!("pub mod {};", member.module),
            )
        })
        .collect();

    Ok(vec![FileEdit::Change { path, edits }])
}

/// One `FileEdit::Change` per caller, carrying every path in that file at once.
pub(crate) fn caller_changes(
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
            .map(|rewrite| manifest_edits::replacement(&text, rewrite.span.clone(), &rewrite.to))
            .collect();
        changes.push(FileEdit::Change { path, edits });
    }
    Ok(changes)
}
