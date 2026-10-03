use super::super::group_members;

use super::super::use_tree;

/// Every name the text's `use` declarations bind, read the way the compiler reads a binding.
///
/// Not the last segment of each path, which is what [`super::imported_paths`] gives: `use a::B as
/// C;` binds `C` and not `B`, `use a::Trait as _;` binds nothing, and `use a::b::{self};` binds `b`.
/// Read as paths, the alias branch of the import pass never saw the line it had just written.
pub(in super::super) fn names_bound(text: &str) -> Vec<String> {
    let mut names = Vec::new();

    for statement in text.split(';') {
        if let Some(tree) = use_tree(statement) {
            collect_bound(tree, "", &mut names);
        }
    }

    names
}

/// Walk a `use` tree, pushing the name each leaf binds.
fn collect_bound(tree: &str, prefix: &str, names: &mut Vec<String>) {
    let tree = tree.trim();

    let Some(open) = tree.find('{') else {
        let (path, alias) = match tree.split_once(" as ") {
            Some((path, alias)) => (path.trim(), Some(alias.trim())),
            None => (tree, None),
        };
        let bound = match alias {
            Some(alias) => alias,
            None if path == "self" => prefix
                .trim_end_matches("::")
                .rsplit("::")
                .next()
                .unwrap_or(""),
            None => path.rsplit("::").next().unwrap_or(""),
        };
        // `_` binds no name, and a glob binds none this can read.
        if !bound.is_empty() && bound != "_" && bound != "*" {
            names.push(bound.to_string());
        }
        return;
    };

    let head = format!("{prefix}{}", &tree[..open]);
    let close = tree.rfind('}').unwrap_or(tree.len());

    for member in group_members(&tree[open + 1..close]) {
        collect_bound(member, &head, names);
    }
}
