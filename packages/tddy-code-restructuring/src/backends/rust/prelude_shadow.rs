//! The lexical half of the import restoration behind `extract_module`: names the parent binds by
//! `use` that the moved code goes on using.
//!
//! The server-driven pass in [`super::imports`] only reacts to names rust-analyzer reports
//! *unresolved*. A name the parent rebinds over the prelude — `Result` as the crate's one-generic
//! alias, say — is never reported: in the new module the prelude's two-generic `std::result::Result`
//! resolves it, so the child silently means a different type and the compiler is the first to say so.
//! This pass reads the parent's `use` declarations and the moved text, and carries across every
//! binding the moved code names and the module does not already have. It over-imports on purpose;
//! the tidy step removes what the compiler reports unused.

use super::early_return::masked_to_code;
use super::imports::{names_bound, rebased_for_child};
use super::{group_members, use_tree, with_module_import};
use crate::Result;

/// Keywords after which an identifier is a declaration of that name.
const DECLARING: [&str; 9] = [
    "fn", "struct", "enum", "type", "const", "static", "trait", "mod", "union",
];

/// What one explicit `use` leaf binds: the name, the path as the parent wrote it, and the alias.
struct Binding {
    name: String,
    path: String,
    alias: Option<String>,
}

/// `text` with the parent's bindings the moved module uses written into it, once each.
///
/// `original` is the file before the assist ran, because the assist drops a name from a `use` group
/// when the seam held its only use. Behaviour for names the server reported unresolved is the other
/// pass's: whatever it restored is bound by the module and left alone here.
pub(super) fn carry_shadowed_imports(original: &str, text: &str, module: &str) -> Result<String> {
    let block = super::imports::module_block(text, module)?;
    let lines: Vec<&str> = text.split('\n').collect();
    let child = lines[block.opened + 1..block.closed].join("\n");

    let mut carried = text.to_string();
    for line in shadowed_imports(original, &child).iter().rev() {
        carried = with_module_import(&carried, module, line)?;
    }
    Ok(carried)
}

/// The `use` lines the child needs: parent bindings it names and does not bind itself.
pub(super) fn shadowed_imports(parent: &str, child: &str) -> Vec<String> {
    let code = masked_to_code(child);
    let used = identifiers(&code, false);
    let mut unavailable = names_bound(child);
    unavailable.extend(identifiers(&code, true));

    let mut lines = Vec::new();
    for binding in parent_bindings(parent) {
        let wanted = used.contains(&binding.name) && !unavailable.contains(&binding.name);
        if wanted && !lines.contains(&use_line(&binding)) {
            lines.push(use_line(&binding));
        }
    }
    lines
}

/// `use <path>;`, rebased for the module one level deeper, with the alias if the parent had one.
fn use_line(binding: &Binding) -> String {
    let path = rebased_for_child(&binding.path);
    match &binding.alias {
        Some(alias) => format!("use {path} as {alias};"),
        None => format!("use {path};"),
    }
}

