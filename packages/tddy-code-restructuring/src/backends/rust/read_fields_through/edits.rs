//! The edit `read_fields_through` makes: the inserted `let`, and every `self` site rewritten.
//!
//! `pub(crate)` so `#reshape` 18's `detach_method` can reuse the self-mode rewrite.

use std::collections::BTreeSet;
use std::ops::Range;

use super::range::SelfSite;

/// `text` with `let <name> = <expr>;` inserted at `range`'s start (at that line's indentation), and
/// every site rewritten: a field read to `<name>.<field>` (its `&` dropped when the field is in
/// `drop_borrow`), a method call or a bare `self` to `<name>`, and a `Self` to `self_type`. In field
/// mode the sites hold no `Self`, so `self_type` is not read.
pub(crate) fn rebound(
    text: &str,
    range: Range<usize>,
    name: &str,
    expr: &str,
    self_type: &str,
    sites: &[SelfSite],
    drop_borrow: &BTreeSet<String>,
) -> String {
    // TODO(reshape-methods-leave-type): implement
    let _ = (text, range, name, expr, self_type, sites, drop_borrow);
    todo!("TODO(reshape-methods-leave-type): implement rebound")
}

#[cfg(test)]
mod tests {
    use super::super::range::SelfUse;
    use super::*;

    const BEFORE: &str = "    fn f(&self) {\n        let a = self.rosters.get();\n        g(&self.config, Self::BASE);\n    }\n";

    fn the_range() -> Range<usize> {
        let start = BEFORE.find("let a").expect("the range starts");
        let end = BEFORE.find("BASE);").expect("the range ends") + "BASE);".len();
        start..end
    }

    fn at(needle: &str) -> usize {
        BEFORE.find(needle).expect("the needle is in the text")
    }

    #[test]
    fn field_mode_inserts_the_let_rewrites_each_read_and_drops_a_borrow_the_state_provides() {
        let sites = [
            SelfSite {
                offset: at("self.rosters"),
                usage: SelfUse::Field {
                    name: "rosters".to_string(),
                    borrowed: None,
                },
            },
            SelfSite {
                offset: at("self.config"),
                usage: SelfUse::Field {
                    name: "config".to_string(),
                    borrowed: Some(at("&self.config")..at("&self.config") + 1),
                },
            },
        ];
        let drop_borrow = BTreeSet::from(["config".to_string()]);

        let after = rebound(
            BEFORE,
            the_range(),
            "state",
            "self.state()",
            "Host",
            &sites,
            &drop_borrow,
        );

        assert_eq!(
            after,
            "    fn f(&self) {\n        let state = self.state();\n        let a = state.rosters.get();\n        g(state.config, Self::BASE);\n    }\n"
        );
    }

    #[test]
    fn receiver_mode_rewrites_self_and_the_self_type() {
        let sites = [
            SelfSite {
                offset: at("self.rosters"),
                usage: SelfUse::Field {
                    name: "rosters".to_string(),
                    borrowed: None,
                },
            },
            SelfSite {
                offset: at("self.config"),
                usage: SelfUse::Field {
                    name: "config".to_string(),
                    borrowed: Some(at("&self.config")..at("&self.config") + 1),
                },
            },
            SelfSite {
                offset: at("Self::BASE"),
                usage: SelfUse::SelfType,
            },
        ];

        let after = rebound(
            BEFORE,
            the_range(),
            "backend",
            "self",
            "Host",
            &sites,
            &BTreeSet::new(),
        );

        assert_eq!(
            after,
            "    fn f(&self) {\n        let backend = self;\n        let a = backend.rosters.get();\n        g(&backend.config, Host::BASE);\n    }\n"
        );
    }
}
