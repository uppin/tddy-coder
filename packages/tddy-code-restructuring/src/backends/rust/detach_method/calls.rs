//! The call sites the server reports for a member, read as the calls they are: `R.m(a)` on a
//! receiver, or `Self::m(a)` / `T::m(a)` through the type, and where each is written from.
//!
//! What is not one of those forms, what sits in a macro, and what a receiver is typed as that is
//! neither the block's type nor a reference to it, is refused by [`shape_of`] and [`lend_of`] with
//! the reason; the survey names the file and line.

use std::ops::Range;

use serde_json::Value;

use super::super::imports::names_bound;
use super::super::item_move::text::{enclosing_modules, scope_of, use_insertion};
use super::scan;
use crate::crate_move::source_scan::items_of_module;
use crate::Result;

/// How a receiver lends itself to the function's first parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Lend {
    /// The receiver is already a reference to the type: passed as it is.
    Pass,
    /// The receiver is the type itself: lent as `&mut R` (`true`) or `&R` (`false`).
    Borrow { mutable: bool },
}

/// The shape of one reference the server reported, read from the masked text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Shape {
    /// `receiver.name(args)`: `span` is the whole call, from the receiver to the closing paren.
    Method {
        span: Range<usize>,
        receiver: Range<usize>,
        receiver_name: String,
        args: Option<Range<usize>>,
        written: String,
    },
    /// `qualifier::name(args)` through `Self` or the block's type: `qualifier` is the span of the
    /// path before the name, `::` included, and `prefix` the module path it writes before the type.
    Path {
        qualifier: Range<usize>,
        prefix: Option<String>,
    },
}

/// Where a call is written from, relative to the function it calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Reach {
    /// The same module: the function is named bare.
    Here,
    /// Another module: the function is named through `written` (`host::`, `crate::`), and the
    /// module it names may need a `use` first.
    Through {
        written: String,
        uses: Option<UseLine>,
    },
}

/// A `use` the caller's module needs before it can name the function's module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UseLine {
    /// The item as written, `use crate::host;`.
    pub(super) text: String,
    /// The offset the line is inserted at.
    pub(super) at: usize,
    /// Whether a blank line has to follow it.
    pub(super) blank: bool,
}

/// The shape of the reference naming `name` at `at` in `text`, read from its masked text, or the
/// reason it cannot be re-pointed.
pub(super) fn shape_of(
    text: &str,
    masked: &str,
    at: usize,
    name: &str,
    self_type: &str,
) -> std::result::Result<Shape, String> {
    let bytes = masked.as_bytes();
    let before = scan::skip_whitespace_back(bytes, at);
    if before > 0 && bytes[before - 1] == b'.' {
        return method_shape(text, masked, at, before - 1, name);
    }
    if before >= 2 && &bytes[before - 2..before] == b"::" {
        return path_shape(text, masked, at, name, self_type);
    }
    Err(format!(
        "`{name}` is named here but not called: this run re-points calls only"
    ))
}

/// A call on a receiver: `receiver . name (args)`, with the dot at `dot`.
fn method_shape(
    text: &str,
    masked: &str,
    name_at: usize,
    dot: usize,
    name: &str,
) -> std::result::Result<Shape, String> {
    let bytes = masked.as_bytes();
    let receiver_end = scan::skip_whitespace_back(bytes, dot);
    let receiver_start = scan::word_start(bytes, receiver_end);
    let written = format!(".{name}(…)");
    if receiver_start == receiver_end {
        return Err(format!(
            "the receiver of `{written}` is not a variable, and this run re-points calls on a \
             variable only"
        ));
    }
    let before_receiver = scan::skip_whitespace_back(bytes, receiver_start);
    if before_receiver > 0 && matches!(bytes[before_receiver - 1], b'.' | b':' | b')' | b']') {
        return Err(format!(
            "the receiver of `{written}` is a field or the result of another expression, and this \
             run re-points calls on a variable only"
        ));
    }
    let receiver_name = &masked[receiver_start..receiver_end];
    let written = format!("{receiver_name}.{name}(…)");
    if !text[receiver_end..dot].trim().is_empty() || !text[dot + 1..name_at].trim().is_empty() {
        return Err(format!(
            "the call `{written}` is broken by a comment, which this run does not rewrite"
        ));
    }
    let open = scan::skip_whitespace(bytes, name_at + name.len());
    if bytes.get(open) != Some(&b'(') {
        return Err(format!(
            "`{name}` is named here but not called: this run re-points calls only"
        ));
    }
    let close = scan::matching_close(bytes, open)
        .ok_or_else(|| format!("the call `{written}` is not closed"))?;
    if scan::inside_a_macro(masked, receiver_start) {
        return Err(format!(
            "the call `{written}` is inside a macro invocation, which this run does not rewrite"
        ));
    }
    let args = (!masked[open + 1..close].trim().is_empty()).then(|| open + 1..close);
    Ok(Shape::Method {
        span: receiver_start..close + 1,
        receiver: receiver_start..receiver_end,
        receiver_name: receiver_name.to_string(),
        args,
        written,
    })
}

