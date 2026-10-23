//! The refusals specific to `apply_code_action`, read from the plan line alone (`#reshape`
//! extension-code-actions, R4).
//!
//! Made before any server is spawned and reported by a plain `check`, `check --deep` and `apply`
//! alike: an `apply_code_action` with no `action`, an `action` naming both `title` and `kind` or
//! neither, an anchor that is neither a range nor `items` / a single `item` (a `symbol` anchor names
//! no range to ask for actions over), and an `action` on any other operation, which would otherwise
//! be ignored. Which actions the server offers over the range is `backends::code_action`'s.

use super::super::malformed;
use super::super::{RefactorKind, RefactorOp};
use crate::{Anchor, Result};

/// Refuse an `apply_code_action` line that misses what it needs or carries what it cannot honour,
/// and an `action` on any other operation; every other line passes untouched.
pub(super) fn rules(op: &RefactorOp) -> Result<()> {
    if op.op != RefactorKind::ApplyCodeAction && op.action.is_none() {
        return Ok(());
    }
    if op.op != RefactorKind::ApplyCodeAction {
        return Err(malformed(
            "`action` names a code action, which only `apply_code_action` honours".to_string(),
        ));
    }
    let Some(action) = &op.action else {
        return Err(malformed(
            "`apply_code_action` needs `action`, naming the code action by `title` or `kind`"
                .to_string(),
        ));
    };
    action.wanted()?;
    match op.anchor {
        Anchor::Symbol { .. } => Err(malformed(
            "`apply_code_action` takes an anchor that is a range, or `items` / a single `item`: \
             a `symbol` anchor names no range to ask for actions over"
                .to_string(),
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use crate::plan::{ActionSelector, Plan};
    use crate::RestructureError;

    const HEADER: &str = r#"{"v":1,"snapshot":{"config/app.toml":"sha256:ab12"}}"#;

    const A_RANGE: &str = r#"{"kind":"range","file":"config/app.toml","start":{"line":2,"col":1},"end":{"line":3,"col":12}}"#;

    /// The one operation of a plan whose only line is `op_line`, or why it was refused.
    fn parsed(op_line: &str) -> Result<crate::plan::RefactorOp, RestructureError> {
        Plan::parse(&format!("{HEADER}\n{op_line}\n")).map(|plan| plan.ops[0].clone())
    }

    /// The reason a line was refused as malformed, or a panic naming what happened instead.
    fn the_refusal_of(op_line: &str) -> String {
        match parsed(op_line) {
            Err(RestructureError::MalformedPlan(reason)) => reason,
            other => panic!("expected a malformed-plan refusal, got {other:?}"),
        }
    }

    #[test]
    fn an_action_chosen_by_title_over_a_range_parses() {
        // Given an `apply_code_action` over a range, choosing its action by title
        let line = format!(
            r#"{{"op":"apply_code_action","anchor":{A_RANGE},"action":{{"title":"Extract function"}}}}"#
        );

        // When the line is parsed
        let op = parsed(&line).expect("the line parses");

        // Then it carries the selector as written
        assert_eq!(
            op.action,
            Some(ActionSelector {
                title: Some("Extract function".to_string()),
                kind: None,
            })
        );
    }

    #[test]
    fn an_action_chosen_by_kind_over_an_item_parses() {
        // Given an `apply_code_action` over an item, choosing its action by kind
        let line = r#"{"op":"apply_code_action","anchor":{"kind":"item","file":"config/app.toml","item":"config/app.toml::server","fingerprint":"sha256:00"},"action":{"kind":"refactor.extract"}}"#;

        // When the line is parsed
        let op = parsed(line).expect("the line parses");

        // Then it carries the kind
        assert_eq!(
            op.action.and_then(|action| action.kind),
            Some("refactor.extract".to_string())
        );
    }

    #[test]
    fn an_apply_code_action_without_an_action_is_refused() {
        // Given an `apply_code_action` that names no action
        let line = format!(r#"{{"op":"apply_code_action","anchor":{A_RANGE}}}"#);

        // When the line is parsed
        let reason = the_refusal_of(&line);

        // Then the refusal names the missing field
        assert_eq!(
            reason,
            "`apply_code_action` needs `action`, naming the code action by `title` or `kind`"
        );
    }

    #[test]
    fn an_action_naming_both_title_and_kind_is_refused() {
        // Given an action naming a title and a kind at once
        let line = format!(
            r#"{{"op":"apply_code_action","anchor":{A_RANGE},"action":{{"title":"Extract function","kind":"refactor.extract"}}}}"#
        );

        // When the line is parsed
        let reason = the_refusal_of(&line);

        // Then the refusal says exactly one of the two is wanted
        assert_eq!(reason, "`action` names exactly one of `title` and `kind`");
    }

    #[test]
    fn an_action_naming_neither_title_nor_kind_is_refused() {
        // Given an empty action
        let line = format!(r#"{{"op":"apply_code_action","anchor":{A_RANGE},"action":{{}}}}"#);

        // When the line is parsed
        let reason = the_refusal_of(&line);

        // Then the refusal says exactly one of the two is wanted
        assert_eq!(reason, "`action` names exactly one of `title` and `kind`");
    }

    #[test]
    fn a_symbol_anchor_is_refused() {
        // Given an `apply_code_action` anchored on a symbol
        let line = r#"{"op":"apply_code_action","anchor":{"kind":"symbol","file":"config/app.toml","path":"server"},"action":{"title":"Extract function"}}"#;

        // When the line is parsed
        let reason = the_refusal_of(line);

        // Then the refusal names the anchors it takes
        assert_eq!(
            reason,
            "`apply_code_action` takes an anchor that is a range, or `items` / a single `item`: \
             a `symbol` anchor names no range to ask for actions over"
        );
    }

    #[test]
    fn an_action_on_another_operation_is_refused_naming_the_field() {
        // Given a rename carrying an action it cannot honour
        let line = format!(
            r#"{{"op":"rename_symbol","anchor":{A_RANGE},"name":"listen_port","action":{{"title":"Extract function"}}}}"#
        );

        // When the line is parsed
        let reason = the_refusal_of(&line);

        // Then the refusal names the field and the one operation that takes it
        assert_eq!(
            reason,
            "`action` names a code action, which only `apply_code_action` honours"
        );
    }

    #[test]
    fn a_line_without_an_action_writes_no_action_field() {
        // Given a rename, which has no action
        let line = format!(r#"{{"op":"rename_symbol","anchor":{A_RANGE},"name":"listen_port"}}"#);
        let op = parsed(&line).expect("the line parses");

        // When it is written back
        let written = serde_json::to_value(&op).expect("the op serializes");

        // Then no `action` appears — a plan written before the field existed is unchanged
        assert_eq!(written.get("action"), None, "unexpected field in {written}");
    }
}
