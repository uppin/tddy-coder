use serde::{Deserialize, Serialize};

use super::Anchor;

use super::Reexport;

use super::RefactorKind;

use super::RefactorOp;

use super::OpId;

use super::malformed;

use crate::Result;

use super::Plan;

use super::FileHint;

use std::collections::BTreeMap;

/// Schema version this executor understands.
const SCHEMA_VERSION: u32 = 1;

/// The schema whose header carries per-file hints instead of refusing snapshot hashes.
const HINTED_SCHEMA_VERSION: u32 = 2;

/// Fields that would carry source text. Their presence means the plan is trying to supply code the
/// language engine should have produced, so the plan is refused rather than partially honoured.
const CODE_BEARING_FIELDS: [&str; 3] = ["text", "code", "content"];

#[derive(Serialize, Deserialize)]
struct SnapshotHeader {
    pub(crate) v: u32,
    pub(crate) snapshot: BTreeMap<String, String>,
}

/// The header of a schema-v2 plan: per-file hints instead of a snapshot that refuses.
#[derive(Serialize, Deserialize)]
struct HintedHeader {
    pub(crate) v: u32,
    pub(crate) files: BTreeMap<String, FileHint>,
}

impl Plan {
    /// Parse a JSONL plan: line 1 is the snapshot header, every later line is one operation.
    pub fn parse(jsonl: &str) -> Result<Plan> {
        let mut lines = jsonl.lines().filter(|line| !line.trim().is_empty());

        let first_line = lines.next().ok_or_else(|| malformed("plan is empty"))?;
        if header_version(first_line) == Some(HINTED_SCHEMA_VERSION) {
            let header: HintedHeader = serde_json::from_str(first_line).map_err(|_| {
                malformed("first line must be a v2 header carrying a `files` map of hints")
            })?;
            let ops = parse_ops(lines)?;
            return Ok(Plan {
                version: header.v,
                snapshot: BTreeMap::new(),
                files: header.files,
                ops,
            });
        }
        let header: SnapshotHeader = serde_json::from_str(first_line).map_err(|_| {
            // A plan whose first line is an operation is only missing its header, and `snapshot`
            // writes one; naming it is what turns a dead end into a next step. A first line that
            // is neither header nor operation is not something that command can help with, so it
            // keeps the plain refusal.
            if Plan::starts_with_an_operation(first_line) {
                malformed(
                    "first line must be a snapshot header; this plan's first line is an operation, \
                     so run `restructure snapshot <plan>` to write one",
                )
            } else {
                malformed("first line must be a snapshot header")
            }
        })?;
        if header.v != SCHEMA_VERSION {
            return Err(malformed(format!(
                "plan declares schema version {} but this executor speaks {SCHEMA_VERSION}",
                header.v
            )));
        }

        let ops = parse_ops(lines)?;

        Ok(Plan {
            version: header.v,
            snapshot: header.snapshot,
            files: BTreeMap::new(),
            ops,
        })
    }

    /// This plan's snapshot header line, with every path it names hashed as the working tree under
    /// `root` holds it now.
    ///
    /// Only the paths the header already names. Adding the files the operations touch would be
    /// inventing a claim the author never made: the header is their statement of which files they
    /// wrote the plan against, and [`Plan::verify_snapshot`] refuses the run when one of them has
    /// moved since. What this produces is that statement, restated about the tree as it stands.
    pub fn rehashed_header(&self, root: &std::path::Path) -> Result<String> {
        if self.version == HINTED_SCHEMA_VERSION {
            let mut files = BTreeMap::new();
            for path in self.files.keys() {
                files.insert(path.clone(), hint_of(&root.join(path))?);
            }
            return serde_json::to_string(&HintedHeader {
                v: self.version,
                files,
            })
            .map_err(|error| malformed(error.to_string()));
        }

        let mut snapshot = BTreeMap::new();
        for path in self.snapshot.keys() {
            snapshot.insert(path.clone(), crate::apply::hash_file(&root.join(path))?);
        }

        serde_json::to_string(&SnapshotHeader {
            v: self.version,
            snapshot,
        })
        .map_err(|error| malformed(error.to_string()))
    }

