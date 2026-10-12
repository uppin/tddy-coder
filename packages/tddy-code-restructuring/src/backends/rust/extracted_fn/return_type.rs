//! Rule 14: every extracted function's return type, spelled the way its caller spells `Result`.
//!
//! rust-analyzer writes the alias expanded — `Result<X, E>` — which is `E0107` in a file that
//! imports a one-argument `Result<T>` (this crate's `crate::Result`, `anyhow::Result`).

/// `produced` with the return type of `name` written through the caller's one-argument `Result`
/// alias, when `origin_header` (the caller's signature) returns one and the server wrote a
/// two-argument `…Result<X, E>`; `produced` unchanged otherwise.
pub(super) fn respelled_return_type(origin_header: &str, produced: &str, name: &str) -> String {
    let _ = (origin_header, produced, name);
    // TODO(reshape-extract-method-clean): implement
    todo!("respelled_return_type")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_two_argument_result_under_a_one_argument_alias_is_written_through_the_alias() {
        let produced = "fn parsed_pair(b: &str, x: u32) -> Result<(u32, u32), String> {\n    \
                        let y = parse(b)?;\n    Ok((y, x + y))\n}\n";

        assert_eq!(
            respelled_return_type(
                "pub fn plain_q(a: &str, b: &str) -> Result<u32> {",
                produced,
                "parsed_pair"
            ),
            produced.replace("-> Result<(u32, u32), String>", "-> Result<(u32, u32)>")
        );
    }

    #[test]
    fn a_qualified_std_result_is_written_through_the_callers_alias_path() {
        let produced =
            "fn parsed(b: &str) -> std::result::Result<u32, Failure> {\n    parse(b)\n}\n";

        assert_eq!(
            respelled_return_type(
                "fn run(b: &str) -> crate::Result<u32> {",
                produced,
                "parsed"
            ),
            "fn parsed(b: &str) -> crate::Result<u32> {\n    parse(b)\n}\n"
        );
    }

    #[test]
    fn a_caller_spelling_both_arguments_or_returning_no_result_changes_nothing() {
        let produced = "fn parsed(b: &str) -> Result<u32, String> {\n    parse(b)\n}\n";

        assert_eq!(
            respelled_return_type(
                "fn run(b: &str) -> Result<u32, String> {",
                produced,
                "parsed"
            ),
            produced
        );
        assert_eq!(
            respelled_return_type("fn run(b: &str) -> Option<u32> {", produced, "parsed"),
            produced
        );
    }
}
