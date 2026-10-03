//! Folding one operation of a held plan into every other held plan.
//!
//! Judged line by line against the edit the operation produced, in the coordinates of the text the
//! edit was made against: what it overlaps is stale, what it merely moves is moved.

use std::path::Path;

use super::super::refresh::resolved_or_left_as_written;
use super::super::{LoadedPlan, PlanKey, PlanStore, StaleReason};
use super::{commit, followed_hints, Refreshed};
use crate::edit::{FileEdit, Position, TextEdit, WorkspaceEdit};
use crate::item_anchor::{absolute_range, ItemResolver, ResolvedItem};
use crate::ledger::PositionLedger;
use crate::plan::{Anchor, ItemPath, OpId, RefactorOp};
use crate::{Fingerprint, RestructureError, Result};

/// Whether `edit` changes any of lines `first..=last` of the text it was made against.
///
/// A whole-line insertion between two lines changes none of them — and so moves an anchored range
/// without editing it — unless it lands between two lines the range covers. Any other edit changes
/// the lines from its start line to its end line, the end line excluded when the edit stops at its
/// first column.
fn overlaps(edit: &TextEdit, first: u32, last: u32) -> bool {
    let (start, end) = (edit.range.start, edit.range.end);
    let whole_lines = edit.new_text.is_empty() || edit.new_text.ends_with('\n');
    if start == end && start.col == 1 && whole_lines {
        return !edit.new_text.is_empty() && start.line > first && start.line <= last;
    }
    let last_edited = if end.col == 1 && end.line > start.line {
        end.line - 1
    } else {
        end.line
    };
    start.line <= last && last_edited >= first
}

/// One operation of another plan, as far as the plans that did not run it are concerned.
struct Fold<'a> {
    edit: &'a WorkspaceEdit,
    ledger: PositionLedger,
}

