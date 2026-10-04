//! Re-pointing the places that name a moved item.
//!
//! Every place comes from the server's own reference set, so a name written in a `use`, in a path in
//! a body, or bare through an import is found by what it resolves to, not by what it looks like. What
//! this reads is only the *shape* around each reported position: whether it ends a `use` item, a
//! qualified path, or stands alone.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::Path;

use super::super::early_return::masked_to_code;
use super::super::seam_refusal;
use super::destination::Module;
use super::text::{
    enclosing_modules, qualifier_start, scope_of, split_use, use_insertion, use_statements, Edit,
};
use crate::item_anchor::module_path_of;
use crate::Result;

/// One place that names a moved item: the file, the byte offset of the name, and which item.
#[derive(Debug, Clone)]
pub(super) struct Site {
    pub(super) path: String,
    pub(super) offset: usize,
    pub(super) name: String,
}

/// The path of the destination as a file writes it: inside the crate, and from outside it.
pub(super) struct Qualifiers {
    pub(super) same_crate: String,
    pub(super) extern_crate: String,
}

/// What re-pointing a file needs to know about the move.
pub(super) struct Context<'a> {
    pub(super) root: &'a Path,
    pub(super) destination: &'a Module,
    pub(super) crate_name: &'a str,
    pub(super) qualifiers: &'a Qualifiers,
    /// Whether callers are re-pointed. With a facade they are not: the old path still resolves.
    pub(super) repoint: bool,
    /// The lines that move, in the file that holds them.
    pub(super) region: (&'a str, Range<usize>),
}

/// The module a file is, below the crate root, when it is a file of the crate being moved within.
///
/// `None` for a file of another crate, or one outside `src/` (a test binary, an example), which name
/// the crate from outside.
pub(super) fn module_of_file(root: &Path, crate_name: &str, path: &str) -> Option<Vec<String>> {
    let mut full = module_path_of(root, path).ok()?;
    (full.first().map(String::as_str) == Some(crate_name)).then(|| full.split_off(1))
}

/// The edits that re-point `sites` in one file.
pub(super) fn edits_for_file(
    context: &Context<'_>,
    path: &str,
    text: &str,
    sites: &[&Site],
) -> Result<Vec<Edit>> {
    let masked = masked_to_code(text);
    let statements = use_statements(&masked);
    let base = module_of_file(context.root, context.crate_name, path);
    let qualifier = if base.is_some() {
        &context.qualifiers.same_crate
    } else {
        &context.qualifiers.extern_crate
    };
    let in_region = |offset: usize| context.region.0 == path && context.region.1.contains(&offset);
    let in_destination = |offset: usize| {
        base.as_ref().is_some_and(|base| {
            let inside = enclosing_modules(text, offset);
            base.iter()
                .chain(&inside)
                .eq(context.destination.path.iter())
        })
    };

    let mut edits = Vec::new();
    let mut covered: BTreeSet<(Vec<String>, String)> = BTreeSet::new();
    let mut handled: BTreeSet<usize> = BTreeSet::new();

    for statement in &statements {
        let inside: Vec<&Site> = sites
            .iter()
            .copied()
            .filter(|site| statement.contains(&site.offset))
            .collect();
        if inside.is_empty() {
            continue;
        }
        handled.extend(inside.iter().map(|site| site.offset));
        let drop = in_destination(statement.start);
        for site in &inside {
            covered.insert((enclosing_modules(text, statement.start), site.name.clone()));
        }
        if context.repoint || drop {
            edits.push(rewrite_statement(
                path, text, statement, &inside, qualifier, drop,
            )?);
        }
    }

    if !context.repoint {
        return Ok(edits);
    }

    let mut needed: BTreeMap<Vec<String>, BTreeSet<&str>> = BTreeMap::new();
    for site in sites.iter().filter(|site| !handled.contains(&site.offset)) {
        let start = qualifier_start(text, site.offset);
        if start < site.offset {
            let relative =
                text[start..].starts_with("self::") || text[start..].starts_with("super::");
            if !(in_region(site.offset) && relative) {
                edits.push(Edit::replace(start..site.offset, format!("{qualifier}::")));
            }
            continue;
        }
        if in_region(site.offset) || in_destination(site.offset) {
            continue;
        }
        let chain = enclosing_modules(text, site.offset);
        let imported = covered
            .iter()
            .any(|(scope, name)| *name == site.name && chain.starts_with(scope));
        if !imported {
            needed.entry(chain).or_default().insert(&site.name);
        }
    }

    for (chain, names) in needed {
        let scope = scope_of(text, &chain).unwrap_or(0..text.len());
        let (at, blank) = use_insertion(text, scope);
        let mut lines: String = names
            .iter()
            .map(|name| format!("use {qualifier}::{name};\n"))
            .collect();
        if blank {
            lines.push('\n');
        }
        edits.push(Edit::insert(at, lines));
    }
    Ok(edits)
}

