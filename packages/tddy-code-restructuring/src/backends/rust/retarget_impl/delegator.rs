//! `retarget_impl`'s forwarding delegator (`variant: "leave_delegator"` with `expr`): the method
//! left in each moved method's slot of the old block so its callers keep compiling, the members
//! that cannot be forwarded (S7), and the delegators nothing calls.
//!
//! The delegator's signature and outer attributes are byte ranges of the moved member; the only
//! text authored is the forwarding body.

use std::ops::Range;

use super::super::early_return::masked_to_code;
use super::super::item_move::sites::Site;
use super::super::seam_refusal;
use crate::Result;

/// Refuse, before any edit, every moved member a forwarding method cannot be written for (S7): a
/// parameter written as a pattern, an associated const or type, a `const fn`. `members` are
/// `(name, text)` of each moved member.
pub(super) fn refuse_unforwardable(members: &[(&str, &str)]) -> Result<()> {
    let refusals: Vec<String> = members
        .iter()
        .filter_map(|(name, text)| unforwardable(name, text))
        .collect();
    if refusals.is_empty() {
        Ok(())
    } else {
        Err(seam_refusal(refusals.join("; ")))
    }
}

/// Why one member cannot be forwarded, when it cannot.
fn unforwardable(name: &str, text: &str) -> Option<String> {
    match syn::parse_str::<syn::ImplItem>(text) {
        Ok(syn::ImplItem::Fn(function)) => {
            if function.sig.constness.is_some() {
                return Some(format!(
                    "`{name}` is a `const fn`, which cannot forward to a non-const call"
                ));
            }
            let parts = Parts::of(text)?;
            parts
                .parameters
                .iter()
                .find(|pattern| !is_a_name(pattern))
                .map(|pattern| {
                    format!(
                        "`{name}` takes `{pattern}` as a pattern, so a forwarding method cannot name it"
                    )
                })
        }
        Ok(syn::ImplItem::Const(_) | syn::ImplItem::Type(_)) => Some(format!(
            "`{name}` is an associated const or type: it has no body to forward"
        )),
        Ok(_) => Some(format!(
            "`{name}` is not a method or an associated function: it has no body to forward"
        )),
        Err(error) => Some(format!(
            "`{name}` cannot be read as an `impl` member ({error}), so no forwarding method can be written for it"
        )),
    }
}

/// Whether a parameter's pattern is one name (`mut` allowed), which a forwarding call can pass on.
fn is_a_name(pattern: &str) -> bool {
    let name = pattern.strip_prefix("mut ").unwrap_or(pattern).trim();
    !name.is_empty()
        && name != "_"
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// The forwarding method for one moved member: its outer attributes but no doc comment, its
/// signature byte for byte, and the body `<expr>.<name>(<argument names>)` (`.await`ed for an
/// `async fn`), or `<new_type>::<name>(…)` for a member with no receiver.
pub(super) fn forwarding_method(member_text: &str, expr: &str, new_type: &str) -> Result<String> {
    let parts = Parts::of(member_text).ok_or_else(|| {
        seam_refusal("a moved member has no signature and body to forward: its text cannot be read")
    })?;
    let arguments: Vec<&str> = parts
        .parameters
        .iter()
        .map(|pattern| pattern.strip_prefix("mut ").unwrap_or(pattern).trim())
        .collect();
    let callee = if parts.receiver {
        format!("{expr}.{}", parts.name)
    } else {
        format!("{new_type}::{}", parts.name)
    };
    let awaited = if parts.asynchronous { ".await" } else { "" };

    let indent = parts.indent;
    let mut out = String::new();
    for attribute in &parts.attributes {
        out.push_str(&format!("{indent}{attribute}\n"));
    }
    out.push_str(&format!("{indent}{}\n", parts.signature));
    out.push_str(&format!(
        "{indent}    {callee}({}){awaited}\n{indent}}}\n",
        arguments.join(", ")
    ));
    Ok(out)
}

/// What a forwarding method reads of a member's text, all but the indentation as slices of it.
struct Parts<'a> {
    /// The outer attributes that are not documentation, each as written.
    attributes: Vec<&'a str>,
    /// From the visibility (or the first qualifier) through the body's opening `{`.
    signature: &'a str,
    name: &'a str,
    asynchronous: bool,
    receiver: bool,
    /// The patterns of the parameters other than the receiver, each as written.
    parameters: Vec<&'a str>,
    /// The whitespace the signature's line starts with.
    indent: &'a str,
}

