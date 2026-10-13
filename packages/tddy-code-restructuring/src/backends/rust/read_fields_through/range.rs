//! The lexical half of `read_fields_through`: which mode `expr` selects, every `self` the range
//! names (in code, comments and strings masked), and the refusals readable from the text alone
//! (RS1-RS4).
//!
//! `pub(crate)` so `#reshape` 18's `detach_method` can reuse the self-mode sites and the shadowing
//! refusal.

use std::ops::Range;

use super::super::early_return::masked_to_code;

/// Which `self` uses a run rebinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// `expr` is a view of the host's fields: only `self.<field>` reads are rebound.
    Fields,
    /// `expr` is `self` itself: every `self` is rebound, and `Self` becomes the impl's self type.
    Receiver,
}

/// The mode `expr` selects: [`Mode::Receiver`] for `self`, `&*self` and `&mut *self` (compared as
/// tokens), [`Mode::Fields`] for anything else.
pub(crate) fn mode_of(expr: &str) -> Mode {
    let tokens: String = expr.split_whitespace().collect();
    match tokens.as_str() {
        "self" | "&*self" | "&mut*self" => Mode::Receiver,
        _ => Mode::Fields,
    }
}

/// One use of `self` (or of `Self`) in the range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelfSite {
    /// The byte offset of the `self` (or `Self`) token in the file's text.
    pub(crate) offset: usize,
    pub(crate) usage: SelfUse,
}

/// How a `self` token is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SelfUse {
    /// `self.<name>` read as a field; `borrowed` is the byte range of a `&` or `&mut ` directly
    /// before it, when there is one and the field is the whole borrowed place.
    Field {
        name: String,
        borrowed: Option<Range<usize>>,
    },
    /// `self.<name>(…)`: a method call.
    Method(String),
    /// `self` used bare: passed, returned, compared.
    Bare,
    /// The type keyword `Self`.
    SelfType,
}

/// Every `self` and `Self` token inside `range` of `text`, in source order; comments and strings
/// are not read.
pub(crate) fn self_sites(text: &str, range: Range<usize>) -> Vec<SelfSite> {
    sites_in(&masked_to_code(text), range)
}

/// [`self_sites`] over text already masked to its code.
fn sites_in(masked: &str, range: Range<usize>) -> Vec<SelfSite> {
    let bytes = masked.as_bytes();
    let mut sites = Vec::new();
    let mut at = range.start;
    while at < range.end {
        if !is_word(bytes[at]) {
            at += 1;
            continue;
        }
        let end = word_end(bytes, at).min(range.end);
        let word = &masked[at..end];
        let whole = !at
            .checked_sub(1)
            .is_some_and(|before| is_word(bytes[before]))
            && end == word_end(bytes, at);
        match word {
            "Self" if whole => sites.push(SelfSite {
                offset: at,
                usage: SelfUse::SelfType,
            }),
            "self" if whole && !is_a_path_root(bytes, end) => sites.push(SelfSite {
                offset: at,
                usage: usage_of(masked, at, end),
            }),
            _ => {}
        }
        at = end;
    }
    sites
}

/// How the `self` at `at..end` is used, from what follows and what precedes it.
fn usage_of(masked: &str, at: usize, end: usize) -> SelfUse {
    let bytes = masked.as_bytes();
    let dot = skip_whitespace(bytes, end);
    if bytes.get(dot) != Some(&b'.') {
        return SelfUse::Bare;
    }
    let name_at = skip_whitespace(bytes, dot + 1);
    let name_end = word_end(bytes, name_at);
    if name_end == name_at {
        return SelfUse::Bare;
    }
    let name = masked[name_at..name_end].to_string();
    let after = skip_whitespace(bytes, name_end);
    if bytes.get(after) == Some(&b'(') || masked[after..].starts_with("::<") {
        return SelfUse::Method(name);
    }
    let whole_place = !matches!(bytes.get(after), Some(b'.' | b'[' | b'?'));
    SelfUse::Field {
        name,
        borrowed: whole_place.then(|| borrow_before(masked, at)).flatten(),
    }
}

/// The span of the `&` or `&mut ` that directly precedes the `self` at `at`, when it borrows (and is
/// not a binary `&` or half of `&&`).
fn borrow_before(masked: &str, at: usize) -> Option<Range<usize>> {
    let bytes = masked.as_bytes();
    let mut start = at;
    if masked[..start].ends_with("mut ") && !masked[..start - "mut ".len()].ends_with(is_word_char)
    {
        start -= "mut ".len();
    }
    start = start.checked_sub(1)?;
    if bytes[start] != b'&' {
        return None;
    }
    let before = masked[..start].trim_end();
    let operand_ends = before.ends_with(is_word_char) || before.ends_with([')', ']', '&']);
    let after_keyword = [
        "return", "in", "else", "break", "match", "if", "while", "yield",
    ]
    .iter()
    .any(|keyword| {
        before.ends_with(keyword) && !before[..before.len() - keyword.len()].ends_with(is_word_char)
    });
    (!operand_ends || after_keyword).then_some(start..at)
}

