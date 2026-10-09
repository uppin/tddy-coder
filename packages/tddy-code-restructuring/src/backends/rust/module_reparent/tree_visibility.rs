//! The visibilities written in the files of a moved module, read for where they now sit.
//!
//! A keyword is a spelling: `pub(super)` in the module's own file meant the old parent, and
//! `pub(in crate::host::attachments)` names a path that no longer exists once the tree moves. Each
//! one is read as the module subtree it covered at its old place. A subtree inside the moved tree
//! travels with it (an absolute path to it is re-pointed with the callers); one that reached outside
//! the tree keeps everything it covered and is widened until it is legal where the file now sits, so
//! no caller that could see the item before loses it. No server is asked: the text answers it.

use std::collections::BTreeMap;

use super::super::early_return::masked_to_code;
use super::super::item_move::rebase::visibility_spans;
use super::super::item_move::scope::Scope;
use super::super::item_move::text::{enclosing_modules, Edit};
use super::super::seam_refusal;
use super::reading::Reparent;
use super::survey::Survey;
use super::tree_reach::Widened;
use crate::edit::VisibilityChange;
use crate::Result;

/// The words that introduce a declaration whose name follows.
const DECLARATIONS: [&str; 9] = [
    "fn", "struct", "enum", "trait", "type", "const", "static", "mod", "union",
];

/// The edits to the moved files (keyed by each file's path before the move) that keep every
/// visibility written in them covering what it covered, and the report of each widening.
///
/// `pub` and `pub(crate)` are never touched, and neither is a scope inside the moved tree: a
/// relative one reads the same where the tree sits now, and an absolute `pub(in crate::<tree>)` is
/// rewritten by the callers' re-pointing, because rust-analyzer reports the module's name in it as a
/// reference. A visibility this cannot read is refused naming the
/// file and the line.
pub(super) fn respelled(
    texts: &BTreeMap<String, String>,
    request: &Reparent,
    survey: &Survey,
) -> Result<Widened> {
    let (old_tree, new_tree) = (request.module_path(), new_tree_of(request, survey));
    let mut edits = Vec::new();
    let mut report = Vec::new();
    for file in &survey.files {
        let text = &texts[&file.from];
        let masked = masked_to_code(text);
        for span in visibility_spans(&masked, &(0..masked.len())) {
            let written = &text[span.clone()];
            if written == "pub(crate)" {
                continue;
            }
            let inside = enclosing_modules(text, span.start);
            let old_at = [old_tree.as_slice(), &file.below, &inside].concat();
            let old_scope = Scope::parse(written, &old_at).ok_or_else(|| {
                let line = text[..span.start].matches('\n').count() + 1;
                seam_refusal(format!(
                    "{}:{line}: the visibility `{written}` is not one this move can read",
                    file.from
                ))
            })?;
            if matches!(&old_scope, Scope::Within(path) if path.starts_with(&old_tree)) {
                continue;
            }
            let new_at = [new_tree.as_slice(), &file.below, &inside].concat();
            let spelled = old_scope.widened_to(&new_at).spelled_in(&new_at);
            if spelled == written {
                continue;
            }
            report.push(VisibilityChange {
                item: declared_after(&masked[span.end..]),
                from: written.to_string(),
                to: spelled.clone(),
                reason: None,
            });
            edits.push((file.from.clone(), Edit::replace(span, spelled)));
        }
    }
    Ok((edits, report))
}

/// The moved module's path below the crate root once it sits under its new parent.
fn new_tree_of(request: &Reparent, survey: &Survey) -> Vec<String> {
    let mut path = survey.new_parent.path.clone();
    path.push(request.name.clone());
    path
}