impl<'a> Parts<'a> {
    fn of(text: &'a str) -> Option<Parts<'a>> {
        let masked = masked_to_code(text);
        let (attributes, signature_start) = attributes_of(text, &masked);
        let body_open = opening_of_final_block(&masked)?;
        let signature = &text[signature_start..=body_open];
        let header = &masked[signature_start..body_open];

        let fn_at = whole_word(header, "fn")?;
        let after_fn = &header[fn_at + "fn".len()..];
        let name_start = after_fn.find(|c: char| !c.is_whitespace())?;
        let name_len = after_fn[name_start..]
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(after_fn.len() - name_start);
        let name_at = signature_start + fn_at + "fn".len() + name_start;
        let name = &text[name_at..name_at + name_len];

        let list = parameter_list(&masked[name_at + name_len..body_open])?;
        let list_at = name_at + name_len + list.start;
        let mut parameters = Vec::new();
        let mut receiver = false;
        for (index, parameter) in split_top_level(&text[list_at..list_at + list.len()])
            .into_iter()
            .enumerate()
        {
            let pattern = parameter
                .split_once(':')
                .filter(|(_, rest)| !rest.starts_with(':'))
                .map_or(parameter, |(head, _)| head)
                .trim();
            let is_receiver = index == 0
                && pattern
                    .split_whitespace()
                    .last()
                    .map(|w| w.trim_start_matches('&'))
                    == Some("self");
            if is_receiver {
                receiver = true;
            } else {
                parameters.push(pattern);
            }
        }

        let line_start = text[..signature_start].rfind('\n').map_or(0, |at| at + 1);
        let indent_len = text[line_start..]
            .find(|c: char| !(c == ' ' || c == '\t'))
            .unwrap_or(0);
        Some(Parts {
            attributes,
            signature,
            name,
            asynchronous: whole_word(header, "async").is_some_and(|at| at < fn_at),
            receiver,
            parameters,
            indent: &text[line_start..line_start + indent_len],
        })
    }
}

/// The outer attributes that open `text` (a doc comment, being blanked in `masked`, is passed over)
/// that are not `#[doc]`, and the offset the signature starts at.
fn attributes_of<'a>(text: &'a str, masked: &str) -> (Vec<&'a str>, usize) {
    let mut attributes = Vec::new();
    let mut at = 0;
    loop {
        at += masked[at..].len() - masked[at..].trim_start().len();
        if !masked[at..].starts_with("#[") {
            return (attributes, at);
        }
        let Some(close) = matching_close(masked, at + 1) else {
            return (attributes, at);
        };
        let attribute = &text[at..=close];
        let is_doc = attribute
            .strip_prefix("#[doc")
            .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'));
        if !is_doc {
            attributes.push(attribute);
        }
        at = close + 1;
    }
}

/// The offset of the `{` that opens the block closed by the last `}` of `masked`.
fn opening_of_final_block(masked: &str) -> Option<usize> {
    let mut depth = 0usize;
    let last = masked.trim_end().len().checked_sub(1)?;
    if masked.as_bytes()[last] != b'}' {
        return None;
    }
    for (at, byte) in masked.bytes().enumerate().take(last + 1).rev() {
        match byte {
            b'}' => depth += 1,
            b'{' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// The offset of the bracket closing the one opened at `open`.
fn matching_close(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, byte) in text.bytes().enumerate().skip(open) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
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

/// The byte span, inside `text`, of what the first parenthesis pair after any generics holds.
fn parameter_list(text: &str) -> Option<Range<usize>> {
    let mut at = text.find(|c: char| !c.is_whitespace())?;
    if text.as_bytes()[at] == b'<' {
        let mut depth = 0usize;
        let mut previous = b' ';
        for (offset, byte) in text.bytes().enumerate().skip(at) {
            match byte {
                b'<' => depth += 1,
                b'>' if previous != b'-' => {
                    depth -= 1;
                    if depth == 0 {
                        at = offset + 1;
                        break;
                    }
                }
                _ => {}
            }
            previous = byte;
        }
        at += text[at..].find(|c: char| !c.is_whitespace())?;
    }
    if text.as_bytes().get(at) != Some(&b'(') {
        return None;
    }
    let close = matching_close(text, at)?;
    Some(at + 1..close)
}

/// `text` split at the commas that sit in no bracket (angle brackets included, a `->` not being
/// one), the empty parts dropped.
fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start, mut previous) = (0usize, 0, ' ');
    for (at, character) in text.char_indices() {
        match character {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            '>' if previous != '-' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&text[start..at]);
                start = at + 1;
            }
            _ => {}
        }
        previous = character;
    }
    parts.push(&text[start..]);
    parts.retain(|part| !part.trim().is_empty());
    parts
}

