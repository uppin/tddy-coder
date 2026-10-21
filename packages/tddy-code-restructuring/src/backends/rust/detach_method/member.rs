//! What one member of the block is, read from its text: whether it is a function or an associated
//! const or type, its receiver, and whether it is `pub`.
//!
//! Every reading takes the masked text and the file's own text, with absolute byte offsets, so the
//! receiver's span is the span the rewrite replaces.

use std::ops::Range;

use super::scan;

/// The receiver a method takes, as its first parameter is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Receiver {
    /// `&self` or `&mut self`, with the lifetime it names, if any; `span` is the parameter.
    Borrowed {
        mutable: bool,
        lifetime: Option<String>,
        span: Range<usize>,
    },
    /// `self` or `mut self`: the method consumes its receiver.
    ByValue { span: Range<usize> },
    /// `self: T`: a receiver whose type is written out.
    Typed { span: Range<usize> },
}

impl Receiver {
    /// The span of the parameter, whatever form it takes.
    pub(super) fn span(&self) -> &Range<usize> {
        match self {
            Receiver::Borrowed { span, .. }
            | Receiver::ByValue { span }
            | Receiver::Typed { span } => span,
        }
    }
}

/// What a member declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Function,
    /// An associated `const` or `type`: nothing a free function could take the place of.
    ConstOrType,
}

/// Where a member is declared: its keyword's kind, the offset of its name, and for a function the
/// offset of the `(` that opens its parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Declaration {
    pub(super) kind: Kind,
    pub(super) name_at: usize,
    pub(super) open: Option<usize>,
}

/// The declaration of `name` within `span`, or `None` when the text declares no such member.
pub(super) fn declaration_of(masked: &str, span: Range<usize>, name: &str) -> Option<Declaration> {
    let bytes = masked.as_bytes();
    let mut at = span.start;
    while at < span.end.min(bytes.len()) {
        let starts_word = scan::is_word(bytes[at]) && (at == 0 || !scan::is_word(bytes[at - 1]));
        if !starts_word {
            at += 1;
            continue;
        }
        let end = scan::word_end(bytes, at);
        let keyword = &masked[at..end];
        if matches!(keyword, "fn" | "const" | "type") {
            let name_at = scan::skip_whitespace(bytes, end);
            let name_end = scan::word_end(bytes, name_at);
            if name_at > end && &masked[name_at..name_end] == name {
                return Some(if keyword == "fn" {
                    Declaration {
                        kind: Kind::Function,
                        name_at,
                        open: parameters_open(bytes, name_end),
                    }
                } else {
                    Declaration {
                        kind: Kind::ConstOrType,
                        name_at,
                        open: None,
                    }
                });
            }
        }
        at = end;
    }
    None
}

