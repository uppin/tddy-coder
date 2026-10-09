//! Rules 12-13: a range holding a `return`, and what an extraction can do with it.
//!
//! Today every such range is refused unless it runs to the function's tail. A run of
//! `return Err(..)` guards in the middle of a function can instead become a function returning
//! `Result<()>`, called with `?`; a range that is a tail-position arm's body keeps its `return`s,
//! which still return from the caller there.

use crate::edit::Range;
use crate::Result;

/// The exits of an `extract_method` range, read from the text before any server exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Exits {
    /// No `return` in the range.
    None,
    /// The range runs to the function's tail, or is the whole body of an arm or branch in tail
    /// position: its `return`s keep their meaning.
    Tail,
    /// Every `return` is `return Err(..)`, in a function returning `Result`: lifted.
    ErrGuards,
    /// Every `return` is `return None`, in a function returning `Option`: lifted.
    NoneGuards,
    /// A range no rule carries, with the refusal's text (after `this seam cannot be cut here: `).
    Refused(String),
}

/// What the range's `return`s are, and so what an extraction may do with them.
pub(super) fn classify(text: &str, range: Range) -> Exits {
    let _ = (text, range);
    // TODO(reshape-extract-method-clean): implement
    todo!("classify")
}

/// Whether `range` ends at the end of a block's tail expression whose block — a match arm body, an
/// `if`/`else` branch or a plain block — is itself in tail position, up to the function body.
pub(super) fn in_tail_position(text: &str, range: Range) -> bool {
    let _ = (text, range);
    // TODO(reshape-extract-method-clean): implement
    todo!("in_tail_position")
}

