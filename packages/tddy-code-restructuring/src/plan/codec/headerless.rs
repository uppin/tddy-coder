//! A plan whose first line is an operation, and the header `snapshot` writes for it.
//!
//! `Plan::parse` stays strict: a plan without a header is refused by every reader, and routing
//! (`plan_file_has_item_anchors`) relies on that to treat it as "no item anchors". The tolerant
//! read lives here, and only `runner::snapshot` calls it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use super::super::malformed;
use super::{parse_ops, Plan};
use crate::Anchor;
use crate::Result;

impl Plan {
    /// Whether the first non-blank line of `jsonl` is an operation rather than a header: a JSON
    /// object with an `op` key and none of `v`, `snapshot`, `files`.
    pub(crate) fn starts_with_an_operation(jsonl: &str) -> bool {
        let Some(first) = jsonl.lines().find(|line| !line.trim().is_empty()) else {
            return false;
        };
        let Ok(serde_json::Value::Object(object)) = serde_json::from_str(first) else {
            return false;
        };
        object.contains_key("op")
            && !["v", "snapshot", "files"]
                .iter()
                .any(|key| object.contains_key(*key))
    }

    /// Read a plan with no header: every non-blank line is an operation, so every operation
    /// refusal `check` would raise surfaces here too. The plan is a v2 plan with no hints.
    pub(crate) fn parse_headerless(jsonl: &str) -> Result<Plan> {
        let ops = parse_ops(jsonl.lines().filter(|line| !line.trim().is_empty()))?;
        Ok(Plan {
            version: super::HINTED_SCHEMA_VERSION,
            snapshot: Default::default(),
            files: Default::default(),
            ops,
        })
    }

    /// The files this plan's operations anchor, primary and `also`, de-duplicated and sorted.
    pub(crate) fn anchored_files(&self) -> BTreeSet<&str> {
        self.ops
            .iter()
            .flat_map(|op| op.anchors())
            .map(|anchor| anchor.file())
            .collect()
    }

    /// The header line for a plan that has none, computed from the files its anchors name under
    /// `root` and validated before anything is written.
    ///
    /// Serialised exactly as [`Plan::rehashed_header`] serialises a headed plan, so a second
    /// `snapshot` of the plan this wrote produces the same line and rewrites nothing.
    pub(crate) fn header_for_anchored_files(&self, root: &Path) -> Result<String> {
        let files = self.anchored_files();

        // Decision O1: a plan whose every anchor is an item, or a run of items, gets the v2 header,
        // whose hints never refuse a run. A range or symbol anchor names coordinates that depend on
        // the whole file, so it gets the v1 header, whose snapshot refuses on drift — the promise
        // the anchor actually makes.
        let all_by_item = self
            .ops
            .iter()
            .flat_map(|op| op.anchors())
            .all(|anchor| matches!(anchor, Anchor::Item { .. } | Anchor::Items { .. }));

        if all_by_item {
            let mut hints = BTreeMap::new();
            for file in &files {
                hints.insert(
                    (*file).to_string(),
                    super::hint_of(&anchored_file_path(root, file)?)?,
                );
            }
            return serde_json::to_string(&super::HintedHeader {
                v: super::HINTED_SCHEMA_VERSION,
                files: hints,
            })
            .map_err(|error| malformed(error.to_string()));
        }

        let mut snapshot = BTreeMap::new();
        for file in &files {
            snapshot.insert(
                (*file).to_string(),
                crate::apply::hash_file(&anchored_file_path(root, file)?)?,
            );
        }
        serde_json::to_string(&super::SnapshotHeader {
            v: super::SCHEMA_VERSION,
            snapshot,
        })
        .map_err(|error| malformed(error.to_string()))
    }
}

/// The workspace path an anchor's `file` names: refused when the name is empty, when it leaves the
/// workspace, or when it is not a regular file.
fn anchored_file_path(root: &Path, file: &str) -> Result<PathBuf> {
    if file.is_empty() {
        return Err(malformed(
            "an anchor names no file: the header lists the files a plan was written against",
        ));
    }
    let relative = Path::new(file);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(malformed(format!(
            "`{file}` is outside the workspace: an anchor's `file` is relative to the workspace root"
        )));
    }
    let path = root.join(relative);
    // `hash_file` reads a missing file as empty, so a v1 header would name a digest that is not the
    // file's. Refused here, before the header is computed, so a name that is not a file never
    // reaches either serialisation.
    if !path.is_file() {
        return Err(malformed(format!("{} is not a file", path.display())));
    }
    Ok(path)
}
