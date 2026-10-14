//! The outline's view of the method a `read_fields_through` range sits in (RS5).

use serde_json::Value;

use super::super::early_return::masked_to_code;
use super::super::lsp_edits::offset_of;
use super::super::{seam_refusal, LspPoint, SYMBOL_KIND_IMPL};
use super::Rebind;
use crate::Result;

/// The outline kinds of a function an `impl` holds: a method and an associated function.
const SYMBOL_KINDS_OF_FUNCTIONS: [u64; 2] = [6, 12];

/// What the outline says of the method a range sits in.
pub(super) struct Method {
    /// The enclosing `impl`'s self type as its header writes it, generics included.
    pub(super) self_type: Option<String>,
}

/// RS5: the deepest function of the outline around the range must take a `self` receiver.
pub(super) fn the_method_around(symbols: &Value, rebind: &Rebind<'_>) -> Result<Method> {
    let line = |offset: usize| rebind.text[..offset].matches('\n').count();
    let (start, end) = (line(rebind.range.start), line(rebind.range.end));
    let mut found = None;
    around(symbols, start, end, None, &mut found);

    let refusal = || seam_refusal("the range is in no method with a `self` receiver");
    let (function, label) = found.ok_or_else(refusal)?;
    let name_at = function
        .pointer("/selectionRange/start")
        .map(|start| LspPoint::read(Some(start)))
        .transpose()?
        .map(|point| offset_of(&rebind.text, point))
        .ok_or_else(refusal)?;
    if !takes_self(&masked_to_code(&rebind.text), name_at) {
        return Err(refusal());
    }
    Ok(Method {
        self_type: label.and_then(|label| self_type_of(&label)),
    })
}

/// Descend through the outline's nodes that hold lines `start..=end` (zero-based), noting the last
/// function met and the label of the `impl` it sits in.
fn around<'a>(
    nodes: &'a Value,
    start: usize,
    end: usize,
    label: Option<String>,
    found: &mut Option<(&'a Value, Option<String>)>,
) {
    for node in nodes.as_array().into_iter().flatten() {
        let line = |which: &str| {
            node.pointer(&format!("/range/{which}/line"))
                .and_then(Value::as_u64)
                .map(|line| line as usize)
        };
        let (Some(first), Some(last)) = (line("start"), line("end")) else {
            continue;
        };
        if first > start || last < end {
            continue;
        }
        let kind = node.get("kind").and_then(Value::as_u64);
        let mut label = label.clone();
        if kind == Some(SYMBOL_KIND_IMPL) {
            label = node.get("name").and_then(Value::as_str).map(str::to_string);
        } else if kind.is_some_and(|kind| SYMBOL_KINDS_OF_FUNCTIONS.contains(&kind)) {
            *found = Some((node, label.clone()));
        }
        if let Some(children) = node.get("children") {
            around(children, start, end, label, found);
        }
    }
}

/// The self type an `impl`'s outline label writes: `Host` of `impl Host`, `Wrapper<T>` of
/// `impl<T> Wrapper<T>`, `Host` of `impl Display for Host`.
fn self_type_of(label: &str) -> Option<String> {
    let rest = label.strip_prefix("impl")?.trim_start();
    let rest = if rest.starts_with('<') {
        let mut depth = 0usize;
        let mut previous = ' ';
        let end = rest.char_indices().find_map(|(at, character)| {
            match character {
                '<' => depth += 1,
                '>' if previous != '-' => depth = depth.saturating_sub(1),
                _ => {}
            }
            previous = character;
            (depth == 0).then_some(at + 1)
        })?;
        rest[end..].trim_start()
    } else {
        rest
    };
    let written = rest.rsplit(" for ").next()?;
    let written = written.split(" where").next()?.trim();
    (!written.is_empty()).then(|| written.to_string())
}

/// Whether the function whose name is at `name_at` of `masked` takes `self` as its first parameter.
fn takes_self(masked: &str, name_at: usize) -> bool {
    let rest = &masked[name_at..];
    let after_name = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    let mut at = after_name;
    let bytes = rest.as_bytes();
    let mut depth = 0usize;
    // Past the generics, to the parenthesis that opens the parameters.
    while at < bytes.len() {
        match bytes[at] {
            b'<' => depth += 1,
            b'>' if at > 0 && bytes[at - 1] != b'-' => depth = depth.saturating_sub(1),
            b'(' if depth == 0 => break,
            _ => {}
        }
        at += 1;
    }
    let parameters = &rest[(at + 1).min(rest.len())..];
    let first = parameters.split([',', ')']).next().unwrap_or_default();
    let pattern = first.split(':').next().unwrap_or_default();
    pattern
        .split_whitespace()
        .last()
        .is_some_and(|word| word.trim_start_matches('&') == "self")
}
