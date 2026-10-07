//! Which files an anchor names: one file for a `symbol` anchor, every file of the module for an
//! anchor lowered from an `items` anchor on a `mod` declaration.
//!
//! A lowered module anchor is the range of the `mod` declaration. It is read back to the module it
//! declares, and the module's file is the entry point of [`crate_move::module_files::files_of`],
//! which follows inline modules and file modules alike. A range that covers no `mod` declaration
//! names no module, and is refused saying so.

use crate::apply::byte_offset;
use crate::crate_move::module_files;
use crate::plan::{Anchor, RefactorOp};
use crate::registry::Workspace;
use crate::Result;

use super::refusals;

/// The files the operation acts on, relative to the workspace root, in a stable order.
pub(super) fn files_of(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    match &op.anchor {
        Anchor::Symbol { file, .. } => Ok(vec![file.clone()]),
        Anchor::Range { file, start, end } => {
            let name = module_declared_in(workspace, file, *start, *end)?;
            let parent_dir = module_files::children_directory(file);
            let (module_file, _) = module_files::file_of_child(workspace, &parent_dir, &name)
                .ok_or_else(|| refusals::no_module_in(file, start.line))?;
            module_files::files_of(workspace, &module_file)
        }
        other => Err(refusals::no_module_in(other.file(), 1)),
    }
}

/// The name of the module the `mod` declaration covered by the range declares, or a refusal.
fn module_declared_in(
    workspace: &Workspace<'_>,
    file: &str,
    start: crate::Position,
    end: crate::Position,
) -> Result<String> {
    let text = workspace.read(file)?;
    let from = byte_offset(&text, start.line, start.col)?;
    let to = byte_offset(&text, end.line, end.col)?.max(from);
    declared_module_name(&text[from..to]).ok_or_else(|| refusals::no_module_in(file, start.line))
}

/// The identifier a `[pub ]mod <name>` in `text` declares, when `text` holds one.
///
/// `text` is the span of the declaration, so the first `mod` keyword at a word boundary that is
/// followed by a name is the one the anchor named.
fn declared_module_name(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    for (at, _) in text.match_indices("mod") {
        if at > 0 && is_identifier_byte(bytes[at - 1]) {
            continue;
        }
        let after = text[at + "mod".len()..].trim_start();
        let name: String = after
            .chars()
            .take_while(|character| character.is_alphanumeric() || *character == '_')
            .collect();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_name_of_a_plain_or_public_module_declaration() {
        assert_eq!(declared_module_name("mod a;").as_deref(), Some("a"));
        assert_eq!(
            declared_module_name("pub mod nested;").as_deref(),
            Some("nested")
        );
    }

    #[test]
    fn a_span_holding_no_mod_declaration_names_nothing() {
        assert_eq!(declared_module_name("use crate::config::Settings;\n"), None);
        assert_eq!(declared_module_name("command;"), None);
    }
}
