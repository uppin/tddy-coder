use super::DEFINING_KEYWORDS;

use super::Kind;

use super::Scan;

use super::UseLeaf;

use std::ops::Range;

/// A `mod` a module declares, inline or backed by a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChildModule {
    pub(crate) name: String,
    /// The span between an inline module's braces; `None` for `mod name;`.
    pub(crate) body: Option<Range<usize>>,
    /// Whether it is written plain `pub mod`, which is what makes it nameable from another crate.
    /// `pub(crate)`, `pub(super)`, `pub(in …)` and no visibility at all all read as not `pub`.
    pub(crate) is_public: bool,
}

/// What a module's own text declares at its top level.
#[derive(Debug, Default)]
pub(crate) struct ModuleItems {
    pub(crate) children: Vec<ChildModule>,
    pub(crate) uses: Vec<UseLeaf>,
    /// Names of the items it defines: functions, types, traits, constants, statics, macros.
    pub(crate) defined: Vec<String>,
}

/// The top level of a module's text: its `mod`s, its `use`s and the items it defines.
pub(crate) fn items_of_module(text: &str) -> ModuleItems {
    let scan = Scan::of(text);
    let mut items = ModuleItems::default();
    let mut depth = 0usize;
    let mut open_child: Option<usize> = None;
    let mut at = 0usize;

    while at < scan.tokens.len() {
        let token = scan.tokens[at];
        match token.kind {
            Kind::Punct(b'{') => depth += 1,
            Kind::Punct(b'}') => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(child) = open_child.take() {
                        if let Some(body) = items.children[child].body.as_mut() {
                            body.end = token.start;
                        }
                    }
                }
            }
            Kind::Ident if depth == 0 => match scan.text(token) {
                "mod" => {
                    if let Some(name) = scan.ident(at + 1) {
                        let body = scan
                            .tokens
                            .get(at + 2)
                            .filter(|_| scan.is_punct(at + 2, b'{'))
                            .map(|opens| opens.end..opens.end);
                        if body.is_some() {
                            open_child = Some(items.children.len());
                        }
                        items.children.push(ChildModule {
                            name: name.to_string(),
                            body,
                            // Plain `pub`: a `pub(…)` writes `)` right before the keyword.
                            is_public: at > 0 && scan.ident(at - 1) == Some("pub"),
                        });
                    }
                }
                "use" => {
                    at = read_use(&scan, at, &mut items);
                    continue;
                }
                keyword if DEFINING_KEYWORDS.contains(&keyword) => {
                    // `const fn name` defines `name`, which the `fn` that follows reads.
                    let name = scan.ident(at + 1).filter(|name| {
                        keyword != "const" || !["fn", "unsafe", "async", "extern"].contains(name)
                    });
                    items.defined.extend(name.map(str::to_string));
                }
                "macro_rules" if scan.is_punct(at + 1, b'!') => {
                    items.defined.extend(scan.ident(at + 2).map(str::to_string));
                }
                _ => {}
            },
            _ => {}
        }
        at += 1;
    }
    items
}

/// Read the top-level `use` item whose keyword is at token `at` into `items`, each leaf stamped with
/// the item's visibility, and return the index of the token after it.
fn read_use(scan: &Scan<'_>, at: usize, items: &mut ModuleItems) -> usize {
    // TODO(reshape-move-item-paths): implement — stamp each new leaf with the `pub…` tokens before
    // the keyword (`UseLeaf::visibility`); today every leaf reads as private.
    let mut cursor = at + 1;
    scan.use_tree(&mut cursor, Vec::new(), &mut items.uses);
    scan.statement_end(cursor) + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The visibility of every leaf `text`'s top-level `use` items bind, in source order.
    fn visibilities_of(text: &str) -> Vec<(String, Option<String>)> {
        items_of_module(text)
            .uses
            .into_iter()
            .map(|leaf| (leaf.segments.join("::"), leaf.visibility))
            .collect()
    }

    fn leaf(path: &str, visibility: Option<&str>) -> (String, Option<String>) {
        (path.to_string(), visibility.map(str::to_string))
    }

    #[test]
    fn a_top_level_use_records_its_visibility_on_every_leaf() {
        // Given a module with a public, a crate-wide grouped, a restricted and a private `use`
        let text = concat!(
            "pub use kernel::util;\n",
            "pub(crate) use crate::a::{b, c};\n",
            "pub(in crate::x) use crate::y::Z;\n",
            "use crate::types::Config;\n",
        );

        // When its top level is read
        let read = visibilities_of(text);

        // Then each leaf carries the visibility of the item that holds it, a private one none
        assert_eq!(
            read,
            vec![
                leaf("kernel::util", Some("pub")),
                leaf("crate::a::b", Some("pub(crate)")),
                leaf("crate::a::c", Some("pub(crate)")),
                leaf("crate::y::Z", Some("pub(in crate::x)")),
                leaf("crate::types::Config", None),
            ]
        );
    }
}