/// What a declaration is called, read from the rest of its line after the visibility: the
/// identifier after the declaration keyword, or the first identifier of a field.
fn declared_after(rest: &str) -> String {
    let line = rest.lines().next().unwrap_or_default();
    let words: Vec<&str> = line
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .collect();
    let named = words.windows(2).find_map(|pair| {
        (DECLARATIONS.contains(&pair[0]) && !DECLARATIONS.contains(&pair[1])).then_some(pair[1])
    });
    named
        .or_else(|| words.first().copied())
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::super::item_move::text::{applied, Edit};
    use super::super::{reading, survey};
    use super::*;
    use crate::edit::VisibilityChange;
    use crate::overlay::Overlay;
    use crate::plan::RefactorOp;
    use crate::registry::Workspace;

    const THE_MANIFEST: &str =
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

    /// An `app` package holding `files` beside its manifest, on disk.
    fn an_app_holding(files: &[(&str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("a temporary directory");
        for (path, text) in std::iter::once(&("Cargo.toml", THE_MANIFEST)).chain(files) {
            let file = root.path().join(path);
            std::fs::create_dir_all(file.parent().expect("a parent directory"))
                .expect("the directory is created");
            std::fs::write(file, text).expect("the file is written");
        }
        root
    }

    /// `host::attachments` re-parented under `split`, as the plan line says it.
    fn moving_attachments_under_split() -> RefactorOp {
        serde_json::from_value(serde_json::json!({
            "op": "reparent_module",
            "anchor": {
                "kind": "items",
                "file": "src/host.rs",
                "items": ["app::host::attachments"],
                "fingerprints": ["sha256:0"],
            },
            "to": "app::split",
        }))
        .expect("a `reparent_module` line parses")
    }

    /// What `respelled` makes of the tree, as the new text of every moved file it changes.
    fn the_respelled_tree(
        root: &Path,
    ) -> Result<(BTreeMap<String, String>, Vec<VisibilityChange>)> {
        let overlay = Overlay::default();
        let workspace = Workspace {
            root,
            overlay: &overlay,
        };
        let request = reading::read(&workspace, &moving_attachments_under_split())?;
        let survey::Reading::Clear(survey) = survey::survey(&workspace, &request)? else {
            panic!("the fixture's move is not obstructed");
        };
        let texts: BTreeMap<String, String> = survey
            .files
            .iter()
            .map(|file| {
                (
                    file.from.clone(),
                    workspace.read(&file.from).expect("readable"),
                )
            })
            .collect();
        let (edits, report) = respelled(&texts, &request, &survey)?;
        let mut changed = BTreeMap::new();
        for (path, text) in &texts {
            let here: Vec<Edit> = edits
                .iter()
                .filter(|(file, _)| file == path)
                .map(|(_, edit)| edit.clone())
                .collect();
            let new = applied(text, &here)?;
            if &new != text {
                changed.insert(path.clone(), new);
            }
        }
        Ok((changed, report))
    }

    const LIB: &str = "pub mod host;\npub mod split;\n";
    const HOST: &str =
        "pub mod attachments;\n\npub fn host_name() -> u32 {\n    attachments::materialize()\n}\n";
    const SPLIT: &str = "pub fn start() -> u32 {\n    0\n}\n";

    #[test]
    fn widens_a_scope_that_reached_outside_the_tree_and_leaves_an_absolute_path_into_it_to_the_re_pointing(
    ) {
        // Given a module whose own `pub(super)` meant its old parent, and a child whose
        // `pub(in …)` names the moved module by its absolute path
        let root = an_app_holding(&[
            ("src/lib.rs", LIB),
            ("src/host.rs", HOST),
            (
                "src/host/attachments.rs",
                "pub mod staging;\n\npub(super) fn materialize() -> u32 {\n    staging::stage()\n}\n",
            ),
            (
                "src/host/attachments/staging.rs",
                "pub(in crate::host::attachments) fn stage() -> u32 {\n    2\n}\n",
            ),
            ("src/split.rs", SPLIT),
        ]);

        // When the tree's visibilities are respelled for `split::attachments`
        let (changed, report) = the_respelled_tree(root.path()).expect("the tree is respelled");

        // Then the `pub(super)` keeps the old parent seeing it by reaching the crate. The absolute
        // path is not this pass's: rust-analyzer reports the module's name inside it as a
        // reference, so the callers' re-pointing already rewrites it, and a second edit of the same
        // bytes would be refused as an overlap
        assert_eq!(
            changed,
            BTreeMap::from([(
                "src/host/attachments.rs".to_string(),
                "pub mod staging;\n\npub(crate) fn materialize() -> u32 {\n    staging::stage()\n}\n"
                    .to_string()
            )])
        );
        assert_eq!(
            report,
            [VisibilityChange {
                item: "materialize".to_string(),
                from: "pub(super)".to_string(),
                to: "pub(crate)".to_string(),
                reason: None,
            }]
        );
    }

    #[test]
    fn leaves_pub_pub_crate_and_a_childs_pub_super_untouched_and_refuses_an_unreadable_visibility_naming_its_line(
    ) {
        // Given a tree whose visibilities mean the same wherever it sits
        let untouched = an_app_holding(&[
            ("src/lib.rs", LIB),
            ("src/host.rs", HOST),
            (
                "src/host/attachments.rs",
                "pub mod staging;\n\npub fn materialize() -> u32 {\n    staging::stage()\n}\n\npub(crate) fn audit() {}\n",
            ),
            (
                "src/host/attachments/staging.rs",
                "pub(super) fn stage() -> u32 {\n    2\n}\n",
            ),
            ("src/split.rs", SPLIT),
        ]);
        // And one whose visibility climbs above the crate root
        let unreadable = an_app_holding(&[
            ("src/lib.rs", LIB),
            ("src/host.rs", HOST),
            (
                "src/host/attachments.rs",
                "pub fn materialize() -> u32 {\n    1\n}\n\npub(in super::super::super) fn lost() {}\n",
            ),
            ("src/split.rs", SPLIT),
        ]);

        // When each tree's visibilities are respelled
        let (changed, report) = the_respelled_tree(untouched.path()).expect("nothing to refuse");
        let refused = the_respelled_tree(unreadable.path())
            .expect_err("a visibility above the crate root is refused");

        // Then nothing in the first changes, and the refusal names the file and the line
        assert_eq!(changed, BTreeMap::new());
        assert_eq!(report, []);
        let said = refused.to_string();
        assert!(
            said.contains("src/host/attachments.rs:5"),
            "the refusal does not name the file and line: {said}"
        );
    }
}
