//! The two findings a `retarget_impl` makes from the plan and the text alone, before any server.
//!
//! Both read files and manifests the tree already holds: the package the anchor's file belongs to
//! and the package `to_type` names (P7), and the module `to_type` names and whether it declares the
//! type (P8). Answering them here is what lets a plain `check` report a retarget that cannot be
//! honoured without paying for an index — and what keeps a sound plan's static findings empty, so a
//! `check --deep` still rehearses it.

use super::super::item_move::destination::{find_module, package_of, Lookup};
use crate::plan::RefactorOp;
use crate::registry::Workspace;
use crate::Result;

/// Everything a static check finds wrong with a `retarget_impl`, without a server.
pub(super) fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    let Some(to_type) = op.to_type.as_deref() else {
        // The codec refuses a `retarget_impl` without a `to_type` before this runs; a caller that
        // reaches here has bypassed it, and there is nothing to say about a type that is absent.
        return Ok(Vec::new());
    };
    let Some(named) = ToType::parse(to_type) else {
        // Not one path type: the codec refused it already.
        return Ok(Vec::new());
    };

    let package = package_of(workspace.root, op.anchor.file())?;
    if named.package != package.crate_name {
        return Ok(vec![format!(
            "`to_type` is in package `{}`, and the anchor is in `{}`: `retarget_impl` does not \
             leave its crate",
            named.package, package.crate_name
        )]);
    }

    match find_module(workspace, &package, &named.module)? {
        Lookup::Missing { .. } => Ok(vec![format!(
            "the package has no module `{}`: `to_type` names `{to_type}`, which no module path \
             reaches",
            named.module_path()
        )]),
        Lookup::Found(found) => {
            let text = workspace.read(&found.file)?;
            if declares_the_type(&text[found.scope.clone()], &named.name) {
                Ok(Vec::new())
            } else {
                Ok(vec![format!(
                    "`{}` declares no struct, enum or union named `{}`",
                    named.module_path(),
                    named.name
                )])
            }
        }
    }
}

/// A `to_type` read apart: the package it is rooted at, the module path below it, and the type.
pub(super) struct ToType {
    pub(super) package: String,
    pub(super) module: Vec<String>,
    pub(super) name: String,
}

impl ToType {
    pub(super) fn parse(text: &str) -> Option<ToType> {
        let segments: Vec<&str> = text.split("::").map(str::trim).collect();
        let (package, rest) = segments.split_first()?;
        let (name, module) = rest.split_last()?;
        // Generics are written on the last segment only; the name is its bare identifier.
        let name = name.split('<').next()?.trim();
        Some(ToType {
            package: package.replace('-', "_"),
            module: module.iter().map(|segment| segment.to_string()).collect(),
            name: name.to_string(),
        })
    }

    /// The path `to_type` names without its last segment, as a reader writes it.
    pub(super) fn module_path(&self) -> String {
        std::iter::once(self.package.as_str())
            .chain(self.module.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("::")
    }
}

/// Whether the module's own text declares a struct, enum or union named `name`.
fn declares_the_type(module_text: &str, name: &str) -> bool {
    syn::parse_file(module_text).is_ok_and(|file| {
        file.items.iter().any(|item| match item {
            syn::Item::Struct(declared) => declared.ident == name,
            syn::Item::Enum(declared) => declared.ident == name,
            syn::Item::Union(declared) => declared.ident == name,
            _ => false,
        })
    })
}
