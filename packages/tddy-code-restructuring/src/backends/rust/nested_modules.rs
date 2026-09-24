//! The assist's rewrite of a reference inside a module the file already had, and its repair.
//!
//! Split out of `backends/rust.rs`, which is past its size budget.

use super::early_return::masked_to_code;
use super::imports::names_bound;
use super::{is_identifier_char, MovedItem};

/// The assist's output with its rewrite of a call a nested module already reaches undone.
///
/// `extract_module` rewrites every reference to an item it moves as `modname::item`, a path
/// relative to the file's own module. Inside a module the file already had, such as its
/// `mod withdrawal_contract_tests`, that path names nothing: `modname` is a child of the file's
/// module, not of the nested one. The assist does repoint that module's own import
/// (`use super::item;` becomes `use super::modname::item;`), and then rewrites the call beside it
/// as well, so `item()` comes back as `modname::item()`. The rename of `modname` cannot reach an
/// unresolved path, and the call was right as it stood: the nested module's import already brings
/// the item in.
///
/// Undone only where the placeholder starts the path, the reference sits inside a module other
/// than the placeholder's, and that module's own `use` declarations bind the moved item's name.
/// The assist rewrites only references to what it moved, so such a binding is the one the
/// reference resolved through. A nested module that reaches the item through `use super::*;`
/// gets `modname` from the same glob, so its rewritten call resolves and the rename finishes it.
///
/// The module blocks are read by brace depth over the code alone — comments and literals masked
/// by [`masked_to_code`], so a `{` in a string or a `//` inside `"http://…"` does not move the
/// depth. The read is still lexical, not a parse, and the compile gate on `apply` is what catches a
/// repair that followed a misread.
pub(super) fn with_nested_references_restored(
    text: &str,
    placeholder: &str,
    moved: &[MovedItem],
) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    // Byte for byte and newline for newline, so a line index into one is a line index into the
    // other.
    let code = masked_to_code(text);
    let code_lines: Vec<&str> = code.split('\n').collect();
    let blocks = module_blocks(&code_lines);

    let restored: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let Some(block) = innermost_block(&blocks, index) else {
                return (*line).to_string();
            };
            if block.name == placeholder {
                return (*line).to_string();
            }
            let bound = names_bound(&lines[block.opened + 1..block.closed].join("\n"));
            let names: Vec<&str> = moved
                .iter()
                .map(|item| item.name.as_str())
                .filter(|name| bound.iter().any(|binding| binding == name))
                .collect();
            without_leading_placeholder(line, placeholder, &names)
        })
        .collect();

    restored.join("\n")
}

/// An inline `mod name { … }` block, by the zero-based lines of its header and closing brace.
struct ModuleBlock<'a> {
    name: &'a str,
    opened: usize,
    closed: usize,
}

/// Every inline module block in the text, read by brace depth. `lines` are code only, with
/// comments and literals already masked.
fn module_blocks<'a>(lines: &[&'a str]) -> Vec<ModuleBlock<'a>> {
    let mut blocks = Vec::new();
    let mut stack: Vec<Option<(&'a str, usize)>> = Vec::new();

    for (index, code) in lines.iter().copied().enumerate() {
        // Where the declaration head of the next `{` starts: after the last delimiter read.
        let mut declaration = 0usize;
        let delimiters = code
            .char_indices()
            .filter(|(_, character)| matches!(character, '{' | '}' | ';'));

        for (at, character) in delimiters {
            match character {
                '{' => stack.push(module_named(&code[declaration..at]).map(|name| (name, index))),
                '}' => blocks.extend(stack.pop().flatten().map(|(name, opened)| ModuleBlock {
                    name,
                    opened,
                    closed: index,
                })),
                _ => {}
            }
            declaration = at + 1;
        }
    }

    blocks
}

/// The name a declaration head opens a module under — `mod tests`, `pub(crate) mod tests`.
fn module_named(head: &str) -> Option<&str> {
    let mut tokens = head
        .split(|character: char| !is_identifier_char(character))
        .filter(|token| !token.is_empty());
    tokens.find(|token| *token == "mod")?;
    tokens.next()
}

/// The innermost module block a zero-based line sits inside, between its header and closing brace.
fn innermost_block<'b, 'a>(
    blocks: &'b [ModuleBlock<'a>],
    line: usize,
) -> Option<&'b ModuleBlock<'a>> {
    blocks
        .iter()
        .filter(|block| block.opened < line && line < block.closed)
        .max_by_key(|block| block.opened)
}