    /// The files a v2 header hinted at whose content no longer hashes to the hint.
    ///
    /// Reported, never refused: an item anchor survives an edit outside its item, so a drifted file
    /// says only that the plan was written against an older tree. A v1 plan has no hints and
    /// reports nothing — its snapshot refuses through [`Plan::verify_snapshot`].
    ///
    /// A hinted file that has been deleted, moved, or can no longer be read has drifted too — it is
    /// the plainest case of "older tree" — so it is reported, never an error. (`hash_file` alone
    /// would read a missing file as empty, and so miss one whose hint was of an empty file.)
    pub fn drifted_hints(&self, root: &std::path::Path) -> Vec<String> {
        self.files
            .iter()
            .filter(|(path, hint)| {
                let file = root.join(path);
                !file.is_file()
                    || crate::apply::hash_file(&file).map_or(true, |hash| hash != hint.sha256)
            })
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// This plan as JSONL: the header its schema version writes, then one line per operation in
    /// order — what the plan store flushes back to disk.
    ///
    /// A field an operation left at its default is not written, so an operation reads back the way
    /// it was authored. Key order and whitespace are this crate's, not the author's.
    pub fn to_jsonl(&self) -> String {
        let header = if self.version == HINTED_SCHEMA_VERSION {
            serde_json::to_string(&HintedHeader {
                v: self.version,
                files: self.files.clone(),
            })
        } else {
            serde_json::to_string(&SnapshotHeader {
                v: self.version,
                snapshot: self.snapshot.clone(),
            })
        }
        .expect("a header of strings serialises");

        let mut text = header;
        text.push('\n');
        for op in &self.ops {
            text.push_str(&serde_json::to_string(op).expect("an operation serialises"));
            text.push('\n');
        }
        text
    }

    /// The first id that two operations of this plan share, if any.
    pub fn repeated_op_id(&self) -> Option<&OpId> {
        let mut seen = std::collections::BTreeSet::new();
        self.ops
            .iter()
            .filter_map(|op| op.id.as_ref())
            .find(|id| !seen.insert(*id))
    }

    /// Give every operation without an id one, and say whether any was given.
    ///
    /// Ids are `op-<n>`, numbered past the highest the plan already uses, so an operation inserted
    /// into a plan that has been loaded before never takes the id of one that was removed from the
    /// middle of it.
    pub fn assign_missing_op_ids(&mut self) -> bool {
        let mut next = self
            .ops
            .iter()
            .filter_map(|op| op.id.as_ref())
            .filter_map(|id| id.0.strip_prefix("op-")?.parse::<u64>().ok())
            .max()
            .map_or(1, |highest| highest + 1);

        let mut assigned = false;
        for op in self.ops.iter_mut().filter(|op| op.id.is_none()) {
            op.id = Some(OpId(format!("op-{next}")));
            next += 1;
            assigned = true;
        }
        assigned
    }

    /// Verify every snapshot hash still matches the working tree. Fails loudly on drift.
    pub fn verify_snapshot(&self, root: &std::path::Path) -> Result<()> {
        for (path, expected) in &self.snapshot {
            let actual = crate::apply::hash_file(&root.join(path))?;
            if &actual != expected {
                return Err(crate::RestructureError::SnapshotMismatch {
                    path: path.clone(),
                    expected: expected.clone(),
                    actual,
                });
            }
        }
        Ok(())
    }
}

// TODO(sharpen): lint correction after the apply — rfc3339's only outside user is plan.rs's test module (changeset D4, approved 2026-10-06).
pub(crate) use crate::plan::codec::file_hint::hint_of;
#[cfg(test)]
pub(crate) use crate::plan::codec::file_hint::rfc3339;
use crate::plan::codec::groups::refuse_split_groups;

/// The `v` a header line declares, read before the header's shape is known.
fn header_version(line: &str) -> Option<u32> {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()?
        .get("v")?
        .as_u64()
        .map(|v| v as u32)
}

/// Every operation line, in order, with each group's members checked to be consecutive.
fn parse_ops<'a>(lines: impl Iterator<Item = &'a str>) -> Result<Vec<RefactorOp>> {
    let ops = lines.map(parse_op).collect::<Result<Vec<_>>>()?;
    refuse_split_groups(&ops)?;
    Ok(ops)
}

