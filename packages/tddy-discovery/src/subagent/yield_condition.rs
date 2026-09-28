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

use crate::subagent::result_summary::ResultSummary;

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
    condition: &YieldCondition,
    tool: &str,
    arguments: &serde_json::Value,
    summary: Option<&ResultSummary>,
) -> bool {
    if condition.tool != tool {
        return false;
    }
    match &condition.when {
        // String fields only, and only fields that are there: a missing or non-string argument
        // cannot contain anything, so the condition does not fire rather than guessing at a
        // stringification the caller never asked for.
        When::Argument { field, contains } => arguments
            .get(field)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value.contains(contains.as_str())),
        When::Outcome { fact } => match (fact, summary) {
            // A dispatch that produced no result has one fact to its name — the error itself.
            (OutcomeFact::Error(true), None) => true,
            // And none besides: no summary is not a zero of anything, so no other fact —
            // `error: false` included, which says the dispatch ran — can be read off it.
            (_, None) => false,
            // It ran: `error: false` fires on any present summary, whatever its facts say, and
            // `error: true` never does.
            (OutcomeFact::Error(false), Some(_)) => true,
            (OutcomeFact::Error(true), Some(_)) => false,
            (fact, Some(summary)) => fact_holds(fact, summary),
        },
    }
}

/// Whether the fact `fact` names, at the value it names, is the one `summary` carries — equality
/// between the condition and the summary's own field:
///
/// | fact         | summary variant      | field         |
/// |--------------|----------------------|---------------|
/// | matchedLines | StrReplace           | matched_lines |
/// | matchCount   | Grep                 | match_count   |
/// | pathCount    | Glob                 | path_count    |
/// | charsRead    | Read                 | chars_read    |
/// | bytesWritten | Write, StrReplace    | bytes_written |
/// | exitCode     | Shell, Await         | exit_code     |
///
/// Any other pairing is a condition [`validate`] should have refused (a fact the tool's summary
/// cannot carry), and an `exitCode` against a background job's summary is a fact with no value
/// yet — each reads as "does not fire", never as a match on a neighbouring fact.
fn fact_holds(fact: &OutcomeFact, summary: &ResultSummary) -> bool {
    match (fact, summary) {
        (OutcomeFact::MatchedLines(want), ResultSummary::StrReplace { matched_lines, .. }) => {
            want == matched_lines
        }
        (OutcomeFact::MatchCount(want), ResultSummary::Grep { match_count, .. }) => {
            want == match_count
        }
        (OutcomeFact::PathCount(want), ResultSummary::Glob { path_count, .. }) => {
            want == path_count
        }
        (OutcomeFact::CharsRead(want), ResultSummary::Read { chars_read, .. }) => {
            want == chars_read
        }
        (OutcomeFact::BytesWritten(want), ResultSummary::Write { bytes_written, .. })
        | (OutcomeFact::BytesWritten(want), ResultSummary::StrReplace { bytes_written, .. }) => {
            want == bytes_written
        }
        (
            OutcomeFact::ExitCode(want),
            ResultSummary::Shell {
                exit_code: Some(code),
                ..
            },
        )
        | (
            OutcomeFact::ExitCode(want),
            ResultSummary::Await {
                exit_code: Some(code),
                ..
            },
        ) => want == code,
        _ => false,
    }
}

/// Reject the conditions a request cannot run with: an unknown tool, a fact the tool's own
/// summary cannot carry, a needle past [`YIELD_CONTAINS_LIMIT`], or more conditions than
/// [`YIELD_CONDITION_LIMIT`] — each rejection naming the offending condition, **before** the
/// turn runs, so a malformed request costs no model turn.
pub fn validate(conditions: &[YieldCondition]) -> Result<(), String> {
    if conditions.len() > YIELD_CONDITION_LIMIT {
        return Err(format!(
            "{} conditions is past the {} a turn may carry",
            conditions.len(),
            YIELD_CONDITION_LIMIT
        ));
    }
    for condition in conditions {
        validate_one(condition)?;
    }
    Ok(())
}

