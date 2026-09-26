//! The symbol an assist introduced, found in what it wrote so the engine can rename it.
//!
//! Split out of `backends/rust.rs`, which is past its size budget.
//!
//! Most assists write a fixed placeholder (`fn fun_name`, `mod modname`) and expect the client to
//! rename it. "Extract into variable" does not, any more: the rust-analyzer the dev shell ships names
//! the binding from the expression (`suggest_name::for_variable`, so a read of `self.clones` becomes
//! `let clones`). Looking for `let var_name` refused every `extract_variable`, so that binding is
//! found by what the assist added instead.

use super::early_return::masked_to_code;
use super::impl_seam::with_method_calls_restored;
use super::nested_modules::with_nested_references_restored;
use super::{server_defect, MovedItem, Placeholder};
use crate::Result;

/// The identifier an assist introduced, and the byte offset its declaration names it at.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Introduced {
    pub(super) name: String,
    pub(super) offset: usize,
}

/// What the assist introduced in `extracted`, and the text with the placeholder's known rewrites
/// undone.
///
/// A fixed placeholder is the only kind those rewrites follow (`modname::` inserted before a moved
/// item's name), so they are undone for it alone. A binding the server named relocates nothing.
pub(super) fn introduced_by(
    placeholder: Placeholder,
    original: &str,
    extracted: String,
    impl_members: &[MovedItem],
    moved: &[MovedItem],
) -> Result<(String, Introduced)> {
    let Some(name) = placeholder.name else {
        let introduced = introduced_binding(original, &extracted)?;
        return Ok((extracted, introduced));
    };

    let restored = with_method_calls_restored(&extracted, name, impl_members);
    let restored = with_nested_references_restored(&restored, name, moved);
    let declaration = format!("{} {name}", placeholder.keyword);
    let offset = restored
        .find(&declaration)
        .map(|offset| offset + placeholder.keyword.len() + 1)
        .ok_or_else(|| {
            server_defect(format!(
                "rust-analyzer did not produce a `{declaration}` to name"
            ))
        })?;

    Ok((
        restored,
        Introduced {
            name: name.to_string(),
            offset,
        },
    ))
}

