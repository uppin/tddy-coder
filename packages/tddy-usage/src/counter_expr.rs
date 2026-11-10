//! Counter expressions: a field path, or field paths joined by `+` / `-` — so a binding can map a
//! source whose counters overlap (cached input inside input, reasoning inside output) onto the
//! record's disjoint counters (`#usage-binding 4/5`).

use serde::{Deserialize, Serialize};

use crate::FieldPath;

/// `path (("+" | "-") path)*`, whitespace around the operators optional. A plain path is an
/// expression of one term.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CounterExpr {
    text: String,
    terms: Vec<Term>,
}

/// One operand, subtracted when `negated`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Term {
    negated: bool,
    path: FieldPath,
}

impl CounterExpr {
    /// The expression's value on `line`: each term's number added or subtracted, a missing or
    /// non-numeric operand counting as 0, a negative result saturating at 0. `None` when no term's
    /// field is present on the line at all.
    pub fn evaluate(&self, line: &serde_json::Value) -> Option<u64> {
        // TODO(codex-rollout): implement
        let _ = (&self.terms, line);
        None
    }

    /// The expression as written.
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl TryFrom<String> for CounterExpr {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        // TODO(codex-rollout): parse `+` / `-` terms; today every expression is one path.
        let path = FieldPath::try_from(text.trim().to_string())?;
        Ok(CounterExpr {
            terms: vec![Term {
                negated: false,
                path,
            }],
            text,
        })
    }
}

impl From<CounterExpr> for String {
    fn from(expr: CounterExpr) -> String {
        expr.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expr(text: &str) -> CounterExpr {
        CounterExpr::try_from(text.to_string()).expect("a valid expression")
    }

    #[test]
    fn a_difference_subtracts_the_second_field_from_the_first() {
        let line = serde_json::json!({ "u": { "input": 100, "cached": 60 } });

        assert_eq!(expr("u.input - u.cached").evaluate(&line), Some(40));
    }

    #[test]
    fn a_sum_adds_every_term() {
        let line = serde_json::json!({ "a": 1, "b": 2, "c": 3 });

        assert_eq!(expr("a + b+c").evaluate(&line), Some(6));
    }

    #[test]
    fn a_missing_operand_counts_as_zero() {
        let line = serde_json::json!({ "a": 7 });

        assert_eq!(expr("a - b").evaluate(&line), Some(7));
    }

    #[test]
    fn a_negative_result_saturates_at_zero() {
        let line = serde_json::json!({ "a": 1, "b": 5 });

        assert_eq!(expr("a - b").evaluate(&line), Some(0));
    }

    #[test]
    fn a_line_with_none_of_the_fields_has_no_value() {
        let line = serde_json::json!({ "other": 1 });

        assert_eq!(expr("a - b").evaluate(&line), None);
    }

    #[test]
    fn an_operator_with_no_operand_is_refused() {
        assert!(CounterExpr::try_from("a -".to_string()).is_err());
    }
}
