//! The import pass behind `extract_module`: restoring the names a relocated module lost.
//!
//! Split out of `backends/rust.rs`, which is past its size budget, and kept to the pass itself —
//! the loop, the choice of the next import, and the reading of what a `use` declaration binds. The
//! lexical helpers it shares with the pruning and facade passes stay beside those passes.

use serde_json::{json, Value};

use super::{
    alias_target, apply_lsp_edit, edits_for, facade_will_bind, group_members, import_order,
    module_bounds, occurrences_of, parent_binding, reached_through_qualifier, seam_refusal,
    server_defect, titled, use_tree, with_module_import, MovedItem, RustBackend, UnresolvedName,
    IMPORT_PASSES, IMPORT_TITLE,
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
    pub(super) fn restore_imports(
        &mut self,
        uri: &str,
        extracted: &str,
        module: &str,
        moved: &[MovedItem],
        reexport: Reexport,
    ) -> Result<String> {
        let mut text = extracted.to_string();
        // Names every offered path failed. Re-asking one would be offered the same useless import
        // again, and every pass would insert another copy of it.
        let mut unimportable: Vec<String> = Vec::new();

        for _ in 0..IMPORT_PASSES {
            self.did_change(uri, &text)?;

            match self.next_import(uri, &text, module, moved, reexport, &mut unimportable)? {
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
    /// Only the new module's names are weighed. A name the server cannot resolve in the parent was
    /// not lost by this seam, and no `use` written into the module can resolve it there — asking
    /// about one is how a file whose alias the server could not see had the same line written into
    /// a module that named nothing, once a pass, until the backstop.
    #[allow(clippy::too_many_arguments)]
    fn next_import(
        &mut self,
        uri: &str,
        text: &str,
        module: &str,
        moved: &[MovedItem],
        reexport: Reexport,
        unimportable: &mut Vec<String>,
    ) -> Result<Option<String>> {
        let mut asked: Vec<String> = Vec::new();
        let unresolved = self.unresolved_in_module(uri, text, module)?;

        for name in &unresolved {
            // One import serves every occurrence of a name, and a name that offered none here will
            // not offer one at its next occurrence either.
            if asked.contains(&name.text) || unimportable.contains(&name.text) {
                continue;
            }
            asked.push(name.text.clone());

            // A name reached through a qualifier is an associated item or a field, and no `use`
            // binds either. rust-analyzer offers one anyway — `use super::new_with_config;` for a
            // constructor called as `NativePDFContextManager::new_with_config` — and that import
            // resolves nothing while looking exactly like a good one.
            if reached_through_qualifier(text, &name.position) {
                unimportable.push(name.text.clone());
                continue;
            }

            // Already bound in this module and still unresolved: the binding that exists is the
            // broken one, and a second is `E0252` however well its path reads.
            if super::already_bound(text, module, &name.text)? {
                unimportable.push(name.text.clone());
                continue;
            }

            // A name this seam's own facade will re-export. The facade is written *after* this
            // pass, so the server sees the name as unresolved and offers a path through the new
            // module — and the named import it writes is private, which then *shadows* the
            // `pub use module::*;` added moments later. The facade is left present and inert, and
            // an outside caller gets `E0603` on a symbol the facade was asked to keep reachable.
            if facade_will_bind(&name.text, moved, reexport) {
                unimportable.push(name.text.clone());
                continue;
            }

            // How many occurrences the import has to account for. Counted rather than asked as a
            // yes/no, because one name is routinely unresolved in several places and only the
            // occurrence this import was offered for is the one it can answer for.
            let before = occurrences_of(&unresolved, &name.text);

            // A name the parent binds under an alias — `ProbeOutcome as ProtoProbeOutcome`, which
            // is how every generated proto type in this workspace is referred to. rust-analyzer
            // offers the *unaliased* path, which does not bind the alias, so asking the server can
            // only produce a `use` that resolves nothing and the run then refuses. The parent's own
            // declaration already says what the moved code meant, so reconstruct it from there.
            if let Some(path) = alias_target(text, module, &name.text) {
                let declaration = format!("use {path} as {};", name.text);
                return self
                    .reconstructed(uri, text, module, &name.text, before, &declaration)
                    .map(Some);
            }

            let actions = self.request_settled(
                "textDocument/codeAction",
                json!({
                    "textDocument": { "uri": uri },
                    "range": { "start": name.position, "end": name.position },
                    "context": { "diagnostics": [], "only": ["quickfix"] }
                }),
            )?;

            let offered: Vec<String> = actions
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|action| action.get("title").and_then(Value::as_str))
                .filter(|title| title.starts_with(IMPORT_TITLE))
                .map(str::to_string)
                .collect();

            if offered.is_empty() {
                // rust-analyzer offers `Import` for items, not for a bare module path, so a name
                // the parent reached through `use crate::tool_engine;` is unresolved in the moved
                // code with nothing on offer for it — and skipping silently is how three modules
                // landed referencing an unlinked crate. The parent's own declaration says what it
                // meant, exactly as for an alias.
                if let Some(path) = parent_binding(text, module, &name.text) {
                    let declaration = format!("use {path};");
                    return self
                        .reconstructed(uri, text, module, &name.text, before, &declaration)
                        .map(Some);
                }
                continue;
            }

            let ordered = import_order(text, &offered).ok_or_else(|| {
                seam_refusal(format!(
                    "`{}` could be imported {} ways and neither rust-analyzer nor this file's own \
                     imports say which the moved code meant: {}",
                    name.text,
                    offered.len(),
                    offered.join(", ")
                ))
            })?;

            for title in &ordered {
                let action = titled(&actions, &title.to_lowercase())
                    .ok_or_else(|| server_defect("the import offered could not be read back"))?;
                let resolved = self.request_settled("codeAction/resolve", action)?;
                let trial = apply_lsp_edit(text, edits_for(&resolved, uri)?);

                self.did_change(uri, &trial)?;

                let after =
                    occurrences_of(&self.unresolved_in_module(uri, &trial, module)?, &name.text);
                if after < before {
                    return Ok(Some(trial));
                }
            }

            // Every path the server offered leaves the name unresolved. Writing one anyway is how a
            // successful run lands source that does not compile, so the operation says which name it
            // could not import and what it tried.
            return Err(seam_refusal(format!(
                "no import rust-analyzer offered for `{}` left fewer of its {} unresolved \
                 occurrence(s) — tried {}. Writing one anyway is how a run reports success over a \
                 `use` that resolves nothing.",
                name.text,
                before,
                ordered.join(", ")
            )));
        }

        Ok(None)
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
        uri: &str,
        text: &str,
        module: &str,
        name: &str,
        before: usize,
        declaration: &str,
    ) -> Result<String> {
        let trial = with_module_import(text, module, declaration)?;
        self.did_change(uri, &trial)?;

        let after = occurrences_of(&self.unresolved_in_module(uri, &trial, module)?, name);
        if after < before {
            return Ok(trial);
        }

        Err(seam_refusal(format!(
            "the moved code names `{name}`, and writing the parent's own `{declaration}` into the \
             module left {after} unresolved occurrence(s) of it, where there were {before}: \
             rust-analyzer cannot resolve what that declaration names. A type a build script generates reads this \
             way until the server has loaded the script's output. Cut the seam where the moved code \
             does not name `{name}`, or make its path resolve first."
        )))
    }

    /// The names the server cannot resolve inside the module the assist wrote, in source order.
    fn unresolved_in_module(
        &mut self,
        uri: &str,
        text: &str,
        module: &str,
    ) -> Result<Vec<UnresolvedName>> {
        let source: Vec<String> = text.split('\n').map(str::to_string).collect();
        let block = module_bounds(&source, module)?;
        let inside = |found: &UnresolvedName| {
            found
                .position
                .get("line")
                .and_then(Value::as_u64)
                .is_some_and(|line| line > block.opened as u64 && line < block.closed as u64)
        };

        Ok(self
            .unresolved_names(uri, text)?
            .into_iter()
            .filter(inside)
            .collect())
    }
}

/// Every name the text's `use` declarations bind, read the way the compiler reads a binding.
///
/// Not the last segment of each path, which is what [`super::imported_paths`] gives: `use a::B as
/// C;` binds `C` and not `B`, `use a::Trait as _;` binds nothing, and `use a::b::{self};` binds `b`.
/// Read as paths, the alias branch of the import pass never saw the line it had just written.
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
