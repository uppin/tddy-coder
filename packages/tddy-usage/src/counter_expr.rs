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
        let mut present = false;
        let mut total: i128 = 0;
        for term in &self.terms {
            let found = term.path.lookup(line);
            present |= found.is_some();
            let value = i128::from(found.and_then(serde_json::Value::as_u64).unwrap_or(0));
            if term.negated {
                total -= value;
            } else {
                total += value;
            }
        }
        if !present {
            return None;
        }
        if total < 0 {
            log::warn!("usage: '{}' is negative on a line; counted as 0", self.text);
        }
        Some(u64::try_from(total.max(0)).unwrap_or(u64::MAX))
    }

    /// The expression as written.
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl TryFrom<String> for CounterExpr {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let terms = parse_terms(&text)?;
        Ok(CounterExpr { terms, text })
    }
}

/// `path (("+" | "-") path)*`: split at the operators, each operand a [`FieldPath`]. A key
/// containing `+` or `-` cannot be written in an expression, since the operator is recognised
/// wherever it stands.
fn parse_terms(text: &str) -> Result<Vec<Term>, String> {
    let refuse = |why: &str| format!("counter expression '{text}' {why}");
    let mut terms = Vec::new();
    let mut negated = false;
    let mut operand = String::new();
    let mut flush = |operand: &mut String, negated: bool| -> Result<(), String> {
        let path = FieldPath::try_from(operand.trim().to_string())
            .map_err(|_| refuse("has an operator with no field path beside it"))?;
        terms.push(Term { negated, path });
        operand.clear();
        Ok(())
    };
    for c in text.chars() {
        match c {
            '+' | '-' => {
                flush(&mut operand, negated)?;
                negated = c == '-';
            }
            _ => operand.push(c),
        }
    }
    flush(&mut operand, negated)?;
    Ok(terms)
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
