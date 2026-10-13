//! Rule 5: `&PathBuf` / `&String` / `&Vec<T>` parameters narrowed to `&Path` / `&str` / `&[T]`
//! (`clippy::ptr_arg`). Which narrowings are kept is the server's call, through its type
//! diagnostics; see `RustBackend::verified_narrowings`.

use super::signature::{generic, signature};
use super::span::{function_span, is_word};
use crate::backends::rust::early_return::masked_to_code;
use crate::backends::rust::imports::names_bound;
use serde_json::Value;

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
    let masked = masked_to_code(text);
    let Some(signature) = function_span(text, name).and_then(|span| signature(&masked, span))
    else {
        return Vec::new();
    };
    let binds_path = names_bound(text).iter().any(|bound| bound == "Path");
    signature
        .parameters
        .iter()
        .filter_map(|parameter| {
            let from = &masked[parameter.ty.clone()];
            Some(Narrowing {
                parameter: parameter.name.clone(),
                from: from.to_string(),
                to: narrower(from, binds_path)?,
            })
        })
        .collect()
}

/// The type a borrowed `PathBuf`, `String` or `Vec<T>` is narrowed to, or `None` for any other.
fn narrower(ty: &str, binds_path: bool) -> Option<String> {
    let owned = ty.strip_prefix('&')?.trim_start();
    match owned {
        "PathBuf" if binds_path => Some("&Path".to_string()),
        "PathBuf" => Some("&std::path::Path".to_string()),
        "std::path::PathBuf" => Some("&std::path::Path".to_string()),
        "String" | "std::string::String" => Some("&str".to_string()),
        _ => generic(owned)
            .filter(|vec| matches!(vec.path, "Vec" | "std::vec::Vec") && vec.arguments.len() == 1)
            .map(|vec| format!("&[{}]", vec.arguments[0])),
    }
}

/// `text` with `chosen` applied to `name`'s signature, and each `<parameter>.clone()` in its body
/// written `<parameter>.to_owned()`.
pub(super) fn narrowed(text: &str, name: &str, chosen: &[Narrowing]) -> String {
    let masked = masked_to_code(text);
    let Some(span) = function_span(text, name) else {
        return text.to_string();
    };
    let Some(signature) = signature(&masked, span) else {
        return text.to_string();
    };
    let mut edits: Vec<(std::ops::Range<usize>, &str)> = Vec::new();
    for narrowing in chosen {
        if let Some(parameter) = signature
            .parameters
            .iter()
            .find(|parameter| parameter.name == narrowing.parameter)
        {
            edits.push((parameter.ty.clone(), narrowing.to.as_str()));
        }
        edits.extend(
            clones_of(&masked[span.open..span.close], &narrowing.parameter)
                .into_iter()
                .map(|at| (span.open + at..span.open + at + "clone".len(), "to_owned")),
        );
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut written = text.to_string();
    for (range, replacement) in edits {
        written.replace_range(range, replacement);
    }
    written
}

/// What the run says about a narrowing it undid.
pub(super) fn kept_note(narrowing: &Narrowing) -> String {
    format!(
        "kept `{}: {}`: the body needs the type rust-analyzer wrote",
        narrowing.parameter, narrowing.from
    )
}

/// The narrowings whose parameter is named on one of the zero-based `lines` of `text`.
pub(super) fn blamed<'a>(
    chosen: &'a [Narrowing],
    text: &str,
    lines: &[usize],
) -> Vec<&'a Narrowing> {
    let source: Vec<&str> = text.split('\n').collect();
    chosen
        .iter()
        .filter(|narrowing| {
            lines.iter().any(|line| {
                source
                    .get(*line)
                    .is_some_and(|written| names_word(written, &narrowing.parameter))
            })
        })
        .collect()
}

/// Whether `line` holds `word` as a whole identifier.
fn names_word(line: &str, word: &str) -> bool {
    let bytes = line.as_bytes();
    line.match_indices(word).any(|(at, _)| {
        (at == 0 || !is_word(bytes[at - 1]))
            && bytes.get(at + word.len()).is_none_or(|b| !is_word(*b))
    })
}

/// The zero-based lines of `text` that a pull-diagnostics `report` raises an `E0308` type mismatch
/// on, inside the function `name` or on the line that calls it.
pub(super) fn mismatch_lines(report: &Value, text: &str, name: &str) -> Vec<usize> {
    let function = function_span(text, name).map(|span| {
        let line_of = |at: usize| text[..at].matches('\n').count();
        line_of(span.keyword)..=line_of(span.close)
    });
    let source: Vec<&str> = text.split('\n').collect();
    let call = format!("{name}(");
    report
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("code").and_then(Value::as_str) == Some("E0308"))
        .filter_map(|item| item.pointer("/range/start/line").and_then(Value::as_u64))
        .map(|line| line as usize)
        .filter(|line| {
            function
                .as_ref()
                .is_some_and(|within| within.contains(line))
                || source
                    .get(*line)
                    .is_some_and(|written| written.contains(&call))
        })
        .collect()
}

/// The offsets of the `clone` in every `<parameter>.clone()` of masked `body`.
fn clones_of(body: &str, parameter: &str) -> Vec<usize> {
    let bytes = body.as_bytes();
    let call = format!("{parameter}.clone()");
    body.match_indices(&call)
        .filter(|(at, _)| *at == 0 || !(is_word(bytes[at - 1]) || bytes[at - 1] == b'.'))
        .map(|(at, _)| at + parameter.len() + 1)
        .collect()
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
