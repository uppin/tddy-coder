use super::PlannedRewrite;

use std::collections::BTreeSet;

use super::facade_line;

use crate::{
    crate_move::{destination, manifest_edits, module_home},
    edit::FileEdit,
};

use super::Survey;

use super::malformed;

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

    /// Where the module file lands.
    pub(crate) fn moved_to(&self) -> String {
        format!("{}/src/{}.rs", self.destination.dir, self.module)
    }

    /// The crate root that has to declare it afterwards.
    pub(crate) fn destination_root(&self) -> String {
        format!("{}/src/lib.rs", self.destination.dir)
    }

    /// The file that declares the module today: its `mod` line is replaced by a facade, or removed.
    pub(crate) fn left_behind(
        &self,
        workspace: &Workspace<'_>,
        survey: &Survey,
    ) -> Result<FileEdit> {
        let path = self.home.declared_in.clone();
        let text = workspace.read(&path)?;
        let span = manifest_edits::module_declaration(&text, &self.module).ok_or_else(|| {
            let where_declared = if self.home.is_top_level() {
                "crate root"
            } else {
                "parent module"
            };
            malformed(format!(
                "{path} declares no `mod {}` — a module this {where_declared} does not declare is \
                 not this crate's to move",
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
            edits: vec![manifest_edits::replacement(&text, span, &line)],
        })
    }

    /// The destination crate root, declaring the module it is about to receive.
    ///
    /// `pub mod`, not `mod`: the module keeps its name and its callers keep writing it, which only
    /// resolves from another crate if the module is public. That is also what a glob facade needs
    /// to re-export.
    pub(crate) fn declared_in_destination(&self, workspace: &Workspace<'_>) -> Result<FileEdit> {
        let path = self.destination_root();
        let text = workspace.read(&path)?;
        let declaration = format!("pub mod {};\n", self.module);
        let at = manifest_edits::after_last_module_declaration(&text);

        Ok(FileEdit::Change {
            path,
            edits: vec![manifest_edits::replacement(&text, at..at, &declaration)],
        })
    }

    /// The destination's manifest, gaining every crate the moved code names.
    ///
    /// Each dependency is copied from the manifest that already declares it rather than written
    /// here: a version this operation invented would be a fact about the world it has no way to
    /// know. The one it does author is the path back to the crate the module left, which is a fact
    /// about this repository's own layout.
    pub(crate) fn destination_manifest(
        &self,
        workspace: &Workspace<'_>,
        named: &BTreeSet<String>,
    ) -> Result<FileEdit> {
        let path = format!("{}/Cargo.toml", self.destination.dir);
        let text = workspace.read(&path)?;
        let origin = workspace.read(&format!("{}/Cargo.toml", self.origin.dir))?;

        let mut lines = Vec::new();
        for extern_name in named {
            if manifest_edits::declares_dependency(&text, extern_name) {
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
            let declared =
                manifest_edits::dependency_line(&origin, extern_name).ok_or_else(|| {
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

        Ok(FileEdit::Change {
            path,
            edits: manifest_edits::with_dependencies(&text, &lines),
        })
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
            if manifest_edits::declares_dependency(&text, &self.destination.extern_name) {
                continue;
            }

            let line = format!(
                "{} = {{ path = \"{}\" }}",
                self.destination.package,
                manifest_edits::relative_from(directory, &self.destination.dir)
            );
            changes.push(FileEdit::Change {
                path,
                edits: manifest_edits::with_dependencies(&text, &[line]),
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
