//! Every declaration of a file, read off rust-analyzer's `documentSymbol` tree, for the widening a
//! cross-crate move needs.
//!
//! `path_reached_within` descends only through modules, because only those change a path. A move
//! across a crate boundary also has to widen the fields and inherent members the origin still names,
//! so this walk descends through every container and records each one in `within`, deciding from
//! the container whether a visibility can be written on what it holds.

use serde_json::Value;

use super::{is_identifier, SYMBOL_KIND_MODULE};
use crate::crate_move::DeclarationKind;

/// The server's symbol kinds that open a container of declarations a visibility can be written on
/// or not: a struct, an enum and a trait.
const SYMBOL_KIND_ENUM: u64 = 10;
const SYMBOL_KIND_TRAIT: u64 = 11;
const SYMBOL_KIND_STRUCT: u64 = 23;
/// What an inherent `impl` holds that takes a visibility: methods, functions and constants. Its
/// associated types are unstable there and are left alone.
const INHERENT_MEMBER_KINDS: [u64; 3] = [6, 12, 14];

/// One declaration, as the server reported it.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Declared {
    pub(super) name: String,
    /// Its containers, outermost first, by the names the server gives them (`inner`, `impl Roster`,
    /// `impl Default for Roster`, `Roster`).
    pub(super) within: Vec<String>,
    /// Where the server reported its name: the position a reference query asks about.
    pub(super) position: Value,
    pub(super) kind: DeclarationKind,
}

/// What the symbols being read are held by, which decides the kind of each.
#[derive(Clone, Copy)]
enum Holder {
    Module,
    Struct,
    InherentImpl,
    NoVisibility,
}

/// Every declaration in `symbols` whose name is an identifier, outermost first and in source order.
///
/// A child of a struct is a `Field`, of an inherent `impl` an `InherentMember`; a child of an
/// `impl … for …`, of a trait or of an enum is `NoVisibility`; everything else — a module-level
/// item, an inline module's item, a `mod` — is an `Item`. A tuple field, which the server names `0`,
/// is not an identifier and is skipped.
pub(super) fn declarations_within(symbols: &Value) -> Vec<Declared> {
    let mut found = Vec::new();
    collect(symbols, &[], Holder::Module, &mut found);
    found
}

fn collect(symbols: &Value, within: &[String], holder: Holder, found: &mut Vec<Declared>) {
    for symbol in symbols.as_array().into_iter().flatten() {
        let name = symbol
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let kind = symbol.get("kind").and_then(Value::as_u64);
        let position = symbol
            .pointer("/selectionRange/start")
            .or_else(|| symbol.pointer("/location/range/start"));

        if let (true, Some(position), Some(declared)) = (
            is_identifier(name),
            position,
            kind.and_then(|kind| kind_in(holder, kind)),
        ) {
            found.push(Declared {
                name: name.to_string(),
                within: within.to_vec(),
                position: position.clone(),
                kind: declared,
            });
        }

        if let (Some(children), Some(inside)) = (symbol.get("children"), holds(name, kind)) {
            let mut containers = within.to_vec();
            containers.push(name.to_string());
            collect(children, &containers, inside, found);
        }
    }
}

/// The kind of a declaration of `symbol_kind` held by `holder`, when it takes a visibility or is
/// one the move has to know it cannot give one.
fn kind_in(holder: Holder, symbol_kind: u64) -> Option<DeclarationKind> {
    match holder {
        Holder::Module => Some(DeclarationKind::Item),
        Holder::Struct => Some(DeclarationKind::Field),
        Holder::InherentImpl => INHERENT_MEMBER_KINDS
            .contains(&symbol_kind)
            .then_some(DeclarationKind::InherentMember),
        Holder::NoVisibility => Some(DeclarationKind::NoVisibility),
    }
}

