//! The edit `read_fields_through` makes: the inserted `let`, and every `self` site rewritten.
//!
//! `pub(crate)` so `#reshape` 18's `detach_method` can reuse the self-mode rewrite.

use std::collections::BTreeSet;
use std::ops::Range;

use super::range::{mode_of, Mode, SelfSite, SelfUse};

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
    let rewrites = rewrites(text, range, name, expr, self_type, sites, drop_borrow);
    let mut out = String::with_capacity(text.len() + 64);
    let mut cursor = 0;
    for (span, replacement) in rewrites {
        out.push_str(&text[cursor..span.start]);
        out.push_str(&replacement);
        cursor = span.end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// The spans of `text` [`rebound`] replaces and what each is replaced with, in source order: the
/// insertion of the `let` first, then one span per site.
///
/// Each is a span of its own, never one over the whole range, so a position between two of them
/// (the start and end of a range an `extract_method` of the same group is anchored on) is a
/// position the coordinate ledger can still translate.
pub(crate) fn rewrites(
    text: &str,
    range: Range<usize>,
    name: &str,
    expr: &str,
    self_type: &str,
    sites: &[SelfSite],
    drop_borrow: &BTreeSet<String>,
) -> Vec<(Range<usize>, String)> {
    let receiver = mode_of(expr) == Mode::Receiver;
    let mut sites: Vec<(Range<usize>, String)> = sites
        .iter()
        .filter(|site| range.contains(&site.offset))
        .filter_map(|site| match &site.usage {
            SelfUse::Field {
                name: field,
                borrowed,
            } => {
                let borrow = borrowed
                    .as_ref()
                    .filter(|_| !receiver && drop_borrow.contains(field));
                let start = borrow.map_or(site.offset, |borrow| borrow.start);
                Some((start..site.offset + "self".len(), name.to_string()))
            }
            SelfUse::Method(_) | SelfUse::Bare => {
                receiver.then(|| (site.offset..site.offset + "self".len(), name.to_string()))
            }
            SelfUse::SelfType => receiver.then(|| {
                (
                    site.offset..site.offset + "Self".len(),
                    self_type.to_string(),
                )
            }),
        })
        .collect();
    sites.sort_by_key(|(span, _)| span.start);

    let line_start = text[..range.start].rfind('\n').map_or(0, |at| at + 1);
    let indent_len = text[line_start..]
        .find(|c: char| !(c == ' ' || c == '\t'))
        .unwrap_or(0);
    let indent = &text[line_start..line_start + indent_len];

    let mut rewrites = vec![(
        range.start..range.start,
        format!("let {name} = {expr};\n{indent}"),
    )];
    rewrites.extend(sites);
    rewrites
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
