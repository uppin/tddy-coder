//! The plan: a command log of named refactoring *intents*, which [`crate::plan_store`] writes back as
//! operations apply.
//!
//! A plan never carries code. If an operation would need a code snippet, the op vocabulary is wrong
//! and the plan is rejected — that rejection is what keeps hand-written code out of the pipeline.
//!
//! The two exceptions are `type` and `expr`, which a signature change and a call-site repair cannot
//! be stated without. Each is exactly one Rust type or one Rust expression and nothing more —
//! [`rust_syntax`] refuses anything else.

use crate::edit::{Position, Range};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod rust_syntax;

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
        item: item_path::ItemPath,
        file: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<Position>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end: Option<Position>,
        fingerprint: item_path::Fingerprint,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hint: Option<Position>,
    },
    /// A contiguous run of sibling items, from the first one's first line to the last one's last,
    /// trivia included — what `extract_module` groups.
    Items {
        file: String,
        items: Vec<item_path::ItemPath>,
        fingerprints: Vec<item_path::Fingerprint>,
    },
}

impl Anchor {
    /// Refuse an anchor whose own fields contradict each other, before any tree is consulted.
    ///
    /// What cannot be judged here — whether a relative range lies inside its item — needs the item
    /// found, and is refused when it is.
    pub fn validate(&self) -> Result<()> {
        match self {
            Anchor::Symbol { .. } | Anchor::Range { .. } => Ok(()),
            Anchor::Item {
                item, start, end, ..
            } => match (start, end) {
                (None, None) => Ok(()),
                (Some(start), Some(end)) => {
                    let one_based = |position: &Position| position.line >= 1 && position.col >= 1;
                    if !one_based(start) || !one_based(end) {
                        return Err(malformed(format!(
                            "the range of `{item}` counts lines and columns from 1"
                        )));
                    }
                    if (start.line, start.col) > (end.line, end.col) {
                        return Err(malformed(format!(
                            "the range of `{item}` ends before it starts"
                        )));
                    }
                    Ok(())
                }
                _ => Err(malformed(format!(
                    "`{item}` gives one end of a relative range — give both `start` and `end`, \
                     or neither to anchor the item itself"
                ))),
            },
            Anchor::Items {
                items,
                fingerprints,
                ..
            } => {
                if items.is_empty() {
                    return Err(malformed("an `items` anchor names no items"));
                }
                if items.len() != fingerprints.len() {
                    return Err(malformed(format!(
                        "an `items` anchor names {} item(s) and {} fingerprint(s) — each item \
                         carries its own",
                        items.len(),
                        fingerprints.len()
                    )));
                }
                Ok(())
            }
        }
    }

    pub fn file(&self) -> &str {
        match self {
            Anchor::Symbol { file, .. }
            | Anchor::Range { file, .. }
            | Anchor::Item { file, .. }
            | Anchor::Items { file, .. } => file,
        }
    }
}

mod item_path;
pub(crate) use item_path::split_path;
pub use item_path::{Fingerprint, ItemPath, ItemSegment};

/// What a v2 header says about one file: a hint, never a refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileHint {
    pub sha256: String,
    /// RFC 3339, when the file's modification time is one it can state — absent for a time before
    /// the epoch rather than a date that is not the file's. Never read; it is there for a person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

pub use crate::plan::refactor_kind::RefactorKind;

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
    /// Only for `move_item` and `reparent_module`: every caller in the crate that holds the moved
    /// code is re-pointed to the new path, as [`Reexport::None`] does, and a facade is left at the
    /// old path for exactly the items something in *another* package reaches, as
    /// [`Reexport::Named`] writes it. Nothing outside reaching an item leaves it no facade, and a
    /// caller outside the crate is never edited — the public path it depends on keeps resolving.
    Outside,
    /// Nothing, which is what an extraction did before this field existed. A path-reached item with a
    /// reference elsewhere is then refused rather than stranded.
    None,
}

