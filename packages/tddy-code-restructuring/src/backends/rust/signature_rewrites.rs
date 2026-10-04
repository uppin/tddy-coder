//! The signature and call-site operations, authored here rather than delegated to an assist.
//!
//! rust-analyzer has no assist that retypes one parameter, adds one, reorders them, or edits one
//! call's arguments, so each of these is an edit this backend writes — to the declaration's parameter
//! list or `-> …` for the signature operations, to one call's argument list for the call-site ones.
//! None reaches past the text it was anchored on: a caller is its own operation, which is what a
//! transactional group makes one unit.
//!
//! Everything here reads the *masked* code ([`masked_to_code`]), in which comments and literals can
//! carry no delimiter, and writes only the span it changes, so the formatting around it survives.

use super::signature::arrow_before;
use crate::edit::{Position, Range, TextEdit};
use crate::Result;

/// A half-open byte range of the text an operation reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    from: usize,
    to: usize,
}

impl Span {
    fn of(self, text: &str) -> &str {
        &text[self.from..self.to]
    }
}

/// One span of the text, and what it becomes.
///
/// An operation answers with the spans it changes rather than with the rewritten file, so the edits
/// it reports are exactly as narrow as the change: a later operation of the same plan anchored on
/// the same line — the function's own name, say — is still addressable.
#[derive(Debug, PartialEq, Eq)]
struct Replacement {
    span: Span,
    with: String,
}

/// `span` replaced by `with`.
fn replacing(span: Span, with: &str) -> Vec<Replacement> {
    vec![Replacement {
        span,
        with: with.to_string(),
    }]
}

/// `inserted` put in at `at`.
fn inserting(at: usize, inserted: &str) -> Vec<Replacement> {
    replacing(Span { from: at, to: at }, inserted)
}

/// `replacements` as the edits of `text`, in the file's own one-based coordinates.
fn edits_of(text: &str, replacements: Vec<Replacement>) -> Vec<TextEdit> {
    let position = |offset: usize| {
        let before = &text[..offset];
        Position {
            line: before.matches('\n').count() as u32 + 1,
            col: before.rsplit('\n').next().map_or(0, str::len) as u32 + 1,
        }
    };
    replacements
        .into_iter()
        .map(|replacement| TextEdit {
            range: Range {
                start: position(replacement.span.from),
                end: position(replacement.span.to),
            },
            new_text: replacement.with,
        })
        .collect()
}

mod declaration;
pub(super) use declaration::{returned_type, rewrite_declaration};

/// The entries between the delimiters at `open` and `close`, split at the commas outside any
/// brackets (and, for a parameter list, outside any generic arguments), each trimmed.
fn entries(code: &str, open: usize, close: usize, counting_angles: bool) -> Vec<Span> {
    let bytes = code.as_bytes();
    let mut found = Vec::new();
    let mut depth = 0usize;
    let mut from = open + 1;
    let mut push = |from: usize, to: usize| {
        let trimmed = code[from..to].trim();
        if !trimmed.is_empty() {
            let start = from + (code[from..to].len() - code[from..to].trim_start().len());
            found.push(Span {
                from: start,
                to: start + trimmed.len(),
            });
        }
    };
    for index in open + 1..close {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b'<' if counting_angles => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'>' if counting_angles && !arrow_before(bytes, index) => {
                depth = depth.saturating_sub(1)
            }
            b',' if depth == 0 => {
                push(from, index);
                from = index + 1;
            }
            _ => {}
        }
    }
    push(from, close);
    found
}

/// What a parameter binds — `price` for `price: u32` and `mut price: u32` — or `None` for a
/// receiver, which binds `self` and declares no type.
fn binding(code: &str, parameter: Span) -> Option<&str> {
    let colon = separator_colon(code, parameter)?;
    let pattern = code[parameter.from..colon].trim();
    Some(
        pattern
            .strip_prefix("mut ")
            .map_or(pattern, str::trim_start),
    )
}

/// The `:` between a parameter's pattern and its type, which is never half of a `::`.
fn separator_colon(code: &str, parameter: Span) -> Option<usize> {
    let bytes = code.as_bytes();
    (parameter.from..parameter.to).find(|at| {
        bytes[*at] == b':'
            && bytes.get(at + 1) != Some(&b':')
            && at.checked_sub(1).is_none_or(|before| bytes[before] != b':')
    })
}

