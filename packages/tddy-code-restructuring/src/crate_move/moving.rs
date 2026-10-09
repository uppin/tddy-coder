use super::PlannedRewrite;

use std::collections::BTreeSet;

use crate::{
    crate_move::{destination, manifest_edits, module_files, module_home},
    edit::FileEdit,
};

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

    /// Every file the module spans — its own first, then each file its `mod` declarations lead to
    /// — with where each lands under the destination's `src/`, keeping its place relative to the
    /// module.
    ///
    /// # Errors
    ///
    /// Refuses when a declaration leads to no file, as [`module_files::files_of`] does.
    #[allow(
        dead_code,
        reason = "TODO(reshape-move-children): implement — `resolve_cluster` and the \
                  preconditions read every carried file"
    )]
    pub(crate) fn carried(
        &self,
        workspace: &Workspace<'_>,
    ) -> Result<Vec<module_files::MovedFile>> {
        // TODO(reshape-move-children): implement
        let _ = workspace;
        todo!("Move::carried")
    }

    /// The module's `mod` declaration in the file that declares it, with its visibility.
    ///
    /// # Errors
    ///
    /// Refuses when that file declares no such module.
    #[allow(
        dead_code,
        reason = "TODO(reshape-move-children): implement — the facade writer keeps its visibility"
    )]
    pub(crate) fn declaration(
        &self,
        workspace: &Workspace<'_>,
    ) -> Result<manifest_edits::ModuleDeclaration> {
        // TODO(reshape-move-children): implement
        let _ = workspace;
        todo!("Move::declaration")
    }

    /// The crate root that has to declare it afterwards.
    pub(crate) fn destination_root(&self) -> String {
        format!("{}/src/lib.rs", self.destination.dir)
    }

    /// Whether the file declaring the module re-exports it by glob or group, which a move with no
    /// facade rewrites to name the destination — and which makes that file's crate depend on it.
    pub(crate) fn has_parent_reexport(&self, workspace: &Workspace<'_>) -> Result<bool> {
        let text = workspace.read(&self.home.declared_in)?;
        Ok(!facade_writer::parent_reexports_of(&text, &self.module).is_empty())
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

mod facade_writer;
pub(crate) use facade_writer::{declared_in_destination, left_behind};

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
        let facade =
            facade_writer::written_facade(text, "dest", "pub", DESTINATION_ROOT).expect("a facade");

        // Then both modules are in it
        assert_eq!(facade.modules, ["alpha", "beta"]);
    }

    /// A line the user wrote for something the destination does not declare is theirs.
    #[test]
    fn leaves_a_hand_written_re_export_of_an_item_alone() {
        // Given a `pub use` of an item, not a module the destination declares
        let text = "pub use dest::HostRegistry;\n";

        // When the written facade is read
        let facade = facade_writer::written_facade(text, "dest", "pub", DESTINATION_ROOT);

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
        let facade = facade_writer::written_facade(text, "dest", "pub", DESTINATION_ROOT);

        // Then it is taken for ours
        assert!(facade.is_some());
    }

    /// A private `mod` in the destination is not how this tool declares what it moves.
    #[test]
    fn leaves_a_line_naming_a_privately_declared_module_alone() {
        // Given a destination declaring `alpha` without `pub`
        let text = "pub use dest::alpha;\n";

        // When the written facade is read
        let facade = facade_writer::written_facade(text, "dest", "pub", "mod alpha;\n");

        // Then there is none
        assert!(facade.is_none());
    }

    /// A later operation extends the facade an earlier one wrote with the same visibility, and
    /// leaves a line of another visibility to itself.
    #[test]
    fn extends_an_earlier_facade_of_the_same_visibility_and_leaves_one_of_another_visibility_alone()
    {
        // Given a `pub(crate)` facade an earlier operation wrote, beside a `pub` one
        let text = "pub use dest::alpha;\npub(crate) use dest::beta;\n";

        // When the facade a `pub(crate)` move would extend is read, and the one a private move would
        let restricted =
            facade_writer::written_facade(text, "dest", "pub(crate)", DESTINATION_ROOT)
                .map(|facade| (&text[facade.span], facade.modules));
        let private = facade_writer::written_facade(text, "dest", "", DESTINATION_ROOT);

        // Then the `pub(crate)` line is the one, and no private line exists to extend
        assert_eq!(
            (restricted, private.is_none()),
            (
                Some(("pub(crate) use dest::beta;\n", vec!["beta".to_string()])),
                true
            )
        );
    }

    /// The parent's own glob over the moved module is re-pointed.
    #[test]
    fn finds_a_top_level_glob_re_export_of_the_module() {
        // Given
        let text = "pub use host_registry::*;\nmod host_registry;\n";

        // When
        let found = facade_writer::parent_reexports_of(text, "host_registry");

        // Then
        assert_eq!(found, [8]);
    }

    /// An inline module's `use host_registry::*;` names that scope's own `host_registry`.
    #[test]
    fn ignores_a_use_inside_an_inline_module() {
        // Given a glob import indented inside an inline module
        let text = "mod inner {\n    use host_registry::*;\n}\n";

        // When
        let found = facade_writer::parent_reexports_of(text, "host_registry");

        // Then nothing is re-pointed
        assert!(found.is_empty());
    }

    /// Unformatted code puts a `use` at column 0 inside a brace; depth, not indentation, decides.
    #[test]
    fn ignores_an_unindented_use_inside_a_function_body() {
        // Given
        let text = "fn f() {\nuse host_registry::*;\n}\nuse host_registry::{a};\n";

        // When
        let found = facade_writer::parent_reexports_of(text, "host_registry");

        // Then only the top-level one is found
        assert_eq!(found, [text.rfind("host_registry").unwrap()]);
    }
}
