//! Rules 12-13: a range holding a `return`, and what an extraction can do with it.
//!
//! Today every such range is refused unless it runs to the function's tail. A run of
//! `return Err(..)` guards in the middle of a function can instead become a function returning
//! `Result<()>`, called with `?`; a range that is a tail-position arm's body keeps its `return`s,
//! which still return from the caller there.

use super::signature::{generic, header_return_type, signature};
use super::span::{
    enclosing_function, function_span, keyword_at, matching_close, matching_parenthesis,
    skip_whitespace, statements, word_end, FnSpan,
};
use super::tail::in_tail_position;
use crate::backends::rust::early_return::{
    byte_offset, early_returns, masked_to_code, refuse_early_returns,
};
use crate::backends::rust::server_defect;
use crate::edit::Range;
use crate::{RestructureError, Result};

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

/// What one `return` of the range returns.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Returned {
    Err,
    None,
    Value,
}

/// What the range's `return`s are, and so what an extraction may do with them.
pub(super) fn classify(text: &str, range: Range) -> Exits {
    let lines = early_returns(text, range);
    if lines.is_empty() {
        return Exits::None;
    }
    let masked = masked_to_code(text);
    let (Some(from), Some(to)) = (byte_offset(text, range.start), byte_offset(text, range.end))
    else {
        return refused_as_today(text, range);
    };
    let Some(function) = enclosing_function(masked.as_bytes(), from) else {
        return refused_as_today(text, range);
    };
    let returns = signature(&masked, function).and_then(|signature| signature.returns);
    if in_tail_position(text, range) && returns.is_some() {
        return Exits::Tail;
    }

    let returned = returns_of(text, &masked, from..to, &lines);
    let has = |kind: Returned| returned.iter().any(|(_, found)| *found == kind);
    if has(Returned::Value) && returned.len() > 1 {
        return Exits::Refused(mixed(text, &returned));
    }
    if has(Returned::Value) {
        return refused_as_today(text, range);
    }
    let want = if has(Returned::Err) {
        "Result"
    } else {
        "Option"
    };
    let ret = returns.map(|returns| masked[returns].to_string());
    if let Some(refusal) =
        unliftable_guards(&masked, from..to, function.close, ret.as_deref(), want)
    {
        return Exits::Refused(refusal);
    }
    if want == "Result" {
        Exits::ErrGuards
    } else {
        Exits::NoneGuards
    }
}

/// Today's refusal of a range holding a `return`, or the exception to it.
fn refused_as_today(text: &str, range: Range) -> Exits {
    match refuse_early_returns(text, range) {
        Err(RestructureError::SeamRefused(refusal)) => Exits::Refused(refusal),
        Err(other) => Exits::Refused(other.to_string()),
        Ok(()) => Exits::Tail,
    }
}

/// Every `return` of the range on one of `lines`, with the line and what it returns.
fn returns_of(
    text: &str,
    masked: &str,
    within: std::ops::Range<usize>,
    lines: &[u32],
) -> Vec<(u32, Returned)> {
    let code = masked.as_bytes();
    let mut found = Vec::new();
    for at in within.clone().filter(|at| keyword_at(code, *at, "return")) {
        let line = text[..at].matches('\n').count() as u32 + 1;
        if !lines.contains(&line) || (at >= 2 && &code[at - 2..at] == b"r#") {
            continue;
        }
        let value = skip_whitespace(code, at + "return".len());
        let word = &masked[value..word_end(code, value)];
        found.push((
            line,
            match word {
                "Err" => Returned::Err,
                "None" => Returned::None,
                _ => Returned::Value,
            },
        ));
    }
    found
}

/// The refusal for a range mixing guards with a return of a value, naming the value returns.
fn mixed(text: &str, returned: &[(u32, Returned)]) -> String {
    let source: Vec<&str> = text.split('\n').collect();
    let values: Vec<(u32, &str)> = returned
        .iter()
        .filter(|(_, kind)| *kind == Returned::Value)
        .map(|(line, _)| (*line, source[*line as usize - 1].trim()))
        .collect();
    let named: Vec<String> = values
        .iter()
        .map(|(line, written)| format!("line {line} (`{written}`)"))
        .collect();
    format!(
        "the range holds error guards and a return of a value, on {}. Only a run whose every \
         return is `return Err(..)` can become a function called with `?`; end the range before \
         line {}.",
        named.join(" and "),
        values[0].0
    )
}

