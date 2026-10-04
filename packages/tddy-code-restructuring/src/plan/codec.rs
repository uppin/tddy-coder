use serde::{Deserialize, Serialize};

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

        let header = lines.next().ok_or_else(|| malformed("plan is empty"))?;
        if header_version(header) == Some(HINTED_SCHEMA_VERSION) {
            let header: HintedHeader = serde_json::from_str(header).map_err(|_| {
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
        let header: SnapshotHeader = serde_json::from_str(header)
            .map_err(|_| malformed("first line must be a snapshot header"))?;
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

/// What a v2 header says about the file at `path` as it stands: its hash and when it last changed.
pub(crate) fn hint_of(path: &std::path::Path) -> Result<FileHint> {
    let modified = std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| malformed(format!("{} could not be read: {error}", path.display())))?;
    Ok(FileHint {
        sha256: crate::apply::hash_file(path)?,
        modified: rfc3339(modified),
    })
}

/// `time` as an RFC 3339 UTC timestamp, to the second — or `None` for a time this cannot state,
/// which is one before the epoch. Answering `1970-01-01` for it would be a date that is not the
/// file's, and the hint is never read, so leaving it out loses nothing.
pub(crate) fn rfc3339(time: std::time::SystemTime) -> Option<String> {
    let seconds = i64::try_from(time.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs()).ok()?;
    let (days, within_day) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));

    // Days since 1970-01-01 to a civil date: the era/day-of-era arithmetic of Howard Hinnant's
    // `civil_from_days`, which holds for every date the proleptic Gregorian calendar has.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    Some(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        within_day / 3_600,
        within_day % 3_600 / 60,
        within_day % 60
    ))
}

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

/// Refuse a group whose members have an operation of another group, or of none, between them: that
/// operation would be applied inside a unit it is not part of, and rolled back with it.
fn refuse_split_groups(ops: &[RefactorOp]) -> Result<()> {
    let mut closed = std::collections::BTreeSet::new();
    let mut current: Option<&str> = None;
    for op in ops {
        let group = op.group.as_deref();
        if group == current {
            continue;
        }
        if let Some(finished) = current {
            closed.insert(finished);
        }
        if let Some(group) = group.filter(|group| closed.contains(group)) {
            return Err(malformed(format!(
                "group `{group}` is split: its members must be consecutive operations, and \
                 another operation sits between them"
            )));
        }
        current = group;
    }
    Ok(())
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

    if op.reexport.is_some() && op.op != RefactorKind::ExtractModule && !op.op.moves_across_crates()
    {
        return Err(malformed(format!(
            "`reexport` asks for a facade where the moved items used to live, which only \
             `extract_module` and the cross-crate moves write — `{:?}` cannot honour one",
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

    Ok(op)
}
