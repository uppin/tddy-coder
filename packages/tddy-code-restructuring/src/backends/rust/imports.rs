//! The import pass behind `extract_module`: restoring the names a relocated module lost.
//!
//! Split out of `backends/rust.rs`, which is past its size budget, and kept to the pass itself —
//! the loop, the choice of the next import, and the reading of what a `use` declaration binds. The
//! lexical helpers it shares with the pruning and facade passes stay beside those passes.

use serde_json::{json, Value};

use super::{
    alias_target, apply_lsp_edit, edits_for, facade_will_bind, group_members, import_order,
    module_bounds, occurrences_of, parent_binding, reached_through_qualifier, seam_refusal,
    server_defect, titled, use_tree, with_module_import, ModuleBlock, MovedItem, RustBackend,
    UnresolvedName, IMPORT_PASSES, IMPORT_TITLE,
};
use crate::plan::Reexport;
use crate::Result;

impl RustBackend {
    /// Import every name the relocated items lost, until the server reports none left to import.
    ///
    /// Extracting a module moves items away from the `use` declarations that gave their references
    /// meaning: the declarations stay in the parent and the names go unresolved in the new scope.
    /// rust-analyzer will not carry them across, but it will say which names it cannot resolve and
    /// what would resolve each one — so every path written here is still the server's own.
    ///
    /// One import per pass. Each inserts a `use` line that moves everything below it, and a name
    /// that looked unimportable often becomes resolvable once the name it hung off is restored.
    ///
    /// Every import is *verified* before it is kept: the trial text goes back to the server and the
    /// name it was meant to resolve has to stop being unresolved. rust-analyzer offers imports that
    /// resolve nothing — one for an inherent associated function, one naming a path two module levels
    /// too high — and the difference between a good and a useless offer is not readable from its
    /// title. Trusting the title wrote four `use` lines that did not compile across one real
    /// restructure, in a run that reported success.
    ///
    /// `original` is the file before the assist ran. It is evidence for choosing between offered
    /// paths, and the produced text is not enough of it: the assist drops a name from the parent's
    /// `use` group when the seam held its only use, so by the time a contested `mpsc` is asked about,
    /// the declaration that said which `mpsc` the moved code meant is gone.
    pub(super) fn restore_imports(
        &mut self,
        uri: &str,
        original: &str,
        extracted: &str,
        module: &str,
        moved: &[MovedItem],
        reexport: Reexport,
    ) -> Result<String> {
        let mut text = extracted.to_string();
        // Names every offered path failed. Re-asking one would be offered the same useless import
        // again, and every pass would insert another copy of it.
        let mut unimportable: Vec<String> = Vec::new();

        // Read once, off the file as it was: what was unresolved before the seam was cut is not
        // something the seam lost.
        self.did_change(uri, original)?;
        let already_unresolved = self
            .unresolved_names(uri, original)?
            .into_iter()
            .map(|found| found.text)
            .collect();
        let seam = Seam {
            uri,
            original,
            module,
            moved,
            reexport,
            already_unresolved,
        };

        for _ in 0..IMPORT_PASSES {
            self.did_change(uri, &text)?;

            match self.next_import(&text, &seam, &mut unimportable)? {
                Some(imported) => text = imported,
                None => return Ok(text),
            }
        }

        Err(server_defect(format!(
            "rust-analyzer was still offering imports after {IMPORT_PASSES} passes"
        )))
    }