/// `produced` with the guard run the assist extracted into `name` written as a function returning
/// the caller's `Result<()>` (`Option<()>`) called with `?`. Shape A (rust-analyzer's
/// `Option<Result<T, E>>` + `if let Some(value) = … { return value; }`) is rewritten; shape B (the
/// range held a `?`, and the server wrote the lifted form itself) is kept. Any other shape is
/// refused as the server's defect.
pub(super) fn lifted(
    origin_header: &str,
    produced: &str,
    name: &str,
    exits: &Exits,
) -> Result<String> {
    let _ = (origin_header, produced, name, exits);
    // TODO(reshape-extract-method-clean): implement
    todo!("lifted")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Position;
    use std::sync::{Arc, Mutex};

    /// One function per `Exits` case, each range a whole-line run of its body.
    const FUNCTIONS: &str = "\
pub fn guards(a: u32, b: &str) -> Res<u32> {
    let n = a + 1;
    if n > 10 {
        return Err(format!(\"too big {n}\").into());
    }
    for c in b.chars() {
        if c == 'x' {
            return Err(\"x\".into());
        }
    }
    Ok(n * 2)
}

pub fn firsts(a: Option<u32>) -> Option<u32> {
    let n = a?;
    if n == 0 {
        return None;
    }
    Some(n)
}

pub fn valued(x: bool) -> Result<u32, String> {
    let base = 2;
    if x {
        return Ok(1);
    }
    Ok(base)
}

pub fn mixed(x: bool, y: bool) -> Result<u32, String> {
    let base = 2;
    if x {
        return Err(\"x\".into());
    }
    if y {
        return Ok(3);
    }
    Ok(base)
}

pub fn bound(x: u32) -> Result<u32, String> {
    let doubled = x * 2;
    if doubled > 9 {
        return Err(\"big\".into());
    }
    Ok(doubled)
}

pub fn arms(k: u8, v: Option<u32>) -> Res<u32> {
    match k {
        0 => Ok(1),
        _ => {
            let base = k as u32;
            let Some(found) = v else {
                return Ok(base);
            };
            Ok(base + found)
        }
    }
}
";

    /// Lines `first..=last` of [`FUNCTIONS`], from the first non-blank character to the end.
    fn lines(first: u32, last: u32) -> Range {
        let source: Vec<&str> = FUNCTIONS.split('\n').collect();
        let opening = source[first as usize - 1];
        Range {
            start: Position {
                line: first,
                col: (opening.len() - opening.trim_start().len()) as u32 + 1,
            },
            end: Position {
                line: last,
                col: source[last as usize - 1].chars().count() as u32 + 1,
            },
        }
    }

    fn a_silent_sink() -> crate::backends::rust::ProgressSink {
        let heard: Arc<Mutex<Vec<String>>> = Arc::default();
        Arc::new(move |line: &str| heard.lock().unwrap().push(line.to_string()))
    }

    #[test]
    fn classifies_each_kind_of_range_holding_a_return() {
        assert_eq!(classify(FUNCTIONS, lines(3, 10)), Exits::ErrGuards);
        assert_eq!(classify(FUNCTIONS, lines(16, 18)), Exits::NoneGuards);
        assert_eq!(classify(FUNCTIONS, lines(53, 57)), Exits::Tail);
        assert_eq!(classify(FUNCTIONS, lines(2, 2)), Exits::None);
    }

    #[test]
    fn a_tail_match_arms_body_is_in_tail_position_and_a_mid_function_run_is_not() {
        assert!(in_tail_position(FUNCTIONS, lines(53, 57)));
        assert!(!in_tail_position(FUNCTIONS, lines(3, 10)));
    }

    #[test]
    fn a_static_check_accepts_the_ranges_resolve_lifts_and_refuses_the_ranges_resolve_refuses() {
        let findings = |range: Range| {
            super::super::extract_method_findings(FUNCTIONS, range, &a_silent_sink())
        };

        assert_eq!(findings(lines(3, 10)), Vec::<String>::new(), "error guards");
        assert_eq!(findings(lines(16, 18)), Vec::<String>::new(), "none guards");
        assert_eq!(findings(lines(53, 57)), Vec::<String>::new(), "a tail arm");
        assert_eq!(
            findings(lines(24, 26)),
            ["this seam cannot be cut here: the range returns early from the function around it, on line 25 (`return Ok(1);`). \
              An extracted function cannot carry an early exit of its caller: the assist copies \
              the `return` verbatim, so it returns from the new function instead — whose return \
              type differs, which is `E0308` at best and a silently skipped exit at worst. Cut the \
              range so it holds no `return`, end it before the first one, or run it to the end of \
              the function's tail expression, where the call becomes the tail and a `return` \
              means what it did."
                .to_string()],
            "a value return"
        );
        assert_eq!(
            findings(lines(32, 37)),
            ["this seam cannot be cut here: the range holds error guards and a return of a \
              value, on line 36 (`return Ok(3);`). Only a run whose every return is \
              `return Err(..)` can become a function called with `?`; end the range before line 36."
                .to_string()],
            "a mix"
        );
        assert_eq!(
            findings(lines(42, 45)),
            [
                "this seam cannot be cut here: the range's guards are followed by code that reads \
              `doubled`, which the range declares. Cut the run so it ends before `let doubled`."
                    .to_string()
            ],
            "a guard run with an output"
        );
    }

    #[test]
    fn rewrites_the_servers_option_of_result_shape_into_a_result_unit_called_with_a_question_mark()
    {
        let produced = "pub fn guards(a: u32, b: &str) -> Res<u32> {
    let n = a + 1;
    if let Some(value) = checked(b, n) {
        return value;
    }
    Ok(n * 2)
}

fn checked(b: &str, n: u32) -> Option<Result<u32, String>> {
    if n > 10 {
        return Some(Err(format!(\"too big {n}\").into()));
    }
    None
}
";

        let text = lifted(
            "pub fn guards(a: u32, b: &str) -> Res<u32> {",
            produced,
            "checked",
            &Exits::ErrGuards,
        )
        .expect("the server's shape is the one the lift rewrites");

        assert_eq!(
            text,
            "pub fn guards(a: u32, b: &str) -> Res<u32> {
    let n = a + 1;
    checked(b, n)?;
    Ok(n * 2)
}

fn checked(b: &str, n: u32) -> Res<()> {
    if n > 10 {
        return Err(format!(\"too big {n}\").into());
    }
    Ok(())
}
"
        );
    }

    #[test]
    fn a_shape_the_lift_does_not_know_is_the_servers_defect() {
        let produced = "fn checked(n: u32) -> ControlFlow<Result<u32, String>> {\n    ControlFlow::Continue(())\n}\n";

        let refusal = lifted(
            "fn guards(n: u32) -> Res<u32> {",
            produced,
            "checked",
            &Exits::ErrGuards,
        )
        .expect_err("an unknown shape is refused")
        .to_string();

        assert!(
            refusal.starts_with("rust-analyzer's answer was unusable:"),
            "{refusal}"
        );
    }
}
