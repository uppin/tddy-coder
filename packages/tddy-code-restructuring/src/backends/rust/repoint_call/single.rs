//! The single form: the callee of one call replaced, the arguments untouched.

use crate::edit::{Range, TextEdit};
use crate::Result;

/// The edits that replace everything before the argument list of the one call `range` covers in
/// `text` with `callee`.
///
/// TODO(repoint-call): implement (the range is exactly one call, the old callee holds no call, a
/// turbofish the new callee cannot restate is refused, a callee equal to the current one is refused).
pub(super) fn rewrite_callee(_text: &str, _range: Range, _callee: &str) -> Result<Vec<TextEdit>> {
    Err(super::unfinished())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Position;
    use pretty_assertions::assert_eq;

    /// The range over the whole of `text`, one-based, the way a plan anchors a call.
    fn over_the_whole_of(text: &str) -> Range {
        let last_line = text.lines().last().unwrap_or_default();
        Range {
            start: Position { line: 1, col: 1 },
            end: Position {
                line: text.lines().count() as u32,
                col: last_line.len() as u32 + 1,
            },
        }
    }

    /// `text` with the edits applied, last first, so earlier offsets stay valid.
    fn applied(text: &str, edits: &[TextEdit]) -> String {
        let mut result = text.to_string();
        let mut ordered: Vec<&TextEdit> = edits.iter().collect();
        ordered.sort_by_key(|edit| (edit.range.start.line, edit.range.start.col));
        for edit in ordered.into_iter().rev() {
            let offset = |at: Position| {
                let before: usize = result
                    .split_inclusive('\n')
                    .take(at.line as usize - 1)
                    .map(str::len)
                    .sum();
                before + at.col as usize - 1
            };
            let (from, to) = (offset(edit.range.start), offset(edit.range.end));
            result.replace_range(from..to, &edit.new_text);
        }
        result
    }

    /// The call `text` is, after re-pointing it to `callee`.
    fn re_pointed(text: &str, callee: &str) -> String {
        let edits = rewrite_callee(text, over_the_whole_of(text), callee)
            .expect("the call can be re-pointed");
        applied(text, &edits)
    }

    /// The refusal re-pointing the call `text` is to `callee` gives.
    fn the_refusal_re_pointing(text: &str, callee: &str) -> String {
        rewrite_callee(text, over_the_whole_of(text), callee)
            .expect_err("the call cannot be re-pointed")
            .to_string()
    }

    #[test]
    fn single_replaces_only_what_precedes_the_argument_list() {
        // Given a call whose arguments hold nested calls, delimiters in strings and a comment
        let call = "self.slot(\n    a, // , )\n    f(\",)\"),\n    g(/* ( */ 1),\n    \")\",\n)";

        // When it is re-pointed through a field
        let after = re_pointed(call, "self.peer.slot");

        // Then everything from the opening parenthesis on is byte-identical
        assert_eq!(
            after,
            "self.peer.slot(\n    a, // , )\n    f(\",)\"),\n    g(/* ( */ 1),\n    \")\",\n)"
        );
    }

    #[test]
    fn single_turns_a_method_call_into_a_field_chain_call_and_keeps_the_receiver_only_if_the_callee_names_it(
    ) {
        // Given a method call
        let call = "self.dir_for(id)";

        // When it is re-pointed to a field chain that names the receiver, and to a path that does not
        let through_a_field = re_pointed(call, "self.peer.dir_for");
        let through_a_path = re_pointed(call, "lookup::dir_for");

        // Then the receiver survives in the first and is dropped in the second, the arguments in both
        assert_eq!(
            (through_a_field, through_a_path),
            (
                "self.peer.dir_for(id)".to_string(),
                "lookup::dir_for(id)".to_string()
            )
        );
    }

    #[test]
    fn single_refuses_an_old_callee_holding_a_call_a_turbofish_the_callee_cannot_restate_and_a_noop(
    ) {
        // Given three calls whose re-pointing would lose an argument, lose a turbofish, or change nothing
        let holding_a_call = "a.m(x).n(y)";
        let with_a_turbofish = "recv.m::<T>(x)";
        let already_there = "self.slot(x)";

        // When each is re-pointed
        let refusals = [
            the_refusal_re_pointing(holding_a_call, "b.n"),
            the_refusal_re_pointing(with_a_turbofish, "self.peer.m"),
            the_refusal_re_pointing(already_there, "self.slot"),
        ];

        // Then each is refused naming what is wrong with it
        assert!(refusals[0].contains("m(x)"), "{}", refusals[0]);
        assert!(refusals[1].contains("::<T>"), "{}", refusals[1]);
        assert!(refusals[2].contains("self.slot"), "{}", refusals[2]);
    }
}
