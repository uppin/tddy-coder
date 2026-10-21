//! What `verify` is told about methods the author detached into functions (`detach_method`), so it
//! can account for them: rule R-detach (the changeset's R7).
//!
//! A declaration, never an inference. `--detach TYPE=NAME` implies `--rebind NAME`, because a
//! detached body is a rebound body. **R-detach** then pairs 1:1 into `repointed`: a signature whose
//! receiver became `NAME: &mut TYPE` / `NAME: &TYPE` with its original (`Self` written `TYPE`); a
//! statement holding `R.m(a)` with one holding `m(R, a)`, `M::m(R, a)`, `m(&mut R, a)` or
//! `m(&R, a)`; `Self::m(a)` with `m(a)`. The module a call is re-pointed through is not declared,
//! so a lowercase qualifier is ignored when the two are compared, as the re-point pass does.
//!
//! One lost `impl TYPE {` header per block the run emptied needs no pairing: `statements` already
//! reads every `impl` line as scaffolding, so it is never a lost statement. A detach nobody declared,
//! a changed argument and a receiver changed other than by the borrow stay reported.

use std::collections::BTreeMap;
use std::str::FromStr;

use super::retarget::{is_word_byte, string_end, word_end, Passed};
use super::statements::strip_qualifiers;
use crate::plan::rust_syntax::is_identifier;

/// The most receiver calls of one statement that are tried in each of their three forms.
const MOST_RECEIVER_CALLS: usize = 3;

/// One declared detach: the members of `impl ty` became functions over `name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detach {
    /// The block's self type, one bare path segment.
    pub ty: String,
    /// The parameter the receiver became, one identifier other than `self`.
    pub name: String,
}

impl FromStr for Detach {
    type Err = String;

    /// Read `TYPE=NAME`, the form `--detach` and `VerifyRequest.detaches` carry.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let refused = || {
            format!(
                "`{text}` is not a detach: write `TYPE=NAME`, a bare type name and one identifier \
                 other than `self`"
            )
        };
        let (ty, name) = text.split_once('=').ok_or_else(refused)?;
        if !is_identifier(ty) || !is_identifier(name) || name == "self" {
            return Err(refused());
        }
        Ok(Detach {
            ty: ty.to_string(),
            name: name.to_string(),
        })
    }
}

/// R-detach: pair each lost statement with a gained one equal to what its detached form reads as,
/// one to one, over what R-rebind left.
pub(crate) fn account(
    missing: Vec<String>,
    added: Vec<String>,
    declared: &super::Declared,
) -> Passed {
    if declared.detaches.is_empty() {
        return Passed {
            missing,
            added,
            pairs: 0,
        };
    }

    let mut by_key: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, statement) in added.iter().enumerate() {
        by_key.entry(key(statement)).or_default().push(index);
    }
    let mut consumed = vec![false; added.len()];
    let mut unpaired = Vec::new();
    let mut pairs = 0;
    for statement in missing {
        let found = candidates(&statement, &declared.detaches)
            .into_iter()
            .find_map(|candidate| by_key.get_mut(&key(&candidate))?.pop());
        match found {
            Some(index) => {
                consumed[index] = true;
                pairs += 1;
            }
            None => unpaired.push(statement),
        }
    }

    let added = added
        .into_iter()
        .zip(consumed)
        .filter_map(|(statement, used)| (!used).then_some(statement))
        .collect();
    Passed {
        missing: unpaired,
        added,
        pairs,
    }
}