impl Reexport {
    /// Whether the callers in the moved code's own crate are re-pointed to the new path.
    pub(crate) fn repoints_callers(self) -> bool {
        matches!(self, Reexport::None | Reexport::Outside)
    }
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

/// One operation of a plan.
///
/// Unknown fields are refused rather than dropped: a misspelt `group` would otherwise run its
/// operations ungrouped, with none of the rollback the author asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefactorOp {
    /// This operation's stable id; absent only in a plan no store has loaded yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<OpId>,
    pub op: RefactorKind,
    pub anchor: Anchor,
    /// New symbol name, for extractions and renames. On a `move_item` it names a module the move
    /// creates: `to` is then that module's parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Destination path, for moves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Which of several actions an engine offers for the same operation, where it offers more than
    /// one. The meaning is op-specific and is validated by the backend that honours it, since only
    /// the backend knows which forms exist. Absent means "whatever this operation did before".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    /// Carry a symbol's private-only dependencies along with it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub with_private_deps: bool,
    /// What to leave in the parent so paths that reached the relocated items keep resolving. Only
    /// `extract_module` can honour one, and an operation that cannot is refused rather than having
    /// the field ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reexport: Option<Reexport>,
    /// Give the extracted module a file of its own, in the same operation that groups it.
    ///
    /// The two steps were two plans because the second anchors on the `mod` keyword the first writes,
    /// which no original coordinate maps to — a constraint on *anchors*, and it disappears when one
    /// operation performs both and never has to name that keyword. Each plan pays its own cold index,
    /// so the recipe a real split needs drops from four plans to two.
    #[serde(default, skip_serializing_if = "is_false")]
    pub to_file: bool,
    /// The modules travelling with the anchor's, for `move_cluster_to_crate`.
    ///
    /// Anchors rather than module names, so every member is addressed exactly the way the first one
    /// is and the [`crate::PositionLedger`] translates them all the same way. The anchor stays the
    /// first member so nothing that reads `op.anchor` has to learn about sets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<Anchor>,
    /// The transactional group this operation belongs to.
    ///
    /// Consecutive operations sharing a group id apply as one unit: `cargo check` runs at the
    /// group's end over the packages it touched, and a group that does not compile there is rolled
    /// back exactly ([`crate::RestructureError::GroupDoesNotCompile`]). Absent means the operation
    /// stands alone, under the end-of-run gate it always had.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// A Rust type, for `change_param_type`, `add_param` and `change_return_type`.
    ///
    /// The one place besides `expr` a plan carries Rust syntax: it is parsed as exactly one
    /// [`syn::Type`] and refused otherwise (see [`rust_syntax::one_type`]), so it can carry neither
    /// statements nor items.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    /// A Rust expression, for `add_call_arg` and `change_call_arg`: parsed as exactly one
    /// [`syn::Expr`] carrying no statement, and refused otherwise (see [`rust_syntax::one_expr`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expr: Option<String>,
    /// The new order, for `reorder_params` (parameter names) and `reorder_call_args` (current
    /// one-based argument positions).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<OrderKey>,
}

/// One entry of an operation's `order`: a parameter's name, or an argument's one-based position.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrderKey {
    /// A call argument's current one-based position.
    Position(u32),
    /// A parameter's name.
    Name(String),
}

impl std::fmt::Display for OrderKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderKey::Position(position) => write!(formatter, "{position}"),
            OrderKey::Name(name) => write!(formatter, "`{name}`"),
        }
    }
}