/// Whether the `self` ending at `end` roots a module path (`self::item`), which is not a receiver.
fn is_a_path_root(bytes: &[u8], end: usize) -> bool {
    bytes[end..].starts_with(b"::")
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_word_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn word_end(bytes: &[u8], at: usize) -> usize {
    bytes[at..]
        .iter()
        .position(|byte| !is_word(*byte))
        .map_or(bytes.len(), |len| at + len)
}

fn skip_whitespace(bytes: &[u8], at: usize) -> usize {
    bytes[at.min(bytes.len())..]
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .map_or(bytes.len(), |len| at + len)
}

/// RS1-RS4, each naming its line: a range that does not start a statement, a range that names no
/// `self`, (field mode only) a method call or a bare `self`, and a `name` the enclosing function
/// already writes.
pub(crate) fn lexical_refusals(
    text: &str,
    range: Range<usize>,
    name: &str,
    mode: Mode,
) -> Vec<String> {
    let masked = masked_to_code(text);
    let sites = sites_in(&masked, range.clone());
    let mut refusals = Vec::new();
    refusals.extend(not_a_statement_start(text, &masked, range.start));
    refusals.extend(names_no_self(&sites, mode));
    if mode == Mode::Fields {
        refusals.extend(calls_or_bare_self(text, &sites, name));
    }
    refusals.extend(shadowed(text, &masked, range.start, name));
    refusals
}

/// RS1: the code before the range ends a statement or opens or closes a block.
fn not_a_statement_start(text: &str, masked: &str, start: usize) -> Option<String> {
    let before = masked[..start].trim_end();
    if before.ends_with([';', '{', '}']) {
        return None;
    }
    let line = line_of(text, start);
    let column = text[text[..start].rfind('\n').map_or(0, |at| at + 1)..start]
        .chars()
        .count()
        + 1;
    Some(format!(
        "the range starts at line {line} col {column}, which is not the start of a statement: a \
         `let` cannot be inserted before it"
    ))
}

/// RS2: there is nothing to rebind. In receiver mode a `Self` is something to rebind.
fn names_no_self(sites: &[SelfSite], mode: Mode) -> Option<String> {
    let rebinds = sites
        .iter()
        .any(|site| mode == Mode::Receiver || !matches!(site.usage, SelfUse::SelfType));
    (!rebinds).then(|| "the range names no `self`: there is nothing to rebind".to_string())
}

/// RS3: in field mode a method call or a bare `self` cannot be read through the state value.
fn calls_or_bare_self(text: &str, sites: &[SelfSite], name: &str) -> Option<String> {
    let calls: Vec<String> = sites
        .iter()
        .filter_map(|site| match &site.usage {
            SelfUse::Method(method) => Some(format!(
                "`self.{method}(…)` at line {}",
                line_of(text, site.offset)
            )),
            _ => None,
        })
        .collect();
    let bare: Vec<String> = sites
        .iter()
        .filter(|site| site.usage == SelfUse::Bare)
        .map(|site| line_of(text, site.offset).to_string())
        .collect();
    if calls.is_empty() && bare.is_empty() {
        return None;
    }
    let mut uses = Vec::new();
    if !calls.is_empty() {
        uses.push(format!("calls {}", calls.join(", ")));
    }
    if !bare.is_empty() {
        uses.push(format!("uses `self` bare at line {}", bare.join(", ")));
    }
    Some(format!(
        "the range {}: read through `{name}` only fields, or rebind every `self` with \
         `\"expr\": \"self\"`",
        uses.join(" and ")
    ))
}

/// RS4: `name` is written in the enclosing function, so a new binding would shadow it. A `.name`
/// is a field or a method, which no local shadows.
fn shadowed(text: &str, masked: &str, start: usize, name: &str) -> Option<String> {
    let body = enclosing_function(masked, start)?;
    let bytes = masked.as_bytes();
    let at = masked[body.clone()]
        .match_indices(name)
        .find_map(|(at, _)| {
            let at = body.start + at;
            let before = at.checked_sub(1).map(|before| bytes[before]);
            let after = bytes.get(at + name.len()).copied();
            let whole = !before.is_some_and(is_word) && !after.is_some_and(is_word);
            let member = masked[..at].trim_end().ends_with('.');
            (whole && !member).then_some(at)
        })?;
    Some(format!(
        "`{name}` is already written at line {} of this function: the new binding would shadow it",
        line_of(text, at)
    ))
}

/// The span of the function holding `offset`: from after its name through its closing brace, parameters
/// included.
fn enclosing_function(masked: &str, offset: usize) -> Option<Range<usize>> {
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    for at in (0..offset).rev() {
        match bytes[at] {
            b'}' => depth += 1,
            b'{' if depth > 0 => depth -= 1,
            b'{' => {
                let header_start = masked[..at].rfind([';', '{', '}']).map_or(0, |end| end + 1);
                let header = &masked[header_start..at];
                if let Some(name_end) = after_fn_name(header) {
                    let close = matching_brace(bytes, at)?;
                    return Some(header_start + name_end..close);
                }
            }
            _ => {}
        }
    }
    None
}

/// The offset in `header` just past the name of the `fn` it declares.
fn after_fn_name(header: &str) -> Option<usize> {
    let bytes = header.as_bytes();
    let fn_at = header.match_indices("fn").map(|(at, _)| at).find(|at| {
        !at.checked_sub(1)
            .is_some_and(|before| is_word(bytes[before]))
            && bytes.get(at + 2).is_some_and(u8::is_ascii_whitespace)
    })?;
    let name_at = skip_whitespace(bytes, fn_at + 2);
    Some(word_end(bytes, name_at))
}

fn matching_brace(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
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

/// The one-based line `offset` is on.
fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].matches('\n').count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_METHOD: &str = "impl Host {\n    fn run(&self) -> u32 {\n        let n = self.count;\n        // self.hidden\n        let s = \"self.quoted\";\n        self.helper(n) + Self::BASE\n    }\n}\n";

    fn the_body_of(text: &str) -> Range<usize> {
        let start = text.find("let n").expect("the body is in the text");
        let end = text.find("Self::BASE").expect("the tail is in the text") + "Self::BASE".len();
        start..end
    }

    #[test]
    fn self_and_its_reborrows_select_receiver_mode_and_anything_else_selects_field_mode() {
        let modes: Vec<Mode> = ["self", "&*self", "&mut *self", "self.state()", "self.inner"]
            .into_iter()
            .map(mode_of)
            .collect();

        assert_eq!(
            modes,
            [
                Mode::Receiver,
                Mode::Receiver,
                Mode::Receiver,
                Mode::Fields,
                Mode::Fields
            ]
        );
    }

    #[test]
    fn the_sites_are_a_field_a_method_and_the_self_type_and_never_a_comment_or_a_string() {
        let usages: Vec<SelfUse> = self_sites(A_METHOD, the_body_of(A_METHOD))
            .into_iter()
            .map(|site| site.usage)
            .collect();

        assert_eq!(
            usages,
            [
                SelfUse::Field {
                    name: "count".to_string(),
                    borrowed: None
                },
                SelfUse::Method("helper".to_string()),
                SelfUse::SelfType,
            ]
        );
    }

    #[test]
    fn a_borrowed_field_read_carries_the_range_of_its_ampersand() {
        let text = "fn f(&self) {\n    g(&self.config);\n}\n";
        let start = text.find("g(").expect("the call is in the text");
        let ampersand = text
            .find("&self.config")
            .expect("the borrow is in the text");

        let sites = self_sites(text, start..start + "g(&self.config);".len());

        assert_eq!(
            sites[0].usage,
            SelfUse::Field {
                name: "config".to_string(),
                borrowed: Some(ampersand..ampersand + 1)
            }
        );
    }

    #[test]
    fn a_method_call_is_refused_in_field_mode_and_admitted_in_receiver_mode() {
        let in_field_mode =
            lexical_refusals(A_METHOD, the_body_of(A_METHOD), "state", Mode::Fields);
        let in_receiver_mode =
            lexical_refusals(A_METHOD, the_body_of(A_METHOD), "backend", Mode::Receiver);

        assert!(
            in_field_mode
                .iter()
                .any(|refusal| refusal.contains("`self.helper(…)` at line 6")),
            "{in_field_mode:?}"
        );
        assert_eq!(in_receiver_mode, Vec::<String>::new());
    }

    #[test]
    fn a_binding_the_function_already_writes_is_refused_as_a_shadow() {
        let refusals = lexical_refusals(A_METHOD, the_body_of(A_METHOD), "n", Mode::Receiver);

        assert!(
            refusals
                .iter()
                .any(|refusal| refusal.contains("`n` is already written at line 3")),
            "{refusals:?}"
        );
    }

    #[test]
    fn a_range_that_starts_mid_statement_is_refused() {
        let start = A_METHOD
            .find("self.count")
            .expect("the read is in the text");

        let refusals = lexical_refusals(A_METHOD, start..start + 10, "state", Mode::Fields);

        assert!(
            refusals
                .iter()
                .any(|refusal| refusal.contains("which is not the start of a statement")),
            "{refusals:?}"
        );
    }
}