/// `entry` made the entry at `index` of the list opening at `open`, leaving the separators and
/// layout around the others as they were.
fn with_an_entry_at(open: usize, entries: &[Span], index: usize, entry: &str) -> Vec<Replacement> {
    match (entries.get(index), entries.last()) {
        (Some(before), _) => inserting(before.from, &format!("{entry}, ")),
        (None, Some(last)) => inserting(last.to, &format!(", {entry}")),
        (None, None) => inserting(open + 1, entry),
    }
}

/// The entry at `spans[k]` made to hold what was at `spans[moved[k]]`, for every `k` that changes.
fn reordered(text: &str, spans: &[Span], moved: &[usize]) -> Vec<Replacement> {
    spans
        .iter()
        .zip(moved)
        .enumerate()
        .filter(|(place, (_, source))| place != *source)
        .map(|(_, (span, source))| Replacement {
            span: *span,
            with: spans[*source].of(text).to_string(),
        })
        .collect()
}

fn required<'a>(field: Option<&'a str>, name: &str) -> Result<&'a str> {
    field.ok_or_else(|| super::failure(format!("the operation needs `{name}`")))
}

mod call_site;
pub(super) use call_site::rewrite_call;

#[cfg(test)]
mod tests {
    use super::super::signature::offset_at;
    use super::*;
    use crate::plan::{Anchor, OrderKey, RefactorKind, RefactorOp};
    use pretty_assertions::assert_eq;

    fn at(line: u32, col: u32) -> Position {
        Position { line, col }
    }

    /// An operation of `kind` with every field absent but the ones `fill` sets.
    fn an_op(kind: RefactorKind, fill: impl FnOnce(&mut RefactorOp)) -> RefactorOp {
        let mut op = RefactorOp {
            id: None,
            op: kind,
            anchor: Anchor::Range {
                file: "src/lib.rs".to_string(),
                start: at(1, 1),
                end: at(1, 1),
            },
            name: None,
            to: None,
            variant: None,
            with_private_deps: false,
            reexport: None,
            to_file: false,
            also: Vec::new(),
            group: None,
            type_: None,
            expr: None,
            order: Vec::new(),
        };
        fill(&mut op);
        op
    }

    /// `text` with `edits` applied.
    fn applied(text: &str, edits: Vec<TextEdit>) -> String {
        let mut applied = text.to_string();
        for edit in edits.iter().rev() {
            let from = offset_at(text, edit.range.start).expect("an edit starts in the text");
            let to = offset_at(text, edit.range.end).expect("an edit ends in the text");
            applied.replace_range(from..to, &edit.new_text);
        }
        applied
    }

    /// `text` with the call on its first line, wholly covered, rewritten as `op` asks.
    fn the_call_rewritten(text: &str, op: &RefactorOp) -> std::result::Result<String, String> {
        let end = text.lines().next().map_or(1, |line| line.len() as u32 + 1);
        call_site::rewrite_call(
            text,
            op,
            Range {
                start: at(1, 1),
                end: at(1, end),
            },
        )
        .map(|edits| applied(text, edits))
        .map_err(|error| error.to_string())
    }

    #[test]
    fn a_new_argument_goes_in_without_disturbing_the_others() {
        // Given a call with a turbofish and a closure, whose commas are not separators
        let call = "f(g::<A, B>(1), |a, b| a, 3)\n";

        // When an argument is added first, and another last
        let first = an_op(RefactorKind::AddCallArg, |op| {
            op.variant = Some("first".to_string());
            op.expr = Some("0".to_string());
        });
        let last = an_op(RefactorKind::AddCallArg, |op| {
            op.variant = Some("last".to_string());
            op.expr = Some("9".to_string());
        });

        // Then each lands beside the right neighbour
        assert_eq!(
            (
                the_call_rewritten(call, &first),
                the_call_rewritten(call, &last)
            ),
            (
                Ok("f(0, g::<A, B>(1), |a, b| a, 3)\n".to_string()),
                Ok("f(g::<A, B>(1), |a, b| a, 3, 9)\n".to_string()),
            )
        );
    }

    #[test]
    fn removing_an_argument_takes_its_separator_with_it() {
        // Given a call of three arguments
        let call = "f(1, 2, 3)\n";
        let removing = |position: &str| {
            an_op(RefactorKind::RemoveCallArg, |op| {
                op.variant = Some(position.to_string())
            })
        };

        // When the first, the middle and the last are removed in turn
        let results =
            ["first", "2", "last"].map(|position| the_call_rewritten(call, &removing(position)));

        // Then none leaves a stray comma
        assert_eq!(
            results,
            [
                Ok("f(2, 3)\n".to_string()),
                Ok("f(1, 3)\n".to_string()),
                Ok("f(1, 2)\n".to_string()),
            ]
        );
    }