/// The one `let` binding `extracted` gained over `original`.
///
/// A binding is new when its name is declared more often than before, and when its name sits in
/// the span the assist changed. The count keeps an existing `let` the assist merely rewrote (its
/// initializer now reads the new binding) from being taken for the new one, and the span picks the
/// right declaration when the new binding shadows an older one of the same name. Comments and
/// literals are masked first, so a `let` written in one is not a binding.
fn introduced_binding(original: &str, extracted: &str) -> Result<Introduced> {
    let before = bindings(original);
    let after = bindings(extracted);
    let (changed_from, changed_to) = changed_span(original, extracted);

    let declared = |set: &[Introduced], name: &str| set.iter().filter(|b| b.name == name).count();
    let mut introduced: Vec<Introduced> = after
        .iter()
        .filter(|binding| declared(&after, &binding.name) > declared(&before, &binding.name))
        .filter(|binding| {
            binding.offset <= changed_to && binding.offset + binding.name.len() >= changed_from
        })
        .map(|binding| Introduced {
            name: binding.name.clone(),
            offset: binding.offset,
        })
        .collect();

    match introduced.len() {
        1 => Ok(introduced.remove(0)),
        0 => Err(server_defect(
            "rust-analyzer's extraction introduced no `let` binding to name",
        )),
        _ => Err(server_defect(format!(
            "rust-analyzer's extraction introduced {} `let` bindings ({}), and one variable was \
             asked for, so which to name is not known",
            introduced.len(),
            introduced
                .iter()
                .map(|binding| format!("`{}`", binding.name))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Every `let name` and `let mut name` in the code of `text`, with the offset of the name.
///
/// A pattern that is not a single identifier (`let (a, b)`, `let _`) binds no name to rename.
fn bindings(text: &str) -> Vec<Introduced> {
    let code = masked_to_code(text);
    let bytes = code.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;

    while let Some(at) = code[from..].find("let").map(|offset| from + offset) {
        from = at + "let".len();
        let opens = at == 0 || !is_identifier_byte(bytes[at - 1]);
        if !opens
            || bytes
                .get(from)
                .is_none_or(|byte| !byte.is_ascii_whitespace())
        {
            continue;
        }

        let mut name_at = skip_whitespace(bytes, from);
        if let Some(after_mut) = keyword_at(bytes, name_at, "mut") {
            name_at = skip_whitespace(bytes, after_mut);
        }
        let name_end = identifier_end(bytes, name_at);
        let name = &code[name_at..name_end];
        if !name.is_empty() && name != "_" && !name.as_bytes()[0].is_ascii_digit() {
            found.push(Introduced {
                name: name.to_string(),
                offset: name_at,
            });
        }
    }

    found
}

/// The byte span of `extracted` that differs from `original`: everything but their common prefix
/// and suffix.
fn changed_span(original: &str, extracted: &str) -> (usize, usize) {
    let (before, after) = (original.as_bytes(), extracted.as_bytes());
    let prefix = before
        .iter()
        .zip(after)
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = before[prefix..]
        .iter()
        .rev()
        .zip(after[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    (prefix, after.len() - suffix)
}

fn skip_whitespace(bytes: &[u8], from: usize) -> usize {
    bytes[from..]
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .map_or(bytes.len(), |offset| from + offset)
}

/// Just past `keyword` when it stands whole at `at` and whitespace follows it.
fn keyword_at(bytes: &[u8], at: usize, keyword: &str) -> Option<usize> {
    let end = at + keyword.len();
    (bytes.get(at..end) == Some(keyword.as_bytes())
        && bytes.get(end).is_some_and(u8::is_ascii_whitespace))
    .then_some(end)
}

fn identifier_end(bytes: &[u8], from: usize) -> usize {
    bytes[from..]
        .iter()
        .position(|byte| !is_identifier_byte(*byte))
        .map_or(bytes.len(), |offset| from + offset)
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Source text from its lines, one per entry.
    fn a_file(lines: &[&str]) -> String {
        lines.join("\n") + "\n"
    }

    #[test]
    fn finds_the_binding_the_assist_named_after_the_field_it_read() {
        // Given
        let original = a_file(&[
            "fn highest(h: &Host) -> u32 {",
            "    h.clones.iter().copied().max().unwrap_or(0)",
            "}",
        ]);
        let extracted = a_file(&[
            "fn highest(h: &Host) -> u32 {",
            "    let clones = &h.clones;",
            "    clones.iter().copied().max().unwrap_or(0)",
            "}",
        ]);

        // When
        let introduced = introduced_binding(&original, &extracted);

        // Then
        assert_eq!(
            introduced.ok(),
            Some(Introduced {
                name: "clones".to_string(),
                offset: extracted.find("clones =").expect("the declaration"),
            })
        );
    }

    /// The assist rewrites the statement it hoisted the read out of, which is a `let` of its own.
    #[test]
    fn reads_past_a_binding_the_assist_only_rewrote() {
        // Given
        let original = a_file(&[
            "fn f(h: &Host) -> u32 {",
            "    let x = h.a.b;",
            "    x",
            "}",
        ]);
        let extracted = a_file(&[
            "fn f(h: &Host) -> u32 {",
            "    let a = &h.a;",
            "    let x = a.b;",
            "    x",
            "}",
        ]);

        // When
        let introduced = introduced_binding(&original, &extracted).map(|binding| binding.name);

        // Then
        assert_eq!(introduced.ok().as_deref(), Some("a"));
    }

    #[test]
    fn finds_a_new_binding_that_shadows_an_older_one_of_the_same_name() {
        // Given
        let original = a_file(&[
            "fn f(h: &Host) -> u32 {",
            "    let count = 1;",
            "    count + h.count",
            "}",
        ]);
        let extracted = a_file(&[
            "fn f(h: &Host) -> u32 {",
            "    let count = 1;",
            "    let mut count = h.count;",
            "    count + count",
            "}",
        ]);

        // When
        let introduced = introduced_binding(&original, &extracted).map(|binding| binding.offset);

        // Then
        assert_eq!(
            introduced.ok(),
            extracted.find("count = h.count"),
            "the second `count` is the one the assist introduced"
        );
    }

    #[test]
    fn refuses_an_extraction_that_introduced_no_binding() {
        // Given an assist that wrote a comment mentioning a `let` and nothing else
        let original = a_file(&["fn f() -> u32 {", "    1", "}"]);
        let extracted = a_file(&["fn f() -> u32 {", "    // let one = 1;", "    1", "}"]);

        // When
        let refusal = introduced_binding(&original, &extracted).map_err(|error| error.to_string());

        // Then
        assert_eq!(
            refusal,
            Err(
                "rust-analyzer's answer was unusable: rust-analyzer's extraction introduced no \
                 `let` binding to name"
                    .to_string()
            )
        );
    }
}