/// The tools a condition may watch — the same nine dispatches
/// [`result_summary::summarize`](crate::subagent::result_summary::summarize) reads. A tool
/// outside this list has no summary the engine could ever produce, so a condition on it could
/// never fire, and it is rejected rather than accepted as one that never does.
const KNOWN_TOOLS: &[&str] = &[
    "READ",
    "GREP",
    "GLOB",
    "STR_REPLACE",
    "WRITE",
    "DELETE",
    "SHELL",
    "AWAIT",
    "READ_LINTS",
];

/// One condition's own well-formedness: a tool the summaries name, and a fact that tool's own
/// summary can carry (or an argument needle inside the bound). Each rejection names the offending
/// condition — tool first, then what it watches — so a caller can drop exactly the broken entry.
fn validate_one(condition: &YieldCondition) -> Result<(), String> {
    let label = condition_label(condition);
    if !KNOWN_TOOLS.contains(&condition.tool.as_str()) {
        return Err(format!(
            "{label}: '{}' is not a tool any result summary can name",
            condition.tool
        ));
    }
    match &condition.when {
        When::Argument { contains, .. } => {
            let needle = contains.chars().count();
            if needle > YIELD_CONTAINS_LIMIT {
                return Err(format!(
                    "{label}: a needle of {needle} characters is past the {YIELD_CONTAINS_LIMIT} \
                     a contains may look for"
                ));
            }
        }
        When::Outcome { fact } => {
            if !carries_fact(&condition.tool, fact) {
                return Err(format!(
                    "{label}: the tool's result summary cannot carry this fact"
                ));
            }
        }
    }
    Ok(())
}

/// What a rejection calls the condition — its tool and what it watches, e.g.
/// `STR_REPLACE matchedLines` or `WRITE contents`.
fn condition_label(condition: &YieldCondition) -> String {
    let watched = match &condition.when {
        When::Outcome { fact } => fact_kind(fact),
        When::Argument { field, .. } => field.as_str(),
    };
    format!("{} {watched}", condition.tool)
}

/// The fact's name as the wire spells it — the vocabulary the conditions and the summaries
/// (`result_summary::ResultSummary`) agree on.
fn fact_kind(fact: &OutcomeFact) -> &'static str {
    match fact {
        OutcomeFact::MatchedLines(_) => "matchedLines",
        OutcomeFact::MatchCount(_) => "matchCount",
        OutcomeFact::PathCount(_) => "pathCount",
        OutcomeFact::CharsRead(_) => "charsRead",
        OutcomeFact::BytesWritten(_) => "bytesWritten",
        OutcomeFact::ExitCode(_) => "exitCode",
        OutcomeFact::Error(_) => "error",
    }
}

/// Whether `tool`'s own summary can carry `fact` — the per-tool fact vocabulary, ruled by
/// `result_summary.rs`'s variants. A fact the summary cannot carry would be a condition that
/// never fires; the [`OutcomeFact`] doc hands that rule here, where the tool names are known.
fn carries_fact(tool: &str, fact: &OutcomeFact) -> bool {
    matches!(
        (tool, fact),
        ("READ", OutcomeFact::CharsRead(_) | OutcomeFact::Error(_))
            | ("GREP", OutcomeFact::MatchCount(_) | OutcomeFact::Error(_))
            | ("GLOB", OutcomeFact::PathCount(_) | OutcomeFact::Error(_))
            | (
                "STR_REPLACE",
                OutcomeFact::MatchedLines(_) | OutcomeFact::BytesWritten(_) | OutcomeFact::Error(_)
            )
            | (
                "WRITE",
                OutcomeFact::BytesWritten(_) | OutcomeFact::Error(_)
            )
            | ("DELETE", OutcomeFact::Error(_))
            | ("SHELL", OutcomeFact::ExitCode(_) | OutcomeFact::Error(_))
            | ("AWAIT", OutcomeFact::ExitCode(_) | OutcomeFact::Error(_))
            | ("READ_LINTS", OutcomeFact::Error(_))
    )
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