    #[test]
    fn a_range_that_is_not_one_call_is_refused() {
        // Given a range over a bare name
        let rewriting = an_op(RefactorKind::ChangeCallArg, |op| {
            op.variant = Some("first".to_string());
            op.expr = Some("1".to_string());
        });

        // When an argument is changed there
        let refusal = the_call_rewritten("basket\n", &rewriting);

        // Then it is refused as not a call, naming the text it was given
        let reason = refusal.expect_err("a bare name is not a call");
        assert!(
            reason.contains("`basket` is not a call expression"),
            "the refusal does not name the text as not a call:\n{reason}"
        );
    }

    #[test]
    fn a_return_type_is_replaced_up_to_the_where_clause() {
        // Given a function returning a generic type, with a where clause
        let text = "fn f<T>(a: T) -> Vec<T> where T: Copy {\n    vec![a]\n}\n";
        let retyping = an_op(RefactorKind::ChangeReturnType, |op| {
            op.type_ = Some("Option<T>".to_string())
        });

        // When its return type changes
        let rewritten = declaration::rewrite_declaration(text, &retyping, at(1, 4))
            .map(|edits| applied(text, edits));

        // Then only the `-> …` changes
        assert_eq!(
            rewritten.map_err(|error| error.to_string()),
            Ok("fn f<T>(a: T) -> Option<T> where T: Copy {\n    vec![a]\n}\n".to_string())
        );
    }

    #[test]
    fn a_parameter_is_added_after_the_receiver_and_by_position() {
        // Given a method with a receiver and one parameter
        let text = "fn f(&self, a: u8) {}\n";
        let adding = |position: &str| {
            an_op(RefactorKind::AddParam, |op| {
                op.name = Some("b".to_string());
                op.type_ = Some("u16".to_string());
                op.variant = Some(position.to_string());
            })
        };

        // When it is added first, and after `a`
        let results = ["first", "after:a"]
            .map(|position| declaration::rewrite_declaration(text, &adding(position), at(1, 4)))
            .map(|rewritten| {
                rewritten
                    .map(|edits| applied(text, edits))
                    .map_err(|error| error.to_string())
            });

        // Then `first` stays behind the receiver
        assert_eq!(
            results,
            [
                Ok("fn f(&self, b: u16, a: u8) {}\n".to_string()),
                Ok("fn f(&self, a: u8, b: u16) {}\n".to_string()),
            ]
        );
    }

    /// `text` with the declaration whose name starts at (1, 4) rewritten as `op` asks.
    fn the_declaration_rewritten(
        text: &str,
        op: &RefactorOp,
    ) -> std::result::Result<String, String> {
        declaration::rewrite_declaration(text, op, at(1, 4))
            .map(|edits| applied(text, edits))
            .map_err(|error| error.to_string())
    }

    #[test]
    fn a_parameter_type_changes_and_the_others_stay() {
        // Given a function with a `mut` parameter and a generic one
        let text = "fn f(mut a: u8, b: Vec<(u8, u8)>) {}\n";
        let retyping = |parameter: &str, new_type: &str| {
            an_op(RefactorKind::ChangeParamType, |op| {
                op.name = Some(parameter.to_string());
                op.type_ = Some(new_type.to_string());
            })
        };

        // When each is retyped, and one that is not a parameter is named
        let results = [
            the_declaration_rewritten(text, &retyping("a", "u64")),
            the_declaration_rewritten(text, &retyping("b", "&[u8]")),
            the_declaration_rewritten(text, &retyping("c", "u8")),
        ];

        // Then only the named parameter's type changes, and the stranger is refused
        assert_eq!(
            results,
            [
                Ok("fn f(mut a: u64, b: Vec<(u8, u8)>) {}\n".to_string()),
                Ok("fn f(mut a: u8, b: &[u8]) {}\n".to_string()),
                Err("this seam cannot be cut here: `c` is not a parameter of the function the anchor names".to_string()),
            ]
        );
    }

    #[test]
    fn parameters_are_reordered_behind_a_receiver_that_stays_put() {
        // Given a method with a receiver and two parameters
        let text = "fn f(&self, a: u8, b: u16) {}\n";
        let reordering = an_op(RefactorKind::ReorderParams, |op| {
            op.order = ["b", "a"]
                .map(|name| OrderKey::Name(name.to_string()))
                .to_vec();
        });

        // When `b` moves first
        let rewritten = the_declaration_rewritten(text, &reordering);

        // Then the receiver is untouched and the two swap
        assert_eq!(rewritten, Ok("fn f(&self, b: u16, a: u8) {}\n".to_string()));
    }
}
