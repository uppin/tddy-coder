//! Rules 2-4: the lexical clean-ups of an extracted function, each safe by construction.

/// `text` with the tail of `name` taken out of `Ok(…)` / `Some(…)` when the function returns
/// `Result<(), …>` / `Option<()>` and the tail wraps an `if`, `match`, loop or block: the
/// expression becomes a statement, followed by `Ok(())` / `Some(())` (`clippy::unit_arg`).
pub(super) fn unit_tail_unwrapped(text: &str, name: &str) -> String {
    let _ = (text, name);
    // TODO(reshape-extract-method-clean): implement
    todo!("unit_tail_unwrapped")
}

/// `text` with every `field: field` in a struct literal inside `name` written `field`
/// (`clippy::redundant_field_names`).
pub(super) fn field_shorthand(text: &str, name: &str) -> String {
    let _ = (text, name);
    // TODO(reshape-extract-method-clean): implement
    todo!("field_shorthand")
}

/// A note when `name` takes more than seven parameters (`&self` not counted) or returns a tuple of
/// three or more, which clippy flags and only the plan can fix.
pub(super) fn shape_notes(text: &str, name: &str) -> Vec<String> {
    let _ = (text, name);
    // TODO(reshape-extract-method-clean): implement
    todo!("shape_notes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_tail_wrapped_in_ok_becomes_a_statement_followed_by_ok_unit() {
        let produced = "fn guarded(dir: &Path, flag: bool) -> Result<(), String> {\n    \
                        Ok(if flag {\n        let _p = dir.join(\"a\");\n        \
                        Err::<(), String>(\"e\".into())?;\n    })\n}\n";

        assert_eq!(
            unit_tail_unwrapped(produced, "guarded"),
            "fn guarded(dir: &Path, flag: bool) -> Result<(), String> {\n    \
             if flag {\n        let _p = dir.join(\"a\");\n        \
             Err::<(), String>(\"e\".into())?;\n    }\n    Ok(())\n}\n"
        );
    }

    #[test]
    fn a_value_tail_in_ok_is_left_alone() {
        let produced =
            "fn counted(n: u32) -> Result<u32, String> {\n    Ok(if n > 1 { n } else { 1 })\n}\n";

        assert_eq!(unit_tail_unwrapped(produced, "counted"), produced);
    }

    #[test]
    fn a_field_initialised_from_a_binding_of_its_own_name_is_written_in_shorthand() {
        let produced = "fn looked_up(base: &Path, root: &Path) -> usize {\n    \
                        let n = consume(&Lookup { base: base, root: root });\n    n\n}\n";

        assert_eq!(
            field_shorthand(produced, "looked_up"),
            "fn looked_up(base: &Path, root: &Path) -> usize {\n    \
             let n = consume(&Lookup { base, root });\n    n\n}\n"
        );
    }

    #[test]
    fn notes_more_than_seven_parameters_and_a_three_tuple_return() {
        let produced = "fn wide(&self, a: u8, b: u8, c: u8, d: u8, e: u8, f: u8, g: u8, h: u8) \
                        -> (u8, u8, u8) {\n    (a, b, c)\n}\n";

        assert_eq!(
            shape_notes(produced, "wide"),
            [
                "`wide` takes 8 parameters; clippy::too_many_arguments fires above 7",
                "`wide` returns a 3-tuple; clippy::type_complexity may fire",
            ]
        );
    }
}