/// Why guards cannot be lifted out of the range, or `None` when they can: the function around it
/// returns the matching `want` (`Result` or `Option`), no label crosses the range's edge, and no
/// binding the range declares is read after it.
fn unliftable_guards(
    masked: &str,
    within: std::ops::Range<usize>,
    function_end: usize,
    returns: Option<&str>,
    want: &str,
) -> Option<String> {
    let code = masked.as_bytes();
    let returns_want = returns.and_then(generic).is_some_and(|ty| {
        let last = ty.path.rsplit("::").next().unwrap_or("");
        !ty.arguments.is_empty() && (last == "Option") == (want == "Option")
    });
    if !returns_want {
        return Some(format!(
            "the range holds `return {}` guards, and the function around it returns `{}`, which \
             is not a `{want}`, so there is nothing for `?` to return",
            if want == "Result" { "Err(..)" } else { "None" },
            returns.unwrap_or("()")
        ));
    }
    let region = &masked[within.clone()];
    if ["break", "continue"]
        .iter()
        .any(|word| labelled(region, word))
    {
        return Some(
            "the range holds a labelled `break` or `continue` beside its guards, and the extracted \
             function could not carry the label; end the range before it"
                .to_string(),
        );
    }
    declared_and_read_after(code, within, function_end).map(|binding| {
        format!(
            "the range's guards are followed by code that reads `{binding}`, which the range \
             declares. Cut the run so it ends before `let {binding}`."
        )
    })
}

/// Whether masked `region` holds `word` followed by a label (`break 'outer`).
fn labelled(region: &str, word: &str) -> bool {
    let code = region.as_bytes();
    (0..code.len())
        .filter(|at| keyword_at(code, *at, word))
        .any(|at| code.get(skip_whitespace(code, at + word.len())) == Some(&b'\''))
}

/// The first binding the range declares at its top level that the function reads after it.
fn declared_and_read_after(
    code: &[u8],
    within: std::ops::Range<usize>,
    function_end: usize,
) -> Option<String> {
    let after = &code[within.end..function_end];
    statements(&code[within.clone()], within.start)
        .iter()
        .filter_map(|statement| {
            let declaration = &code[statement.start..statement.end];
            let from = skip_whitespace(declaration, "let".len());
            let bound = declaration.starts_with(b"let ").then(|| {
                let from = if declaration[from..].starts_with(b"mut ") {
                    skip_whitespace(declaration, from + "mut".len())
                } else {
                    from
                };
                String::from_utf8_lossy(&declaration[from..word_end(declaration, from)])
                    .into_owned()
            })?;
            Some(bound).filter(|bound| !bound.is_empty() && bound != "_")
        })
        .find(|bound| reads(after, bound))
}

/// Whether `bound` is named as a whole word in `code`.
fn reads(code: &[u8], bound: &str) -> bool {
    (0..code.len()).any(|at| keyword_at(code, at, bound))
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
    let (wrapper, unit) = match exits {
        Exits::ErrGuards => ("Err", "Ok(())"),
        Exits::NoneGuards => ("None", "Some(())"),
        _ => return Ok(produced.to_string()),
    };
    let masked = masked_to_code(produced);
    let function =
        function_span(produced, name).ok_or_else(|| unusable(&format!("a `fn {name}`")))?;
    let returns = signature(&masked, function)
        .and_then(|found| found.returns)
        .ok_or_else(|| unusable("a return type"))?;
    let lift = Lift {
        produced,
        masked: &masked,
        name,
        wrapper,
        unit,
        function,
        returns,
    };

    let mut edits = match call_statement(masked.as_bytes(), function.keyword, name) {
        Some((statement, call)) => lift.shape_a(origin_header, statement, call)?,
        None => lift.accept_shape_b()?,
    };
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut written = produced.to_string();
    for (range, replacement) in edits {
        written.replace_range(range, &replacement);
    }
    Ok(written)
}

/// The bytes of a rewrite and what replaces them.
type Edits = Vec<(std::ops::Range<usize>, String)>;

fn unusable(expected: &str) -> RestructureError {
    server_defect(format!(
        "rust-analyzer wrote the guard run extracted into a function in a shape the lift does not \
         know: expected {expected}"
    ))
}

/// What the lift reads of the server's answer: the whole file and the function the range became.
struct Lift<'a> {
    produced: &'a str,
    masked: &'a str,
    name: &'a str,
    /// What each `return` of the guard run returns (`Err`, `None`), and the unit tail that ends it.
    wrapper: &'a str,
    unit: &'a str,
    function: FnSpan,
    returns: std::ops::Range<usize>,
}

