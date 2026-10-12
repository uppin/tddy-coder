//! Rule 5: `&PathBuf` / `&String` / `&Vec<T>` parameters narrowed to `&Path` / `&str` / `&[T]`
//! (`clippy::ptr_arg`). Which narrowings are kept is the server's call, through its type
//! diagnostics; see `RustBackend::verified_narrowings`.

/// One parameter of the extracted function, and the type it is narrowed to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Narrowing {
    pub parameter: String,
    pub from: String,
    pub to: String,
}

/// Every parameter of `name` whose type is a borrowed `PathBuf`, `String` or `Vec<T>` (not
/// `&mut`), with the narrower type. `Path` is written bare where the file binds it by `use`,
/// `std::path::Path` otherwise; a qualified spelling keeps its qualifier.
pub(super) fn narrowings(text: &str, name: &str) -> Vec<Narrowing> {
    let _ = (text, name);
    // TODO(reshape-extract-method-clean): implement
    todo!("narrowings")
}

/// `text` with `chosen` applied to `name`'s signature, and each `<parameter>.clone()` in its body
/// written `<parameter>.to_owned()`.
pub(super) fn narrowed(text: &str, name: &str, chosen: &[Narrowing]) -> String {
    let _ = (text, name, chosen);
    // TODO(reshape-extract-method-clean): implement
    todo!("narrowed")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRODUCED: &str = "use std::path::PathBuf;\n\n\
                            fn kept_joined(dir: &PathBuf, label: &String, items: &Vec<u32>, out: &mut PathBuf) -> PathBuf {\n    \
                            let copy = dir.clone();\n    copy.join(label).join(items.len().to_string())\n}\n";

    fn narrowing(parameter: &str, from: &str, to: &str) -> Narrowing {
        Narrowing {
            parameter: parameter.to_string(),
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    #[test]
    fn narrows_borrowed_path_string_and_vec_parameters_and_leaves_a_mutable_borrow() {
        assert_eq!(
            narrowings(PRODUCED, "kept_joined"),
            [
                narrowing("dir", "&PathBuf", "&std::path::Path"),
                narrowing("label", "&String", "&str"),
                narrowing("items", "&Vec<u32>", "&[u32]"),
            ]
        );
    }

    #[test]
    fn writes_path_bare_where_the_file_binds_it() {
        let binding_path =
            PRODUCED.replace("use std::path::PathBuf;", "use std::path::{Path, PathBuf};");

        assert_eq!(
            narrowings(&binding_path, "kept_joined")[0],
            narrowing("dir", "&PathBuf", "&Path")
        );
    }

    #[test]
    fn a_narrowed_parameter_the_body_clones_becomes_an_owned_copy() {
        let chosen = narrowings(PRODUCED, "kept_joined");

        let text = narrowed(PRODUCED, "kept_joined", &chosen);

        assert!(
            text.contains(
                "fn kept_joined(dir: &std::path::Path, label: &str, items: &[u32], out: &mut PathBuf) -> PathBuf {"
            ),
            "{text}"
        );
        assert!(text.contains("let copy = dir.to_owned();"), "{text}");
    }
}