/// The `(` that opens a function's parameters, past its generic parameters, when there is one.
fn parameters_open(bytes: &[u8], name_end: usize) -> Option<usize> {
    let mut at = scan::skip_whitespace(bytes, name_end);
    if bytes.get(at) == Some(&b'<') {
        let mut depth = 0usize;
        loop {
            match bytes.get(at)? {
                b'<' => depth += 1,
                b'>' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            at += 1;
        }
        at = scan::skip_whitespace(bytes, at + 1);
    }
    (bytes.get(at) == Some(&b'(')).then_some(at)
}

/// The receiver of the function declared at `declaration`, read from the first parameter.
pub(super) fn receiver_of(text: &str, masked: &str, declaration: &Declaration) -> Option<Receiver> {
    let bytes = masked.as_bytes();
    let start = scan::skip_whitespace(bytes, declaration.open? + 1);
    if bytes.get(start) == Some(&b'&') {
        let mut at = scan::skip_whitespace(bytes, start + 1);
        let mut lifetime = None;
        if bytes.get(at) == Some(&b'\'') {
            let end = scan::word_end(bytes, at + 1);
            lifetime = Some(text[at..end].to_string());
            at = scan::skip_whitespace(bytes, end);
        }
        let mutable = bytes[at..].starts_with(b"mut") && !is_word_at(bytes, at + 3);
        if mutable {
            at = scan::skip_whitespace(bytes, at + 3);
        }
        return is_self_at(bytes, at).then(|| Receiver::Borrowed {
            mutable,
            lifetime,
            span: start..at + "self".len(),
        });
    }
    let mut at = start;
    if bytes[at..].starts_with(b"mut") && !is_word_at(bytes, at + 3) {
        at = scan::skip_whitespace(bytes, at + 3);
    }
    if !is_self_at(bytes, at) {
        return None;
    }
    let end = at + "self".len();
    let span = start..end;
    if bytes.get(scan::skip_whitespace(bytes, end)) == Some(&b':') {
        Some(Receiver::Typed { span })
    } else {
        Some(Receiver::ByValue { span })
    }
}

/// Whether `self` (the receiver, not a path such as `self::m`) starts at `at`.
fn is_self_at(bytes: &[u8], at: usize) -> bool {
    bytes[at.min(bytes.len())..].starts_with(b"self")
        && !is_word_at(bytes, at + "self".len())
        && !bytes[(at + "self".len()).min(bytes.len())..].starts_with(b"::")
}

/// Whether an identifier byte is at `at`.
fn is_word_at(bytes: &[u8], at: usize) -> bool {
    bytes.get(at).is_some_and(|byte| scan::is_word(*byte))
}

/// Whether the member's text (its attributes and doc comments aside) begins with plain `pub`.
pub(super) fn is_public(masked: &str, span: Range<usize>) -> bool {
    let bytes = masked.as_bytes();
    let mut at = span.start;
    loop {
        at = scan::skip_whitespace(bytes, at);
        if bytes[at.min(bytes.len())..].starts_with(b"#[") {
            match scan::matching_close(bytes, at + 1) {
                Some(close) => at = close + 1,
                None => return false,
            }
        } else {
            break;
        }
    }
    bytes[at.min(bytes.len())..].starts_with(b"pub")
        && bytes
            .get(at + "pub".len())
            .is_some_and(|byte| byte.is_ascii_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: &str = "impl Host {\n    /// Counts.\n    #[inline]\n    pub(crate) fn bump(&'a mut self, by: u32) -> u32 {\n        by\n    }\n    pub fn shown(&self) {}\n    fn with(mut self) -> Self {\n        self\n    }\n    pub(crate) const LIMIT: u32 = 3;\n}\n";

    /// The declaration of `name`, and the span from the start of its line to the end of the text.
    fn member(name: &str) -> (Option<Declaration>, Range<usize>) {
        let masked = scan::masked(HOST);
        let at = HOST
            .find(&format!("fn {name}"))
            .or_else(|| HOST.find(&format!("const {name}")))
            .expect("the member is written");
        let begins = HOST[..at].rfind('\n').map_or(0, |line| line + 1);
        let span = begins..HOST.len();
        (declaration_of(&masked, span.clone(), name), span)
    }

    #[test]
    fn a_borrowed_receiver_keeps_its_lifetime_and_its_mutability() {
        let (declaration, _) = member("bump");
        let masked = scan::masked(HOST);

        let receiver = receiver_of(HOST, &masked, &declaration.expect("bump is declared"));

        assert!(matches!(
            receiver,
            Some(Receiver::Borrowed { mutable: true, lifetime: Some(ref lifetime), .. }) if lifetime == "'a"
        ));
    }

    #[test]
    fn a_consumed_receiver_is_by_value_and_a_shared_one_is_borrowed() {
        let masked = scan::masked(HOST);
        let (with, _) = member("with");
        let (shown, _) = member("shown");

        let by_value = receiver_of(HOST, &masked, &with.expect("with is declared"));
        let shared = receiver_of(HOST, &masked, &shown.expect("shown is declared"));

        assert!(matches!(by_value, Some(Receiver::ByValue { .. })));
        assert!(matches!(
            shared,
            Some(Receiver::Borrowed {
                mutable: false,
                lifetime: None,
                ..
            })
        ));
    }

    #[test]
    fn a_const_is_a_const_and_plain_pub_is_public_while_pub_crate_is_not() {
        let masked = scan::masked(HOST);
        let (limit, _) = member("LIMIT");
        let (_, shown_span) = member("shown");
        let (_, bump_span) = member("bump");

        assert_eq!(limit.map(|d| d.kind), Some(Kind::ConstOrType));
        assert!(is_public(&masked, shown_span));
        assert!(!is_public(&masked, bump_span));
    }
}