impl Lift<'_> {
    /// Shape A: the call's `if let Some(..)` statement becomes `<call>?;`, the return type the
    /// caller's with a unit first argument, each `return Some(<guard>)` a `return <guard>` and the
    /// `None` tail `Ok(())` (`Some(())`).
    fn shape_a(
        &self,
        origin_header: &str,
        statement: std::ops::Range<usize>,
        call: std::ops::Range<usize>,
    ) -> Result<Edits> {
        let written = generic(&self.masked[self.returns.clone()])
            .filter(|ty| ty.path.ends_with("Option") && ty.arguments.len() == 1);
        written.ok_or_else(|| {
            unusable(&format!(
                "`Option<..>` as the return type of `{}`",
                self.name
            ))
        })?;
        let caller = header_return_type(origin_header)
            .and_then(|caller| with_unit_first_argument(&caller))
            .ok_or_else(|| unusable("a caller returning a generic type"))?;

        let mut edits = vec![
            (statement, format!("{}?;", &self.produced[call])),
            (self.returns.clone(), caller),
        ];
        edits.extend(self.guard_returns()?);
        Ok(edits)
    }

    /// Shape B: `<name>(..)?;` calls a function that already returns `Result<(), E>` (`Option<()>`).
    fn accept_shape_b(&self) -> Result<Edits> {
        let code = self.masked.as_bytes();
        let calls_with_question_mark =
            self.masked
                .match_indices(&format!("{}(", self.name))
                .any(|(at, _)| {
                    at < self.function.keyword
                        && matching_parenthesis(code, at + self.name.len())
                            .is_some_and(|end| code.get(end + 1) == Some(&b'?'))
                });
        let unit = generic(&self.masked[self.returns.clone()]).is_some_and(|ty| {
            ty.arguments.first() == Some(&"()")
                && (self.wrapper == "Err") != ty.path.ends_with("Option")
        });
        if calls_with_question_mark && unit {
            return Ok(Vec::new());
        }
        Err(unusable(&format!(
            "`if let Some(..) = {name}(..) {{ return ..; }}` or `{name}(..)?;`",
            name = self.name
        )))
    }

    /// The edits unwrapping each `return Some(<guard>)` of the function and replacing its `None`
    /// tail with the unit one; refused when a `return` is not wrapped that way.
    fn guard_returns(&self) -> Result<Edits> {
        let (code, function) = (self.masked.as_bytes(), self.function);
        let mut edits = Vec::new();
        for at in (function.open..function.close).filter(|at| keyword_at(code, *at, "return")) {
            let wrapped = skip_whitespace(code, at + "return".len());
            let inner = skip_whitespace(code, wrapped + "Some(".len());
            let guard = code[wrapped..].starts_with(b"Some(")
                && code[inner..].starts_with(self.wrapper.as_bytes());
            let end = matching_parenthesis(code, wrapped + "Some".len()).filter(|_| guard);
            let end = end.ok_or_else(|| {
                unusable(&format!(
                    "every `return` to be `return Some({}..)`",
                    self.wrapper
                ))
            })?;
            edits.push((end..end + 1, String::new()));
            edits.push((wrapped..wrapped + "Some(".len(), String::new()));
        }
        let body = statements(&code[function.open + 1..function.close], function.open + 1);
        let tail = body
            .last()
            .filter(|tail| !tail.terminated && &code[tail.start..tail.end] == b"None")
            .ok_or_else(|| unusable("a body that ends in `None`"))?;
        edits.push((tail.start..tail.end, self.unit.to_string()));
        Ok(edits)
    }
}

/// `Result<u32, String>` as `Result<(), String>`: the type with its first generic argument `()`.
fn with_unit_first_argument(ty: &str) -> Option<String> {
    let ty = generic(ty)?;
    let mut arguments = ty.arguments;
    *arguments.first_mut()? = "()";
    Some(format!("{}<{}>", ty.path, arguments.join(", ")))
}

/// The statement `if let Some(<v>) = <name>(<args>) { return <v>; }` calling the function — its
/// bytes, and the bytes of the call itself — in masked `code` before the function at `before`.
fn call_statement(
    code: &[u8],
    before: usize,
    name: &str,
) -> Option<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let masked = std::str::from_utf8(code).ok()?;
    let at = masked[..before].find("if let Some(")?;
    let bound_end = at + masked[at..].find(')')?;
    let bound = masked[at + "if let Some(".len()..bound_end].trim();
    let call_start = skip_whitespace(code, skip_whitespace(code, bound_end + 1) + 1);
    let named = masked[call_start..].starts_with(&format!("{name}("));
    let call_end = matching_parenthesis(code, call_start + name.len())?;
    let open = skip_whitespace(code, call_end + 1);
    let close = matching_close(code, open).filter(|_| code[open] == b'{')?;
    let body: String = masked[open + 1..close].split_whitespace().collect();
    (named && !bound.is_empty() && body == format!("return{bound};"))
        .then_some((at..close + 1, call_start..call_end + 1))
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