    /// The text with one more import restored, or `None` once no name is left to import.
    ///
    /// A name with no import offered is skipped rather than refused: most of them are methods and
    /// fields that are unresolved only because their receiver's type is, and they come back on
    /// their own once it does. What is left when no import remains is for the compiler to judge.
    ///
    /// Only the names the seam lost are weighed — see [`Self::unresolved_the_seam_lost`]. A name
    /// the parent could not resolve before the cut is not one, and no `use` written into the module
    /// can resolve it there: asking about one is how a file whose alias the server could not see had
    /// the same line written into a module that named nothing, once a pass, until the backstop.
    ///
    /// A name the parent binds under an alias — `ProbeOutcome as ProtoProbeOutcome`, which is how
    /// every generated proto type in this workspace is referred to — is reconstructed from the
    /// parent's own declaration before the server is asked: rust-analyzer offers the *unaliased*
    /// path, which does not bind the alias, so its offer could only produce a `use` that resolves
    /// nothing. A name the server offers nothing for, reached through a bare module path, is
    /// reconstructed the same way. Both reconstructions write into the module, so they answer only
    /// for an occurrence inside it; one the parent lost is left to what the server offers there.
    fn next_import(
        &mut self,
        text: &str,
        seam: &Seam<'_>,
        unimportable: &mut Vec<String>,
    ) -> Result<Option<String>> {
        let mut asked: Vec<String> = Vec::new();
        let unresolved = self.unresolved_the_seam_lost(text, seam)?;
        let block = module_block(text, seam.module)?;

        for name in &unresolved {
            // One import serves every occurrence of a name, and a name that offered none here will
            // not offer one at its next occurrence either.
            if asked.contains(&name.text) || unimportable.contains(&name.text) {
                continue;
            }
            asked.push(name.text.clone());

            if no_use_can_bind(text, name, seam)? {
                unimportable.push(name.text.clone());
                continue;
            }

            let lost = Lost {
                name: &name.text,
                before: occurrences_of(&unresolved, &name.text),
            };
            let inside = within(&block, name);

            if let Some(declaration) = inside
                .then(|| aliased_declaration(text, seam.module, &name.text))
                .flatten()
            {
                return self
                    .reconstructed(text, seam, &lost, &declaration)
                    .map(Some);
            }

            let offers = self.offered_imports(seam.uri, &name.position)?;
            if !offers.titles.is_empty() {
                return self
                    .first_offer_that_resolves(text, seam, &lost, &offers)
                    .map(Some);
            }

            // rust-analyzer offers `Import` for items, not for a bare module path, so a name the
            // parent reached through `use crate::tool_engine;` is unresolved in the moved code with
            // nothing on offer for it — and skipping silently is how three modules landed
            // referencing an unlinked crate. The parent's own declaration says what it meant,
            // exactly as for an alias.
            if let Some(declaration) = inside
                .then(|| bound_declaration(text, seam.module, &name.text))
                .flatten()
            {
                return self
                    .reconstructed(text, seam, &lost, &declaration)
                    .map(Some);
            }
        }

        Ok(None)
    }

    /// The quick-fix imports rust-analyzer offers for the unresolved name at `position`.
    fn offered_imports(&mut self, uri: &str, position: &Value) -> Result<Offers> {
        let actions = self.request_settled(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": uri },
                "range": { "start": position, "end": position },
                "context": { "diagnostics": [], "only": ["quickfix"] }
            }),
        )?;

        let titles = actions
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|action| action.get("title").and_then(Value::as_str))
            .filter(|title| title.starts_with(IMPORT_TITLE))
            .map(str::to_string)
            .collect();

        Ok(Offers { actions, titles })
    }

    /// The text with the first offered import that leaves fewer of the lost name's occurrences
    /// unresolved, tried in the order the file's own imports make most likely.
    ///
    /// Refused when the file's imports cannot order the offers, and when none of them helps.
    fn first_offer_that_resolves(
        &mut self,
        text: &str,
        seam: &Seam<'_>,
        lost: &Lost<'_>,
        offers: &Offers,
    ) -> Result<String> {
        let ordered = import_order(&seam.evidence_with(text), &offers.titles).ok_or_else(|| {
            seam_refusal(format!(
                "`{}` could be imported {} ways and neither rust-analyzer nor this file's own \
                 imports say which the moved code meant: {}",
                lost.name,
                offers.titles.len(),
                offers.titles.join(", ")
            ))
        })?;

        for title in &ordered {
            let action = titled(&offers.actions, &title.to_lowercase())
                .ok_or_else(|| server_defect("the import offered could not be read back"))?;
            let resolved = self.request_settled("codeAction/resolve", action)?;
            let trial = apply_lsp_edit(text, edits_for(&resolved, seam.uri)?);

            if self.unresolved_after(&trial, seam, lost)? < lost.before {
                return Ok(trial);
            }
        }

        // Every path the server offered leaves the name unresolved. Writing one anyway is how a
        // successful run lands source that does not compile, so the operation says which name it
        // could not import and what it tried.
        Err(seam_refusal(format!(
            "no import rust-analyzer offered for `{}` left fewer of its {} unresolved \
             occurrence(s) — tried {}. Writing one anyway is how a run reports success over a \
             `use` that resolves nothing.",
            lost.name,
            lost.before,
            ordered.join(", ")
        )))
    }

    /// The text with the parent's own declaration of `name` written into the module, verified the
    /// way an offered import is: it has to leave fewer of `name`'s occurrences unresolved.
    ///
    /// Unverified, a declaration the server cannot resolve — an alias of a type generated into an
    /// `OUT_DIR` it has not loaded — changed nothing, the name was unresolved on the next pass
    /// exactly as on this one, and the same line was written again until the backstop. An import that
    /// makes no progress is therefore the end of the pass, refused by name: the seam can move only
    /// once the server resolves what the moved code names.
    fn reconstructed(
        &mut self,
        text: &str,
        seam: &Seam<'_>,
        lost: &Lost<'_>,
        declaration: &str,
    ) -> Result<String> {
        let trial = with_module_import(text, seam.module, declaration)?;

        let after = self.unresolved_after(&trial, seam, lost)?;
        if after < lost.before {
            return Ok(trial);
        }

        let (name, before) = (lost.name, lost.before);
        Err(seam_refusal(format!(
            "the moved code names `{name}`, and writing the parent's own `{declaration}` into the \
             module left {after} unresolved occurrence(s) of it, where there were {before}: \
             rust-analyzer cannot resolve what that declaration names. A type a build script generates reads this \
             way until the server has loaded the script's output. Cut the seam where the moved code \
             does not name `{name}`, or make its path resolve first."
        )))
    }

    /// How many of the lost name's occurrences the server still cannot resolve in `trial` — the
    /// count every import is verified against, and kept only when it is below `lost.before`.
    fn unresolved_after(&mut self, trial: &str, seam: &Seam<'_>, lost: &Lost<'_>) -> Result<usize> {
        self.did_change(seam.uri, trial)?;
        Ok(occurrences_of(
            &self.unresolved_the_seam_lost(trial, seam)?,
            lost.name,
        ))
    }

    /// The names the server cannot resolve that the seam lost, in source order.
    ///
    /// Every unresolved occurrence inside the module the assist wrote counts. One in the parent
    /// counts only for a name the file resolved everywhere before the cut: that is a reference the
    /// move stranded — a trait the parent still names bare, which the assist does not rewrite — and
    /// a `use` in the parent restores it. A name already unresolved before the cut was not lost here,
    /// and is not this pass's to answer for.
    fn unresolved_the_seam_lost(
        &mut self,
        text: &str,
        seam: &Seam<'_>,
    ) -> Result<Vec<UnresolvedName>> {
        let block = module_block(text, seam.module)?;

        Ok(self
            .unresolved_names(seam.uri, text)?
            .into_iter()
            .filter(|found| within(&block, found) || !seam.already_unresolved.contains(&found.text))
            .collect())
    }
}