/// The line with `placeholder::` removed before each of `names`, wherever the placeholder starts
/// the path: not after `::`, a `.` or another identifier.
fn without_leading_placeholder(line: &str, placeholder: &str, names: &[&str]) -> String {
    let mut restored = line.to_string();

    for name in names {
        let rewritten = format!("{placeholder}::{name}");
        let mut from = 0;
        while let Some(found) = restored[from..]
            .find(&rewritten)
            .map(|offset| from + offset)
        {
            let end = found + rewritten.len();
            let before = &restored[..found];
            let leads = !before.ends_with("::")
                && !before.ends_with('.')
                && !before.ends_with(is_identifier_char);
            let whole = !restored[end..].starts_with(is_identifier_char);
            if leads && whole {
                restored.replace_range(found..found + placeholder.len() + "::".len(), "");
            }
            from = found + 1;
        }
    }

    restored
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A free function the seam moves.
    fn a_moved_function(name: &str) -> MovedItem {
        MovedItem {
            name: name.to_string(),
            within: Vec::new(),
            visibility: "pub".to_string(),
            stranded_in: Vec::new(),
            reached_from_outside: true,
            referenced_in_impl_at: Vec::new(),
        }
    }

    /// The file after the assist, where `mod tests` imports `base` by name.
    const IMPORTED_BY_NAME: &str = "\
pub fn level() -> u32 {
    modname::base() + 1
}

mod modname {
    pub fn base() -> u32 {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::modname::base;

    #[test]
    fn reads_the_base() {
        let read = modname::base();
        assert_eq!(read, 2);
    }
}
";

    #[test]
    fn puts_back_a_call_the_nested_module_s_own_import_reaches() {
        // When
        let restored = with_nested_references_restored(
            IMPORTED_BY_NAME,
            "modname",
            &[a_moved_function("base")],
        );

        // Then
        assert_eq!(
            restored,
            IMPORTED_BY_NAME.replace("let read = modname::base();", "let read = base();")
        );
    }

    #[test]
    fn leaves_a_reference_in_the_file_s_own_module_alone() {
        // Given
        let produced = "pub fn level() -> u32 {\n    modname::base() + 1\n}\n";

        // When
        let restored =
            with_nested_references_restored(produced, "modname", &[a_moved_function("base")]);

        // Then
        assert_eq!(restored, produced);
    }

    /// `use super::*;` brings `modname` in with everything else, so the rewritten call resolves.
    #[test]
    fn leaves_a_call_in_a_nested_module_that_reaches_through_a_glob_alone() {
        // Given
        let produced = "mod tests {\n    use super::*;\n\n    fn read() -> u32 {\n        \
                        modname::base()\n    }\n}\n";

        // When
        let restored =
            with_nested_references_restored(produced, "modname", &[a_moved_function("base")]);

        // Then
        assert_eq!(restored, produced);
    }

    #[test]
    fn leaves_a_path_through_super_alone() {
        // Given
        let produced =
            "mod tests {\n    use super::modname::base;\n\n    fn read() -> u32 {\n        \
                        super::modname::base()\n    }\n}\n";

        // When
        let restored =
            with_nested_references_restored(produced, "modname", &[a_moved_function("base")]);

        // Then
        assert_eq!(restored, produced);
    }

    #[test]
    fn leaves_a_name_the_nested_module_does_not_bind_alone() {
        // Given
        let produced =
            "mod tests {\n    use super::modname::base;\n\n    fn read() -> u32 {\n        \
                        modname::other()\n    }\n}\n";

        // When
        let restored = with_nested_references_restored(
            produced,
            "modname",
            &[a_moved_function("base"), a_moved_function("other")],
        );

        // Then
        assert_eq!(restored, produced);
    }

    #[test]
    fn puts_back_a_call_in_a_module_nested_two_deep() {
        // Given
        let produced =
            "mod tests {\n    mod inner {\n        use super::super::modname::base;\n\n        \
                        fn read() -> u32 {\n            modname::base()\n        }\n    }\n}\n";

        // When
        let restored =
            with_nested_references_restored(produced, "modname", &[a_moved_function("base")]);

        // Then
        assert_eq!(
            restored,
            produced.replace("            modname::base()", "            base()")
        );
    }
}
