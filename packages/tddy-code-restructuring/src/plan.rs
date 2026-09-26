//! The plan: an immutable command log of named refactoring *intents*.
//!
//! A plan never carries code. If an operation would need a code snippet, the op vocabulary is wrong
//! and the plan is rejected — that rejection is what keeps hand-written code out of the pipeline.

use crate::edit::{Position, Range};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where an operation applies. Anchors are always expressed in *original snapshot* coordinates;
/// the [`crate::PositionLedger`] translates them to current coordinates at execution time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Anchor {
    /// A named symbol — survives edits above it, so preferred where the operation allows it.
    Symbol { file: String, path: String },
    /// A source range — required by extractions, which act on statements rather than a symbol.
    Range {
        file: String,
        start: Position,
        end: Position,
    },
    /// An item named by its crate-rooted path, resolved to an exact position through the language
    /// server's outline — the anchor that survives edits outside the item.
    ///
    /// `file` says where to look and nothing searches elsewhere. `start`/`end` are relative to the
    /// item's full range, attributes and doc comments included: `line` 1 is the item's first line
    /// and `col` is the column on that line. Both absent means the item itself, at its name.
    /// `fingerprint` is the item's text when the anchor was written, so an edit *to* the item is
    /// refused rather than re-targeted. `hint` is the absolute position, for orientation only —
    /// nothing resolves through it.
    Item {
        item: ItemPath,
        file: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<Position>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end: Option<Position>,
        fingerprint: Fingerprint,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hint: Option<Position>,
    },
    /// A contiguous run of sibling items, from the first one's first line to the last one's last,
    /// trivia included — what `extract_module` groups.
    Items {
        file: String,
        items: Vec<ItemPath>,
        fingerprints: Vec<Fingerprint>,
    },
}

impl Anchor {
    pub fn file(&self) -> &str {
        match self {
            Anchor::Symbol { file, .. }
            | Anchor::Range { file, .. }
            | Anchor::Item { file, .. }
            | Anchor::Items { file, .. } => file,
        }
    }
}

/// A crate-rooted path to an item: the crate's name, its module path, then the item and member
/// segments — `tddy_core::workflow::Stack::new`.
///
/// A member of a trait impl that shares its name with another member of the same type is addressed
/// with the trait named, the way Rust's own qualified path does: `tddy_core::workflow::<Stack as
/// Display>::fmt`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ItemPath(String);

/// One step of an [`ItemPath`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemSegment {
    /// A module, type, function or member, by name.
    Named(String),
    /// A member reached through one trait impl of a type: `<Stack as Display>`.
    TraitImpl {
        self_type: String,
        trait_name: String,
    },
}

impl ItemPath {
    /// Parse and validate a path; a path with fewer than two segments names no item in a crate.
    pub fn parse(text: &str) -> Result<ItemPath> {
        // TODO(item-anchors): implement
        let _ = text;
        todo!("item-anchors: parse an item path")
    }

    /// The crate the path is rooted in.
    pub fn crate_name(&self) -> &str {
        // TODO(item-anchors): implement
        todo!("item-anchors: the crate segment")
    }

