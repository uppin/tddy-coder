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
}

impl Anchor {
    pub fn file(&self) -> &str {
        match self {
            Anchor::Symbol { file, .. } | Anchor::Range { file, .. } => file,
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefactorOp {
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
}

impl RefactorOp {
    /// The same operation, addressed at a translated anchor.
    pub fn with_anchor(&self, anchor: Anchor) -> RefactorOp {
        RefactorOp {
            anchor,
            ..self.clone()
        }
    }
}

/// A parsed plan: the snapshot header plus the ordered operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub version: u32,
    /// Content hash per file the plan touches, taken when the plan was written.
    pub snapshot: BTreeMap<String, String>,
    pub ops: Vec<RefactorOp>,
}

/// Schema version this executor understands.
const SCHEMA_VERSION: u32 = 1;

/// Fields that would carry source text. Their presence means the plan is trying to supply code the
/// language engine should have produced, so the plan is refused rather than partially honoured.
const CODE_BEARING_FIELDS: [&str; 3] = ["text", "code", "content"];

#[derive(Deserialize)]
struct SnapshotHeader {
    v: u32,
    snapshot: BTreeMap<String, String>,
}

impl Plan {
    /// Parse a JSONL plan: line 1 is the snapshot header, every later line is one operation.
    pub fn parse(jsonl: &str) -> Result<Plan> {
        let mut lines = jsonl.lines().filter(|line| !line.trim().is_empty());

        let header = lines.next().ok_or_else(|| malformed("plan is empty"))?;
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
            ops,
        })
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
    if op.reexport.is_some() && op.op != RefactorKind::ExtractModule {
        return Err(malformed(format!(
            "`reexport` asks for a facade in the parent module, which only `extract_module` writes — \
             `{:?}` cannot honour one",
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
            Anchor::Symbol { .. } => None,
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
        let jsonl = "{\"v\":2,\"snapshot\":{}}\n";

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
}