/// What the children of the symbol called `name` are held by, or `None` for a symbol whose children
/// are not declarations of the file's own surface — the items inside a function body.
fn holds(name: &str, symbol_kind: Option<u64>) -> Option<Holder> {
    let is_impl = name
        .strip_prefix("impl")
        .is_some_and(|rest| rest.starts_with([' ', '<']));
    match symbol_kind? {
        _ if is_impl && name.contains(" for ") => Some(Holder::NoVisibility),
        _ if is_impl => Some(Holder::InherentImpl),
        SYMBOL_KIND_MODULE => Some(Holder::Module),
        SYMBOL_KIND_STRUCT => Some(Holder::Struct),
        SYMBOL_KIND_TRAIT | SYMBOL_KIND_ENUM => Some(Holder::NoVisibility),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn a_symbol(name: &str, kind: u64, line: u64, children: Vec<Value>) -> Value {
        json!({
            "name": name,
            "kind": kind,
            "range": { "start": { "line": line, "character": 0 }, "end": { "line": line + 1, "character": 0 } },
            "selectionRange": { "start": { "line": line, "character": 4 }, "end": { "line": line, "character": 8 } },
            "children": children,
        })
    }

    fn kinds_of(declared: &[Declared]) -> Vec<(String, Vec<String>, DeclarationKind)> {
        declared
            .iter()
            .map(|one| (one.name.clone(), one.within.clone(), one.kind))
            .collect()
    }

    fn named(
        name: &str,
        within: &[&str],
        kind: DeclarationKind,
    ) -> (String, Vec<String>, DeclarationKind) {
        (
            name.to_string(),
            within
                .iter()
                .map(|container| (*container).to_string())
                .collect(),
            kind,
        )
    }

    #[test]
    fn a_struct_s_fields_and_an_inherent_impl_s_members_are_widenable_and_a_trait_impl_s_are_not() {
        // Given a struct with a named and a tuple-like field, an inherent impl and a trait impl
        let symbols = json!([
            a_symbol(
                "Roster",
                23,
                0,
                vec![a_symbol("rev", 8, 1, vec![]), a_symbol("0", 8, 2, vec![])]
            ),
            a_symbol(
                "impl Roster",
                19,
                4,
                vec![a_symbol("broadcast", 6, 5, vec![])]
            ),
            a_symbol(
                "impl Default for Roster",
                19,
                8,
                vec![a_symbol("default", 6, 9, vec![])]
            ),
        ]);

        // When its declarations are read
        let declared = declarations_within(&symbols);

        // Then each has its containers and its kind, and the tuple field is skipped
        assert_eq!(
            kinds_of(&declared),
            [
                named("Roster", &[], DeclarationKind::Item),
                named("rev", &["Roster"], DeclarationKind::Field),
                named(
                    "broadcast",
                    &["impl Roster"],
                    DeclarationKind::InherentMember
                ),
                named(
                    "default",
                    &["impl Default for Roster"],
                    DeclarationKind::NoVisibility
                ),
            ]
        );
    }

    #[test]
    fn an_inline_module_s_items_are_items_and_trait_items_and_enum_variants_are_not_widenable() {
        // Given an inline module, a trait and an enum
        let symbols = json!([
            a_symbol("inner", 2, 0, vec![a_symbol("Probe", 23, 1, vec![])]),
            a_symbol("Progressing", 11, 3, vec![a_symbol("tick", 6, 4, vec![])]),
            a_symbol("Phase", 10, 6, vec![a_symbol("Idle", 22, 7, vec![])]),
        ]);

        // When its declarations are read
        let declared = declarations_within(&symbols);

        // Then
        assert_eq!(
            kinds_of(&declared),
            [
                named("inner", &[], DeclarationKind::Item),
                named("Probe", &["inner"], DeclarationKind::Item),
                named("Progressing", &[], DeclarationKind::Item),
                named("tick", &["Progressing"], DeclarationKind::NoVisibility),
                named("Phase", &[], DeclarationKind::Item),
                named("Idle", &["Phase"], DeclarationKind::NoVisibility),
            ]
        );
    }
}