/// Whether `name` is one no `use` written into the module can bind, so asking the server for an
/// import would only be offered one that looks right and resolves nothing.
///
/// - Reached through a qualifier: an associated item or a field, and no `use` binds either.
///   rust-analyzer offers one anyway — `use super::new_with_config;` for a constructor called as
///   `NativePDFContextManager::new_with_config`.
/// - Already bound in this module and still unresolved: the binding that exists is the broken one,
///   and a second is `E0252` however well its path reads.
/// - Re-exported by this seam's own facade. The facade is written *after* this pass, so the server
///   sees the name as unresolved and offers a path through the new module — and the named import it
///   writes is private, which then *shadows* the `pub use module::*;` added moments later. The
///   facade is left present and inert, and an outside caller gets `E0603` on a symbol the facade
///   was asked to keep reachable.
fn no_use_can_bind(text: &str, name: &UnresolvedName, seam: &Seam<'_>) -> Result<bool> {
    Ok(reached_through_qualifier(text, &name.position)
        || super::already_bound(text, seam.module, &name.text)?
        || facade_will_bind(&name.text, seam.moved, seam.reexport))
}

/// The parent's declaration binding `alias` under that alias, rewritten for the module.
///
/// The declaration is the parent's, and the module is the parent's *child*: a relative path in it
/// is rebased one level down first (see [`rebased_for_child`]).
fn aliased_declaration(text: &str, module: &str, alias: &str) -> Option<String> {
    alias_target(text, module, alias)
        .map(|path| format!("use {} as {alias};", rebased_for_child(&path)))
}

/// The parent's declaration binding `name` without an alias, rewritten for the module the same way
/// as [`aliased_declaration`].
fn bound_declaration(text: &str, module: &str, name: &str) -> Option<String> {
    parent_binding(text, module, name).map(|path| format!("use {};", rebased_for_child(&path)))
}

/// The bounds of `module`'s inline block in `text`.
fn module_block(text: &str, module: &str) -> Result<ModuleBlock> {
    let source: Vec<String> = text.split('\n').map(str::to_string).collect();
    module_bounds(&source, module)
}