/// Identifiers in masked code that stand alone — not reached through a `::` or `.` qualifier.
///
/// With `declared`, only the ones a declaring keyword introduces.
fn identifiers(code: &str, declared: bool) -> Vec<String> {
    let bytes = code.as_bytes();
    let mut found = Vec::new();
    let mut previous_word = "";
    let mut at = 0;

    while at < bytes.len() {
        if !is_word(bytes[at]) {
            at += 1;
            continue;
        }
        let end = word_end(bytes, at);
        let word = &code[at..end];
        let standalone = !qualified(bytes, at);
        if standalone && declared == DECLARING.contains(&previous_word) {
            found.push(word.to_string());
        }
        previous_word = word;
        at = end;
    }
    found
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn word_end(bytes: &[u8], from: usize) -> usize {
    bytes[from..]
        .iter()
        .position(|byte| !is_word(*byte))
        .map_or(bytes.len(), |length| from + length)
}

/// Whether the word at `at` follows a `::` or a `.`, so it names a member and not a binding.
fn qualified(bytes: &[u8], at: usize) -> bool {
    let before = code_before(bytes, at);
    before.ends_with(b"::") || before.ends_with(b".") && !before.ends_with(b"..")
}

/// The bytes before `at`, trailing whitespace trimmed.
fn code_before(bytes: &[u8], at: usize) -> &[u8] {
    let mut end = at;
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &bytes[..end]
}

/// Every explicit binding the parent's top-level `use` declarations make, in file order.
fn parent_bindings(parent: &str) -> Vec<Binding> {
    let mut bindings = Vec::new();

    for statement in parent.split(';') {
        let flat = without_restriction(statement);
        if let Some(tree) = use_tree(&flat).filter(|_| starts_a_line(&flat)) {
            collect(tree, "", &mut bindings);
        }
    }
    bindings
}

/// The statement with a `pub(crate) use` read as `use`, which is all a binding's name depends on.
fn without_restriction(statement: &str) -> String {
    statement
        .split_inclusive('\n')
        .map(|line| match line.strip_prefix("pub(") {
            Some(rest) => rest
                .split_once(") use ")
                .map_or(line.to_string(), |(_, tree)| format!("use {tree}")),
            None => line.to_string(),
        })
        .collect()
}

/// Whether the statement's `use` begins in column zero: a function-local one is not the file's.
fn starts_a_line(statement: &str) -> bool {
    statement
        .lines()
        .rfind(|line| line.trim_start().starts_with("use ") || line.starts_with("pub use "))
        .is_some_and(|line| !line.starts_with(char::is_whitespace))
}

/// Walk a `use` tree, recording what each leaf binds. Globs and `_` bind nothing readable.
fn collect(tree: &str, prefix: &str, bindings: &mut Vec<Binding>) {
    let tree = tree.trim();

    let Some(open) = tree.find('{') else {
        bindings.extend(leaf(tree, prefix));
        return;
    };

    let head = format!("{prefix}{}", &tree[..open]);
    let close = tree.rfind('}').unwrap_or(tree.len());
    for member in group_members(&tree[open + 1..close]) {
        collect(member, &head, bindings);
    }
}

fn leaf(tree: &str, prefix: &str) -> Option<Binding> {
    let (written, alias) = match tree.split_once(" as ") {
        Some((path, alias)) => (path.trim(), Some(alias.trim().to_string())),
        None => (tree, None),
    };
    let path = match written {
        "self" => prefix.trim_end_matches("::").to_string(),
        _ => format!("{prefix}{written}"),
    };
    let name = alias
        .clone()
        .unwrap_or_else(|| path.rsplit("::").next().unwrap_or("").to_string());

    let readable = !name.is_empty() && name != "_" && !name.ends_with('*');
    readable.then_some(Binding { name, path, alias })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARENT: &str = "use crate::{Overlay, Result};\n\nfn parent() {}\n";

    #[test]
    fn carries_an_alias_the_parent_binds_and_the_moved_code_names() {
        // When
        let imports = shadowed_imports(PARENT, "fn run() -> Result<()> {\n    Ok(())\n}");

        // Then
        assert_eq!(imports, vec!["use crate::Result;"]);
    }

    #[test]
    fn leaves_a_bound_name_the_moved_code_only_quotes_in_a_string_or_a_comment() {
        // Given
        let child = "// Result of the run\nfn say() -> &'static str {\n    \"Result\"\n}";

        // When
        let imports = shadowed_imports(PARENT, child);

        // Then
        assert_eq!(imports, Vec::<String>::new());
    }

    #[test]
    fn leaves_a_name_the_moved_code_reaches_through_a_qualifier() {
        // When
        let imports = shadowed_imports(PARENT, "fn run() -> std::fmt::Result {\n    todo!()\n}");

        // Then
        assert_eq!(imports, Vec::<String>::new());
    }

    #[test]
    fn leaves_a_name_the_child_already_binds() {
        // Given
        let child = "use std::io::Result;\n\nfn run() -> Result<()> {\n    Ok(())\n}";

        // When
        let imports = shadowed_imports(PARENT, child);

        // Then
        assert_eq!(imports, Vec::<String>::new());
    }

    #[test]
    fn leaves_a_name_the_moved_code_declares_itself() {
        // Given
        let child = "pub type Result<T> = std::result::Result<T, String>;\n\nfn run() -> Result<()> {\n    Ok(())\n}";

        // When
        let imports = shadowed_imports(PARENT, child);

        // Then
        assert_eq!(imports, Vec::<String>::new());
    }

    #[test]
    fn carries_the_path_of_an_aliased_member_of_a_nested_group() {
        // Given
        let parent = "use a::{b::C as D, e};\n";

        // When
        let imports = shadowed_imports(parent, "fn f(value: D) {}");

        // Then
        assert_eq!(imports, vec!["use a::b::C as D;"]);
    }

    #[test]
    fn rebases_a_relative_path_for_the_deeper_module() {
        // Given
        let parent = "use super::Failure;\nuse self::proto::Event;\n";

        // When
        let imports = shadowed_imports(parent, "fn f(a: Failure, b: Event) {}");

        // Then
        assert_eq!(
            imports,
            vec!["use super::super::Failure;", "use super::proto::Event;"]
        );
    }

    #[test]
    fn carries_a_module_bound_through_self_in_a_group() {
        // When
        let imports = shadowed_imports("use a::b::{self, C};\n", "fn f() { b::run(); }");

        // Then
        assert_eq!(imports, vec!["use a::b;"]);
    }

    #[test]
    fn ignores_a_use_written_inside_a_function() {
        // Given
        let parent = "fn parent() {\n    use crate::Result;\n}\n";

        // When
        let imports = shadowed_imports(parent, "fn run() -> Result<()> { Ok(()) }");

        // Then
        assert_eq!(imports, Vec::<String>::new());
    }

    #[test]
    fn gives_nothing_for_a_prelude_name_the_parent_never_imported() {
        // When
        let imports = shadowed_imports(PARENT, "fn pick() -> Option<u32> {\n    Some(1)\n}");

        // Then
        assert_eq!(imports, Vec::<String>::new());
    }
}
