//! A move that creates the module it moves into.
//!
//! A `move_item` line that carries `name` makes `to` the **parent** and declares a new, empty module
//! called `name` in it before the items arrive. It is explicit on purpose: a mistyped `to` alone
//! still refuses as a module that does not exist, so a typo cannot silently grow a file. What a
//! lexical read can say about it — the parent is there, the name is free — is answered here, so
//! `check` reports it without a server.

use std::ops::Range;

use super::super::is_identifier;
use super::super::module_reparent::relocation::directory_of;
use super::destination::Module;
use super::preflight::{names_declared_in, Named};
use crate::registry::Workspace;
use crate::Result;

/// The module a move lands in: one that exists, or one the move creates.
pub(in crate::backends::rust) struct Destination {
    /// Where the items land. For a created module its file is not on disk yet and its scope is empty.
    pub(in crate::backends::rust) module: Module,
    /// The parent that gains the declaration, when the move creates the module.
    pub(in crate::backends::rust) parent: Option<Module>,
}

impl Destination {
    pub(super) fn existing(module: Module) -> Destination {
        Destination {
            module,
            parent: None,
        }
    }
}

/// The findings for creating `name` in `parent`, and the destination it would be.
pub(super) fn within(
    workspace: &Workspace<'_>,
    named: &Named,
    name: &str,
    parent: Module,
) -> Result<(Vec<String>, Option<Destination>)> {
    let mut findings = Vec::new();
    if !is_identifier(name) {
        findings.push(format!("`{name}` is not a module name"));
    }
    let text = workspace.read(&parent.file)?;
    if names_declared_in(&text[parent.scope.clone()]).contains(name) {
        findings.push(format!(
            "`{name}` is already declared in `{}`, so a new module of that name would be `E0428`",
            named.to
        ));
    }
    let directory = directory_of(workspace.root, &parent)?;
    let module = Module {
        file: directory
            .join(format!("{name}.rs"))
            .to_string_lossy()
            .to_string(),
        scope: 0..0,
        path: parent
            .path
            .iter()
            .cloned()
            .chain([name.to_string()])
            .collect(),
    };
    Ok((
        findings,
        Some(Destination {
            module,
            parent: Some(parent),
        }),
    ))
}

/// The parent's lines that declare the new module: where they go, and what to write.
pub(super) fn declaration(
    parent_text: &str,
    scope: &Range<usize>,
    visibility: &str,
    name: &str,
) -> (usize, String) {
    let spelled = if visibility.is_empty() {
        String::new()
    } else {
        format!("{visibility} ")
    };
    super::super::module_reparent::declaration::insertion(
        parent_text,
        scope,
        &format!("{spelled}mod {name};\n"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declares_the_module_below_the_last_declaration() {
        let text = "mod a;\n\nfn f() {}\n";

        let (at, written) = declaration(text, &(0..text.len()), "pub(crate)", "b");

        assert_eq!(
            crate::backends::rust::item_move::text::applied(
                text,
                &[crate::backends::rust::item_move::text::Edit::insert(
                    at, written
                )]
            )
            .unwrap(),
            "mod a;\npub(crate) mod b;\n\nfn f() {}\n"
        );
    }
}