/// A call through a path: `Self::name(args)` or `Type::name(args)`, with `name` at `name_at`.
fn path_shape(
    text: &str,
    masked: &str,
    name_at: usize,
    name: &str,
    self_type: &str,
) -> std::result::Result<Shape, String> {
    let bytes = masked.as_bytes();
    let separator = scan::skip_whitespace_back(bytes, name_at) - 2;
    let mut start = scan::word_start(bytes, separator);
    while start >= 2 && &bytes[start - 2..start] == b"::" {
        let earlier = scan::word_start(bytes, start - 2);
        if earlier == start - 2 {
            break;
        }
        start = earlier;
    }
    let qualifier_text = &text[start..separator];
    let written = format!("{qualifier_text}::{name}(…)");
    let segments: Vec<&str> = qualifier_text.split("::").collect();
    let last = segments.last().copied().unwrap_or_default();
    if last != "Self" && last != self_type {
        return Err(format!(
            "the call `{written}` is not through `Self` or `{self_type}`, which this run does not \
             re-point"
        ));
    }
    let open = scan::skip_whitespace(bytes, name_at + name.len());
    if bytes.get(open) != Some(&b'(') {
        return Err(format!(
            "`{name}` is named here but not called: this run re-points calls only"
        ));
    }
    if scan::inside_a_macro(masked, start) {
        return Err(format!(
            "the call `{written}` is inside a macro invocation, which this run does not rewrite"
        ));
    }
    let prefix = segments[..segments.len() - 1].join("::");
    Ok(Shape::Path {
        qualifier: start..name_at,
        prefix: (!prefix.is_empty()).then_some(prefix),
    })
}

/// How the receiver of a call is lent: passed as it is when it is a reference to `self_type`, and
/// borrowed when it is `self_type` itself. Any other type is refused, naming it.
pub(super) fn lend_of(
    shape: &Shape,
    receiver_type: &str,
    self_type: &str,
    mutable: bool,
) -> std::result::Result<Lend, String> {
    let Shape::Method {
        receiver_name,
        written,
        ..
    } = shape
    else {
        return Err("a path call has no receiver to lend".to_string());
    };
    let written_type = receiver_type.trim();
    let (is_reference, base) = match written_type.strip_prefix('&') {
        Some(rest) => (true, strip_reference_head(rest)),
        None => (false, written_type),
    };
    if base.contains('<') || base.rsplit("::").next() != Some(self_type) {
        return Err(format!(
            "the call `{written}` has receiver `{receiver_name}` of type `{written_type}`, which is \
             neither `{self_type}` nor a reference to it"
        ));
    }
    Ok(if is_reference {
        Lend::Pass
    } else {
        Lend::Borrow { mutable }
    })
}

/// The type after a `&`: its lifetime and its `mut` taken off.
fn strip_reference_head(rest: &str) -> &str {
    let mut rest = rest.trim_start();
    if let Some(after) = rest.strip_prefix('\'') {
        let name_end = after
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        rest = after[name_end..].trim_start();
    }
    match rest.strip_prefix("mut") {
        Some(after) if after.starts_with(char::is_whitespace) => after.trim_start(),
        _ => rest,
    }
}

