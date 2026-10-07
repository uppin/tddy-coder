//! What `verify` is told about a call re-point the author made (`repoint_call`), so it can account
//! for it: rule R-call.
//!
//! A declaration, never an inference: from the two trees alone an inserted `.hop` cannot be told from
//! a hand edit. A plain re-point to a lowercase module (`f(` becoming `m::f(`) is already excused by
//! the re-point pass without a declaration; what needs one is a hop on a receiver (`self.slot(` becoming
//! `self.peer.slot(`), a method chain, or a path whose new qualifier is a type.
//!
//! **R-call** pairs a lost and a gained statement 1:1 when the gained one equals the lost one with
//! every occurrence of `from` immediately followed by `(` (or `::<`) replaced by `to`, outside
//! strings, comments and lifetimes. The pair is counted in `Excused::repointed`. A hop nobody
//! declared, a replaced hop, a changed argument and a different method stay reported.

use std::collections::BTreeMap;
use std::str::FromStr;

use super::retarget::{is_word_byte, string_end, word_end, Passed};

/// One declared call re-point: every call written `from(..)` is now written `to(..)`. Callee texts
/// as the plan wrote them, `self.slot=self.peer.slot`; for the bulk form the method and its new
/// hops, `.slot=.agent_roster().slot`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repoint {
    pub from: String,
    pub to: String,
}

impl FromStr for Repoint {
    type Err = String;

    /// Read `OLD=NEW`, the form `--repoint` and `VerifyRequest.repoints` carry.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.split_once('=') {
            Some((from, to)) if !from.trim().is_empty() && !to.trim().is_empty() && from != to => {
                Ok(Repoint {
                    from: from.trim().to_string(),
                    to: to.trim().to_string(),
                })
            }
            _ => Err(format!(
                "`{text}` is not a repoint: write `OLD=NEW`, two different callee texts"
            )),
        }
    }
}

/// R-call: pair each lost statement with a gained one equal to it once every call of a declared
/// `from` is written `to`, one to one.
pub(crate) fn account(
    missing: Vec<String>,
    added: Vec<String>,
    declared: &super::Declared,
) -> Passed {
    if declared.repoints.is_empty() {
        return Passed {
            missing,
            added,
            pairs: 0,
        };
    }

    let mut by_text: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for statement in added {
        by_text
            .entry(statement.clone())
            .or_default()
            .push(statement);
    }

    let mut unpaired = Vec::new();
    let mut pairs = 0;
    for statement in missing {
        let paired = declared.repoints.iter().any(|repoint| {
            let rewritten = replace_calls(&statement, &repoint.from, &repoint.to);
            by_text
                .get_mut(&rewritten)
                .filter(|bucket| !bucket.is_empty())
                .map(|bucket| {
                    bucket.pop();
                    pairs += 1;
                })
                .is_some()
        });
        if !paired {
            unpaired.push(statement);
        }
    }

    let mut added: Vec<String> = by_text.into_values().flatten().collect();
    added.sort();
    Passed {
        missing: unpaired,
        added,
        pairs,
    }
}

/// `text` with every occurrence of `from` that is immediately followed by `(` or `::<` replaced by
/// `to`, leaving string literals, comments and lifetimes as written.
fn replace_calls(text: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let pattern: Vec<char> = from.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        let character = chars[at];
        if character == '"' {
            let end = string_end(&chars, at);
            out.extend(&chars[at..end]);
            at = end;
        } else if character == '/' && chars.get(at + 1) == Some(&'/') {
            out.extend(&chars[at..]);
            break;
        } else if character == '\'' && is_word_byte(chars.get(at + 1)) {
            let end = word_end(&chars, at + 1);
            out.extend(&chars[at..end]);
            at = end;
        } else if chars[at..].starts_with(&pattern) && opens_a_call(&chars, at + pattern.len()) {
            out.push_str(to);
            at += pattern.len();
        } else {
            out.push(character);
            at += 1;
        }
    }
    out
}

/// Whether a call follows the callee text at `at`: its argument list `(`, or a turbofish `::<`.
fn opens_a_call(chars: &[char], at: usize) -> bool {
    match chars.get(at) {
        Some('(') => true,
        Some(':') => chars.get(at + 1) == Some(&':') && chars.get(at + 2) == Some(&'<'),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(pairs: &[(&str, &str)]) -> super::super::Declared {
        super::super::Declared {
            retargets: Vec::new(),
            repoints: pairs
                .iter()
                .map(|(from, to)| super::super::Repoint {
                    from: (*from).to_string(),
                    to: (*to).to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn rewrites_only_a_call_of_the_declared_callee() {
        assert_eq!(
            replace_calls("let t = h.slot(1);", ".slot", ".peer.slot"),
            "let t = h.peer.slot(1);"
        );
        assert_eq!(
            replace_calls("let t = h.slots(1);", ".slot", ".peer.slot"),
            "let t = h.slots(1);"
        );
        assert_eq!(
            replace_calls("let t = h.slot(1); // h.slot(2)", ".slot", ".peer.slot"),
            "let t = h.peer.slot(1); // h.slot(2)"
        );
    }

    #[test]
    fn pairs_a_hop_that_was_declared_and_reports_one_that_was_not() {
        // Given a declared `.slot=.peer.slot`
        let declaration = declared(&[(".slot", ".peer.slot")]);
        let missing = vec!["let total = h.slot(1);".to_string()];
        let added = vec!["let total = h.peer.slot(1);".to_string()];

        // When the pass runs
        let passed = account(missing, added, &declaration);

        // Then the pair is excused and nothing is left
        assert_eq!(
            (passed.pairs, passed.missing, passed.added),
            (1, Vec::<String>::new(), Vec::<String>::new())
        );
    }

    #[test]
    fn leaves_a_different_hop_and_a_changed_argument_reported() {
        // Given the same declaration
        let declaration = declared(&[(".slot", ".peer.slot")]);

        // When the gained statement is a different hop, and a changed argument
        let another = account(
            vec!["let total = h.slot(1);".to_string()],
            vec!["let total = h.roster.slot(1);".to_string()],
            &declaration,
        );
        let changed = account(
            vec!["let total = h.slot(1);".to_string()],
            vec!["let total = h.peer.slot(2);".to_string()],
            &declaration,
        );

        // Then neither pairs
        assert_eq!((another.pairs, changed.pairs), (0, 0));
        assert_eq!(another.missing, ["let total = h.slot(1);"]);
        assert_eq!(changed.missing, ["let total = h.slot(1);"]);
    }
}