/// The `use` item `statement`, written again with the names that moved pointing at their new home.
///
/// A plain `use a::b::name;` has its qualifier replaced. A name inside a group leaves the group and
/// gets a `use` of its own, which is the one thing a group cannot do — carry two qualifiers — and the
/// reason a nested group is refused rather than guessed at. `drop` is for the destination itself,
/// which has no use for an import of what it now defines.
fn rewrite_statement(
    path: &str,
    text: &str,
    statement: &Range<usize>,
    sites: &[&Site],
    qualifier: &str,
    drop: bool,
) -> Result<Edit> {
    let written = &text[statement.clone()];
    let refused = |why: &str| {
        seam_refusal(format!(
            "`{written}` in {path} {why}, so it cannot be re-pointed: write one `use` per path \
             and plan again"
        ))
    };
    let (head, tree) =
        split_use(written).ok_or_else(|| refused("has no `use` keyword this can read"))?;

    let Some(open) = tree.find('{') else {
        let (path_text, alias) = tree
            .split_once(" as ")
            .map_or((tree, String::new()), |(path_text, alias)| {
                (path_text.trim(), format!(" as {}", alias.trim()))
            });
        let name = path_text.rsplit("::").next().unwrap_or(path_text);
        if sites.len() != 1 || sites[0].name != name {
            return Err(refused(
                "names the moved item in a form this move does not read",
            ));
        }
        let replacement = if drop {
            String::new()
        } else {
            format!("{head}use {qualifier}::{name}{alias};")
        };
        return Ok(Edit::replace(statement.clone(), replacement));
    };

    let prefix = tree[..open].trim_end();
    if !prefix.ends_with("::") || !tree.ends_with('}') {
        return Err(refused("is a group this move does not read"));
    }
    let members = members_of(&tree[open + 1..tree.len() - 1]);
    let (moved, kept): (Vec<&str>, Vec<&str>) = members.iter().copied().partition(|member| {
        let name = member.split_whitespace().next().unwrap_or_default();
        !member.contains("::")
            && !member.contains('{')
            && sites.iter().any(|site| site.name == name)
    });
    if moved.len() != sites.len() {
        return Err(refused("names a moved item inside a nested group"));
    }

    let mut lines = Vec::new();
    if !kept.is_empty() {
        lines.push(format!("{head}use {prefix}{{{}}};", kept.join(", ")));
    }
    if !drop {
        lines.extend(
            moved
                .iter()
                .map(|member| format!("{head}use {qualifier}::{member};")),
        );
    }
    Ok(Edit::replace(statement.clone(), lines.join("\n")))
}

/// The members of a `use` group, split at the commas that are not inside a nested group.
fn members_of(inner: &str) -> Vec<&str> {
    let mut members = Vec::new();
    let (mut depth, mut from) = (0usize, 0usize);
    for (at, character) in inner.char_indices() {
        match character {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                members.push(inner[from..at].trim());
                from = at + 1;
            }
            _ => {}
        }
    }
    members.push(inner[from..].trim());
    members.retain(|member| !member.is_empty());
    members
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(text: &str, name: &str) -> Site {
        Site {
            path: "src/handler.rs".to_string(),
            offset: text.rfind(name).unwrap(),
            name: name.to_string(),
        }
    }

    fn rewritten(text: &str, name: &str, drop: bool) -> String {
        let statement = 0..text.find(';').unwrap() + 1;
        let site = site(text, name);
        let edit = rewrite_statement(
            "src/handler.rs",
            text,
            &statement,
            &[&site],
            "crate::answers",
            drop,
        )
        .unwrap();
        super::super::text::applied(text, &[edit]).unwrap()
    }

    #[test]
    fn re_points_the_qualifier_of_a_plain_use() {
        assert_eq!(
            rewritten("use crate::pairing::name;", "name", false),
            "use crate::answers::name;"
        );
    }

    #[test]
    fn keeps_an_alias_and_a_visibility_of_a_plain_use() {
        assert_eq!(
            rewritten(
                "pub(crate) use super::pairing::name as other;",
                "name",
                false
            ),
            "pub(crate) use crate::answers::name as other;"
        );
    }

    #[test]
    fn takes_a_moved_name_out_of_a_group_into_a_use_of_its_own() {
        assert_eq!(
            rewritten("use crate::pairing::{name, other};", "name", false),
            "use crate::pairing::{other};\nuse crate::answers::name;"
        );
    }

    #[test]
    fn drops_the_import_of_a_name_the_destination_now_defines() {
        assert_eq!(rewritten("use crate::pairing::name;", "name", true), "");
    }

    #[test]
    fn refuses_a_name_inside_a_nested_group() {
        let text = "use crate::{pairing::{name}, other};";
        let statement = 0..text.len();
        let site = site(text, "name");

        let refusal = rewrite_statement(
            "src/a.rs",
            text,
            &statement,
            &[&site],
            "crate::answers",
            false,
        );

        assert!(refusal.is_err());
    }
}