/// The type a hover states for the variable `name`: the `name: Type` line of its code, the `let`
/// and `mut` a binding is written with left out.
pub(super) fn type_of(hover: &Value, name: &str) -> Result<String> {
    let mut texts = Vec::new();
    collect_texts(hover.get("contents").unwrap_or(&Value::Null), &mut texts);
    texts
        .iter()
        .flat_map(|text| text.lines())
        .find_map(|line| {
            let line = line.trim();
            let line = line.strip_prefix("let ").unwrap_or(line).trim_start();
            let line = line.strip_prefix("mut ").unwrap_or(line).trim_start();
            let (variable, written) = line.split_once(':')?;
            (variable.trim() == name).then(|| written.trim().trim_end_matches(';').to_string())
        })
        .ok_or_else(|| {
            super::super::server_defect(format!(
                "its hover over `{name}` states no `{name}: Type` line: {hover}"
            ))
        })
}

/// Every string a hover's `contents` holds.
fn collect_texts(contents: &Value, texts: &mut Vec<String>) {
    match contents {
        Value::String(text) => texts.push(text.clone()),
        Value::Array(entries) => entries.iter().for_each(|entry| collect_texts(entry, texts)),
        Value::Object(fields) => {
            if let Some(Value::String(text)) = fields.get("value") {
                texts.push(text.clone());
            }
        }
        _ => {}
    }
}

/// Where a call in `caller_text` at `offset`, of a function in `block_module`, is written from.
/// `prefix` is the module path the call itself writes before the type, when it writes one.
pub(super) fn reach(
    block_module: &[String],
    caller_module: &[String],
    caller_text: &str,
    offset: usize,
    prefix: Option<&str>,
) -> Reach {
    if caller_module == block_module {
        return Reach::Here;
    }
    if let Some(prefix) = prefix {
        return Reach::Through {
            written: format!("{prefix}::"),
            uses: None,
        };
    }
    if block_module.len() == 1 {
        return Reach::Through {
            written: "crate::".to_string(),
            uses: None,
        };
    }
    let Some(module) = block_module.last() else {
        return Reach::Here;
    };
    let written = format!("{module}::");
    let chain = enclosing_modules(caller_text, offset);
    let scope = if chain.is_empty() {
        0..caller_text.len()
    } else {
        scope_of(caller_text, &chain).unwrap_or(0..caller_text.len())
    };
    let scope_text = &caller_text[scope.clone()];
    let bound = names_bound(scope_text).contains(module)
        || items_of_module(scope_text)
            .children
            .iter()
            .any(|child| &child.name == module);
    if bound {
        return Reach::Through {
            written,
            uses: None,
        };
    }
    let (at, blank) = use_insertion(caller_text, scope);
    Reach::Through {
        written,
        uses: Some(UseLine {
            text: format!("use crate::{};", block_module[1..].join("::")),
            at,
            blank,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_hover_states_a_bindings_type_with_or_without_let_and_mut() {
        let hover = json!({ "contents": { "kind": "markdown", "value": "```rust\nlet mut fresh: Host\n```" } });

        assert_eq!(
            type_of(&hover, "fresh").expect("the type is stated"),
            "Host"
        );
    }

    #[test]
    fn a_reference_with_a_lifetime_lends_as_it_is_and_a_box_is_refused_naming_its_type() {
        let shape = Shape::Method {
            span: 0..1,
            receiver: 0..1,
            receiver_name: "b".to_string(),
            args: None,
            written: "b.total(…)".to_string(),
        };

        assert_eq!(lend_of(&shape, "&'a Host", "Host", false), Ok(Lend::Pass));
        assert_eq!(
            lend_of(&shape, "Host", "Host", true),
            Ok(Lend::Borrow { mutable: true })
        );
        assert!(lend_of(&shape, "Box<Host>", "Host", false)
            .expect_err("a box is refused")
            .contains("of type `Box<Host>`, which is neither `Host` nor a reference to it"));
    }
}