    /// Every segment after the crate's.
    pub fn segments(&self) -> Vec<ItemSegment> {
        // TODO(item-anchors): implement
        todo!("item-anchors: the segments after the crate")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ItemPath {
    type Error = crate::RestructureError;

    fn try_from(text: String) -> Result<ItemPath> {
        ItemPath::parse(&text)
    }
}

impl From<ItemPath> for String {
    fn from(path: ItemPath) -> String {
        path.0
    }
}

impl std::fmt::Display for ItemPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// `sha256:<hex>` of an item's text: every line the item's full range touches, whole — leading
/// indentation, outer attributes and doc comments included — joined by `\n`, without a trailing
/// newline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fingerprint(pub String);

impl Fingerprint {
    /// The fingerprint of `text`, exactly as the anchored item reads.
    pub fn of(text: &str) -> Fingerprint {
        // TODO(item-anchors): implement
        let _ = text;
        todo!("item-anchors: hash an item's text")
    }
}

/// What a v2 header says about one file: a hint, never a refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileHint {
    pub sha256: String,
    /// RFC 3339.
    pub modified: String,
}

/// The operations a plan may contain.
///
/// There is deliberately no `CreateFile`, `InsertText`, or `DeleteRange`: files are *caused* by
/// refactors, and code text is produced by language engines.
///
/// Every variant is backed by a real assist in at least one engine — verified by asking each engine
/// what it offers rather than by assumption. `inline_symbol` and `rewrite_import_path` were dropped
/// for having no engine behind them; a vocabulary that advertises what cannot be performed is worse
/// than a smaller one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefactorKind {
    /// TypeScript `Extract Symbol`/`function_scope`; rust-analyzer `Extract into function`.
    ExtractMethod,
    /// TypeScript `Extract Symbol`/`constant_scope`; rust-analyzer `Extract into variable`.
    ExtractVariable,
    /// TypeScript `Extract type`. No rust-analyzer equivalent.
    ExtractType,
    /// TypeScript `Move to file`. rust-analyzer has no whole-symbol move.
    MoveSymbol,
    /// TypeScript, which rewrites every importer as part of the move.
    MoveFile,
    /// Both engines, via `textDocument/rename` semantics.
    RenameSymbol,
    /// rust-analyzer `Extract Module` — Rust's file-splitting primitive.
    ExtractModuleToFile,
    /// TypeScript.
    OrganizeImports,
    /// TypeScript.
    AddMissingImports,
    /// Moves whole class members into a class of their own. The only operation in this vocabulary
    /// with no engine behind it: TypeScript ships neither extract-class nor move-member, so the
    /// sidecar performs the transformation itself. See `AGENTS.md` for what that costs.
    ExtractClass,
    /// rust-analyzer `Extract Module` over a selection — groups loose items into an inline `mod`,
    /// which is a different assist from the `ExtractModuleToFile` above.
    ExtractModule,
    /// rust-analyzer `Generate trait from impl`. TypeScript offers nothing equivalent on a class.
    ExtractTrait,
    /// rust-analyzer `Inline into all callers`. TypeScript has no inline refactor at all.
    InlineMethod,
    /// Moves a module's file into another crate, rewrites its own `use` header, and re-points every
    /// caller. The **third** operation in this vocabulary with no engine behind it, after
    /// `extract_class` and the facade `use` line — rust-analyzer has no cross-crate move assist, so
    /// there is nothing to delegate to.
    ///
    /// It is engine-*informed* rather than engine-*performed*: every caller it rewrites comes from a
    /// real `textDocument/references` result, not a text search. That is the line this operation
    /// holds, and it is why it is not the `rewrite_import_path` the vocabulary already rejected —
    /// that one had no way to find what to rewrite.
    ///
    /// Takes `to` (the destination crate's directory) and honours a crate-level `reexport`, which
    /// leaves `pub use <new_crate>::…;` behind so a move can have zero caller diff — the same
    /// principle as `extract_module`'s facade, one level up.
    MoveModuleToCrate,
    /// Moves **a set of modules** to one crate as a single operation — the same transformation as
    /// `move_module_to_crate` over more than one module, applied all or not at all.
    ///
    /// A mutually-referencing set cannot move one module at a time: the first operation re-points a
    /// sibling's `crate::` path at the crate it is itself about to leave, and between the first
    /// operation and the last the tree does not compile. Naming the whole set in one operation is
    /// what lets a `crate::<sibling>` path that is coming along be told from one staying behind.
    ///
    /// The anchor is the first member and `also` names the rest. One operation is one journal
    /// entry and one edit, so `--resume`, `--from` and `--stop-after` keep meaning what they mean:
    /// a cluster is never half applied.
    MoveClusterToCrate,
    /// Moves a **test binary** — `<crate>/tests/<name>.rs` — to the crate whose code it exercises.
    ///
    /// A sibling of [`Self::MoveModuleToCrate`] rather than a generalisation of it, because a test
    /// binary is a different shape in the two ways that matter, and both make it *simpler*:
    ///
    /// - **Cargo auto-discovers `tests/*.rs`**, so there is no `mod` declaration anywhere to find,
    ///   remove or rewrite. A module move's whole `left_behind` pass has nothing to do here.
    /// - **Nothing can reference a test binary**, so there is no caller to keep resolving and
    ///   therefore no facade. `reexport` is refused rather than ignored.
    ///
    /// What it does share is the header pass — and it needs the strongest form of it, because a
    /// test's `use` path may reach its subject through **two** re-export facades before it lands on
    /// the crate that defines the item.
    ///
    /// Takes `to`, the destination crate's directory. The destination's `[dev-dependencies]` gain
    /// what the moved test names, not its `[dependencies]`.
    MoveTestBinaryToCrate,
}

impl RefactorKind {
    /// Whether this operation moves modules out of the crate that holds them.
    ///
    /// The two cross-crate moves differ only in how many modules travel, so every decision taken
    /// about one — the destination it must name, the facade it may leave, the preconditions read
    /// before a server is spawned — is taken about both.
    ///
    /// [`RefactorKind::MoveTestBinaryToCrate`] crosses a crate boundary too and is still **not**
    /// one of these, because this predicate does not mean "crosses a boundary" — it means "moves a
    /// *module*", and every caller reads it that way. Each of the three refusals it gates is
    /// already made for a test binary, earlier and in words about a test binary; and the fourth
    /// caller, [`unrunnable_moves`](crate::unrunnable_moves), reads each anchor as
    /// `<crate>/src/<module>.rs` to find the `mod` line it is about. A test binary has no `mod`
    /// line anywhere and does not live under `src/`, so admitting it here would report every
    /// well-formed test-binary move as an operation that cannot run.
    ///
    /// TODO(carve-test-homes): give `check` a static preflight for a test-binary move of its own —
    /// the anchor's shape and both crates' manifests are all readable before a server is spawned,
    /// so `apply`'s refusals are reachable statically the way a module move's are.
    #[must_use]
    pub fn moves_across_crates(self) -> bool {
        matches!(
            self,
            RefactorKind::MoveModuleToCrate | RefactorKind::MoveClusterToCrate
        )
    }
}