/// The offset of the first whole-word `word` in `text`.
fn whole_word(text: &str, word: &str) -> Option<usize> {
    let is_word = |byte: Option<&u8>| byte.is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
    text.match_indices(word).map(|(at, _)| at).find(|at| {
        let before = at.checked_sub(1).and_then(|i| text.as_bytes().get(i));
        !is_word(before) && !is_word(text.as_bytes().get(at + word.len()))
    })
}

/// One note per moved member whose every reference lies inside the moved range of `file`: its
/// delegator has no caller in the workspace.
pub(super) fn dead_delegators(
    sites: &[Site],
    file: &str,
    moved: Range<usize>,
    members: &[&str],
    old_type: &str,
) -> Vec<String> {
    members
        .iter()
        .filter(|member| {
            !sites.iter().any(|site| {
                site.name == **member && !(site.path == file && moved.contains(&site.offset))
            })
        })
        .map(|member| {
            format!(
                "retarget_impl: the delegator `{old_type}::{member}` has no caller in the \
                 workspace: remove it, or retarget without leave_delegator"
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_method_forwards_through_the_expression_keeping_its_signature_and_attributes_but_not_its_docs(
    ) {
        let member = "    /// Replace the count.\n    #[inline]\n    pub fn put(&mut self, v: u32) {\n        self.n = v;\n    }\n";

        let delegator = forwarding_method(member, "self.roster", "Roster").expect("it forwards");

        assert_eq!(
            delegator,
            "    #[inline]\n    pub fn put(&mut self, v: u32) {\n        self.roster.put(v)\n    }\n"
        );
    }

    #[test]
    fn an_async_method_is_awaited_and_an_associated_function_forwards_through_the_new_type() {
        let async_member = "    pub async fn load(&self, id: u32) -> u32 {\n        id\n    }\n";
        let associated = "    pub fn build(n: u32) -> Self {\n        Self { n }\n    }\n";

        let forwarded = [
            forwarding_method(async_member, "self.roster()", "Roster").expect("it forwards"),
            forwarding_method(associated, "self.roster()", "Roster").expect("it forwards"),
        ];

        assert_eq!(
            forwarded,
            [
                "    pub async fn load(&self, id: u32) -> u32 {\n        self.roster().load(id).await\n    }\n",
                "    pub fn build(n: u32) -> Self {\n        Roster::build(n)\n    }\n",
            ]
        );
    }

    #[test]
    fn a_pattern_parameter_an_associated_const_and_a_const_fn_are_refused_together() {
        let members = [
            (
                "pair",
                "    fn pair(&self, (a, b): (u32, u32)) -> u32 {\n        a + b\n    }\n",
            ),
            ("LIMIT", "    const LIMIT: u32 = 3;\n"),
            (
                "zero",
                "    const fn zero(&self) -> u32 {\n        0\n    }\n",
            ),
        ];

        let refusal = refuse_unforwardable(&members)
            .map_err(|error| error.to_string())
            .expect_err("none of them can forward");

        assert!(
            refusal.contains("`pair` takes `(a, b)` as a pattern")
                && refusal.contains("`LIMIT` is an associated const or type")
                && refusal.contains("`zero` is a `const fn`"),
            "{refusal}"
        );
    }

    #[test]
    fn a_member_referenced_only_inside_the_moved_range_has_a_dead_delegator() {
        let sites = [
            Site {
                path: "src/host.rs".to_string(),
                offset: 40,
                name: "get".to_string(),
            },
            Site {
                path: "src/caller.rs".to_string(),
                offset: 10,
                name: "put".to_string(),
            },
        ];

        let notes = dead_delegators(&sites, "src/host.rs", 20..80, &["get", "put"], "Host");

        assert_eq!(
            notes,
            ["retarget_impl: the delegator `Host::get` has no caller in the workspace: remove it, or retarget without leave_delegator"]
        );
    }
}