/// The comparison key: the statement without module qualifiers, its whitespace collapsed.
fn key(statement: &str) -> String {
    strip_qualifiers(statement)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every form a lost statement takes once each declared detach is applied to it: its signature
/// with the receiver rewritten, and its body with `self` and `Self` rewritten and its receiver calls
/// in each of their forms.
fn candidates(statement: &str, detaches: &[super::Detach]) -> Vec<String> {
    let mut found = Vec::new();
    for detach in detaches {
        found.extend(signature_candidate(statement, &detach.ty, &detach.name));
        found.extend(body_candidates(statement, &detach.ty, &detach.name));
    }
    found
}

/// The signature a borrowed receiver becomes: `fn m(&mut self, …) {` as `fn m(NAME: &mut TYPE, …) {`,
/// with `Self` written `TYPE`. `None` for a statement that is no such signature.
fn signature_candidate(statement: &str, ty: &str, name: &str) -> Option<String> {
    let line = statement.trim();
    if !line.ends_with('{') {
        return None;
    }
    let fn_at = line.find("fn ")?;
    let open = fn_at + line[fn_at..].find('(')?;
    let parameters = &line[open + 1..];
    let start = open + 1 + (parameters.len() - parameters.trim_start().len());
    let parameter = &line[start..];
    let borrow = parameter.strip_prefix('&')?;
    let mut cursor = start + 1 + (borrow.len() - borrow.trim_start().len());
    let mut lifetime = String::new();
    if line[cursor..].starts_with('\'') {
        let end = cursor + 1 + word_length(&line[cursor + 1..]);
        lifetime = format!("{} ", &line[cursor..end]);
        cursor = end + (line[end..].len() - line[end..].trim_start().len());
    }
    let mutable = line[cursor..].starts_with("mut ");
    if mutable {
        cursor += "mut ".len();
    }
    let rest = &line[cursor..];
    let after_self = rest.strip_prefix("self")?;
    if after_self.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let receiver = format!(
        "{name}: &{lifetime}{}{ty}",
        if mutable { "mut " } else { "" }
    );
    let rewritten = format!("{}{receiver}{after_self}", &line[..start]);
    Some(map_words(&rewritten, &|word| {
        (word == "Self").then(|| ty.to_string())
    }))
}

/// The body forms of a statement: `self` as `name`, `Self` as `ty`, `ty::m(` as `m(`, and each
/// receiver call `R.m(a)` written in each of its three forms.
fn body_candidates(statement: &str, ty: &str, name: &str) -> Vec<String> {
    let renamed = map_words(statement, &|word| match word {
        "self" => Some(name.to_string()),
        "Self" => Some(ty.to_string()),
        _ => None,
    });
    let plain = drop_type_qualifier(&renamed, ty);
    receiver_forms(&plain)
}

/// `text` with every `ty::` written before a call's name dropped: the same-module form of `Self::m(`.
fn drop_type_qualifier(text: &str, ty: &str) -> String {
    let prefix = format!("{ty}::");
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let (mut cursor, mut at) = (0, 0);
    while let Some(found) = text[at..].find(&prefix) {
        let start = at + found;
        let after = start + prefix.len();
        let at_a_word = start == 0 || !is_word_byte(Some(&(bytes[start - 1] as char)));
        let end = ident_end(bytes, after);
        if at_a_word && end > after && bytes.get(end) == Some(&b'(') {
            out.push_str(&text[cursor..start]);
            cursor = after;
        }
        at = after;
    }
    out.push_str(&text[cursor..]);
    out
}

/// Every form of `text` with its receiver calls (the outermost, at most [`MOST_RECEIVER_CALLS`])
/// written each as `m(R, a)`, `m(&mut R, a)` and `m(&R, a)`, with `text` as it stands among them.
fn receiver_forms(text: &str) -> Vec<String> {
    let calls = receiver_calls(text);
    let mut forms = vec![String::new()];
    let mut cursor = 0;
    for (span, method, receiver, arguments) in calls.iter().take(MOST_RECEIVER_CALLS) {
        let variants = [
            call_form(method, receiver, "", arguments.as_deref()),
            call_form(method, receiver, "&mut ", arguments.as_deref()),
            call_form(method, receiver, "&", arguments.as_deref()),
        ];
        let mut next = Vec::with_capacity(forms.len() * variants.len());
        for form in &forms {
            for variant in &variants {
                next.push(format!("{form}{}{variant}", &text[cursor..span.start]));
            }
        }
        forms = next;
        cursor = span.end;
    }
    forms
        .into_iter()
        .map(|form| format!("{form}{}", &text[cursor..]))
        .collect()
}

/// The top-level receiver calls `R.m(a)` of `text`: the span of each, its method, its receiver
/// (an identifier chain) and its arguments (`None` when there are none).
fn receiver_calls(text: &str) -> Vec<(std::ops::Range<usize>, String, String, Option<String>)> {
    let bytes = text.as_bytes();
    let mut calls = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'.' {
            at += 1;
            continue;
        }
        let method_end = ident_end(bytes, at + 1);
        let open = method_end;
        let receiver_start = receiver_start(bytes, at);
        if method_end == at + 1 || bytes.get(open) != Some(&b'(') || receiver_start == at {
            at += 1;
            continue;
        }
        let Some(close) = matching_paren(bytes, open) else {
            break;
        };
        let inner = text[open + 1..close].trim();
        calls.push((
            receiver_start..close + 1,
            text[at + 1..method_end].to_string(),
            text[receiver_start..at].to_string(),
            (!inner.is_empty()).then(|| text[open + 1..close].to_string()),
        ));
        at = close + 1;
    }
    calls
}