impl Fold<'_> {
    /// Where a file the edit may have moved is now.
    fn followed(&self, file: &str) -> String {
        self.ledger
            .current_path(Path::new(file))
            .display()
            .to_string()
    }

    /// The text edits made to `file`, whether the edit names it as it was or as it is.
    fn edits_to(&self, file: &str, now: &str) -> Vec<&TextEdit> {
        self.edit
            .changes
            .iter()
            .filter_map(|change| match change {
                FileEdit::Change { path, edits } if path == file || path == now => Some(edits),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn edited_inside(&self, file: &str, now: &str, first: u32, last: u32) -> bool {
        self.edits_to(file, now)
            .iter()
            .any(|edit| overlaps(edit, first, last))
    }

    /// `at` once the edit has been made, or `None` when the edit removed the text it was in.
    fn translate(&self, file: &str, at: Position) -> Result<Option<Position>> {
        match self.ledger.translate(Path::new(file), at) {
            Ok(moved) => Ok(Some(moved)),
            Err(RestructureError::AnchorInvalidated { .. }) => Ok(None),
            Err(failure) => Err(failure),
        }
    }
}

/// Fold one operation, which produced `edit`, into every held plan but `from`.
pub(in crate::plan_store) fn fold(
    store: &mut PlanStore,
    from: &PlanKey,
    op: &OpId,
    edit: &WorkspaceEdit,
    resolver: &mut dyn ItemResolver,
) -> Result<()> {
    let touched = crate::apply::touched_paths(edit);
    store.live.own_writes.extend(touched.iter().cloned());
    let mut ledger = PositionLedger::new();
    ledger.record(edit);
    let fold = Fold { edit, ledger };

    // Every plan is worked out before any is changed, so a refusal part-way leaves them all as they
    // were.
    let reason = StaleReason::EditedBy {
        plan: from.clone(),
        op: op.clone(),
    };
    let mut folded = Vec::new();
    for (key, held) in store.plans.iter().filter(|(key, _)| *key != from) {
        let refreshed = fold_plan(store, held, (&fold, &reason), &touched, resolver)?;
        folded.push((key.clone(), refreshed));
    }
    for (key, refreshed) in folded {
        commit(store, &key, refreshed);
    }
    Ok(())
}

/// `held` after the fold: `through` is the edit and what an operation it overlaps is stale for.
fn fold_plan(
    store: &PlanStore,
    held: &LoadedPlan,
    through: (&Fold, &StaleReason),
    touched: &[String],
    resolver: &mut dyn ItemResolver,
) -> Result<Refreshed> {
    let (fold, reason) = through;
    let mut ops = held.plan.ops.clone();
    let mut verdicts = Vec::new();
    // TODO(live-plans): every operation is folded, including ones the plan already ran, because the
    // store holds no journal. An operation that already ran can be reported stale for an edit that
    // overlaps where it used to anchor; a run only refuses the ones at or after its start.
    // See docs/dev/todo/2026-10-03-live-plans-three-gaps-in-staleness-reporting-and-snapshot-routing.md.
    for op in &mut ops {
        let Some(id) = op.id.clone() else { continue };
        // A stale operation is left exactly as it was: following the tree would be re-targeting it.
        if store.live.reason_for(&held.key, &id).is_some() {
            continue;
        }
        match fold_op(fold, op, resolver)? {
            Some(folded) => *op = folded,
            None => verdicts.push((id, Some(reason.clone()))),
        }
    }
    let files = followed_hints(&held.plan.files, &fold.ledger, &store.root, touched)?;
    Ok(Refreshed {
        ops,
        files,
        verdicts,
    })
}

/// `op` as the edit left the tree, or `None` when the edit overlapped something it names.
fn fold_op(
    fold: &Fold,
    op: &RefactorOp,
    resolver: &mut dyn ItemResolver,
) -> Result<Option<RefactorOp>> {
    let Some(anchor) = fold_anchor(fold, &op.anchor, resolver)? else {
        return Ok(None);
    };
    let mut also = Vec::with_capacity(op.also.len());
    for member in &op.also {
        match fold_anchor(fold, member, resolver)? {
            Some(folded) => also.push(folded),
            None => return Ok(None),
        }
    }
    let mut folded = op.with_anchor(anchor);
    folded.also = also;
    Ok(Some(folded))
}

fn fold_anchor(
    fold: &Fold,
    anchor: &Anchor,
    resolver: &mut dyn ItemResolver,
) -> Result<Option<Anchor>> {
    match anchor {
        Anchor::Symbol { file, path } => Ok(Some(Anchor::Symbol {
            file: fold.followed(file),
            path: path.clone(),
        })),
        Anchor::Range { file, start, end } => fold_range(fold, file, *start, *end),
        Anchor::Item { .. } => fold_item(fold, anchor, resolver),
        Anchor::Items {
            file,
            items,
            fingerprints,
        } => fold_items(fold, file, items, fingerprints, resolver),
    }
}

fn fold_range(fold: &Fold, file: &str, start: Position, end: Position) -> Result<Option<Anchor>> {
    let now = fold.followed(file);
    if fold.edited_inside(file, &now, start.line, end.line) {
        return Ok(None);
    }
    let (Some(start), Some(end)) = (fold.translate(file, start)?, fold.translate(file, end)?)
    else {
        return Ok(None);
    };
    Ok(Some(Anchor::Range {
        file: now,
        start,
        end,
    }))
}

/// An item anchor after the edit: it follows its item when the item is untouched or the edit stayed
/// outside the range it names, and is `None` when the edit reached into that range.
///
/// Judged against the tree the edit left: the item resolves there, and a fingerprint that moved
/// says the edit touched the item. Where in the item it touched is then read from the anchor's
/// `hint`, the only record of where the anchored range sat before.
fn fold_item(
    fold: &Fold,
    anchor: &Anchor,
    resolver: &mut dyn ItemResolver,
) -> Result<Option<Anchor>> {
    let Anchor::Item {
        item,
        file,
        start,
        end,
        fingerprint,
        hint,
    } = anchor
    else {
        return Ok(Some(anchor.clone()));
    };
    let now = fold.followed(file);
    let mut followed = Anchor::Item {
        item: item.clone(),
        file: now.clone(),
        start: *start,
        end: *end,
        fingerprint: fingerprint.clone(),
        hint: *hint,
    };
    if fold.edits_to(file, &now).is_empty() {
        return Ok(Some(followed));
    }
    let Some(found) = resolved_or_left_as_written(resolver.resolve_item(&now, item))? else {
        return Ok(None);
    };
    if found.fingerprint != *fingerprint {
        return follow_changed_item(fold, file, &found, &followed);
    }
    if let (Anchor::Item { hint, .. }, Ok(range)) =
        (&mut followed, absolute_range(&found, *start, *end))
    {
        *hint = Some(range.start);
    }
    Ok(Some(followed))
}

/// The anchor of an item the edit changed, followed through the edit when the edit stayed outside
/// the range the anchor names.
fn follow_changed_item(
    fold: &Fold,
    written_in: &str,
    found: &ResolvedItem,
    anchor: &Anchor,
) -> Result<Option<Anchor>> {
    let Anchor::Item {
        item,
        file,
        start,
        end,
        hint,
        ..
    } = anchor
    else {
        return Ok(Some(anchor.clone()));
    };
    let Some(hint) = *hint else {
        return Ok(None);
    };
    let relative = start.zip(*end);
    let lines = relative.map_or(0, |(start, end)| end.line.saturating_sub(start.line));
    if fold.edited_inside(written_in, file, hint.line, hint.line + lines) {
        return Ok(None);
    }
    let (start, end, hint) = match relative {
        None => (None, None, found.name),
        Some(relative) => {
            let Some((start, end)) = rebased_range(fold, written_in, found, hint, relative)? else {
                return Ok(None);
            };
            let Ok(range) = absolute_range(found, Some(start), Some(end)) else {
                return Ok(None);
            };
            (Some(start), Some(end), range.start)
        }
    };
    Ok(Some(Anchor::Item {
        item: item.clone(),
        file: file.clone(),
        start,
        end,
        fingerprint: found.fingerprint.clone(),
        hint: Some(hint),
    }))
}

/// The range an item anchor names, relative to its item as the edit left it: the absolute range it
/// had (`hint` is where it started) carried through the edit, then measured from the item's new
/// first line.
fn rebased_range(
    fold: &Fold,
    written_in: &str,
    found: &ResolvedItem,
    hint: Position,
    relative: (Position, Position),
) -> Result<Option<(Position, Position)>> {
    let (start, end) = relative;
    let last = Position {
        line: hint.line + end.line.saturating_sub(start.line),
        col: end.col,
    };
    let (Some(new_start), Some(new_end)) = (
        fold.translate(written_in, hint)?,
        fold.translate(written_in, last)?,
    ) else {
        return Ok(None);
    };
    let first = found.range.start.line;
    if new_start.line < first || new_end.line < first {
        return Ok(None);
    }
    Ok(Some((
        Position {
            line: new_start.line - first + 1,
            col: start.col,
        },
        Position {
            line: new_end.line - first + 1,
            col: end.col,
        },
    )))
}

fn fold_items(
    fold: &Fold,
    file: &str,
    items: &[ItemPath],
    fingerprints: &[Fingerprint],
    resolver: &mut dyn ItemResolver,
) -> Result<Option<Anchor>> {
    let now = fold.followed(file);
    let followed = Anchor::Items {
        file: now.clone(),
        items: items.to_vec(),
        fingerprints: fingerprints.to_vec(),
    };
    if fold.edits_to(file, &now).is_empty() {
        return Ok(Some(followed));
    }
    for (item, fingerprint) in items.iter().zip(fingerprints) {
        match resolved_or_left_as_written(resolver.resolve_item(&now, item))? {
            Some(found) if found.fingerprint == *fingerprint => {}
            _ => return Ok(None),
        }
    }
    Ok(Some(followed))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::*;
    use crate::plan_store::{FlushPolicy, OpStaleness};
    use crate::Range;

    fn at(line: u32, col: u32) -> Position {
        Position { line, col }
    }

    fn an_edit(first: Position, last: Position, new_text: &str) -> TextEdit {
        TextEdit {
            range: Range {
                start: first,
                end: last,
            },
            new_text: new_text.to_string(),
        }
    }

    // --- which lines an edit changes -------------------------------------------------------

    #[test]
    fn a_whole_line_insertion_between_two_covered_lines_overlaps_the_range() {
        // Given lines 20–22 and three lines inserted at the start of line 21
        let insertion = an_edit(at(21, 1), at(21, 1), "a\nb\nc\n");

        // Then it overlaps: it lands between two lines the range covers
        assert!(overlaps(&insertion, 20, 22));
    }

    #[test]
    fn a_whole_line_insertion_before_the_last_covered_line_overlaps_the_range() {
        // Given lines 20–22 and a line inserted at the start of line 22
        let insertion = an_edit(at(22, 1), at(22, 1), "a\n");

        // Then it overlaps: it lands between lines 21 and 22
        assert!(overlaps(&insertion, 20, 22));
    }

    #[test]
    fn a_whole_line_insertion_just_above_the_range_only_moves_it() {
        // Given lines 20–22 and a line inserted at the start of line 20
        let insertion = an_edit(at(20, 1), at(20, 1), "a\n");

        // Then it does not overlap: nothing the range covers was changed
        assert!(!overlaps(&insertion, 20, 22));
    }

    #[test]
    fn a_whole_line_insertion_just_below_the_range_only_moves_what_follows() {
        // Given lines 20–22 and a line inserted at the start of line 23
        let insertion = an_edit(at(23, 1), at(23, 1), "a\n");

        // Then it does not overlap
        assert!(!overlaps(&insertion, 20, 22));
    }

    #[test]
    fn an_insertion_inside_a_covered_line_overlaps_the_range() {
        // Given lines 20–22 and text inserted mid-line in line 21
        let insertion = an_edit(at(21, 5), at(21, 5), "x");

        // Then it overlaps: line 21 changed
        assert!(overlaps(&insertion, 20, 22));
    }

    #[test]
    fn an_edit_ending_at_the_first_column_of_the_first_covered_line_does_not_reach_it() {
        // Given lines 20–22 and an edit replacing line 19, which stops where line 20 begins
        let replacement = an_edit(at(19, 1), at(20, 1), "x\n");

        // Then it does not overlap: line 20 is not part of what it replaced
        assert!(!overlaps(&replacement, 20, 22));
    }

    #[test]
    fn an_edit_ending_past_the_first_column_of_the_first_covered_line_reaches_it() {
        // Given lines 20–22 and an edit that stops inside line 20
        let replacement = an_edit(at(19, 1), at(20, 2), "x");

        // Then it overlaps
        assert!(overlaps(&replacement, 20, 22));
    }

    #[test]
    fn an_edit_ending_at_the_first_column_of_the_line_after_the_range_covers_its_last_line() {
        // Given lines 20–22 and an edit replacing line 22, which stops where line 23 begins
        let replacement = an_edit(at(22, 1), at(23, 1), "x\n");

        // Then it overlaps: line 22 was replaced
        assert!(overlaps(&replacement, 20, 22));
    }

    #[test]
    fn an_edit_starting_on_the_line_after_the_range_does_not_reach_it() {
        // Given lines 20–22 and an edit replacing line 23
        let replacement = an_edit(at(23, 1), at(24, 1), "x\n");

        // Then it does not overlap
        assert!(!overlaps(&replacement, 20, 22));
    }

    #[test]
    fn a_deletion_that_starts_above_and_ends_inside_the_range_overlaps_it() {
        // Given lines 20–22 and a deletion of lines 18–20
        let deletion = an_edit(at(18, 1), at(21, 1), "");

        // Then it overlaps
        assert!(overlaps(&deletion, 20, 22));
    }

    // --- folding another plan's operation --------------------------------------------------

    /// A resolver whose answer the test chooses: the item starting at `first_line`, six lines long,
    /// fingerprinted as given — or not there at all.
    enum AResolverAnswering {
        Item {
            first_line: u32,
            fingerprint: &'static str,
        },
        Missing,
    }

    impl ItemResolver for AResolverAnswering {
        fn resolve_item(&mut self, file: &str, item: &ItemPath) -> Result<ResolvedItem> {
            match self {
                AResolverAnswering::Item {
                    first_line,
                    fingerprint,
                } => Ok(ResolvedItem {
                    range: Range {
                        start: at(*first_line, 1),
                        end: at(*first_line + 5, 2),
                    },
                    name: at(*first_line, 8),
                    fingerprint: Fingerprint(fingerprint.to_string()),
                }),
                AResolverAnswering::Missing => Err(RestructureError::MalformedPlan(format!(
                    "`{item}` is not declared in {file}"
                ))),
            }
        }
    }

    const A_RANGE_OP: &str = r#"{"id":"b1","op":"extract_method","anchor":{"kind":"range","file":"src/a.rs","start":{"line":20,"col":5},"end":{"line":22,"col":6}},"name":"f"}"#;

    /// Written when `a::f` began at line 19: the anchored range is lines 20–21 of the file.
    const AN_ITEM_OP: &str = r#"{"id":"b1","op":"extract_method","anchor":{"kind":"item","item":"a::f","file":"src/a.rs","start":{"line":2,"col":5},"end":{"line":3,"col":6},"fingerprint":"sha256:written","hint":{"line":20,"col":5}},"name":"g"}"#;
    const AN_ITEM_OP_WITHOUT_A_HINT: &str = r#"{"id":"b1","op":"extract_method","anchor":{"kind":"item","item":"a::f","file":"src/a.rs","start":{"line":2,"col":5},"end":{"line":3,"col":6},"fingerprint":"sha256:written"},"name":"g"}"#;
    const A_WHOLE_ITEM_OP: &str = r#"{"id":"b1","op":"extract_method","anchor":{"kind":"item","item":"a::f","file":"src/a.rs","fingerprint":"sha256:written","hint":{"line":19,"col":8}},"name":"g"}"#;
    const AN_ITEMS_OP: &str = r#"{"id":"b1","op":"extract_module","anchor":{"kind":"items","file":"src/a.rs","items":["a::f","a::g"],"fingerprints":["sha256:written","sha256:written"]},"name":"m"}"#;

    /// `first.jsonl` holding the one operation `a1`, whose edit is folded, beside `second.jsonl`
    /// holding the operation `b1` given.
    fn second_holding(op: &str) -> (tempfile::TempDir, PlanStore, PlanKey, PlanKey) {
        let root = tempfile::tempdir().unwrap();
        let first_op = r#"{"id":"a1","op":"rename_symbol","anchor":{"kind":"symbol","file":"src/a.rs","path":"A"},"name":"B"}"#;
        for (name, op) in [("first.jsonl", first_op), ("second.jsonl", op)] {
            std::fs::write(
                root.path().join(name),
                format!("{{\"v\":1,\"snapshot\":{{}}}}\n{op}\n"),
            )
            .unwrap();
        }
        let mut store = PlanStore::new(
            root.path(),
            FlushPolicy {
                debounce: Duration::from_secs(3600),
            },
        );
        store
            .load(&[PathBuf::from("first.jsonl"), PathBuf::from("second.jsonl")])
            .unwrap();
        let first = store.key_for(Path::new("first.jsonl")).unwrap();
        let second = store.key_for(Path::new("second.jsonl")).unwrap();
        (root, store, first, second)
    }

    fn editing(edits: Vec<TextEdit>) -> WorkspaceEdit {
        WorkspaceEdit {
            changes: vec![FileEdit::Change {
                path: "src/a.rs".to_string(),
                edits,
            }],
        }
    }

    fn folded(
        store: &mut PlanStore,
        first: &PlanKey,
        edit: &WorkspaceEdit,
        resolver: &mut AResolverAnswering,
    ) {
        store
            .fold_foreign_op(first, &OpId("a1".to_string()), edit, resolver)
            .unwrap();
    }

    fn anchor_of_b1(store: &PlanStore, second: &PlanKey) -> Anchor {
        store.get(second).unwrap().plan.ops[0].anchor.clone()
    }

    fn edited_by_a1(first: &PlanKey) -> Vec<OpStaleness> {
        vec![OpStaleness {
            op: OpId("b1".to_string()),
            reason: StaleReason::EditedBy {
                plan: first.clone(),
                op: OpId("a1".to_string()),
            },
        }]
    }

    fn an_item_anchor(
        (start, end): (Option<Position>, Option<Position>),
        fingerprint: &str,
        hint: Option<Position>,
    ) -> Anchor {
        Anchor::Item {
            item: ItemPath::parse("a::f").unwrap(),
            file: "src/a.rs".to_string(),
            start,
            end,
            fingerprint: Fingerprint(fingerprint.to_string()),
            hint,
        }
    }

    #[test]
    fn a_whole_line_insertion_between_covered_lines_marks_a_range_op_edited_by_the_other_plan() {
        // Given plan `second` anchored at lines 20–22
        let (_root, mut store, first, second) = second_holding(A_RANGE_OP);

        // When the other plan's op inserted a line between lines 20 and 21
        folded(
            &mut store,
            &first,
            &editing(vec![an_edit(at(21, 1), at(21, 1), "x();\n")]),
            &mut AResolverAnswering::Missing,
        );

        // Then the op is stale, and its anchor is left as written
        assert_eq!(store.stale_ops(&second), edited_by_a1(&first));
        assert_eq!(
            anchor_of_b1(&store, &second),
            Anchor::Range {
                file: "src/a.rs".to_string(),
                start: at(20, 5),
                end: at(22, 6)
            }
        );
    }

    #[test]
    fn a_whole_line_insertion_just_above_a_range_op_moves_it_without_marking_it_stale() {
        // Given plan `second` anchored at lines 20–22
        let (_root, mut store, first, second) = second_holding(A_RANGE_OP);

        // When the other plan's op inserted a line at the start of line 20
        folded(
            &mut store,
            &first,
            &editing(vec![an_edit(at(20, 1), at(20, 1), "x();\n")]),
            &mut AResolverAnswering::Missing,
        );

        // Then the op moved down a line and is not stale
        assert_eq!(
            anchor_of_b1(&store, &second),
            Anchor::Range {
                file: "src/a.rs".to_string(),
                start: at(21, 5),
                end: at(23, 6)
            }
        );
        assert_eq!(store.stale_ops(&second), Vec::new());
    }

    #[test]
    fn an_edit_below_the_anchored_range_of_a_changed_item_leaves_the_anchor_on_its_lines() {
        // Given an item anchor on lines 20–21 of `a::f`, which spans lines 19–24
        let (_root, mut store, first, second) = second_holding(AN_ITEM_OP);

        // When the other plan's op replaced line 23 inside the item, below the anchored lines
        folded(
            &mut store,
            &first,
            &editing(vec![an_edit(at(23, 1), at(24, 1), "x();\ny();\n")]),
            &mut AResolverAnswering::Item {
                first_line: 19,
                fingerprint: "sha256:after",
            },
        );

        // Then the anchor keeps its range, takes the item's new fingerprint, and is not stale
        assert_eq!(
            anchor_of_b1(&store, &second),
            an_item_anchor(
                (Some(at(2, 5)), Some(at(3, 6))),
                "sha256:after",
                Some(at(20, 5))
            )
        );
        assert_eq!(store.stale_ops(&second), Vec::new());
    }

    #[test]
    fn an_edit_above_the_anchored_range_of_a_changed_item_moves_the_range_within_the_item() {
        // Given an item anchor on lines 20–21 of `a::f`, which begins at line 19
        let (_root, mut store, first, second) = second_holding(AN_ITEM_OP);

        // When the other plan's op replaced line 19 with three lines, and the item still begins there
        folded(
            &mut store,
            &first,
            &editing(vec![an_edit(at(19, 1), at(20, 1), "a\nb\nc\n")]),
            &mut AResolverAnswering::Item {
                first_line: 19,
                fingerprint: "sha256:after",
            },
        );

        // Then the range is two lines further into the item, and the hint is where it now starts
        assert_eq!(
            anchor_of_b1(&store, &second),
            an_item_anchor(
                (Some(at(4, 5)), Some(at(5, 6))),
                "sha256:after",
                Some(at(22, 5))
            )
        );
        assert_eq!(store.stale_ops(&second), Vec::new());
    }

    #[test]
    fn an_edit_inside_the_anchored_range_of_an_item_marks_its_op_edited_by_the_other_plan() {
        // Given an item anchor on lines 20–21
        let (_root, mut store, first, second) = second_holding(AN_ITEM_OP);

        // When the other plan's op replaced line 21
        folded(
            &mut store,
            &first,
            &editing(vec![an_edit(at(21, 1), at(22, 1), "x();\n")]),
            &mut AResolverAnswering::Item {
                first_line: 19,
                fingerprint: "sha256:after",
            },
        );

        // Then the op is stale
        assert_eq!(store.stale_ops(&second), edited_by_a1(&first));
    }

    #[test]
    fn a_changed_item_whose_anchor_has_no_hint_is_marked_stale() {
        // Given an item anchor with no hint, so no record of where its range sat
        let (_root, mut store, first, second) = second_holding(AN_ITEM_OP_WITHOUT_A_HINT);

        // When the other plan's op changed the item, below where the range would be
        folded(
            &mut store,
            &first,
            &editing(vec![an_edit(at(23, 1), at(24, 1), "x();\n")]),
            &mut AResolverAnswering::Item {
                first_line: 19,
                fingerprint: "sha256:after",
            },
        );

        // Then the op is stale: it cannot be told whether the edit reached the range
        assert_eq!(store.stale_ops(&second), edited_by_a1(&first));
    }

    #[test]
    fn an_edit_elsewhere_in_a_changed_item_leaves_a_whole_item_anchor_on_the_item() {
        // Given an anchor of the whole item `a::f`, hinted at its name on line 19
        let (_root, mut store, first, second) = second_holding(A_WHOLE_ITEM_OP);

        // When the other plan's op changed line 23 of the item
        folded(
            &mut store,
            &first,
            &editing(vec![an_edit(at(23, 1), at(24, 1), "x();\n")]),
            &mut AResolverAnswering::Item {
                first_line: 19,
                fingerprint: "sha256:after",
            },
        );

        // Then the anchor takes the item's new fingerprint, hinted at the item's name
        assert_eq!(
            anchor_of_b1(&store, &second),
            an_item_anchor((None, None), "sha256:after", Some(at(19, 8)))
        );
        assert_eq!(store.stale_ops(&second), Vec::new());
    }

    #[test]
    fn an_edit_above_an_unchanged_item_rewrites_only_the_hint() {
        // Given an item anchor on lines 20–21
        let (_root, mut store, first, second) = second_holding(AN_ITEM_OP);

        // When the other plan's op inserted three lines at the top of the file, and the item,
        // now beginning at line 22, is byte for byte as it was
        folded(
            &mut store,
            &first,
            &editing(vec![an_edit(at(1, 1), at(1, 1), "a\nb\nc\n")]),
            &mut AResolverAnswering::Item {
                first_line: 22,
                fingerprint: "sha256:written",
            },
        );

        // Then the relative range and fingerprint stand, and the hint follows the item
        assert_eq!(
            anchor_of_b1(&store, &second),
            an_item_anchor(
                (Some(at(2, 5)), Some(at(3, 6))),
                "sha256:written",
                Some(at(23, 5))
            )
        );
        assert_eq!(store.stale_ops(&second), Vec::new());
    }

    #[test]
    fn an_item_the_edit_left_unresolvable_marks_its_op_edited_by_the_other_plan() {
        // Given an item anchor
        let (_root, mut store, first, second) = second_holding(AN_ITEM_OP);

        // When the other plan's op edited the file and the item is no longer in it
        folded(
            &mut store,
            &first,
            &editing(vec![an_edit(at(1, 1), at(1, 1), "a\n")]),
            &mut AResolverAnswering::Missing,
        );

        // Then the op is stale
        assert_eq!(store.stale_ops(&second), edited_by_a1(&first));
    }

    #[test]
    fn an_items_anchor_whose_items_are_intact_follows_a_file_the_edit_changed() {
        // Given an anchor of two items in `src/a.rs`
        let (_root, mut store, first, second) = second_holding(AN_ITEMS_OP);

        // When the other plan's op edited the file, and both items still fingerprint as written
        folded(
            &mut store,
            &first,
            &editing(vec![an_edit(at(1, 1), at(1, 1), "a\n")]),
            &mut AResolverAnswering::Item {
                first_line: 20,
                fingerprint: "sha256:written",
            },
        );

        // Then the anchor is as it was, and not stale
        assert_eq!(store.stale_ops(&second), Vec::new());
        assert_eq!(
            anchor_of_b1(&store, &second),
            Anchor::Items {
                file: "src/a.rs".to_string(),
                items: vec![
                    ItemPath::parse("a::f").unwrap(),
                    ItemPath::parse("a::g").unwrap()
                ],
                fingerprints: vec![Fingerprint("sha256:written".to_string()); 2],
            }
        );
    }

    #[test]
    fn an_items_anchor_follows_a_file_the_other_plan_moved() {
        // Given an anchor of two items in `src/a.rs`
        let (_root, mut store, first, second) = second_holding(AN_ITEMS_OP);

        // When the other plan's op renamed the file
        folded(
            &mut store,
            &first,
            &WorkspaceEdit {
                changes: vec![FileEdit::Rename {
                    from: "src/a.rs".to_string(),
                    to: "src/b.rs".to_string(),
                }],
            },
            &mut AResolverAnswering::Missing,
        );

        // Then the anchor names the file where it now is, and is not stale
        assert_eq!(anchor_of_b1(&store, &second).file(), "src/b.rs");
        assert_eq!(store.stale_ops(&second), Vec::new());
    }

    #[test]
    fn an_items_anchor_with_a_changed_item_is_marked_stale() {
        // Given an anchor of two items in `src/a.rs`
        let (_root, mut store, first, second) = second_holding(AN_ITEMS_OP);

        // When the other plan's op edited the file and the items no longer fingerprint as written
        folded(
            &mut store,
            &first,
            &editing(vec![an_edit(at(1, 1), at(1, 1), "a\n")]),
            &mut AResolverAnswering::Item {
                first_line: 20,
                fingerprint: "sha256:after",
            },
        );

        // Then the op is stale
        assert_eq!(store.stale_ops(&second), edited_by_a1(&first));
    }

    #[test]
    fn an_items_anchor_with_a_missing_item_is_marked_stale() {
        // Given an anchor of two items in `src/a.rs`
        let (_root, mut store, first, second) = second_holding(AN_ITEMS_OP);

        // When the other plan's op edited the file and the items are not in it
        folded(
            &mut store,
            &first,
            &editing(vec![an_edit(at(1, 1), at(1, 1), "a\n")]),
            &mut AResolverAnswering::Missing,
        );

        // Then the op is stale
        assert_eq!(store.stale_ops(&second), edited_by_a1(&first));
    }
}
