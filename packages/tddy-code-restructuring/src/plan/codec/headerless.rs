//! A plan whose first line is an operation, and the header `snapshot` writes for it.
//!
//! `Plan::parse` stays strict: a plan without a header is refused by every reader, and routing
//! (`plan_file_has_item_anchors`) relies on that to treat it as "no item anchors". The tolerant
//! read lives here, and only `runner::snapshot` calls it.

use std::collections::BTreeSet;

use super::super::malformed;
use super::{parse_ops, Plan};
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
    pub(crate) fn header_for_anchored_files(&self, root: &std::path::Path) -> Result<String> {
        // TODO(plan-header): implement. Validate each anchored file (relative, no `..`, a regular
        // file), pick the version (Decision O1), and serialise through `hint_of` /
        // `hash_file` as `rehashed_header` does. Until then a headerless `snapshot` refuses.
        let files = self.anchored_files();
        Err(malformed(format!(
            "snapshot cannot yet write the header of a plan that has none: {} anchored file(s) \
             under {} (TODO(plan-header): implement)",
            files.len(),
            root.display()
        )))
    }
}