/// One receiver call in one form: `method(` with the receiver lent as `borrow` first, then the
/// arguments exactly as written.
fn call_form(method: &str, receiver: &str, borrow: &str, arguments: Option<&str>) -> String {
    match arguments {
        Some(arguments) => format!("{method}({borrow}{receiver}, {arguments})"),
        None => format!("{method}({borrow}{receiver})"),
    }
}

/// Where the identifier chain ending just before the `.` at `dot` starts.
fn receiver_start(bytes: &[u8], dot: usize) -> usize {
    let mut start = dot;
    loop {
        let word = word_start(bytes, start);
        if word == start {
            return start;
        }
        start = word;
        if start >= 1 && bytes[start - 1] == b'.' {
            start -= 1;
        } else {
            return start;
        }
    }
}

/// The start of the identifier ending at `end`.
fn word_start(bytes: &[u8], end: usize) -> usize {
    let mut start = end;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    start
}

/// The end of the identifier starting at `at`.
fn ident_end(bytes: &[u8], at: usize) -> usize {
    let mut end = at;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    end
}

/// The `)` closing the `(` at `open`.
fn matching_paren(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// The characters of `text` that form an identifier at its start.
fn word_length(text: &str) -> usize {
    text.chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .map(char::len_utf8)
        .sum()
}

/// `text` with each whole identifier `map` rewrites replaced, leaving string literals, comments and
/// lifetimes as they are.
fn map_words(text: &str, map: &dyn Fn(&str) -> Option<String>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        let character = chars[at];
        if character == '"' {
            let end = string_end(&chars, at);
            out.extend(&chars[at..end]);
            at = end;
        } else if character == '/' && chars.get(at + 1) == Some(&'/') {
            out.extend(&chars[at..]);
            break;
        } else if character == '\'' && is_word_byte(chars.get(at + 1)) {
            let end = word_end(&chars, at + 1);
            out.extend(&chars[at..end]);
            at = end;
        } else if is_word_byte(Some(&character)) {
            let end = word_end(&chars, at);
            let word: String = chars[at..end].iter().collect();
            out.push_str(&map(&word).unwrap_or(word));
            at = end;
        } else {
            out.push(character);
            at += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_detach_as_a_bare_type_and_one_identifier_other_than_self() {
        assert_eq!(
            "Host=session".parse::<Detach>(),
            Ok(Detach {
                ty: "Host".to_string(),
                name: "session".to_string()
            })
        );
        assert!("app::Host=session".parse::<Detach>().is_err());
        assert!("Host=self".parse::<Detach>().is_err());
        assert!("Host=fn".parse::<Detach>().is_err());
        assert!("Host".parse::<Detach>().is_err());
    }

    #[test]
    fn a_receiver_call_pairs_with_each_of_its_forms() {
        assert!(body_candidates("h.bump(2)", "Host", "session").contains(&"bump(h, 2)".to_string()));
        assert!(
            body_candidates("h.total()", "Host", "session").contains(&"total(&mut h)".to_string())
        );
    }

    #[test]
    fn a_borrowed_signature_becomes_the_named_parameter_of_its_type() {
        assert_eq!(
            signature_candidate(
                "pub(super) fn bump(&'a mut self, by: u32) -> Self {",
                "Host",
                "session"
            ),
            Some("pub(super) fn bump(session: &'a mut Host, by: u32) -> Host {".to_string())
        );
        assert_eq!(
            signature_candidate("fn step(by: u32) -> u32 {", "Host", "session"),
            None
        );
    }
}
