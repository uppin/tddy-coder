//! `reexport: outside`: which callers lie outside the crate that holds the moved code.
//!
//! The one decision both `move_item` and `reparent_module` take, so they share it. A site is outside
//! when the package that owns its file is not the package that owns the moved code — read from the
//! manifests, the way [`package_of`] reads every package question here, and not guessed from a
//! visibility keyword. The sites inside are re-pointed like `none` re-points them; the names the
//! outside sites reach are the only ones a facade is left for; the outside sites themselves are never
//! edited.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::destination::{package_of, Package};
use super::scope::Scope;
use super::sites::Site;
use crate::plan::Reexport;
use crate::Result;

/// The callers of a move, split for its `reexport`.
pub(in crate::backends::rust) struct Reach {
    /// The sites the move re-points or measures: all of them, except that under `outside` the ones in
    /// another package are left out, because they are not edited.
    pub(in crate::backends::rust) sites: Vec<Site>,
    /// The moved names something in another package reaches. Empty unless the move is `outside`.
    pub(in crate::backends::rust) outside: BTreeSet<String>,
}

impl Reach {
    /// Split `sites` by the package that owns each file, when the move is `outside`; otherwise leave
    /// them whole.
    pub(in crate::backends::rust) fn of(
        reexport: Reexport,
        root: &Path,
        package: &Package,
        sites: Vec<Site>,
    ) -> Result<Reach> {
        if reexport != Reexport::Outside {
            return Ok(Reach {
                sites,
                outside: BTreeSet::new(),
            });
        }
        let mut owners: BTreeMap<String, bool> = BTreeMap::new();
        let mut inside = Vec::new();
        let mut outside = BTreeSet::new();
        for site in sites {
            if !owners.contains_key(&site.path) {
                let owned = package_of(root, &site.path)?.dir == package.dir;
                owners.insert(site.path.clone(), owned);
            }
            if owners[&site.path] {
                inside.push(site);
            } else {
                outside.insert(site.name);
            }
        }
        Ok(Reach {
            sites: inside,
            outside,
        })
    }
}

/// The items a facade is left for: every one under `glob` and `named`, and under `outside` only the
/// ones something outside the crate reaches.
pub(in crate::backends::rust) fn facade_items(
    reexport: Reexport,
    items: &[(String, Scope)],
    outside: &BTreeSet<String>,
) -> Vec<(String, Scope)> {
    items
        .iter()
        .filter(|(name, _)| reexport != Reexport::Outside || outside.contains(name))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<(String, Scope)> {
        ["a", "b"]
            .iter()
            .map(|name| (name.to_string(), Scope::Public))
            .collect()
    }

    #[test]
    fn keeps_every_item_unless_the_move_is_outside() {
        assert_eq!(
            facade_items(Reexport::Named, &items(), &BTreeSet::new()).len(),
            2
        );
    }

    #[test]
    fn keeps_only_the_items_something_outside_reaches() {
        let reached = BTreeSet::from(["b".to_string()]);
        let kept = facade_items(Reexport::Outside, &items(), &reached);
        assert_eq!(
            kept.iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["b"]
        );
    }

    #[test]
    fn keeps_nothing_when_nothing_outside_reaches() {
        assert!(facade_items(Reexport::Outside, &items(), &BTreeSet::new()).is_empty());
    }
}