/// What a module extraction leaves in the parent so a path that reached the moved items still
/// resolves.
///
/// rust-analyzer has no "move item to another module" assist, so there is no engine to delegate the
/// facade to and this package authors the `use` line itself — the second place it does that, after
/// `extract_class`. What a plan carries is still only the intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reexport {
    /// `pub use <name>::*;` — one line, and legal whatever moved: a glob re-export caps at each
    /// item's own visibility rather than failing on a member less visible than itself.
    Glob,
    /// One grouped `use` per visibility tier, naming only the items something outside the new module
    /// reaches. A *named* re-export of a less visible item is `E0365`, which the tiers avoid; naming
    /// an item nothing outside reaches would force it public for no caller.
    Named,
    /// Nothing, which is what an extraction did before this field existed. A path-reached item with a
    /// reference elsewhere is then refused rather than stranded.
    None,
}

/// An operation's stable identity inside its plan.
///
/// Opaque, assigned by the plan store to any operation loaded without one and written back on the
/// next flush, so an operation keeps its identity when a human reorders or inserts lines. The
/// journal, the events and `--from` name operations by it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpId(pub String);

impl std::fmt::Display for OpId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefactorOp {
    /// This operation's stable id; absent only in a plan no store has loaded yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<OpId>,
    pub op: RefactorKind,
    pub anchor: Anchor,
    /// New symbol name, for extractions and renames.
    pub name: Option<String>,
    /// Destination path, for moves.
    pub to: Option<String>,
    /// Which of several actions an engine offers for the same operation, where it offers more than
    /// one. The meaning is op-specific and is validated by the backend that honours it, since only
    /// the backend knows which forms exist. Absent means "whatever this operation did before".
    pub variant: Option<String>,
    /// Carry a symbol's private-only dependencies along with it.
    #[serde(default)]
    pub with_private_deps: bool,
    /// What to leave in the parent so paths that reached the relocated items keep resolving. Only
    /// `extract_module` can honour one, and an operation that cannot is refused rather than having
    /// the field ignored.
    #[serde(default)]
    pub reexport: Option<Reexport>,
    /// Give the extracted module a file of its own, in the same operation that groups it.
    ///
    /// The two steps were two plans because the second anchors on the `mod` keyword the first writes,
    /// which no original coordinate maps to — a constraint on *anchors*, and it disappears when one
    /// operation performs both and never has to name that keyword. Each plan pays its own cold index,
    /// so the recipe a real split needs drops from four plans to two.
    #[serde(default)]
    pub to_file: bool,
    /// The modules travelling with the anchor's, for `move_cluster_to_crate`.
    ///
    /// Anchors rather than module names, so every member is addressed exactly the way the first one
    /// is and the [`crate::PositionLedger`] translates them all the same way. The anchor stays the
    /// first member so nothing that reads `op.anchor` has to learn about sets.
    #[serde(default)]
    pub also: Vec<Anchor>,
}

impl RefactorOp {
    /// The same operation, addressed at a translated anchor.
    pub fn with_anchor(&self, anchor: Anchor) -> RefactorOp {
        RefactorOp {
            anchor,
            ..self.clone()
        }
    }

    /// Every place this operation applies: its own anchor first, then each co-moving member's.
    ///
    /// One operation addressed at one anchor is this over a set of one, which is what lets the
    /// preconditions and the stranded-sibling check read a cluster without a second code path.
    pub fn anchors(&self) -> impl Iterator<Item = &Anchor> {
        std::iter::once(&self.anchor).chain(self.also.iter())
    }
}

/// A parsed plan: the snapshot header plus the ordered operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub version: u32,
    /// Content hash per file the plan touches, taken when the plan was written.
    pub snapshot: BTreeMap<String, String>,
    /// Schema v2's per-file hints: a drifted hash is reported, never refused. Empty for a v1 plan.
    pub files: BTreeMap<String, FileHint>,
    pub ops: Vec<RefactorOp>,
}

/// Schema version this executor understands.
const SCHEMA_VERSION: u32 = 1;

/// The schema whose header carries per-file hints instead of refusing snapshot hashes.
const HINTED_SCHEMA_VERSION: u32 = 2;

/// Fields that would carry source text. Their presence means the plan is trying to supply code the
/// language engine should have produced, so the plan is refused rather than partially honoured.
const CODE_BEARING_FIELDS: [&str; 3] = ["text", "code", "content"];

#[derive(Serialize, Deserialize)]
struct SnapshotHeader {
    v: u32,
    snapshot: BTreeMap<String, String>,
}