/// Whether a flag is off — what keeps a default out of a plan written back, so an operation reads
/// the way its author wrote it.
fn is_false(flag: &bool) -> bool {
    !*flag
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

mod codec;
mod refactor_kind;
pub(crate) use codec::hint_of;

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
    use crate::{plan::codec::rfc3339, RestructureError};

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

    /// A parameter removal names the parameter it drops, and the function it is anchored on.
    #[test]
    fn reads_a_parameter_removal_naming_its_parameter() {
        let plan = Plan::parse(&plan_with(
            r#"{"op":"remove_unused_param","anchor":{"kind":"symbol","file":"src/spawn.rs","path":"build_claude_argv"},"name":"initial_prompt"}"#,
        ))
        .unwrap();

        assert_eq!(plan.ops[0].op, RefactorKind::RemoveUnusedParam);
        assert_eq!(plan.ops[0].name.as_deref(), Some("initial_prompt"));
    }

    /// A tuple-return conversion names the struct it introduces.
    #[test]
    fn reads_a_tuple_return_conversion_naming_its_struct() {
        let plan = Plan::parse(&plan_with(
            r#"{"op":"convert_tuple_return_to_struct","anchor":{"kind":"symbol","file":"src/placement.rs","path":"resolve_placement"},"name":"Placement"}"#,
        ))
        .unwrap();

        assert_eq!(plan.ops[0].op, RefactorKind::ConvertTupleReturnToStruct);
        assert_eq!(plan.ops[0].name.as_deref(), Some("Placement"));
    }

    /// With no `name` a tuple-return conversion has nothing to call the struct, and the assist's own
    /// placeholder would land in the tree.
    #[test]
    fn refuses_a_tuple_return_conversion_that_names_no_struct() {
        let error = Plan::parse(&plan_with(
            r#"{"op":"convert_tuple_return_to_struct","anchor":{"kind":"symbol","file":"src/placement.rs","path":"resolve_placement"}}"#,
        ))
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "plan is malformed: `convert_tuple_return_to_struct` needs `name`: the new struct's name"
        );
    }

    /// The refusal `Plan::parse` gives for a single operation line, as text.
    fn the_refusal_of(op_line: &str) -> String {
        Plan::parse(&plan_with(op_line))
            .map(|plan| plan.ops.len())
            .expect_err("the operation is refused")
            .to_string()
    }

    /// A `type` is one Rust type: two of them side by side are code, not a type.
    #[test]
    fn a_type_that_is_not_one_rust_type_is_refused() {
        // Given a parameter-type change whose `type` is two types
        let line = r#"{"op":"change_param_type","anchor":{"kind":"symbol","file":"src/pricing.rs","path":"label"},"name":"count","type":"u32 u32"}"#;

        // When it is parsed
        let refusal = the_refusal_of(line);

        // Then it is refused as malformed, naming the text
        assert_eq!(
            refusal,
            "plan is malformed: `type` must be exactly one Rust type, and `u32 u32` is not"
        );
    }

    /// An `expr` is one Rust expression: two calls separated by `;` are statements.
    #[test]
    fn an_expr_that_is_not_one_expression_is_refused() {
        // Given a call-argument change whose `expr` is two expressions
        let line = r#"{"op":"change_call_arg","anchor":{"kind":"symbol","file":"src/checkout.rs","path":"basket"},"variant":"first","expr":"total(1); total(2)"}"#;

        // When it is parsed
        let refusal = the_refusal_of(line);

        // Then it is refused as malformed, naming the text
        assert_eq!(
            refusal,
            "plan is malformed: `expr` must be exactly one Rust expression, and `total(1); total(2)` is not"
        );
    }

    /// A block is one expression, but the `let` inside it is a statement — code the engine should
    /// have produced, smuggled in as an argument.
    #[test]
    fn an_expr_carrying_a_statement_is_refused() {
        // Given a call-argument addition whose `expr` is a block holding a `let`
        let line = r#"{"op":"add_call_arg","anchor":{"kind":"symbol","file":"src/checkout.rs","path":"basket"},"variant":"last","expr":"{ let unit = \"items\"; unit }"}"#;

        // When it is parsed
        let refusal = the_refusal_of(line);

        // Then it is refused as malformed, saying it carries a statement
        assert_eq!(
            refusal,
            "plan is malformed: `expr` may carry no statement, and `{ let unit = \"items\"; unit }` carries one"
        );
    }

    /// A signature operation refused for what its fields say, each naming what is wrong.
    #[test]
    fn a_signature_operation_missing_or_misusing_a_field_is_refused_naming_it() {
        // Given operations that each break one rule of the signature fields
        let on_a_function = r#""anchor":{"kind":"symbol","file":"src/pricing.rs","path":"label"}"#;
        let on_a_symbol_call =
            r#""anchor":{"kind":"symbol","file":"src/checkout.rs","path":"basket"}"#;
        let lines = [
            // a `type` on an operation that cannot honour it
            format!(r#"{{"op":"reorder_params",{on_a_function},"type":"u32","order":["a"]}}"#),
            // a parameter change that names no parameter
            format!(r#"{{"op":"change_param_type",{on_a_function},"type":"u32"}}"#),
            // a position `add_param` does not know
            format!(
                r#"{{"op":"add_param",{on_a_function},"name":"unit","type":"u32","variant":"middle"}}"#
            ),
            // both a `type` and a `variant` on one return-type change
            format!(
                r#"{{"op":"change_return_type",{on_a_function},"type":"u32","variant":"unwrap"}}"#
            ),
            // a reorder with nothing to reorder to
            format!(r#"{{"op":"reorder_params",{on_a_function}}}"#),
            // a call-site operation anchored on a whole item
            format!(
                r#"{{"op":"change_call_arg",{on_a_symbol_call},"variant":"first","expr":"1"}}"#
            ),
        ];

        // When each is parsed
        let refusals = lines.map(|line| the_refusal_of(&line));

        // Then each is refused naming what is wrong
        assert_eq!(
            refusals,
            [
                "plan is malformed: `type` names a Rust type, which only `change_param_type`, `add_param` and `change_return_type` honour — `ReorderParams` cannot",
                "plan is malformed: `ChangeParamType` needs `name`",
                "plan is malformed: `add_param`'s `variant` is `first`, `last` or `after:<parameter>`, and `middle` is none of them",
                "plan is malformed: `change_return_type` needs exactly one of `type` and `variant`",
                "plan is malformed: `ReorderParams` needs `order`",
                "plan is malformed: `ChangeCallArg` is anchored on one call expression: an item anchor with a relative range over the call",
            ]
        );
    }

    /// The JSON field is `type`; the Rust field is `type_`.
    #[test]
    fn reads_a_parameter_type_change_naming_its_parameter_and_type() {
        // Given a plan changing `count` to `&str`, written the way a plan author writes it
        let jsonl = plan_with(
            r#"{"op":"change_param_type","anchor":{"kind":"symbol","file":"src/pricing.rs","path":"label"},"name":"count","type":"&str"}"#,
        );

        // When it is parsed
        let plan = Plan::parse(&jsonl).unwrap();

        // Then the operation carries the parameter and its new type
        assert_eq!(
            (
                plan.ops[0].op,
                plan.ops[0].name.as_deref(),
                plan.ops[0].type_.as_deref()
            ),
            (RefactorKind::ChangeParamType, Some("count"), Some("&str"))
        );
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

    const AN_ITEMS_ANCHOR: &str =
        r#"{"kind":"items","file":"src/a.rs","items":["app::a::f"],"fingerprints":["sha256:f"]}"#;

    #[test]
    fn reads_a_move_item_with_the_facade_it_asks_for() {
        let plan = Plan::parse(&plan_with(&format!(
            r#"{{"op":"move_item","anchor":{AN_ITEMS_ANCHOR},"to":"app::b","reexport":"glob"}}"#
        )))
        .unwrap();

        assert_eq!(plan.ops[0].op, RefactorKind::MoveItem);
        assert_eq!(plan.ops[0].reexport, Some(Reexport::Glob));
        assert!(!plan.ops[0].op.moves_across_crates());
    }

    #[test]
    fn refuses_a_move_item_that_names_no_destination() {
        let error = Plan::parse(&plan_with(&format!(
            r#"{{"op":"move_item","anchor":{AN_ITEMS_ANCHOR}}}"#
        )))
        .unwrap_err()
        .to_string();

        assert!(
            error.contains("move_item") && error.contains("`to`"),
            "{error}"
        );
    }

    #[test]
    fn refuses_a_move_item_anchored_by_range() {
        let error = Plan::parse(&plan_with(
            r#"{"op":"move_item","anchor":{"kind":"range","file":"src/a.rs","start":{"line":1,"col":1},"end":{"line":9,"col":1}},"to":"app::b"}"#,
        ))
        .unwrap_err()
        .to_string();

        assert!(error.contains("anchors by item"), "{error}");
    }

    #[test]
    fn refuses_a_move_item_that_asks_for_a_file_of_its_own() {
        let error = Plan::parse(&plan_with(&format!(
            r#"{{"op":"move_item","anchor":{AN_ITEMS_ANCHOR},"to":"app::b","to_file":true}}"#
        )))
        .unwrap_err()
        .to_string();

        assert!(
            error.contains("to_file") && error.contains("MoveItem"),
            "{error}"
        );
    }

    const A_MOD_DECLARATION_ANCHOR: &str = r#"{"kind":"items","file":"src/host.rs","items":["app::host::attachments"],"fingerprints":["sha256:f"]}"#;

    #[test]
    fn reads_a_reparent_module_with_the_facade_it_asks_for() {
        let plan = Plan::parse(&plan_with(&format!(
            r#"{{"op":"reparent_module","anchor":{A_MOD_DECLARATION_ANCHOR},"to":"app::split","reexport":"glob"}}"#
        )))
        .unwrap();

        assert_eq!(plan.ops[0].op, RefactorKind::ReparentModule);
        assert_eq!(plan.ops[0].reexport, Some(Reexport::Glob));
        assert!(!plan.ops[0].op.moves_across_crates());
    }

    #[test]
    fn refuses_a_reparent_module_that_names_no_new_parent() {
        let error = Plan::parse(&plan_with(&format!(
            r#"{{"op":"reparent_module","anchor":{A_MOD_DECLARATION_ANCHOR}}}"#
        )))
        .unwrap_err()
        .to_string();

        assert!(
            error.contains("reparent_module") && error.contains("`to`"),
            "{error}"
        );
    }

    #[test]
    fn refuses_a_reparent_module_anchored_by_range() {
        let error = Plan::parse(&plan_with(
            r#"{"op":"reparent_module","anchor":{"kind":"range","file":"src/host.rs","start":{"line":1,"col":1},"end":{"line":1,"col":20}},"to":"app::split"}"#,
        ))
        .unwrap_err()
        .to_string();

        assert!(error.contains("anchors by item"), "{error}");
    }

    #[test]
    fn refuses_a_named_facade_for_a_module() {
        let error = Plan::parse(&plan_with(&format!(
            r#"{{"op":"reparent_module","anchor":{A_MOD_DECLARATION_ANCHOR},"to":"app::split","reexport":"named"}}"#
        )))
        .unwrap_err()
        .to_string();

        assert!(
            error.contains("reparent_module") && error.contains("glob"),
            "{error}"
        );
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
        let path = item_path::ItemPath::parse("tddy_core::workflow::Stack::new").unwrap();

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
        let path = item_path::ItemPath::parse("stacks::workflow::<Stack as Debug>::fmt").unwrap();

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
    fn an_inherent_impl_segment_names_its_type() {
        let path = item_path::ItemPath::parse("stacks::workflow::<Stack>").unwrap();

        assert_eq!(
            path.segments().last(),
            Some(&ItemSegment::InherentImpl {
                self_type: "Stack".to_string(),
                nth: None,
            })
        );
    }

    #[test]
    fn an_inherent_impl_segment_carries_its_one_based_ordinal() {
        let path = item_path::ItemPath::parse("stacks::workflow::<Stack>#2").unwrap();

        assert_eq!(
            path.segments().last(),
            Some(&ItemSegment::InherentImpl {
                self_type: "Stack".to_string(),
                nth: Some(2),
            })
        );
    }

    #[test]
    fn an_inherent_impl_of_a_generic_type_keeps_its_arguments() {
        let path = item_path::ItemPath::parse("c::m::<Wrapper<T>>").unwrap();

        assert_eq!(
            path.segments().last(),
            Some(&ItemSegment::InherentImpl {
                self_type: "Wrapper<T>".to_string(),
                nth: None,
            })
        );
    }

    #[test]
    fn a_lifetime_generic_self_type_parses_as_an_inherent_impl() {
        let path = item_path::ItemPath::parse("c::m::<Foo<'a>>").unwrap();

        assert_eq!(path.segments().len(), 2);
        assert_eq!(path.segments()[1].spelled(), "<Foo<'a>>");
    }

    #[test]
    fn a_trait_separator_inside_nested_brackets_is_not_the_impls_trait() {
        let path = item_path::ItemPath::parse("c::m::<Wrapper<A as B>>").unwrap();

        assert_eq!(
            path.segments().last(),
            Some(&ItemSegment::InherentImpl {
                self_type: "Wrapper<A as B>".to_string(),
                nth: None,
            })
        );
    }

    #[test]
    fn inherent_impl_paths_round_trip_through_their_spelling_and_serde() {
        for written in [
            "crate::m::<Stack>",
            "crate::m::<Stack>#2",
            "crate::m::<Wrapper<T>>",
            "crate::m::<Stack as Display>",
        ] {
            let path = item_path::ItemPath::parse(written).unwrap();
            let spelled: Vec<String> = path
                .segments()
                .iter()
                .map(item_path::ItemSegment::spelled)
                .collect();
            let json = serde_json::to_string(&path).unwrap();

            assert_eq!(format!("crate::{}", spelled.join("::")), written);
            assert_eq!(
                serde_json::from_str::<ItemPath>(&json).unwrap().to_string(),
                written
            );
        }
    }

    #[test]
    fn a_malformed_inherent_impl_segment_is_refused() {
        for written in [
            "crate::m::<>",
            "crate::m::<Stack>#0",
            "crate::m::<Stack>#x",
            "crate::m::<Stack>#",
        ] {
            assert!(ItemPath::parse(written).is_err(), "{written} was accepted");
        }
    }

    #[test]
    fn an_item_path_of_one_segment_is_refused() {
        // When a bare name is parsed as an item path
        let parsed = item_path::ItemPath::parse("Stack");

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
                    modified: Some("2026-09-26T00:00:00Z".to_string()),
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
    fn a_relative_range_with_only_one_end_is_refused() {
        // Given an item anchor that gives a start and no end
        let line = r#"{"op":"rename_symbol","anchor":{"kind":"item","item":"a::b::C","file":"src/b.rs","start":{"line":2,"col":1},"fingerprint":"sha256:cd"},"name":"D"}"#;
        let jsonl = format!(r#"{{"v":2,"files":{{}}}}{}{line}"#, "\n");

        // When the plan is parsed
        let refused = Plan::parse(&jsonl)
            .map(|_| ())
            .map_err(|error| error.to_string());

        // Then it is refused naming the item
        assert_eq!(
            refused,
            Err(
                "plan is malformed: `a::b::C` gives one end of a relative range — give both \
                 `start` and `end`, or neither to anchor the item itself"
                    .to_string()
            )
        );
    }

    #[test]
    fn an_items_anchor_with_a_fingerprint_missing_is_refused() {
        let line = r#"{"op":"extract_module","anchor":{"kind":"items","file":"src/b.rs","items":["a::b::C","a::b::D"],"fingerprints":["sha256:cd"]},"name":"m"}"#;
        let jsonl = format!(r#"{{"v":2,"files":{{}}}}{}{line}"#, "\n");

        assert!(Plan::parse(&jsonl)
            .err()
            .is_some_and(|error| error.to_string().contains("2 item(s) and 1 fingerprint(s)")));
    }

    #[test]
    fn a_v2_header_is_restated_with_the_hash_and_time_the_file_holds_now() {
        // Given a v2 plan hinting at one file, and that file's content
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("lib.rs"), "pub struct A;\n").unwrap();
        let plan = Plan {
            version: 2,
            snapshot: BTreeMap::new(),
            files: BTreeMap::from([(
                "lib.rs".to_string(),
                FileHint {
                    sha256: "sha256:stale".to_string(),
                    modified: Some("2020-01-01T00:00:00Z".to_string()),
                },
            )]),
            ops: vec![],
        };

        // When its header is rehashed
        let header = plan.rehashed_header(workspace.path()).unwrap();

        // Then it is a v2 header whose hint is the file as it stands
        let reread = Plan::parse(&header).unwrap();
        assert_eq!(reread.version, 2);
        assert_eq!(
            reread.files["lib.rs"].sha256,
            crate::apply::hash_file(&workspace.path().join("lib.rs")).unwrap()
        );
        assert_eq!(plan.drifted_hints(workspace.path()), vec!["lib.rs"]);
        assert_eq!(reread.drifted_hints(workspace.path()), Vec::<String>::new());
    }

    #[test]
    fn a_hinted_file_that_is_gone_is_reported_as_drift_not_as_an_error() {
        // Given a v2 plan hinting at a file the tree no longer has — even one hinted while empty,
        // which hashes the same as a missing file
        let workspace = tempfile::tempdir().unwrap();
        let empty = crate::apply::hash_file(&workspace.path().join("absent.rs")).unwrap();
        let plan = Plan {
            version: 2,
            snapshot: BTreeMap::new(),
            files: BTreeMap::from([(
                "gone.rs".to_string(),
                FileHint {
                    sha256: empty,
                    modified: None,
                },
            )]),
            ops: vec![],
        };

        // When the hints are compared with the tree
        let drifted = plan.drifted_hints(workspace.path());

        // Then the missing file is named as drifted
        assert_eq!(drifted, vec!["gone.rs"]);
    }

    #[test]
    fn a_timestamp_is_written_as_rfc_3339_utc() {
        let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);

        assert_eq!(rfc3339(time).as_deref(), Some("2001-09-09T01:46:40Z"));
    }

    #[test]
    fn a_time_before_the_epoch_is_left_out_rather_than_written_as_1970() {
        let time = std::time::UNIX_EPOCH - std::time::Duration::from_secs(60);

        assert_eq!(rfc3339(time), None);
    }

    #[test]
    fn an_unbalanced_qualified_segment_is_not_an_item_path() {
        assert!(ItemPath::parse("a::<Stack as Debug::fmt").is_err());
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

    #[test]
    fn ids_are_numbered_past_the_highest_one_a_plan_already_uses() {
        // Given a plan whose second operation was removed, leaving op-1 and op-3, and a new one
        let jsonl = concat!(
            r#"{"v":1,"snapshot":{}}"#,
            "\n",
            r#"{"id":"op-1","op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"A"},"name":"B"}"#,
            "\n",
            r#"{"id":"op-3","op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"C"},"name":"D"}"#,
            "\n",
            r#"{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"E"},"name":"F"}"#,
            "\n"
        );
        let mut plan = Plan::parse(jsonl).unwrap();

        // When ids are assigned
        let assigned = plan.assign_missing_op_ids();

        // Then the new operation takes the next number, not the free one
        let ids: Vec<_> = plan.ops.iter().filter_map(|op| op.id.clone()).collect();
        assert_eq!(
            (assigned, ids),
            (
                true,
                vec![
                    OpId("op-1".to_string()),
                    OpId("op-3".to_string()),
                    OpId("op-4".to_string())
                ]
            )
        );
    }

    /// A rename of `symbol` in `src/shapes.rs`, as one plan line, in `group` when it names one.
    fn a_rename_line(symbol: &str, to: &str, group: Option<&str>) -> String {
        let group = group.map_or(String::new(), |group| format!(r#","group":"{group}""#));
        format!(
            r#"{{"op":"rename_symbol","anchor":{{"kind":"symbol","file":"src/shapes.rs","path":"{symbol}"}},"name":"{to}"{group}}}"#
        )
    }

    /// A group applies as one unit, so its members have to be one run of operations: an ungrouped
    /// operation between two of them would be applied inside a unit it is not part of, and rolled
    /// back with it.
    #[test]
    fn non_consecutive_members_of_one_group_are_refused() {
        // Given a group whose two members have an ungrouped operation between them
        let jsonl = format!(
            "{HEADER}\n{}\n{}\n{}\n",
            a_rename_line("Circle", "Disc", Some("shapes")),
            a_rename_line("Square", "Block", None),
            a_rename_line("Triangle", "Wedge", Some("shapes")),
        );

        // When
        let outcome = Plan::parse(&jsonl);

        // Then it is refused as malformed, naming the group
        let refusal = outcome
            .map(|plan| plan.ops.len())
            .map_err(|error| match error {
                RestructureError::MalformedPlan(reason) => reason.contains("`shapes`"),
                _ => false,
            });
        assert_eq!(refusal, Err(true));
    }

    /// A misspelt field is how a group would be lost without a word: `"gruop"` parsed and was
    /// dropped, and the operations ran ungrouped, with none of the rollback the author asked for.
    #[test]
    fn an_unknown_operation_field_is_refused() {
        // Given an operation carrying a field the schema does not define
        let line = r#"{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/shapes.rs","path":"Circle"},"name":"Disc","gruop":"shapes"}"#;

        // When
        let outcome = Plan::parse(&plan_with(line));

        // Then it is refused as malformed, naming the field
        let refusal = outcome
            .map(|plan| plan.ops.len())
            .map_err(|error| match error {
                RestructureError::MalformedPlan(reason) => reason.contains("gruop"),
                _ => false,
            });
        assert_eq!(refusal, Err(true));
    }

    #[test]
    fn rejects_a_move_item_anchored_by_range_or_symbol() {
        let refusals: Vec<String> = [
            r#"{"kind":"range","file":"src/a.rs","start":{"line":1,"col":1},"end":{"line":2,"col":1}}"#,
            r#"{"kind":"symbol","file":"src/a.rs","path":"f"}"#,
        ]
        .iter()
        .map(|anchor| {
            let line = format!(r#"{{"op":"move_item","anchor":{anchor},"to":"app::b"}}"#);
            match Plan::parse(&plan_with(&line)) {
                Err(RestructureError::MalformedPlan(reason)) => reason,
                other => format!("not refused as malformed: {other:?}"),
            }
        })
        .collect();

        assert!(
            refusals
                .iter()
                .all(|reason| reason.contains("names no module-level item to move")),
            "{refusals:?}"
        );
    }

    #[test]
    fn rejects_a_name_on_reparent_module_which_creates_no_module() {
        let line = r#"{"op":"reparent_module","anchor":{"kind":"items","file":"src/a.rs","items":["app::a::m"],"fingerprints":["sha256:ab"]},"to":"app::b","name":"x"}"#;

        let refusal = match Plan::parse(&plan_with(line)) {
            Err(RestructureError::MalformedPlan(reason)) => reason,
            other => format!("not refused as malformed: {other:?}"),
        };

        assert!(refusal.contains("creates no module"), "{refusal}");
    }
}
