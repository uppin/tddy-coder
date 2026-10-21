//! Rule 3: a type the signature of a widened declaration names is widened too, until nothing new is,
//! so a `-D warnings` gate does not fail on `private_interfaces`.

use std::collections::BTreeMap;

use super::Widened;
use crate::apply::byte_offset;
use crate::crate_move::source_scan::masked_code;
use crate::crate_move::{DeclarationKind, Survey};

/// The top-level `struct`, `enum`, `type` and `trait` declarations of the surveyed moving files —
/// the declarations a signature can name that only widening makes nameable. `texts` holds each
/// moving file's text by path.
pub(crate) fn type_declarations(
    texts: &BTreeMap<String, String>,
    surveys: &[Survey],
) -> Vec<Widened> {
    let mut types = Vec::new();
    for survey in surveys {
        let Some(code) = texts.get(&survey.source).map(|text| masked_code(text)) else {
            continue;
        };
        for item in survey
            .declarations
            .iter()
            .filter(|item| item.kind == DeclarationKind::Item && item.within.is_empty())
            .filter(|item| declares_a_type(&code, item.declared_at))
        {
            types.push(Widened::of(&survey.source, item, None));
        }
    }
    types
}

/// Whether the keyword in front of the name at `declared_at` introduces a type.
fn declares_a_type(code: &str, declared_at: crate::edit::Position) -> bool {
    let Ok(name_at) = byte_offset(code, declared_at.line, declared_at.col) else {
        return false;
    };
    let line_start = code[..name_at].rfind('\n').map_or(0, |at| at + 1);
    matches!(
        code[line_start..name_at].split_whitespace().next_back(),
        Some("struct" | "enum" | "type" | "trait")
    )
}

/// The `candidates` (declarations of the moving files not yet widened) whose name is an identifier
/// token in the signature of a `widened` one — a `fn`'s up to its body, a field's type — or of a
/// candidate so kept, each with the reason `` named by the signature of `<item>` ``. `texts` holds
/// each moving file's text by path.
pub(crate) fn escaping(
    texts: &BTreeMap<String, String>,
    widened: &[Widened],
    candidates: &[Widened],
) -> Vec<Widened> {
    let codes: BTreeMap<&str, String> = texts
        .iter()
        .map(|(file, text)| (file.as_str(), masked_code(text)))
        .collect();
    let mut kept: Vec<Widened> = Vec::new();
    let mut carrying: Vec<Widened> = widened.to_vec();

    let mut next = 0;
    while let Some(carrier) = carrying.get(next).cloned() {
        next += 1;
        let Some(code) = codes.get(carrier.file.as_str()) else {
            continue;
        };
        for name in signature_names(code, &carrier) {
            for found in candidates.iter().filter(|candidate| candidate.name == name) {
                if kept
                    .iter()
                    .any(|already| already.is_the_declaration_of(found))
                {
                    continue;
                }
                let named = Widened {
                    reason: Some(format!("named by the signature of `{}`", carrier.name)),
                    ..found.clone()
                };
                carrying.push(named.clone());
                kept.push(named);
            }
        }
    }
    kept
}

/// The identifier tokens of the signature of the declaration `carrier` names, in order, its own name
/// left out: a field's or constant's type up to a `,`, `=`, `;` or the closing brace of its struct;
/// anything else up to the `{` or `;` that ends it.
fn signature_names<'a>(code: &'a str, carrier: &Widened) -> Vec<&'a str> {
    let Ok(start) = byte_offset(code, carrier.declared_at.line, carrier.declared_at.col) else {
        return Vec::new();
    };
    let Some(rest) = code.get(start..) else {
        return Vec::new();
    };
    let after_name = rest.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_');
    let typed =
        after_name.trim_start().starts_with(':') && !after_name.trim_start().starts_with("::");
    let signature = &after_name[..signature_len(after_name, typed)];
    signature
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty())
        .collect()
}

/// Where a signature ends, in the text after its name: at the first of the `ends` met outside every
/// bracket pair, and for a typed one — a field — at the brace that closes its struct.
fn signature_len(rest: &str, typed: bool) -> usize {
    let mut depth = 0i32;
    let mut previous = ' ';
    for (at, c) in rest.char_indices() {
        let angle = typed && !(c == '>' && previous == '-');
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            '<' if angle => depth += 1,
            '>' if angle => depth -= 1,
            '}' if typed && depth <= 0 => return at,
            ',' | '=' if typed && depth <= 0 => return at,
            ';' if depth <= 0 => return at,
            '{' if !typed && depth <= 0 => return at,
            _ => {}
        }
        previous = c;
    }
    rest.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Position;

    fn declared(name: &str, line: u32, col: u32) -> Widened {
        Widened {
            file: "origin/src/roster.rs".to_string(),
            name: name.to_string(),
            declared_at: Position { line, col },
            reason: None,
        }
    }

    #[test]
    fn a_type_a_widened_fn_returns_is_kept_and_a_type_nothing_names_is_not() {
        // Given a widened `make` returning `Helper`, and an unrelated `Other`
        let text = "pub(crate) struct Helper;\n\npub(crate) struct Other;\n\n\
                    pub(crate) fn make() -> Helper {\n    Helper\n}\n";
        let texts = BTreeMap::from([("origin/src/roster.rs".to_string(), text.to_string())]);

        // When the escaping types are read
        let kept = escaping(
            &texts,
            &[declared("make", 5, 15)],
            &[declared("Helper", 1, 19), declared("Other", 3, 19)],
        );

        // Then `Helper` is kept, with the signature as its reason
        assert_eq!(
            kept,
            vec![Widened {
                reason: Some("named by the signature of `make`".to_string()),
                ..declared("Helper", 1, 19)
            }]
        );
    }

    #[test]
    fn a_type_a_kept_type_s_field_names_is_kept_in_turn() {
        // Given a widened `make` returning `Outer`, whose field holds `Inner`
        let text = "pub(crate) struct Inner;\n\npub(crate) struct Outer {\n    pub(crate) inner: Inner,\n}\n\n\
                    pub(crate) fn make() -> Outer {\n    Outer { inner: Inner }\n}\n";
        let texts = BTreeMap::from([("origin/src/roster.rs".to_string(), text.to_string())]);

        // When the escaping types are read, with the field already widened
        let kept = escaping(
            &texts,
            &[declared("make", 7, 15), declared("Outer::inner", 4, 16)],
            &[declared("Inner", 1, 19), declared("Outer", 3, 19)],
        );

        // Then both types are kept
        assert_eq!(
            kept.iter()
                .map(|widened| widened.name.as_str())
                .collect::<Vec<_>>(),
            ["Outer", "Inner"]
        );
    }
}
