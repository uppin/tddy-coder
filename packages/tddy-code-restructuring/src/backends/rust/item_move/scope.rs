//! Where an item is visible, reduced to the one thing a move can change: a module and every module
//! below it.
//!
//! A visibility keyword is a *spelling*: `pub(super)` means "the parent of the module this is
//! written in", so the same text means something else once the item is written in another module.
//! Reading each keyword as the module subtree it covers is what lets a moved item keep its meaning,
//! lets the move widen it by exactly as much as a caller needs, and lets the result be spelled again
//! for the module it lands in.

/// A visibility, as the module subtree it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Scope {
    Public,
    /// The module at this path below the crate root, and every module under it.
    Within(Vec<String>),
}

impl Scope {
    /// `visibility`, as written in the module at `module` (both below the crate root).
    ///
    /// `None` for a spelling that is not one of the forms rustc accepts, or that climbs above the
    /// crate root — the caller reports it rather than guessing what it meant.
    pub(super) fn parse(visibility: &str, module: &[String]) -> Option<Scope> {
        let text = visibility.trim();
        if text.is_empty() || text == "pub(self)" {
            return Some(Scope::Within(module.to_vec()));
        }
        if text == "pub" {
            return Some(Scope::Public);
        }
        let inner = text.strip_prefix("pub(")?.strip_suffix(')')?.trim();
        let path = inner.strip_prefix("in ").map_or(inner, str::trim);

        let mut parts = path.split("::").map(str::trim).peekable();
        let mut segments: Vec<String> = match parts.peek() {
            Some(&"crate") => {
                parts.next();
                Vec::new()
            }
            Some(&("self" | "super")) => module.to_vec(),
            _ => return None,
        };
        for part in parts {
            match part {
                "self" => {}
                "super" => {
                    segments.pop()?;
                }
                name => segments.push(name.to_string()),
            }
        }
        Some(Scope::Within(segments))
    }

    /// The narrowest scope that covers this one and `module`.
    pub(super) fn widened_to(self, module: &[String]) -> Scope {
        match self {
            Scope::Public => Scope::Public,
            Scope::Within(path) => {
                let shared = path
                    .iter()
                    .zip(module)
                    .take_while(|(own, other)| own == other)
                    .count();
                Scope::Within(path[..shared].to_vec())
            }
        }
    }

    /// The keyword that gives an item written in `at` this scope. Empty for private.
    pub(super) fn spelled_in(&self, at: &[String]) -> String {
        match self {
            Scope::Public => "pub".to_string(),
            Scope::Within(path) if path == at => String::new(),
            Scope::Within(path) if path.is_empty() => "pub(crate)".to_string(),
            Scope::Within(path) if at.split_last().is_some_and(|(_, parent)| parent == path) => {
                "pub(super)".to_string()
            }
            Scope::Within(path) => format!("pub(in crate::{})", path.join("::")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(path: &str) -> Vec<String> {
        path.split("::")
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn reads_a_private_item_as_visible_in_its_own_module() {
        assert_eq!(
            Scope::parse("", &module("a::b")),
            Some(Scope::Within(module("a::b")))
        );
    }

    #[test]
    fn reads_pub_super_as_the_parent_of_the_module_it_is_written_in() {
        assert_eq!(
            Scope::parse("pub(super)", &module("a::b")),
            Some(Scope::Within(module("a")))
        );
    }

    #[test]
    fn reads_pub_in_path_through_its_relative_forms() {
        assert_eq!(
            Scope::parse("pub(in super::super::x)", &module("a::b")),
            Some(Scope::Within(module("x")))
        );
    }

    #[test]
    fn refuses_a_visibility_that_climbs_above_the_crate_root() {
        assert_eq!(Scope::parse("pub(super)", &module("")), None);
    }

    #[test]
    fn widens_a_private_item_to_the_module_that_holds_it_and_its_new_caller() {
        let widened = Scope::Within(module("pairing")).widened_to(&module("answers"));

        assert_eq!(widened, Scope::Within(module("")));
        assert_eq!(widened.spelled_in(&module("answers")), "pub(crate)");
    }

    #[test]
    fn never_narrows_a_public_item() {
        assert_eq!(Scope::Public.widened_to(&module("answers")), Scope::Public);
    }

    #[test]
    fn spells_the_parent_as_pub_super_below_the_crate_root() {
        assert_eq!(
            Scope::Within(module("a")).spelled_in(&module("a::b")),
            "pub(super)"
        );
    }

    #[test]
    fn spells_a_distant_ancestor_as_pub_in() {
        assert_eq!(
            Scope::Within(module("a")).spelled_in(&module("a::b::c")),
            "pub(in crate::a)"
        );
    }

    #[test]
    fn spells_the_module_itself_as_private() {
        assert_eq!(Scope::Within(module("a")).spelled_in(&module("a")), "");
    }
}