/// Whether an unresolved name sits inside the module block, between its header and closing brace.
fn within(block: &ModuleBlock, found: &UnresolvedName) -> bool {
    found
        .position
        .get("line")
        .and_then(Value::as_u64)
        .is_some_and(|line| line > block.opened as u64 && line < block.closed as u64)
}

/// A name the seam lost, and how many of its occurrences were unresolved before an import was tried.
///
/// Counted rather than asked as a yes/no, because one name is routinely unresolved in several places
/// and only the occurrence an import was offered for is the one it can answer for.
struct Lost<'a> {
    name: &'a str,
    before: usize,
}

/// The code actions the server answered for one name, and the titles of the imports among them.
struct Offers {
    actions: Value,
    titles: Vec<String>,
}

/// What the import pass knows about the seam it is restoring names for.
struct Seam<'a> {
    /// The document the seam is cut in, as the server knows it.
    uri: &'a str,
    /// The file before the assist ran.
    original: &'a str,
    /// The name of the module the assist wrote.
    module: &'a str,
    moved: &'a [MovedItem],
    reexport: Reexport,
    /// Every name the server could not resolve anywhere in the file before the assist ran.
    already_unresolved: Vec<String>,
}

impl Seam<'_> {
    /// The text whose `use` declarations settle a contested import: the file as it was, then as it
    /// is now.
    ///
    /// Both, because each holds evidence the other lacks. The original still carries a binding the
    /// assist removed with the last use of it, and the current text carries every import earlier
    /// passes restored. Joined on a line break, which reads as two texts to `imported_paths`: the
    /// original cannot end inside a `use` declaration, so no statement straddles the join.
    fn evidence_with(&self, text: &str) -> String {
        format!("{}\n{text}", self.original)
    }
}

/// A path the parent's `use` declaration names, as the module the seam becomes has to write it.
///
/// That module is a **child** of the file's own, so a path relative to the file's module starts one
/// level too high there: the parent's `super::X` is `super::super::X` in the child, and its `self::X`
/// is `super::X`. Written verbatim, `use super::SplitStartFailure;` in a module under
/// `svc_spawn_split_agent` named `svc_spawn_split_agent::SplitStartFailure`, which does not exist.
///
/// A path from the crate root (`crate::`), an absolute one (`::`) and one through an extern crate
/// mean the same thing from any module, and are left alone. So is a bare path through an item the
/// parent declares (`sibling::X`, reached through 2018's uniform paths): it cannot be told from an
/// extern crate by reading, and the verification behind every reconstruction refuses it by name.
pub(super) fn rebased_for_child(path: &str) -> String {
    match path.split("::").next() {
        Some("super") => format!("super::{path}"),
        Some("self") => format!("super{}", &path["self".len()..]),
        _ => path.to_string(),
    }
}

/// Every name the text's `use` declarations bind, read the way the compiler reads a binding.
///
/// Not the last segment of each path, which is what [`super::imported_paths`] gives: `use a::B as
/// C;` binds `C` and not `B`, `use a::Trait as _;` binds nothing, and `use a::b::{self};` binds `b`.
/// Read as paths, the alias branch of the import pass never saw the line it had just written.
/// The function-local `use` items of the function spanning `origin_function` that bind any of
/// `names` — what an extracted function not nested in its origin must carry into its own body.
#[allow(dead_code)] // TODO(extraction-defects): the extract-method import pass carries these.
pub(super) fn function_local_uses_reaching(
    text: &str,
    origin_function: crate::edit::Range,
    names: &[String],
) -> Vec<String> {
    // TODO(extraction-defects): implement
    let _ = (text, origin_function, names);
    todo!("extraction-defects: the function-local uses an extraction must carry")
}

pub(super) fn names_bound(text: &str) -> Vec<String> {
    let mut names = Vec::new();

    for statement in text.split(';') {
        if let Some(tree) = use_tree(statement) {
            collect_bound(tree, "", &mut names);
        }
    }

    names
}

