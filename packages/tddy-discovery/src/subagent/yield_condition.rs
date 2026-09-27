//! The conditions on a tool call that yield a subagent's turn back to its caller — the
//! mid-turn control `maxTurns` never was: stop the turn *at the call*, not after a budget
//! of further calls.
//!
//! Split out of `subagent.rs` on the oversized-file record
//! (`packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md`): the predicate
//! types, their evaluation and their validation are a self-contained concern, called from the
//! turn loop's tool-call append site and from the MCP argument parsers alike.
//!
//! Conditions are **per-turn-request** (stateless): they apply for this turn only, never the
//! conversation. Two predicate kinds, both bounded — an **outcome** fact compared for
//! equality, read from the call's result summary (the vocabulary `result_summary.rs` extracts),
//! and an **argument** string field containing a bounded substring.

/// The greatest number of conditions one turn request may carry. A bound rather than the
/// caller's judgement: each condition is evaluated after *every* tool call, and an unbounded
/// list would let one request spend the turn's wall clock on predicate matches.
pub const YIELD_CONDITION_LIMIT: usize = 8;

/// The longest substring an [`When::Argument`] predicate may look for. Bounded so a malformed
/// request cannot ship a whole document as a "needle" into every tool call's arguments.
pub const YIELD_CONTAINS_LIMIT: usize = 256;

/// One fact of a tool call's outcome, compared for equality — the `resultSummary` vocabulary
/// (`result_summary::ResultSummary`) as a predicate can name it.
///
/// The fact a condition names must be a fact the tool's own summary can carry (a
/// `matchedLines` condition on a `WRITE` is a rejection, not a condition that never fires) —
/// enforced by [`validate`], not by the enum, because the tool-dependent half of that rule
/// lives where the tool names are known.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutcomeFact {
    MatchedLines(u64),
    MatchCount(u64),
    PathCount(u64),
    CharsRead(u64),
    BytesWritten(u64),
    ExitCode(i64),
    /// `true` = the dispatch produced no result; `false` = it ran.
    Error(bool),
}

/// What one condition watches for: the call's outcome, or one string field of its arguments.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum When {
    /// An outcome fact, compared for equality against the call's result summary.
    Outcome { fact: OutcomeFact },
    /// A string field of the call's arguments containing `contains`.
    Argument { field: String, contains: String },
}

/// One condition: the tool it watches, and what it watches for.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YieldCondition {
    pub tool: String,
    pub when: When,
}

/// Whether `condition` fires on the call `tool` made with `arguments`, whose result summarized
/// to `summary` — `summary` `None` for a dispatch that produced nothing.
///
/// Called at the turn loop's tool-call append site, where all three inputs are in hand, after
/// the result is appended: a fired condition stops the turn with the result in the transcript
/// and the model never sent it.
pub fn evaluate(
    _condition: &YieldCondition,
    _tool: &str,
    _arguments: &serde_json::Value,
    _summary: Option<&crate::subagent::result_summary::ResultSummary>,
) -> bool {
    // TODO(yield-conditions): match the condition's tool, then its When — an outcome fact
    // against the summary's value, an argument's field value containing the bounded needle.
    false
}

