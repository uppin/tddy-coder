//! Rule 3: a type the signature of a widened declaration names is widened too, until nothing new is,
//! so a `-D warnings` gate does not fail on `private_interfaces`.

use std::collections::BTreeMap;

use super::Widened;

/// The `candidates` (declarations of the moving files not yet widened) whose name is an identifier
/// token in the signature of a `widened` one — a `fn`'s up to its body, a field's type — or of a
/// candidate so kept, each with the reason `` named by the signature of `<item>` ``. `texts` holds
/// each moving file's text by path.
#[allow(
    dead_code,
    reason = "TODO(reshape-move-widen): called by `widened` in the green phase"
)]
pub(crate) fn escaping(
    texts: &BTreeMap<String, String>,
    widened: &[Widened],
    candidates: &[Widened],
) -> Vec<Widened> {
    // TODO(reshape-move-widen): implement
    let _ = (texts, widened, candidates);
    todo!("the types a widened signature names")
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
