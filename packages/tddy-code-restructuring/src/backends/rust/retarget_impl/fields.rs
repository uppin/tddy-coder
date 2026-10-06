//! The field refusal: a moved member that reads a field the new type does not declare (S4), or a
//! new type whose fields cannot be read (S5).
//!
//! Raised by `resolve` before any edit is built, so `check --deep` names it and `apply` refuses with
//! the tree untouched. A moved member that calls a method the new type lacks is not refused: the
//! compile gate names it.

use std::collections::BTreeSet;

use syn::visit::Visit;

use super::super::item_move::destination::{find_module, package_of, Lookup};
use super::super::seam_refusal;
use super::preflight::ToType;
use crate::registry::Workspace;
use crate::Result;

/// What the new type declares, as far as the engine can read it.
pub(super) enum Declaration {
    /// A plain struct: the fields it declares.
    Struct(BTreeSet<String>),
    /// An enum or a union, which has no field a moved member could read.
    NotAStruct,
    /// A macro or a type alias: the fields cannot be read.
    Opaque,
}

/// The declaration of the type `to_type` names, read from its module's text.
pub(super) fn declaration(
    workspace: &Workspace<'_>,
    anchor_file: &str,
    to_type: &str,
) -> Result<Declaration> {
    let Some(named) = ToType::parse(to_type) else {
        return Ok(Declaration::Opaque);
    };
    let package = package_of(workspace.root, anchor_file)?;
    let Lookup::Found(module) = find_module(workspace, &package, &named.module)? else {
        return Ok(Declaration::Opaque);
    };
    let text = workspace.read(&module.file)?;
    Ok(read_declaration(&text[module.scope.clone()], &named.name))
}

/// Refuse the moved members when one of them reads a field `new_type` does not declare.
///
/// `members` are `(name, text)` of each moved member. Nothing is refused when no field is read, even
/// for a declaration the engine cannot read.
pub(super) fn refuse(
    members: &[(&str, &str)],
    new_type: &str,
    declaration: &Declaration,
    file: &str,
) -> Result<()> {
    let reads: Vec<(String, String)> = members
        .iter()
        .flat_map(|(member, text)| {
            fields_read(text)
                .into_iter()
                .map(|field| ((*member).to_string(), field))
        })
        .collect();
    if reads.is_empty() {
        return Ok(());
    }

    match declaration {
        Declaration::Struct(fields) => {
            let missing: Vec<String> = reads
                .iter()
                .filter(|(_, field)| !fields.contains(field))
                .map(|(member, field)| format!("`{member}` reads `self.{field}`"))
                .collect();
            if missing.is_empty() {
                return Ok(());
            }
            let list = fields.iter().cloned().collect::<Vec<_>>().join(", ");
            Err(seam_refusal(format!(
                "{}, which `{new_type}` does not declare (its fields: {list})",
                missing.join(", ")
            )))
        }
        Declaration::NotAStruct => Err(seam_refusal(format!(
            "`{new_type}` is an enum or a union, which has no field `{}`: retarget only members \
             that read no field",
            reads[0].1
        ))),
        Declaration::Opaque => Err(seam_refusal(format!(
            "`{new_type}` is declared by a macro or as a type alias in `{file}`, so its fields \
             cannot be read: retarget only members that read no field"
        ))),
    }
}

/// The declaration the module's own text gives the type named `name`.
fn read_declaration(module_text: &str, name: &str) -> Declaration {
    let Ok(file) = syn::parse_file(module_text) else {
        return Declaration::Opaque;
    };
    for item in &file.items {
        match item {
            syn::Item::Struct(declared) if declared.ident == name => {
                return Declaration::Struct(field_names(&declared.fields));
            }
            syn::Item::Enum(declared) if declared.ident == name => return Declaration::NotAStruct,
            syn::Item::Union(declared) if declared.ident == name => return Declaration::NotAStruct,
            syn::Item::Type(declared) if declared.ident == name => return Declaration::Opaque,
            syn::Item::Macro(declared)
                if declared.ident.as_ref().is_some_and(|ident| ident == name) =>
            {
                return Declaration::Opaque;
            }
            _ => {}
        }
    }
    Declaration::Opaque
}

/// The fields of a struct as a moved member would name them: named fields by name, tuple fields by
/// index.
fn field_names(fields: &syn::Fields) -> BTreeSet<String> {
    match fields {
        syn::Fields::Named(named) => named
            .named
            .iter()
            .filter_map(|field| field.ident.as_ref().map(ToString::to_string))
            .collect(),
        syn::Fields::Unnamed(unnamed) => (0..unnamed.unnamed.len())
            .map(|index| index.to_string())
            .collect(),
        syn::Fields::Unit => BTreeSet::new(),
    }
}

/// Every field a member's text reads through `self`, or names in a `Self { … }` expression or
/// pattern.
fn fields_read(member_text: &str) -> BTreeSet<String> {
    let Ok(block) = syn::parse_str::<syn::ItemImpl>(&format!("impl T {{\n{member_text}\n}}\n")) else {
        return BTreeSet::new();
    };
    let mut reads = Reads::default();
    reads.visit_item_impl(&block);
    reads.fields
}

/// The fields a body reads, collected by a walk of its syntax.
#[derive(Default)]
struct Reads {
    fields: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for Reads {
    fn visit_expr_field(&mut self, node: &'ast syn::ExprField) {
        if let syn::Expr::Path(path) = &*node.base {
            if path.path.is_ident("self") {
                self.fields.insert(member_name(&node.member));
            }
        }
        syn::visit::visit_expr_field(self, node);
    }

    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        if node.path.is_ident("Self") {
            for field in &node.fields {
                self.fields.insert(member_name(&field.member));
            }
        }
        syn::visit::visit_expr_struct(self, node);
    }

    fn visit_pat_struct(&mut self, node: &'ast syn::PatStruct) {
        if node.path.is_ident("Self") {
            for field in &node.fields {
                self.fields.insert(member_name(&field.member));
            }
        }
        syn::visit::visit_pat_struct(self, node);
    }
}

fn member_name(member: &syn::Member) -> String {
    match member {
        syn::Member::Named(ident) => ident.to_string(),
        syn::Member::Unnamed(index) => index.index.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_fields_a_method_touches_through_self() {
        let reads = fields_read("pub fn bump(&mut self) {\n    self.count += 1;\n}\n");

        assert_eq!(reads, BTreeSet::from(["count".to_string()]));
    }

    #[test]
    fn reads_the_fields_of_a_self_struct_expression() {
        let reads = fields_read("pub fn new(n: u32) -> Self {\n    Self { n }\n}\n");

        assert_eq!(reads, BTreeSet::from(["n".to_string()]));
    }

    #[test]
    fn reads_the_fields_of_a_tuple_struct_by_index() {
        let declaration = read_declaration("pub struct Pair(u32, u32);\n", "Pair");

        assert!(matches!(
            declaration,
            Declaration::Struct(fields) if fields == BTreeSet::from(["0".to_string(), "1".to_string()])
        ));
    }

    #[test]
    fn refuses_a_field_the_struct_lacks() {
        let declaration = Declaration::Struct(BTreeSet::from(["n".to_string()]));
        let members = [("bump", "pub fn bump(&mut self) {\n    self.count += 1;\n}\n")];

        let refusal = refuse(&members, "Roster", &declaration, "src/host.rs")
            .map(|_| ())
            .map_err(|error| error.to_string());

        assert!(refusal
            .unwrap_err()
            .contains("`bump` reads `self.count`, which `Roster` does not declare (its fields: n)"));
    }
}
