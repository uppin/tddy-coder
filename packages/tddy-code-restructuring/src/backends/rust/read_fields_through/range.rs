//! The lexical half of `read_fields_through`: which mode `expr` selects, every `self` the range
//! names (in code, comments and strings masked), and the refusals readable from the text alone
//! (RS1-RS4).
//!
//! `pub(crate)` so `#reshape` 18's `detach_method` can reuse the self-mode sites and the shadowing
//! refusal.

use std::ops::Range;

/// Which `self` uses a run rebinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// `expr` is a view of the host's fields: only `self.<field>` reads are rebound.
    Fields,
    /// `expr` is `self` itself: every `self` is rebound, and `Self` becomes the impl's self type.
    Receiver,
}

/// The mode `expr` selects: [`Mode::Receiver`] for `self`, `&*self` and `&mut *self` (compared as
/// tokens), [`Mode::Fields`] for anything else.
pub(crate) fn mode_of(expr: &str) -> Mode {
    // TODO(reshape-methods-leave-type): implement
    let _ = expr;
    todo!("TODO(reshape-methods-leave-type): implement mode_of")
}

/// One use of `self` (or of `Self`) in the range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelfSite {
    /// The byte offset of the `self` (or `Self`) token in the file's text.
    pub(crate) offset: usize,
    pub(crate) usage: SelfUse,
}

/// How a `self` token is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SelfUse {
    /// `self.<name>` read as a field; `borrowed` is the byte range of a `&` or `&mut ` directly
    /// before it, when there is one.
    Field {
        name: String,
        borrowed: Option<Range<usize>>,
    },
    /// `self.<name>(…)`: a method call.
    Method(String),
    /// `self` used bare: passed, returned, compared.
    Bare,
    /// The type keyword `Self`.
    SelfType,
}

/// Every `self` and `Self` token inside `range` of `text`, in source order; comments and strings
/// are not read.
pub(crate) fn self_sites(text: &str, range: Range<usize>) -> Vec<SelfSite> {
    // TODO(reshape-methods-leave-type): implement over `early_return::masked_to_code`
    let _ = (text, range);
    todo!("TODO(reshape-methods-leave-type): implement self_sites")
}

/// RS1-RS4, each naming its line: a range that does not start a statement, a range that names no
/// `self`, (field mode only) a method call or a bare `self`, and a `name` the enclosing function
/// already writes.
pub(crate) fn lexical_refusals(
    text: &str,
    range: Range<usize>,
    name: &str,
    mode: Mode,
) -> Vec<String> {
    // TODO(reshape-methods-leave-type): implement
    let _ = (text, range, name, mode);
    todo!("TODO(reshape-methods-leave-type): implement lexical_refusals")
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_METHOD: &str = "impl Host {\n    fn run(&self) -> u32 {\n        let n = self.count;\n        // self.hidden\n        let s = \"self.quoted\";\n        self.helper(n) + Self::BASE\n    }\n}\n";

    fn the_body_of(text: &str) -> Range<usize> {
        let start = text.find("let n").expect("the body is in the text");
        let end = text.find("Self::BASE").expect("the tail is in the text") + "Self::BASE".len();
        start..end
    }

    #[test]
    fn self_and_its_reborrows_select_receiver_mode_and_anything_else_selects_field_mode() {
        let modes: Vec<Mode> = ["self", "&*self", "&mut *self", "self.state()", "self.inner"]
            .into_iter()
            .map(mode_of)
            .collect();

        assert_eq!(
            modes,
            [
                Mode::Receiver,
                Mode::Receiver,
                Mode::Receiver,
                Mode::Fields,
                Mode::Fields
            ]
        );
    }

    #[test]
    fn the_sites_are_a_field_a_method_and_the_self_type_and_never_a_comment_or_a_string() {
        let usages: Vec<SelfUse> = self_sites(A_METHOD, the_body_of(A_METHOD))
            .into_iter()
            .map(|site| site.usage)
            .collect();

        assert_eq!(
            usages,
            [
                SelfUse::Field {
                    name: "count".to_string(),
                    borrowed: None
                },
                SelfUse::Method("helper".to_string()),
                SelfUse::SelfType,
            ]
        );
    }

    #[test]
    fn a_borrowed_field_read_carries_the_range_of_its_ampersand() {
        let text = "fn f(&self) {\n    g(&self.config);\n}\n";
        let start = text.find("g(").expect("the call is in the text");
        let ampersand = text
            .find("&self.config")
            .expect("the borrow is in the text");

        let sites = self_sites(text, start..start + "g(&self.config);".len());

        assert_eq!(
            sites[0].usage,
            SelfUse::Field {
                name: "config".to_string(),
                borrowed: Some(ampersand..ampersand + 1)
            }
        );
    }

    #[test]
    fn a_method_call_is_refused_in_field_mode_and_admitted_in_receiver_mode() {
        let in_field_mode =
            lexical_refusals(A_METHOD, the_body_of(A_METHOD), "state", Mode::Fields);
        let in_receiver_mode =
            lexical_refusals(A_METHOD, the_body_of(A_METHOD), "backend", Mode::Receiver);

        assert!(
            in_field_mode
                .iter()
                .any(|refusal| refusal.contains("`self.helper(…)` at line 6")),
            "{in_field_mode:?}"
        );
        assert_eq!(in_receiver_mode, Vec::<String>::new());
    }

    #[test]
    fn a_binding_the_function_already_writes_is_refused_as_a_shadow() {
        let refusals = lexical_refusals(A_METHOD, the_body_of(A_METHOD), "n", Mode::Receiver);

        assert!(
            refusals
                .iter()
                .any(|refusal| refusal.contains("`n` is already written at line 3")),
            "{refusals:?}"
        );
    }

    #[test]
    fn a_range_that_starts_mid_statement_is_refused() {
        let start = A_METHOD
            .find("self.count")
            .expect("the read is in the text");

        let refusals = lexical_refusals(A_METHOD, start..start + 10, "state", Mode::Fields);

        assert!(
            refusals
                .iter()
                .any(|refusal| refusal.contains("which is not the start of a statement")),
            "{refusals:?}"
        );
    }
}