/// Reject the conditions a request cannot run with: an unknown tool, a fact the tool's own
/// summary cannot carry, a needle past [`YIELD_CONTAINS_LIMIT`], or more conditions than
/// [`YIELD_CONDITION_LIMIT`] — each rejection naming the offending condition, **before** the
/// turn runs, so a malformed request costs no model turn.
pub fn validate(_conditions: &[YieldCondition]) -> Result<(), String> {
    // TODO(yield-conditions): bound the count, the needle and the tool's fact vocabulary.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_str_replace_no_match() -> YieldCondition {
        YieldCondition {
            tool: "STR_REPLACE".to_string(),
            when: When::Outcome {
                fact: OutcomeFact::MatchedLines(0),
            },
        }
    }

    fn str_replace_no_match_args() -> serde_json::Value {
        serde_json::json!({
            "path": "src/lib.rs",
            "old_string": "let c = 3;",
            "new_string": "let c = 0;"
        })
    }

    #[test]
    fn an_outcome_condition_fires_when_the_fact_matches_the_summary() {
        let summary = crate::subagent::result_summary::ResultSummary::StrReplace {
            replaced: false,
            matched_lines: 0,
            bytes_written: 0,
        };
        assert!(
            evaluate(
                &a_str_replace_no_match(),
                "STR_REPLACE",
                &str_replace_no_match_args(),
                Some(&summary)
            ),
            "a no-match STR_REPLACE under a matchedLines:0 condition yields"
        );
    }

    #[test]
    fn an_outcome_condition_does_not_fire_when_the_fact_differs() {
        let summary = crate::subagent::result_summary::ResultSummary::StrReplace {
            replaced: true,
            matched_lines: 1,
            bytes_written: 128,
        };
        assert!(
            !evaluate(
                &a_str_replace_no_match(),
                "STR_REPLACE",
                &str_replace_no_match_args(),
                Some(&summary)
            ),
            "one matched line does not satisfy a matchedLines:0 condition"
        );
    }

    #[test]
    fn an_argument_condition_fires_when_the_field_contains_the_needle() {
        let condition = YieldCondition {
            tool: "WRITE".to_string(),
            when: When::Argument {
                field: "contents".to_string(),
                contains: "TODO(".to_string(),
            },
        };
        let args =
            serde_json::json!({ "path": "src/lib.rs", "contents": "let a = 1; // TODO(fix)" });
        assert!(
            evaluate(&condition, "WRITE", &args, None),
            "a WRITE whose contents carry TODO( yields"
        );
    }

    #[test]
    fn a_condition_does_not_fire_on_a_tool_it_does_not_watch() {
        let condition = a_str_replace_no_match();
        let args = serde_json::json!({ "pattern": "main" });
        assert!(
            !evaluate(
                &condition,
                "GREP",
                &args,
                Some(&crate::subagent::result_summary::ResultSummary::Grep {
                    match_count: 0,
                    truncated: false,
                    total_matches: 0
                })
            ),
            "a condition on STR_REPLACE never fires on a GREP call"
        );
    }

    #[test]
    fn a_condition_on_a_dispatch_that_produced_nothing_fires_on_the_error_fact() {
        let condition = YieldCondition {
            tool: "READ".to_string(),
            when: When::Outcome {
                fact: OutcomeFact::Error(true),
            },
        };
        assert!(
            evaluate(
                &condition,
                "READ",
                &serde_json::json!({ "path": "src/lib.rs" }),
                None
            ),
            "an error dispatch satisfies an error:true condition — the summary is None, and a \
             dispatch that produced nothing is an error by the outcome's own is_error flag"
        );
    }

    #[test]
    fn malformed_conditions_are_rejected_before_the_turn_runs() {
        // An unknown tool: no summary the engine could produce would ever carry its facts.
        assert!(validate(&[YieldCondition {
            tool: "TELEPATHY".to_string(),
            when: When::Outcome {
                fact: OutcomeFact::MatchedLines(0),
            },
        }])
        .is_err());
        // A fact the tool's own summary cannot carry: WRITE has no matched lines.
        assert!(validate(&[YieldCondition {
            tool: "WRITE".to_string(),
            when: When::Outcome {
                fact: OutcomeFact::MatchedLines(0),
            },
        }])
        .is_err());
        // More conditions than the limit.
        let too_many: Vec<YieldCondition> = std::iter::repeat(a_str_replace_no_match())
            .take(YIELD_CONDITION_LIMIT + 1)
            .collect();
        assert!(validate(&too_many).is_err());
        // A needle past the bound.
        assert!(validate(&[YieldCondition {
            tool: "WRITE".to_string(),
            when: When::Argument {
                field: "contents".to_string(),
                contains: "x".repeat(YIELD_CONTAINS_LIMIT + 1),
            },
        }])
        .is_err());
        // And the well-formed one passes.
        assert!(validate(&[a_str_replace_no_match()]).is_ok());
    }

    #[test]
    fn the_yielded_stop_reason_serializes_in_the_family_of_its_siblings() {
        let wire = serde_json::to_value(StopReason::YieldedToCaller).expect("serializable");
        assert_eq!(wire, serde_json::json!("yielded_to_caller"));
        assert_eq!(
            serde_json::from_value::<StopReason>(wire).expect("parseable"),
            StopReason::YieldedToCaller
        );
    }

    use crate::subagent::StopReason;
}
