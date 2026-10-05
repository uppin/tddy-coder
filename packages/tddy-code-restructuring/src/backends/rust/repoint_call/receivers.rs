//! The bulk form's receiver walk: where the receiver of one method call begins and ends.

use std::ops::Range;

use crate::Result;

/// The byte range of the receiver of the method call whose name token starts at `site` in `text`:
/// the maximal postfix chain ending at the `.` before the name, parentheses included, and nothing
/// that binds looser (`&x.m()` has the receiver `x`, `a + b.m()` has `b`).
///
/// TODO(repoint-call): implement; refuse a receiver that ends in a block or a closure, or holds a
/// comment, and a candidate `receiver.name(args)` that `syn` does not read as one method call.
pub(super) fn receiver_span(_text: &str, _site: usize) -> Result<Range<usize>> {
    Err(super::unfinished())
}

/// The offset at which the hops of a template are inserted for the method call at `site`: the end
/// of its receiver.
///
/// TODO(repoint-call): called by `sites` once the bulk form is written; `hops` is carried so the
/// refusal of an insertion that would not parse can name it.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn insertions_for(text: &str, site: usize, _hops: &str) -> Result<usize> {
    receiver_span(text, site).map(|receiver| receiver.end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// The receiver of the call of `m` in `statement`, as text.
    fn the_receiver_of_m_in(statement: &str) -> String {
        let site = statement.find("m(").expect("the statement calls `m`");
        let span = receiver_span(statement, site).expect("the receiver is read");
        statement[span].to_string()
    }

    /// The refusal reading the receiver of the call of `m` in `statement` gives.
    fn the_refusal_for_the_receiver_of_m_in(statement: &str) -> String {
        let site = statement.find("m(").expect("the statement calls `m`");
        insertions_for(statement, site, ".peer")
            .expect_err("the receiver is refused")
            .to_string()
    }

    #[test]
    fn the_receiver_walk_stops_at_the_operator_that_binds_looser() {
        // Given calls of `m` preceded by an operator that binds looser, or by a postfix that belongs
        let receivers = [
            the_receiver_of_m_in("let y = &x.m(1);"),
            the_receiver_of_m_in("let y = -x.m(1);"),
            the_receiver_of_m_in("let y = a + b.m(1);"),
            the_receiver_of_m_in("let y = x?.m(1);"),
            the_receiver_of_m_in("let y = fut.await.m(1);"),
            the_receiver_of_m_in("let y = v[0].m(1);"),
            the_receiver_of_m_in("let y = (a + b).m(1);"),
        ];

        // Then the receiver stops at the operator, and keeps `?`, `.await`, indexing and parentheses
        assert_eq!(
            receivers,
            ["x", "x", "b", "x?", "fut.await", "v[0]", "(a + b)"]
        );
    }

    #[test]
    fn a_receiver_ending_in_a_block_or_holding_a_comment_is_refused() {
        // Given calls of `m` on a block, on a closure's block body, and on a receiver holding a comment
        let refusals = [
            the_refusal_for_the_receiver_of_m_in("let y = unsafe { x }.m(1);"),
            the_refusal_for_the_receiver_of_m_in("let y = (|| { x })().m(1);"),
            the_refusal_for_the_receiver_of_m_in("let y = x /* why */ .m(1);"),
        ];

        // Then each is refused for its receiver, not for the operation being unimplemented
        assert!(
            refusals.iter().all(|refusal| refusal.contains("receiver")),
            "{refusals:?}"
        );
    }
}
