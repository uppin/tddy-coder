//! Rule 14: every extracted function's return type, spelled the way its caller spells `Result`.
//!
//! rust-analyzer writes the alias expanded — `Result<X, E>` — which is `E0107` in a file that
//! imports a one-argument `Result<T>` (this crate's `crate::Result`, `anyhow::Result`).

use super::signature::{generic, header_return_type, signature, Generic};
use super::span::{function_span, keyword_at};
use crate::backends::rust::early_return::masked_to_code;

/// `produced` with the return type of `name` written through the caller's one-argument `Result`
/// alias, when `origin_header` (the caller's signature) returns one and the server wrote a
/// two-argument `…Result<X, E>`; `produced` unchanged otherwise.
///
/// An alias not named `Result` (`Res<T>`) is read the same way when the function propagates —
/// holds a `?` or a `return` — because rust-analyzer expands every alias it prints, and a function
/// that propagates into the caller's error returns the caller's alias.
pub(super) fn respelled_return_type(origin_header: &str, produced: &str, name: &str) -> String {
    let masked = masked_to_code(produced);
    let respelled = function_span(produced, name).and_then(|span| {
        let returns = signature(&masked, span)?.returns?;
        let caller = header_return_type(origin_header)?;
        let alias = generic(&caller).filter(|ty| ty.arguments.len() == 1)?;
        let written = generic(&produced[returns.clone()]).filter(is_two_argument_result)?;
        let propagates = (span.open..span.close)
            .any(|at| masked.as_bytes()[at] == b'?' || keyword_at(masked.as_bytes(), at, "return"));
        (is_result(&alias) || (propagates && !is_option(&alias)))
            .then(|| (returns, format!("{}<{}>", alias.path, written.arguments[0])))
    });
    match respelled {
        Some((returns, spelled)) => {
            let mut written = produced.to_string();
            written.replace_range(returns, &spelled);
            written
        }
        None => produced.to_string(),
    }
}

fn last_segment<'a>(ty: &Generic<'a>) -> &'a str {
    ty.path.rsplit("::").next().unwrap_or("")
}

fn is_result(ty: &Generic<'_>) -> bool {
    last_segment(ty) == "Result"
}

fn is_option(ty: &Generic<'_>) -> bool {
    last_segment(ty) == "Option"
}

/// `Result<X, E>` through some path: what the server writes.
fn is_two_argument_result(ty: &Generic<'_>) -> bool {
    is_result(ty) && ty.arguments.len() == 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_two_argument_result_under_a_one_argument_alias_is_written_through_the_alias() {
        // Given a produced two-argument `Result`, extracted from a caller returning a one-argument alias
        let produced = "fn parsed_pair(b: &str, x: u32) -> Result<(u32, u32), String> {\n    \
                        let y = parse(b)?;\n    Ok((y, x + y))\n}\n";
        let caller = "pub fn plain_q(a: &str, b: &str) -> Result<u32> {";

        // When the return type is respelled
        let respelled = respelled_return_type(caller, produced, "parsed_pair");

        // Then it is written through the alias
        assert_eq!(
            respelled,
            produced.replace("-> Result<(u32, u32), String>", "-> Result<(u32, u32)>")
        );
    }

    #[test]
    fn a_qualified_std_result_is_written_through_the_callers_alias_path() {
        // Given a produced qualified `std::result::Result`, from a caller returning `crate::Result`
        let produced =
            "fn parsed(b: &str) -> std::result::Result<u32, Failure> {\n    parse(b)\n}\n";
        let caller = "fn run(b: &str) -> crate::Result<u32> {";

        // When the return type is respelled
        let respelled = respelled_return_type(caller, produced, "parsed");

        // Then it is written through the caller's alias path
        assert_eq!(
            respelled,
            "fn parsed(b: &str) -> crate::Result<u32> {\n    parse(b)\n}\n"
        );
    }

    #[test]
    fn a_caller_spelling_both_arguments_or_returning_no_result_changes_nothing() {
        // Given a produced `Result`, and callers spelling both arguments or returning an `Option`
        let produced = "fn parsed(b: &str) -> Result<u32, String> {\n    parse(b)\n}\n";
        let spelling_both = "fn run(b: &str) -> Result<u32, String> {";
        let returning_option = "fn run(b: &str) -> Option<u32> {";

        // When the return type is respelled for each
        let for_both = respelled_return_type(spelling_both, produced, "parsed");
        let for_option = respelled_return_type(returning_option, produced, "parsed");

        // Then the function is as it was in both
        assert_eq!(for_both, produced);
        assert_eq!(for_option, produced);
    }
}