fn parse_op(line: &str) -> Result<RefactorOp> {
    let raw: serde_json::Value =
        serde_json::from_str(line).map_err(|error| malformed(error.to_string()))?;

    // Checked before the operation kind, so a code-bearing line reports the reason that matters
    // rather than merely that its name is unrecognised.
    if let Some(object) = raw.as_object() {
        for field in CODE_BEARING_FIELDS {
            if object.contains_key(field) {
                return Err(crate::RestructureError::CodeTextInPlan {
                    field: field.to_string(),
                });
            }
        }
    }

    // A `repoint_facade_imports` line carries only an anchor, and a field it must not carry is
    // refused by name before the line is deserialized — so a value that is no `Reexport` names
    // `reexport` rather than an unknown variant.
    facade_imports_fields::refuse_a_facade_repoint_it_cannot_honour(&raw)?;

    let op: RefactorOp =
        serde_json::from_value(raw).map_err(|error| malformed(error.to_string()))?;

    for anchor in op.anchors() {
        anchor.validate()?;
    }

    // Silently ignoring the field would be worse than refusing it: the plan author asked for a
    // facade, would not get one, and would read the resulting stranded-reference refusal as the
    // facade having failed to help.
    // `move_module_to_crate` writes the same kind of facade one level up — `pub use <crate>::…;` in
    // the crate the module left — so it honours the field for the same reason and with the same
    // failure mode if the field were ignored.
    //
    // A test binary can have no facade at all, and the reason is worth its own refusal rather than
    // the generic one above: a facade exists to keep a *caller* resolving, and **nothing can
    // reference a test binary**. Cargo builds each `tests/*.rs` as its own crate root; no `use` path
    // anywhere in the workspace can name one. So `reexport` here is not merely unhonourable, it is
    // meaningless, and saying so is what stops a plan author reaching for it by analogy.
    if op.op == RefactorKind::MoveTestBinaryToCrate && op.reexport.is_some() {
        return Err(malformed(
            "`move_test_binary_to_crate` cannot write a facade: a facade keeps a caller resolving, \
             and nothing can reference a test binary — cargo builds each `tests/*.rs` as its own \
             crate root, so no `use` path anywhere can name it",
        ));
    }

    // The destination is what the whole operation is for.
    if op.op == RefactorKind::MoveTestBinaryToCrate && op.to.is_none() {
        return Err(malformed(
            "`move_test_binary_to_crate` needs `to`: the destination crate's directory, relative \
             to the repository root",
        ));
    }

    if op.reexport.is_some()
        && !matches!(
            op.op,
            RefactorKind::ExtractModule | RefactorKind::MoveItem | RefactorKind::ReparentModule
        )
        && !op.op.moves_across_crates()
    {
        return Err(malformed(format!(
            "`reexport` asks for a facade where the moved items used to live, which only \
             `extract_module` and the cross-crate moves write — `{:?}` cannot honour one",
            op.op
        )));
    }

    // A move inside a crate names its destination module, and defaulting one would guess at the
    // module — the one thing a plan of intents must never do on the author's behalf. Its anchor is
    // the items it moves, by path: a range or a symbol names no module-level item.
    if op.op == RefactorKind::MoveItem {
        names_a_destination_and_anchors_by_item(
            &op,
            "move_item",
            "the module the items move into",
            "a range or a symbol names no module-level item to move",
        )?;
    }

    // The same two rules for a module move, whose anchor is the module's `mod` declaration. A
    // `named` facade lists items, and a module has none to list: the one it needs is the module
    // itself, which is what `glob` writes.
    if op.op == RefactorKind::ReparentModule {
        names_a_destination_and_anchors_by_item(
            &op,
            "reparent_module",
            "the module that becomes the parent",
            "a range or a symbol names no `mod` declaration to move",
        )?;
        if op.name.is_some() {
            return Err(malformed(
                "`reparent_module` creates no module: `name` belongs to `move_item`, which declares \
                 a new module in `to` when its line carries one",
            ));
        }
        if op.reexport == Some(Reexport::Named) {
            return Err(malformed(
                "`reparent_module` cannot write a named facade: a module has no items of its own \
                 to list — use `glob`, which leaves `pub use <new parent>::<module>;` where the \
                 module was",
            ));
        }
    }

    // `outside` partitions the callers by crate and leaves a facade for the outside ones, which only
    // the two same-crate moves know how to do: the others have no such partition to make.
    if op.reexport == Some(Reexport::Outside)
        && !matches!(op.op, RefactorKind::MoveItem | RefactorKind::ReparentModule)
    {
        return Err(malformed(format!(
            "`reexport: outside` belongs to `move_item` and `reparent_module`, which re-point the \
             callers in their own crate and leave a facade for the ones outside it; `{:?}` has no \
             such split — use `glob`, `named` or `none`",
            op.op
        )));
    }

    // A named facade cannot serve a *module* move, and refusing it beats emitting a tree that does
    // not compile. Callers of a moved module write `crate::<module>::Item`, so the facade has to put
    // something at `crate::<module>`; a `pub use <crate>::{Item, …};` puts the items at the crate
    // root instead, and because a facade also suppresses caller re-pointing, every one of those
    // callers is left naming a module that no longer exists. `glob` works because
    // `pub use <crate>::*;` re-exports the destination's `pub mod <module>` under its own name.
    // Refusing follows this file's own rule that a vocabulary advertising what it cannot perform is
    // worse than a smaller one.
    if op.op.moves_across_crates() && op.reexport == Some(Reexport::Named) {
        return Err(malformed(
            "a cross-crate move cannot write a named facade: a caller writes              `crate::<module>::Item`, and a named re-export puts the items at the crate root, so              every caller would stop resolving — use `glob`, which re-exports the module itself",
        ));
    }

    // A cross-crate move with no destination has nowhere to go, and defaulting one would guess at a
    // crate — the one thing a plan of intents must never do on the author's behalf.
    if op.op.moves_across_crates() && op.to.is_none() {
        return Err(malformed(format!(
            "`{:?}` needs `to`: the destination crate's directory, relative to the repository root",
            op.op
        )));
    }

    // A set of one is `move_module_to_crate`, and accepting it here would give that operation a
    // second name — the vocabulary refuses one for the same reason it refuses a name for what no
    // engine performs.
    if op.op == RefactorKind::MoveClusterToCrate && op.also.is_empty() {
        return Err(malformed(
            "`move_cluster_to_crate` needs `also`: the other modules travelling with the anchor's \
             — a set of one module is `move_module_to_crate`",
        ));
    }

    // Ignoring the field would move the anchor's module alone and re-point its siblings' paths at
    // the crate it just left, which is the defect this operation exists to remove — and the author
    // would read the resulting refusal as the set having been named wrongly.
    if !op.also.is_empty() && op.op != RefactorKind::MoveClusterToCrate {
        return Err(malformed(format!(
            "`also` names the other modules moving in the same operation, which only \
             `move_cluster_to_crate` honours — `{:?}` cannot",
            op.op
        )));
    }

    if op.to_file && op.op != RefactorKind::ExtractModule {
        return Err(malformed(format!(
            "`to_file` gives an extracted module a file of its own, which only `extract_module` can \
             do — `{:?}` cannot honour it",
            op.op
        )));
    }

    // Without a name there is nothing to remove, and nothing to call the struct: the assist would
    // either be asked for an arbitrary parameter or leave its own placeholder in the tree.
    if op.name.is_none() {
        match op.op {
            RefactorKind::RemoveUnusedParam => {
                return Err(malformed(
                    "`remove_unused_param` needs `name`: the parameter to remove",
                ))
            }
            RefactorKind::ConvertTupleReturnToStruct => {
                return Err(malformed(
                    "`convert_tuple_return_to_struct` needs `name`: the new struct's name",
                ))
            }
            _ => {}
        }
    }

    // `type` and `expr` are the two fields that carry Rust syntax; each must be exactly one type or
    // one expression, or the plan is refused before any server is spawned.
    if let Some(type_) = op.type_.as_deref() {
        super::rust_syntax::one_type(type_)?;
    }
    if let Some(expr) = op.expr.as_deref() {
        super::rust_syntax::one_expr(expr)?;
    }

    signature_fields::refuse_a_signature_operation_it_cannot_honour(&op)?;
    canonical_paths::refuse_canonical_paths_outside_move_item(&op)?;
    retarget_fields::refuse_a_retarget_it_cannot_honour(&op)?;
    repoint_call_fields::refuse_a_repoint_it_cannot_honour(&op)?;
    rebind_fields::refuse_a_rebind_it_cannot_honour(&op)?;

    Ok(op)
}

mod canonical_paths;
mod facade_imports_fields;
mod file_hint;
mod groups;
mod headerless;
mod rebind_fields;
mod repoint_call_fields;
mod retarget_fields;
mod signature_fields;

/// The destination and the by-item anchor an operation within one crate cannot do without.
fn names_a_destination_and_anchors_by_item(
    op: &RefactorOp,
    operation: &str,
    destination: &str,
    anchor_reason: &str,
) -> Result<()> {
    if op.to.is_none() {
        return Err(malformed(format!(
            "`{operation}` needs `to`: {destination}, rooted at the package name like an item path"
        )));
    }
    if !matches!(op.anchor, Anchor::Items { .. } | Anchor::Item { .. }) {
        return Err(malformed(format!(
            "`{operation}` anchors by item (`items`, or a single `item`): {anchor_reason}"
        )));
    }
    Ok(())
}
