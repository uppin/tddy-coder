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
            edits: leaving(workspace, path, &text, &here)?,
        });
    }
    Ok(changes)
}

/// The edits one declaring file needs for the members it declares.
fn leaving(
    workspace: &Workspace<'_>,
    path: &str,
    text: &str,
    members: &[(&Move, &Survey)],
) -> Result<Vec<TextEdit>> {
    let facade_members: Vec<&Move> = members
        .iter()
        .map(|(member, _)| *member)
        .filter(|member| member.reexport == Reexport::Glob)
        .collect();
    let moved: Vec<(destination::Destination, String)> = facade_members
        .iter()
        .map(|member| (member.destination.clone(), member.module.clone()))
        .collect();

    let mut edits = Vec::new();
    let mut facade_written = false;
    if let Some(earlier) = earlier_facade(workspace, text, &facade_members) {
        edits.push(extended_facade(text, &earlier, &facade_members, &moved));
        facade_written = true;
    }

    for (member, survey) in members {
        let span = declaration_of(path, text, member)?;
        let facade = match member.reexport {
            Reexport::Glob if facade_written => None,
            Reexport::Glob => {
                facade_written = true;
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
        edits.extend(parent_reexport_edits(text, member));
    }
    Ok(edits)
}

/// The facade an earlier operation of the plan wrote for the first facade member's destination.
fn earlier_facade(
    workspace: &Workspace<'_>,
    text: &str,
    facade_members: &[&Move],
) -> Option<WrittenFacade> {
    let first = facade_members.first()?;
    let root = workspace.read(&first.destination_root()).ok()?;
    written_facade(text, &first.destination.extern_name, &root)
}

/// The edit that rewrites an earlier facade line to also name the modules moving now.
fn extended_facade(
    text: &str,
    earlier: &WrittenFacade,
    facade_members: &[&Move],
    moved: &[(destination::Destination, String)],
) -> TextEdit {
    let destination = &facade_members[0].destination;
    let all: Vec<_> = earlier
        .modules
        .iter()
        .map(|module| (destination.clone(), module.clone()))
        .chain(moved.iter().cloned())
        .collect();
    let line = facade_lines_for_plan(&all).join("\n");
    manifest_edits::replacement(text, earlier.span.clone(), &format!("{line}\n"))
}

/// Where the file declares `member`'s module, or the refusal when it does not.
fn declaration_of(path: &str, text: &str, member: &Move) -> Result<std::ops::Range<usize>> {
    manifest_edits::module_declaration(text, &member.module).ok_or_else(|| {
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
    })
}

/// The edits that point a moved-without-facade module's parent re-exports at the destination.
fn parent_reexport_edits(text: &str, member: &Move) -> Vec<TextEdit> {
    if member.reexport != Reexport::None {
        return Vec::new();
    }
    parent_reexports_of(text, &member.module)
        .into_iter()
        .map(|at| {
            manifest_edits::replacement(
                text,
                at..at + member.module.len(),
                &format!("{}::{}", member.destination.extern_name, member.module),
            )
        })
        .collect()
}

/// A grouped facade an earlier operation wrote for one destination.
struct WrittenFacade {
    /// The line, newline included.
    span: std::ops::Range<usize>,
    modules: Vec<String>,
}

/// The `pub use <crate>::{a, b};` (or `pub use <crate>::a;`) line this tool wrote, if the file has
/// one.
///
/// Provenance is read off the destination: the tool declares every module it moves as a `pub mod`
/// in the destination root (`destination_root`), so a line is extended only when **every** module it
/// names is declared there. A `pub use <crate>::a;` the user wrote by hand for an item or a module
/// the destination does not declare is left alone. A hand-written line naming only modules the
/// destination does declare is indistinguishable from ours and is extended; that limit is pinned by
/// a test.
fn written_facade(text: &str, extern_name: &str, destination_root: &str) -> Option<WrittenFacade> {
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
        let plain_and_declared = modules.iter().all(|name| {
            !name.is_empty()
                && name.chars().all(header::is_path_character)
                && is_declared_pub(destination_root, name)
        });
        if plain_and_declared {
            return Some(WrittenFacade {
                span: start..offset,
                modules,
            });
        }
    }
    None
}

/// Whether the root declares `module` as `pub mod`, which is how the tool declares what it moves.
fn is_declared_pub(root: &str, module: &str) -> bool {
    manifest_edits::module_declaration(root, module)
        .is_some_and(|span| root[span].trim_start().starts_with("pub mod "))
}

/// Where `module` starts in each top-level `use module::*;` / `use module::{…};` of a file, whatever
/// the visibility. A path to one item is not here: the caller survey sees it and re-points it.
///
/// Only declarations at column 0 and outside any brace nesting count: a `use` inside an inline
/// module or a function body names that scope's own `module`, not the parent's.
fn parent_reexports_of(text: &str, module: &str) -> Vec<usize> {
    let mut found = Vec::new();
    let mut offset = 0usize;
    let mut depth = 0i32;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();

        let code = line.split("//").next().unwrap_or(line);
        let top_level = depth == 0 && !line.starts_with(char::is_whitespace);
        depth += code.matches('{').count() as i32 - code.matches('}').count() as i32;
        if !top_level {
            continue;
        }

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
            found.push(start + (line.len() - tree.len()));
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

#[cfg(test)]
mod tests {
    use super::*;

    const DESTINATION_ROOT: &str = "pub mod alpha;\npub mod beta;\n";

    /// A facade line naming modules the destination root declares is one this tool wrote.
    #[test]
    fn extends_a_facade_naming_only_modules_the_destination_declares() {
        // Given a facade naming two modules the destination declares `pub mod`
        let text = "pub use dest::{alpha, beta};\nmod runtime;\n";

        // When the written facade is read
        let facade = written_facade(text, "dest", DESTINATION_ROOT).expect("a facade");

        // Then both modules are in it
        assert_eq!(facade.modules, ["alpha", "beta"]);
    }

    /// A line the user wrote for something the destination does not declare is theirs.
    #[test]
    fn leaves_a_hand_written_re_export_of_an_item_alone() {
        // Given a `pub use` of an item, not a module the destination declares
        let text = "pub use dest::HostRegistry;\n";

        // When the written facade is read
        let facade = written_facade(text, "dest", DESTINATION_ROOT);

        // Then there is none
        assert!(facade.is_none());
    }

    /// The limit of provenance, pinned: a hand-written line naming only declared modules cannot be
    /// told from ours.
    #[test]
    fn cannot_tell_a_hand_written_line_naming_declared_modules_from_its_own() {
        // Given a user-written `pub use` of a module the destination declares
        let text = "pub use dest::alpha;\n";

        // When the written facade is read
        let facade = written_facade(text, "dest", DESTINATION_ROOT);

        // Then it is taken for ours
        assert!(facade.is_some());
    }

    /// A private `mod` in the destination is not how this tool declares what it moves.
    #[test]
    fn leaves_a_line_naming_a_privately_declared_module_alone() {
        // Given a destination declaring `alpha` without `pub`
        let text = "pub use dest::alpha;\n";

        // When the written facade is read
        let facade = written_facade(text, "dest", "mod alpha;\n");

        // Then there is none
        assert!(facade.is_none());
    }

    /// The parent's own glob over the moved module is re-pointed.
    #[test]
    fn finds_a_top_level_glob_re_export_of_the_module() {
        // Given
        let text = "pub use host_registry::*;\nmod host_registry;\n";

        // When
        let found = parent_reexports_of(text, "host_registry");

        // Then
        assert_eq!(found, [8]);
    }

    /// An inline module's `use host_registry::*;` names that scope's own `host_registry`.
    #[test]
    fn ignores_a_use_inside_an_inline_module() {
        // Given a glob import indented inside an inline module
        let text = "mod inner {\n    use host_registry::*;\n}\n";

        // When
        let found = parent_reexports_of(text, "host_registry");

        // Then nothing is re-pointed
        assert!(found.is_empty());
    }

    /// Unformatted code puts a `use` at column 0 inside a brace; depth, not indentation, decides.
    #[test]
    fn ignores_an_unindented_use_inside_a_function_body() {
        // Given
        let text = "fn f() {\nuse host_registry::*;\n}\nuse host_registry::{a};\n";

        // When
        let found = parent_reexports_of(text, "host_registry");

        // Then only the top-level one is found
        assert_eq!(found, [text.rfind("host_registry").unwrap()]);
    }
}