/// Walk a `use` tree, pushing the name each leaf binds.
fn collect_bound(tree: &str, prefix: &str, names: &mut Vec<String>) {
    let tree = tree.trim();

    let Some(open) = tree.find('{') else {
        let (path, alias) = match tree.split_once(" as ") {
            Some((path, alias)) => (path.trim(), Some(alias.trim())),
            None => (tree, None),
        };
        let bound = match alias {
            Some(alias) => alias,
            None if path == "self" => prefix
                .trim_end_matches("::")
                .rsplit("::")
                .next()
                .unwrap_or(""),
            None => path.rsplit("::").next().unwrap_or(""),
        };
        // `_` binds no name, and a glob binds none this can read.
        if !bound.is_empty() && bound != "_" && bound != "*" {
            names.push(bound.to_string());
        }
        return;
    };

    let head = format!("{prefix}{}", &tree[..open]);
    let close = tree.rfind('}').unwrap_or(tree.len());

    for member in group_members(&tree[open + 1..close]) {
        collect_bound(member, &head, names);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_function_local_use_binding_a_name_the_range_reaches_is_carried() {
        // Given a function whose body imports `BTreeMap` locally, and a range naming it
        let text = "fn build() -> usize {\n    use std::collections::BTreeMap;\n    \
                    let map: BTreeMap<u32, u32> = BTreeMap::new();\n    map.len()\n}\n";
        let function = crate::edit::Range {
            start: crate::edit::Position { line: 1, col: 1 },
            end: crate::edit::Position { line: 5, col: 2 },
        };

        // Then that `use` is what the extracted function carries
        assert_eq!(
            function_local_uses_reaching(text, function, &["BTreeMap".to_string()]),
            vec!["use std::collections::BTreeMap;".to_string()]
        );
    }

    #[test]
    fn a_function_local_use_binding_nothing_the_range_reaches_is_not_carried() {
        let text = "fn build() -> usize {\n    use std::collections::BTreeMap;\n    \
                    let n = 3;\n    n\n}\n";
        let function = crate::edit::Range {
            start: crate::edit::Position { line: 1, col: 1 },
            end: crate::edit::Position { line: 5, col: 2 },
        };

        assert_eq!(
            function_local_uses_reaching(text, function, &["n".to_string()]),
            Vec::<String>::new()
        );
    }

    use super::*;

    #[test]
    fn rebases_a_path_through_super_one_level_further_up() {
        // When
        let rebased = rebased_for_child("super::SplitStartFailure");

        // Then
        assert_eq!(rebased, "super::super::SplitStartFailure");
    }

    #[test]
    fn rebases_a_path_already_climbing_several_levels() {
        // When
        let rebased = rebased_for_child("super::super::config::Setting");

        // Then
        assert_eq!(rebased, "super::super::super::config::Setting");
    }

    #[test]
    fn rebases_a_path_through_self_onto_the_parent() {
        // When
        let rebased = rebased_for_child("self::proto::Event");

        // Then
        assert_eq!(rebased, "super::proto::Event");
    }

    #[test]
    fn leaves_a_path_from_the_crate_root_alone() {
        // When
        let rebased = rebased_for_child("crate::connection_service::hooks_and_urls");

        // Then
        assert_eq!(rebased, "crate::connection_service::hooks_and_urls");
    }

    #[test]
    fn leaves_an_absolute_path_alone() {
        // When
        let rebased = rebased_for_child("::std::sync::Arc");

        // Then
        assert_eq!(rebased, "::std::sync::Arc");
    }

    #[test]
    fn leaves_a_path_through_an_extern_crate_alone() {
        // When
        let rebased = rebased_for_child("tddy_rpc::Status");

        // Then
        assert_eq!(rebased, "tddy_rpc::Status");
    }

    #[test]
    fn leaves_a_segment_that_only_starts_with_super_alone() {
        // When
        let rebased = rebased_for_child("superset::Thing");

        // Then
        assert_eq!(rebased, "superset::Thing");
    }

    /// The reconstruction reads a grouped declaration as one flat path per member, so rebasing the
    /// flat path rebases the member.
    #[test]
    fn rebases_a_member_of_a_group_through_super() {
        // Given
        let text =
            "use super::{AttachmentMaterialization, SplitStartFailure};\n\nmod teardown {\n}\n";

        // When
        let rebased = parent_binding(text, "teardown", "SplitStartFailure")
            .map(|path| rebased_for_child(&path));

        // Then
        assert_eq!(rebased.as_deref(), Some("super::super::SplitStartFailure"));
    }

    #[test]
    fn rebases_an_aliased_member_of_a_group_through_self() {
        // Given
        let text = "use self::{proto::Event as StartSessionEventKind, other::Thing};\n\nmod readings {\n}\n";

        // When
        let rebased = alias_target(text, "readings", "StartSessionEventKind")
            .map(|path| rebased_for_child(&path));

        // Then
        assert_eq!(rebased.as_deref(), Some("super::proto::Event"));
    }
}