impl Plan {
    /// Parse a JSONL plan: line 1 is the snapshot header, every later line is one operation.
    pub fn parse(jsonl: &str) -> Result<Plan> {
        let mut lines = jsonl.lines().filter(|line| !line.trim().is_empty());

        let header = lines.next().ok_or_else(|| malformed("plan is empty"))?;
        if header_version(header) == Some(HINTED_SCHEMA_VERSION) {
            // TODO(item-anchors): implement — parse a v2 header's `files` hints.
            todo!("item-anchors: parse a v2 header")
        }
        let header: SnapshotHeader = serde_json::from_str(header)
            .map_err(|_| malformed("first line must be a snapshot header"))?;
        if header.v != SCHEMA_VERSION {
            return Err(malformed(format!(
                "plan declares schema version {} but this executor speaks {SCHEMA_VERSION}",
                header.v
            )));
        }

        let ops = lines.map(parse_op).collect::<Result<Vec<_>>>()?;

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

    /// This plan as JSONL: the header its schema version writes, then one line per operation in
    /// order — what the plan store flushes back to disk.
    pub fn to_jsonl(&self) -> String {
        // TODO(plan-store): implement
        todo!("plan-store: serialise a plan back to JSONL")
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

/// The `v` a header line declares, read before the header's shape is known.
fn header_version(line: &str) -> Option<u32> {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()?
        .get("v")?
        .as_u64()
        .map(|v| v as u32)
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

    Ok(op)
}

fn malformed(reason: impl Into<String>) -> crate::RestructureError {
    crate::RestructureError::MalformedPlan(reason.into())
}

/// Convenience for backends that need the anchor as a range.
impl Anchor {
    pub fn as_range(&self) -> Option<Range> {
        match self {
            Anchor::Range { start, end, .. } => Some(Range {
                start: *start,
                end: *end,
            }),
            Anchor::Symbol { .. } | Anchor::Item { .. } | Anchor::Items { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RestructureError;

    const HEADER: &str = r#"{"v":1,"snapshot":{"src/shapes.ts":"sha256:ab12"}}"#;

    fn plan_with(op_line: &str) -> String {
        format!("{HEADER}\n{op_line}\n")
    }

    #[test]
    fn reads_the_version_and_snapshot_from_the_header_line() {
        let plan = Plan::parse(&plan_with(
            r#"{"op":"organize_imports","anchor":{"kind":"symbol","file":"src/shapes.ts","path":""}}"#,
        ))
        .unwrap();

        assert_eq!(plan.version, 1);
        assert_eq!(
            plan.snapshot.get("src/shapes.ts").map(String::as_str),
            Some("sha256:ab12")
        );
    }

    #[test]
    fn preserves_operation_order() {
        let jsonl = format!(
            "{HEADER}\n{}\n{}\n",
            r#"{"op":"extract_method","anchor":{"kind":"range","file":"src/shapes.ts","start":{"line":1,"col":1},"end":{"line":2,"col":1}},"name":"first"}"#,
            r#"{"op":"move_symbol","anchor":{"kind":"symbol","file":"src/shapes.ts","path":"first"},"to":"src/other.ts"}"#,
        );

        let plan = Plan::parse(&jsonl).unwrap();

        assert_eq!(plan.ops.len(), 2);
        assert_eq!(plan.ops[0].op, RefactorKind::ExtractMethod);
        assert_eq!(plan.ops[1].op, RefactorKind::MoveSymbol);
    }

    #[test]
    fn reads_a_range_anchor_in_original_snapshot_coordinates() {
        let plan = Plan::parse(&plan_with(
            r#"{"op":"extract_method","anchor":{"kind":"range","file":"src/shapes.ts","start":{"line":412,"col":5},"end":{"line":468,"col":6}},"name":"computeBounds"}"#,
        ))
        .unwrap();

        assert_eq!(
            plan.ops[0].anchor,
            Anchor::Range {
                file: "src/shapes.ts".to_string(),
                start: Position { line: 412, col: 5 },
                end: Position { line: 468, col: 6 },
            }
        );
    }

    #[test]
    fn rejects_a_plan_written_for_a_different_schema_version() {
        let jsonl = "{\"v\":3,\"snapshot\":{}}\n";

        let outcome = Plan::parse(jsonl);

        assert!(matches!(outcome, Err(RestructureError::MalformedPlan(_))));
    }

    #[test]
    fn rejects_an_operation_the_vocabulary_does_not_define() {
        let outcome = Plan::parse(&plan_with(
            r#"{"op":"reticulate_splines","anchor":{"kind":"symbol","file":"src/shapes.ts","path":"x"}}"#,
        ));

        assert!(matches!(outcome, Err(RestructureError::MalformedPlan(_))));
    }

    /// Plans hold intents. A code-bearing operation means the vocabulary was wrong, and accepting
    /// it would reintroduce hand-written code — the one thing this pipeline exists to prevent.
    #[test]
    fn rejects_an_operation_that_carries_literal_code() {
        let outcome = Plan::parse(&plan_with(
            r#"{"op":"insert_text","anchor":{"kind":"symbol","file":"src/shapes.ts","path":"x"},"text":"const a = 1;"}"#,
        ));

        assert!(matches!(
            outcome,
            Err(RestructureError::CodeTextInPlan { .. })
        ));
    }

    #[test]
    fn rejects_an_operation_that_declares_a_new_file() {
        let outcome = Plan::parse(&plan_with(
            r#"{"op":"create_file","anchor":{"kind":"symbol","file":"src/new.ts","path":""}}"#,
        ));

        assert!(matches!(outcome, Err(RestructureError::MalformedPlan(_))));
    }

    #[test]
    fn rejects_a_plan_with_no_snapshot_header() {
        let outcome = Plan::parse(
            r#"{"op":"organize_imports","anchor":{"kind":"symbol","file":"src/shapes.ts","path":""}}"#,
        );

        assert!(matches!(outcome, Err(RestructureError::MalformedPlan(_))));
    }

    #[test]
    fn names_the_drifted_file_when_a_snapshot_hash_no_longer_matches() {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(
            workspace.path().join("shapes.ts"),
            "changed since planning\n",
        )
        .unwrap();
        let plan = Plan {
            version: 1,
            snapshot: BTreeMap::from([("shapes.ts".to_string(), "sha256:stale".to_string())]),
            files: BTreeMap::new(),
            ops: vec![],
        };

        let outcome = plan.verify_snapshot(workspace.path());

        match outcome {
            Err(RestructureError::SnapshotMismatch { path, .. }) => assert_eq!(path, "shapes.ts"),
            other => panic!("expected a snapshot mismatch, got {other:?}"),
        }
    }

    #[test]
    fn accepts_a_snapshot_that_still_matches_the_working_tree() {
        let workspace = tempfile::tempdir().unwrap();
        let contents = "unchanged\n";
        std::fs::write(workspace.path().join("shapes.ts"), contents).unwrap();
        let digest = {
            use sha2::{Digest, Sha256};
            format!("sha256:{:x}", Sha256::digest(contents.as_bytes()))
        };
        let plan = Plan {
            version: 1,
            snapshot: BTreeMap::from([("shapes.ts".to_string(), digest)]),
            files: BTreeMap::new(),
            ops: vec![],
        };

        assert!(plan.verify_snapshot(workspace.path()).is_ok());
    }

    /// `variant` picks between several actions an engine offers for one operation. Its absence has
    /// to keep meaning "whatever this operation did before", or every plan already written changes
    /// behaviour the day the field lands.
    #[test]
    fn reads_the_variant_an_operation_asks_for() {
        let plan = Plan::parse(&plan_with(
            r#"{"op":"extract_method","anchor":{"kind":"range","file":"src/shapes.ts","start":{"line":477,"col":12},"end":{"line":477,"col":41}},"name":"perimeterOf","variant":"module"}"#,
        ))
        .unwrap();

        assert_eq!(plan.ops[0].variant.as_deref(), Some("module"));
    }

    #[test]
    fn leaves_the_variant_unset_when_an_operation_omits_it() {
        let plan = Plan::parse(&plan_with(
            r#"{"op":"extract_method","anchor":{"kind":"range","file":"src/shapes.ts","start":{"line":477,"col":12},"end":{"line":477,"col":41}},"name":"perimeterOf"}"#,
        ))
        .unwrap();

        assert_eq!(plan.ops[0].variant, None);
    }

    /// The ledger re-addresses every pending operation at a translated anchor. Losing the variant
    /// there would quietly downgrade a module-scope extraction to an inner one partway through a
    /// plan, which nothing downstream could detect.
    #[test]
    fn carries_the_variant_through_an_anchor_translation() {
        let plan = Plan::parse(&plan_with(
            r#"{"op":"extract_type","anchor":{"kind":"range","file":"src/shapes.ts","start":{"line":447,"col":40},"end":{"line":447,"col":79}},"name":"Spacing","variant":"interface"}"#,
        ))
        .unwrap();

        let translated = plan.ops[0].with_anchor(Anchor::Range {
            file: "src/shapes.ts".to_string(),
            start: Position { line: 501, col: 40 },
            end: Position { line: 501, col: 79 },
        });

        assert_eq!(translated.variant.as_deref(), Some("interface"));
    }

    #[test]
    fn reads_a_class_extraction_with_its_destination_file() {
        let plan = Plan::parse(&plan_with(
            r#"{"op":"extract_class","anchor":{"kind":"range","file":"src/shapes.ts","start":{"line":462,"col":3},"end":{"line":474,"col":4}},"name":"BoxDescriber","to":"src/box-describer.ts"}"#,
        ))
        .unwrap();

        assert_eq!(plan.ops[0].op, RefactorKind::ExtractClass);
        assert_eq!(plan.ops[0].name.as_deref(), Some("BoxDescriber"));
        assert_eq!(plan.ops[0].to.as_deref(), Some("src/box-describer.ts"));
    }

    /// `extract_module` groups loose items into an inline `mod`, and is a different operation from
    /// the `extract_module_to_file` the vocabulary already had — the two must not collapse.
    #[test]
    fn reads_a_module_extraction_as_distinct_from_extracting_a_module_to_a_file() {
        let jsonl = format!(
            "{HEADER}\n{}\n{}\n",
            r#"{"op":"extract_module","anchor":{"kind":"range","file":"src/lib.rs","start":{"line":40,"col":1},"end":{"line":48,"col":2}},"name":"bounds"}"#,
            r#"{"op":"extract_module_to_file","anchor":{"kind":"symbol","file":"src/lib.rs","path":"bounds"}}"#,
        );

        let plan = Plan::parse(&jsonl).unwrap();

        assert_eq!(plan.ops[0].op, RefactorKind::ExtractModule);
        assert_eq!(plan.ops[1].op, RefactorKind::ExtractModuleToFile);
    }

    #[test]
    fn reads_a_trait_extraction_from_a_caret_on_an_impl() {
        let plan = Plan::parse(&plan_with(
            r#"{"op":"extract_trait","anchor":{"kind":"range","file":"src/lib.rs","start":{"line":56,"col":1},"end":{"line":56,"col":1}},"name":"Readable"}"#,
        ))
        .unwrap();

        assert_eq!(plan.ops[0].op, RefactorKind::ExtractTrait);
        assert_eq!(plan.ops[0].name.as_deref(), Some("Readable"));
    }

    /// Inlining names no new symbol, so `name` stays absent — the parser must not require one.
    #[test]
    fn reads_an_inline_that_names_no_new_symbol() {
        let plan = Plan::parse(&plan_with(
            r#"{"op":"inline_method","anchor":{"kind":"symbol","file":"src/lib.rs","path":"scaled"}}"#,
        ))
        .unwrap();

        assert_eq!(plan.ops[0].op, RefactorKind::InlineMethod);
        assert_eq!(plan.ops[0].name, None);
    }

    /// `extract_class` is the one operation with no engine behind it, which makes it the one most
    /// tempting to hand a snippet to. It is inside the same guard as everything else.
    #[test]
    fn rejects_a_class_extraction_that_carries_literal_code() {
        let outcome = Plan::parse(&plan_with(
            r#"{"op":"extract_class","anchor":{"kind":"range","file":"src/shapes.ts","start":{"line":462,"col":3},"end":{"line":474,"col":4}},"name":"BoxDescriber","to":"src/box-describer.ts","content":"export class BoxDescriber {}"}"#,
        ));

        assert!(matches!(
            outcome,
            Err(RestructureError::CodeTextInPlan { .. })
        ));
    }

    #[test]
    fn reads_the_reexport_a_module_extraction_asks_for() {
        let plan = Plan::parse(&plan_with(
            r#"{"op":"extract_module","anchor":{"kind":"range","file":"src/a.rs","start":{"line":1,"col":1},"end":{"line":9,"col":1}},"name":"api","reexport":"glob"}"#,
        ))
        .unwrap();

        assert_eq!(plan.ops[0].reexport, Some(Reexport::Glob));
    }

    #[test]
    fn reads_the_destination_crate_a_cross_crate_move_names() {
        let plan = Plan::parse(&plan_with(
            r#"{"op":"move_module_to_crate","anchor":{"kind":"symbol","file":"packages/tddy-daemon/src/host_registry.rs","path":"host_registry"},"to":"packages/tddy-host-service"}"#,
        ))
        .unwrap();

        assert_eq!(plan.ops[0].op, RefactorKind::MoveModuleToCrate);
        assert_eq!(
            plan.ops[0].to.as_deref(),
            Some("packages/tddy-host-service")
        );
    }

    /// The facade is the whole reason a cross-crate move can be reviewed: with one, no caller moves.
    #[test]
    fn reads_the_crate_level_facade_a_cross_crate_move_asks_for() {
        let plan = Plan::parse(&plan_with(
            r#"{"op":"move_module_to_crate","anchor":{"kind":"symbol","file":"packages/tddy-daemon/src/host_registry.rs","path":"host_registry"},"to":"packages/tddy-host-service","reexport":"glob"}"#,
        ))
        .unwrap();

        assert_eq!(plan.ops[0].reexport, Some(Reexport::Glob));
    }

    /// Defaulting a destination would guess at a crate, which is the one thing a plan of intents
    /// must never do on the author's behalf.
    #[test]
    fn refuses_a_cross_crate_move_with_no_destination() {
        let outcome = Plan::parse(&plan_with(
            r#"{"op":"move_module_to_crate","anchor":{"kind":"symbol","file":"packages/tddy-daemon/src/host_registry.rs","path":"host_registry"}}"#,
        ));

        let Err(RestructureError::MalformedPlan(reason)) = outcome else {
            panic!("expected a malformed-plan refusal, got {outcome:?}");
        };
        assert!(reason.contains("`to`"), "{reason}");
    }

    /// A named facade would leave every caller naming a module that no longer exists, and a facade
    /// also suppresses caller re-pointing — so the tree would not compile and nothing would say why.
    #[test]
    fn refuses_a_named_facade_on_a_cross_crate_move() {
        let outcome = Plan::parse(&plan_with(
            r#"{"op":"move_module_to_crate","anchor":{"kind":"symbol","file":"packages/tddy-daemon/src/host_registry.rs","path":"host_registry"},"to":"packages/tddy-host-service","reexport":"named"}"#,
        ));

        let Err(RestructureError::MalformedPlan(reason)) = outcome else {
            panic!("expected a malformed-plan refusal, got {outcome:?}");
        };
        assert!(
            reason.contains("glob"),
            "the refusal must name the alternative: {reason}"
        );
    }

    /// The operation the cluster defect needs: one intent naming the whole set, so no member is
    /// re-pointed at a crate its siblings are about to leave.
    #[test]
    fn reads_every_member_of_a_cluster_move() {
        let plan = Plan::parse(&plan_with(
            r#"{"op":"move_cluster_to_crate","anchor":{"kind":"symbol","file":"packages/tddy-daemon/src/spawner.rs","path":"spawner"},"also":[{"kind":"symbol","file":"packages/tddy-daemon/src/spawn_worker.rs","path":"spawn_worker"}],"to":"packages/tddy-host-service"}"#,
        ))
        .unwrap();

        assert_eq!(plan.ops[0].op, RefactorKind::MoveClusterToCrate);
        assert_eq!(
            plan.ops[0]
                .anchors()
                .map(Anchor::file)
                .collect::<Vec<&str>>(),
            vec![
                "packages/tddy-daemon/src/spawner.rs",
                "packages/tddy-daemon/src/spawn_worker.rs"
            ]
        );
    }

    /// A set of one is `move_module_to_crate`. Accepting it here would give that operation a second
    /// name, which is exactly what this vocabulary refuses to grow.
    #[test]
    fn refuses_a_cluster_move_that_names_no_second_module() {
        let error = Plan::parse(&plan_with(
            r#"{"op":"move_cluster_to_crate","anchor":{"kind":"symbol","file":"packages/tddy-daemon/src/spawner.rs","path":"spawner"},"to":"packages/tddy-host-service"}"#,
        ))
        .unwrap_err()
        .to_string();

        assert!(error.contains("`also`"), "{error}");
        assert!(error.contains("move_module_to_crate"), "{error}");
    }

    /// A cluster with nowhere to go is refused for the reason a single move is: defaulting one
    /// would guess at a crate.
    #[test]
    fn refuses_a_cluster_move_with_no_destination() {
        let error = Plan::parse(&plan_with(
            r#"{"op":"move_cluster_to_crate","anchor":{"kind":"symbol","file":"packages/tddy-daemon/src/spawner.rs","path":"spawner"},"also":[{"kind":"symbol","file":"packages/tddy-daemon/src/spawn_worker.rs","path":"spawn_worker"}]}"#,
        ))
        .unwrap_err()
        .to_string();

        assert!(error.contains("`to`"), "{error}");
    }

    /// Ignoring `also` would move the anchor's module alone and re-point its siblings at the crate
    /// it just left — the defect the operation exists to remove, reported as something else.
    #[test]
    fn refuses_co_moving_members_on_an_operation_that_moves_one_module() {
        let error = Plan::parse(&plan_with(
            r#"{"op":"move_module_to_crate","anchor":{"kind":"symbol","file":"packages/tddy-daemon/src/spawner.rs","path":"spawner"},"also":[{"kind":"symbol","file":"packages/tddy-daemon/src/spawn_worker.rs","path":"spawn_worker"}],"to":"packages/tddy-host-service"}"#,
        ))
        .unwrap_err()
        .to_string();

        assert!(error.contains("MoveModuleToCrate"), "{error}");
        assert!(error.contains("`also`"), "{error}");
    }

    /// A cluster leaves the same facade a single move does, and refuses the named one for the same
    /// reason: `crate::<module>::Item` stops resolving at the crate root.
    #[test]
    fn refuses_a_named_facade_on_a_cluster_move() {
        let error = Plan::parse(&plan_with(
            r#"{"op":"move_cluster_to_crate","anchor":{"kind":"symbol","file":"packages/tddy-daemon/src/spawner.rs","path":"spawner"},"also":[{"kind":"symbol","file":"packages/tddy-daemon/src/spawn_worker.rs","path":"spawn_worker"}],"to":"packages/tddy-host-service","reexport":"named"}"#,
        ))
        .unwrap_err()
        .to_string();

        assert!(error.contains("glob"), "{error}");
    }

    /// Every plan written before the field existed has to go on meaning what it meant.
    #[test]
    fn treats_a_missing_reexport_as_absent() {
        let plan = Plan::parse(&plan_with(
            r#"{"op":"extract_module","anchor":{"kind":"range","file":"src/a.rs","start":{"line":1,"col":1},"end":{"line":9,"col":1}},"name":"api"}"#,
        ))
        .unwrap();

        assert_eq!(plan.ops[0].reexport, None);
    }

    /// Ignoring the field would be worse than refusing it: the author would get no facade and would
    /// read the stranded-reference refusal that follows as the facade having failed to help.
    #[test]
    fn refuses_a_reexport_on_an_operation_that_cannot_honour_one() {
        let error = Plan::parse(&plan_with(
            r#"{"op":"extract_method","anchor":{"kind":"range","file":"src/a.rs","start":{"line":1,"col":1},"end":{"line":9,"col":1}},"name":"helper","reexport":"glob"}"#,
        ))
        .unwrap_err()
        .to_string();

        assert!(error.contains("ExtractMethod"), "{error}");
        assert!(error.contains("reexport"), "{error}");
    }

    #[test]
    fn refuses_a_reexport_the_vocabulary_does_not_define() {
        assert!(Plan::parse(&plan_with(
            r#"{"op":"extract_module","anchor":{"kind":"range","file":"src/a.rs","start":{"line":1,"col":1},"end":{"line":9,"col":1}},"name":"api","reexport":"partial"}"#,
        ))
        .is_err());
    }

    #[test]
    fn an_item_path_names_its_crate_and_its_segments() {
        // Given a crate-rooted path to a method
        let path = ItemPath::parse("tddy_core::workflow::Stack::new").unwrap();

        // Then the crate and every later segment are read apart
        assert_eq!(path.crate_name(), "tddy_core");
        assert_eq!(
            path.segments(),
            vec![
                ItemSegment::Named("workflow".to_string()),
                ItemSegment::Named("Stack".to_string()),
                ItemSegment::Named("new".to_string()),
            ]
        );
    }

    #[test]
    fn a_trait_qualified_segment_names_its_type_and_its_trait() {
        // Given a member reached through one trait impl
        let path = ItemPath::parse("stacks::workflow::<Stack as Debug>::fmt").unwrap();

        // Then the qualified segment carries both names
        assert_eq!(
            path.segments(),
            vec![
                ItemSegment::Named("workflow".to_string()),
                ItemSegment::TraitImpl {
                    self_type: "Stack".to_string(),
                    trait_name: "Debug".to_string(),
                },
                ItemSegment::Named("fmt".to_string()),
            ]
        );
    }

    #[test]
    fn an_item_path_of_one_segment_is_refused() {
        // When a bare name is parsed as an item path
        let parsed = ItemPath::parse("Stack");

        // Then it is refused as naming no item in a crate
        assert_eq!(
            parsed.map_err(|error| error.to_string()),
            Err(
                "plan is malformed: `Stack` is not an item path — write it crate-rooted, as \
                 `<crate>::<module>::Stack`"
                    .to_string()
            )
        );
    }

    #[test]
    fn an_item_path_displays_as_it_was_written() {
        let written = "stacks::workflow::<Stack as Debug>::fmt";

        assert_eq!(ItemPath::parse(written).unwrap().to_string(), written);
    }

    #[test]
    fn a_fingerprint_is_the_sha256_of_the_items_text() {
        assert_eq!(
            Fingerprint::of("pub struct A;"),
            Fingerprint(
                "sha256:29124c31393737708604d695f0300b693d832bc25a2fe272ec8e6ac9c9cc24b4"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_v2_header_carries_its_file_hints() {
        // Given a v2 plan with one hinted file and one item-anchored op
        let jsonl = concat!(
            r#"{"v":2,"files":{"src/lib.rs":{"sha256":"sha256:ab","modified":"2026-09-26T00:00:00Z"}}}"#,
            "\n",
            r#"{"op":"rename_symbol","anchor":{"kind":"item","item":"a::b::C","file":"src/b.rs","fingerprint":"sha256:cd"},"name":"D"}"#,
            "\n"
        );

        // When it is parsed
        let plan = Plan::parse(jsonl).unwrap();

        // Then the hint is kept and nothing is read as a refusing snapshot
        assert_eq!(plan.version, 2);
        assert_eq!(plan.snapshot, BTreeMap::new());
        assert_eq!(
            plan.files,
            BTreeMap::from([(
                "src/lib.rs".to_string(),
                FileHint {
                    sha256: "sha256:ab".to_string(),
                    modified: "2026-09-26T00:00:00Z".to_string(),
                }
            )])
        );
    }

    #[test]
    fn an_item_anchor_round_trips_through_its_json() {
        // Given an item anchor with a relative range and a hint
        let line = r#"{"kind":"item","item":"a::b::C::f","file":"src/b.rs","start":{"line":2,"col":9},"end":{"line":3,"col":10},"fingerprint":"sha256:cd","hint":{"line":40,"col":9}}"#;

        // When it is read and written back
        let anchor: Anchor = serde_json::from_str(line).unwrap();

        // Then it is the same line
        assert_eq!(serde_json::to_string(&anchor).unwrap(), line);
    }

    #[test]
    fn a_plan_written_back_reads_as_the_same_plan() {
        // Given a v1 plan with an op that carries an id and one that does not
        let jsonl = concat!(
            r#"{"v":1,"snapshot":{"src/a.rs":"sha256:ab"}}"#,
            "\n",
            r#"{"id":"op-1","op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"A"},"name":"B"}"#,
            "\n",
            r#"{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"C"},"name":"D"}"#,
            "\n"
        );
        let plan = Plan::parse(jsonl).unwrap();

        // When it is written back
        let written = plan.to_jsonl();

        // Then it is the same text, line for line
        assert_eq!(written, jsonl);
    }
}
