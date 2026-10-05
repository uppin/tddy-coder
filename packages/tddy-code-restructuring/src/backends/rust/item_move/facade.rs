//! The `pub use` a move leaves behind when the callers are to keep their old path.

use std::collections::BTreeMap;

use super::scope::Scope;
use crate::plan::Reexport;

/// The lines that stand where the moved items were, so that `module::name` goes on resolving.
///
/// `qualifier` is the destination as the source module writes it (`crate::answers`) and `items` are
/// the moved names with the scope each had, so the facade is exactly as visible as the item was: a
/// `pub use` of an item that is not `pub` is `E0364`, and a facade narrower than the item would
/// strand the callers it exists for.
///
/// `glob` re-exports the whole destination, as the cross-crate move does; `named` lists only what
/// moved, one line per visibility. `outside` writes those lines for the items its caller passes,
/// which are the ones something outside the crate reaches. `none` leaves nothing.
pub(in crate::backends::rust) fn lines(
    reexport: Reexport,
    qualifier: &str,
    source: &[String],
    items: &[(String, Scope)],
) -> Vec<String> {
    match reexport {
        Reexport::None => Vec::new(),
        Reexport::Glob => {
            let widest = items
                .iter()
                .map(|(_, scope)| scope.clone())
                .reduce(|one, other| match (one, other) {
                    (Scope::Public, _) | (_, Scope::Public) => Scope::Public,
                    (Scope::Within(path), Scope::Within(other)) => {
                        Scope::Within(path).widened_to(&other)
                    }
                })
                .unwrap_or(Scope::Public);
            vec![with_visibility(
                &widest.spelled_in(source),
                &format!("use {qualifier}::*;"),
            )]
        }
        Reexport::Named | Reexport::Outside => {
            let mut by_visibility: BTreeMap<String, Vec<&str>> = BTreeMap::new();
            for (name, scope) in items {
                by_visibility
                    .entry(scope.spelled_in(source))
                    .or_default()
                    .push(name);
            }
            by_visibility
                .into_iter()
                .map(|(visibility, names)| {
                    let tree = match names.as_slice() {
                        [only] => (*only).to_string(),
                        many => format!("{{{}}}", many.join(", ")),
                    };
                    with_visibility(&visibility, &format!("use {qualifier}::{tree};"))
                })
                .collect()
        }
    }
}

fn with_visibility(visibility: &str, statement: &str) -> String {
    if visibility.is_empty() {
        statement.to_string()
    } else {
        format!("{visibility} {statement}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<(String, Scope)> {
        vec![
            ("a".to_string(), Scope::Public),
            ("b".to_string(), Scope::Within(Vec::new())),
            ("c".to_string(), Scope::Public),
        ]
    }

    #[test]
    fn leaves_nothing_when_no_facade_was_asked_for() {
        assert!(lines(Reexport::None, "crate::x", &[], &items()).is_empty());
    }

    #[test]
    fn re_exports_the_destination_as_visibly_as_the_widest_item() {
        assert_eq!(
            lines(Reexport::Glob, "crate::x", &[], &items()),
            ["pub use crate::x::*;"]
        );
    }

    #[test]
    fn names_the_moved_items_one_line_per_visibility() {
        assert_eq!(
            lines(Reexport::Named, "crate::x", &["m".to_string()], &items()),
            ["pub use crate::x::{a, c};", "pub(crate) use crate::x::b;"]
        );
    }
}
