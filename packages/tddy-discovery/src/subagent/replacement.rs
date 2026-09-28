//! The replacement tool call and result a caller appends to a yielded conversation — the resume
//! that says *"here is the call I meant to make and the result it would have produced; continue
//! from that."*
//!
//! The design is **keep original + append** (PRD decision, approved): the yielded conversation's
//! history is untouched — the failed call stays, and the subagent sees both its own failed
//! attempt and the operator's fix, in the same shape a real call would have appeared in. The
//! transcript is append-only here; nothing is rewritten or hidden.
//!
//! Split out of `subagent.rs` on the oversized-file record
//! (`packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md`).

use super::tool_arguments::validate_tool_arguments;

/// The longest result text a replacement may carry. Bounded so a caller cannot append a whole
/// document as one `tool` message — the same bound the tool results themselves respect in
/// spirit, and the transcript's own cost model. Counted in `char`s, not bytes, for the reason
/// `preview_of` counts in them: a byte bound would make the limit depend on the text's spelling.
pub const REPLACEMENT_RESULT_LIMIT: usize = 16 * 1024;

/// A caller-provided tool call and its result, appended after the yielded call it replaces —
/// never dispatched, recorded as history.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Replacement {
    /// The tool the caller's call names — a tool this build knows, validated the same way the
    /// real dispatch path validates a model's call.
    pub tool: String,
    /// The call's arguments, as the model would have written them.
    pub arguments: serde_json::Value,
    /// The result the caller provides for the call — its text, recorded verbatim. JSON-shaped
    /// in practice (every tool result is), and validated as such so the subagent reads a
    /// payload its own tool loop could have produced.
    pub result: String,
}

/// The tools a replacement's call may name. Read from the advertised definitions rather than a
/// second list written here — the discipline `tool_arguments.rs` states, for the same reason:
/// a hand-kept roster drifts the moment a tool is added. These are the tools the turn loop's
/// `dispatch_tool_call` records, so a replacement's call is indistinguishable in kind from one
/// the model could have made.
///
/// Not `yield_condition`'s nine: that list is the vocabulary
/// [`result_summary::summarize`](crate::subagent::result_summary::summarize) can name, and a
/// replacement's result is the caller's own text — no dispatch ran, so no summary ever reads it.
/// `SEMANTIC_SEARCH` is recordable without being summarizable, and a replacement of one is legal.
fn known_tools() -> Vec<String> {
    crate::openai::discovery_tool_definitions()
        .into_iter()
        .chain(crate::openai::mutation_tool_definitions())
        .chain(crate::openai::engine_tool_definitions())
        .map(|definition| definition.function.name)
        .collect()
}

/// Reject a replacement a conversation cannot run with: an unknown tool, arguments the tool's
/// own schema refuses, or a result that is not JSON or past [`REPLACEMENT_RESULT_LIMIT`] — each
/// rejection naming the offending field, **before** the turn runs, so a malformed replacement
/// costs no model turn.
pub fn validate_replacement(replacement: &Replacement) -> Result<(), String> {
    if !known_tools().contains(&replacement.tool) {
        return Err(format!(
            "tool: '{}' is not a tool this build advertises, so the replacement's call could \
             never have been one the turn loop recorded",
            replacement.tool
        ));
    }
    // The same check a model-issued call gets at dispatch — a replacement the schema would
    // refuse must not buy its way into the history the schema has no place for.
    let violations = validate_tool_arguments(&replacement.tool, &replacement.arguments);
    if !violations.is_empty() {
        let faults: Vec<String> = violations
            .iter()
            .map(|violation| format!("{}: {}", violation.argument, violation.problem.describe()))
            .collect();
        return Err(format!(
            "arguments: {} of them do not match {}'s advertised schema ({})",
            violations.len(),
            replacement.tool,
            faults.join("; ")
        ));
    }
    let length = replacement.result.chars().count();
    if length > REPLACEMENT_RESULT_LIMIT {
        return Err(format!(
            "result: {length} characters is past the {REPLACEMENT_RESULT_LIMIT} a replacement's \
             result may carry"
        ));
    }
    if serde_json::from_str::<serde_json::Value>(&replacement.result).is_err() {
        return Err(
            "result: not JSON — the subagent would read a payload no tool loop could produce"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_replacement() -> Replacement {
        Replacement {
            tool: "STR_REPLACE".to_string(),
            arguments: serde_json::json!({
                "path": "src/lib.rs",
                "old_string": "let a = 1;",
                "new_string": "let a = 0;"
            }),
            result: r#"{"replaced": true, "matchedOccurrences": 1, "bytes_written": 128}"#
                .to_string(),
        }
    }

    #[test]
    fn a_well_formed_replacement_is_accepted() {
        assert!(validate_replacement(&a_replacement()).is_ok());
    }

    #[test]
    fn an_unknown_tool_is_rejected_before_the_turn_runs() {
        let replacement = Replacement {
            tool: "TELEPATHY".to_string(),
            ..a_replacement()
        };
        let rejection = validate_replacement(&replacement)
            .expect_err("an unknown tool cannot be recorded as a call the model made");
        assert!(
            rejection.contains("TELEPATHY"),
            "names the tool: {rejection}"
        );
    }

    #[test]
    fn arguments_the_tools_own_schema_refuses_are_rejected() {
        let replacement = Replacement {
            arguments: serde_json::json!({
                "path": "src/lib.rs",
                "old_string": "let a = 1;",
                // `new_string` missing: the STR_REPLACE schema requires it.
            }),
            ..a_replacement()
        };
        assert!(
            validate_replacement(&replacement).is_err(),
            "the replacement's arguments are validated like the model's own call's"
        );
    }

    #[test]
    fn a_result_that_is_not_json_is_rejected() {
        let replacement = Replacement {
            result: "not json".to_string(),
            ..a_replacement()
        };
        let rejection = validate_replacement(&replacement)
            .expect_err("the subagent would read a payload no tool loop could produce");
        assert!(rejection.contains("result"), "names the field: {rejection}");
    }

    #[test]
    fn a_result_past_the_bound_is_rejected() {
        let replacement = Replacement {
            result: "1".repeat(REPLACEMENT_RESULT_LIMIT + 1),
            ..a_replacement()
        };
        assert!(validate_replacement(&replacement).is_err());
    }
}
