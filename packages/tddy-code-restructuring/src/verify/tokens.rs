use std::collections::BTreeMap;

/// How many distinct tokens the `tokens lost` / `tokens gained` line names.
const TOKENS_SHOWN: usize = 10;

/// Counts of the code tokens of every statement in `statements`.
fn token_counts(statements: &[String]) -> BTreeMap<String, i64> {
    let mut counts = BTreeMap::new();
    for token in statements
        .iter()
        .flat_map(|statement| code_tokens(statement))
    {
        *counts.entry(token).or_default() += 1;
    }
    counts
}

/// `a` minus `b`, keeping only what `a` has more of.
fn surplus(a: &BTreeMap<String, i64>, b: &BTreeMap<String, i64>) -> Vec<(String, i64)> {
    let mut over: Vec<(String, i64)> = a
        .iter()
        .map(|(token, count)| (token.clone(), count - b.get(token).copied().unwrap_or(0)))
        .filter(|(_, count)| *count > 0)
        .collect();
    over.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(&y.0)));
    over
}

fn token_list(tokens: &[(String, i64)]) -> String {
    if tokens.is_empty() {
        return "none".to_string();
    }
    let shown: Vec<String> = tokens
        .iter()
        .take(TOKENS_SHOWN)
        .map(|(token, count)| format!("{token} x{count}"))
        .collect();
    shown.join(", ")
}

/// The one line saying which code tokens the unexplained statements lost and gained, or `None`
/// when their token multisets are equal (nothing changed but layout).
///
/// An author reading 263 unexplained lines cannot see that one callee was renamed; this can.
pub fn token_difference(missing: &[String], added: &[String]) -> Option<String> {
    let lost = token_counts(missing);
    let gained = token_counts(added);
    let (lost_only, gained_only) = (surplus(&lost, &gained), surplus(&gained, &lost));
    if lost_only.is_empty() && gained_only.is_empty() {
        return None;
    }
    Some(format!(
        "verify: tokens lost: {}; tokens gained: {}",
        token_list(&lost_only),
        token_list(&gained_only)
    ))
}

/// The code tokens of a statement, with everything layout-only removed.
///
/// Identifiers, numbers, string and char literals (one opaque token each), a `//` comment (one
/// token, its whole text) and single punctuation characters are kept. Whitespace, `{`, `}`, `,` and
/// `;` are dropped, so block-versus-expression and trailing-comma differences vanish, and so is
/// every lowercase module qualifier `segment::` before an identifier (the re-point rule).
fn code_tokens(statement: &str) -> Vec<String> {
    let chars: Vec<char> = statement.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;
    let mut chain_end = usize::MAX;
    while at < chars.len() {
        let c = chars[at];
        let len = if c.is_whitespace() || "{},;".contains(c) {
            1
        } else if chars[at..].starts_with(&['/', '/']) {
            tokens.push(chars[at..].iter().collect());
            chars.len() - at
        } else if c == '"' {
            push_slice(&mut tokens, &chars, at, string_len(&chars, at))
        } else if c == '\'' && char_literal_len(&chars, at) > 0 {
            push_slice(&mut tokens, &chars, at, char_literal_len(&chars, at))
        } else if c.is_alphabetic() || c == '_' {
            let qualifier = token_qualifier_len(
                &chars,
                at,
                at == chain_end || at == 0 || chars[at - 1] != ':',
            );
            if qualifier > 0 {
                chain_end = at + qualifier;
                qualifier
            } else {
                push_slice(&mut tokens, &chars, at, word_len(&chars, at))
            }
        } else if c.is_ascii_digit() {
            push_slice(&mut tokens, &chars, at, word_len(&chars, at))
        } else {
            push_slice(&mut tokens, &chars, at, 1)
        };
        at += len;
    }
    tokens
}

fn push_slice(tokens: &mut Vec<String>, chars: &[char], at: usize, len: usize) -> usize {
    tokens.push(chars[at..at + len].iter().collect());
    len
}

fn word_len(chars: &[char], at: usize) -> usize {
    chars[at..]
        .iter()
        .take_while(|c| c.is_alphanumeric() || **c == '_')
        .count()
}

/// Length of the string literal opening at `at`, to its closing quote or the end of the text.
fn string_len(chars: &[char], at: usize) -> usize {
    let mut len = 1;
    while let Some(&c) = chars.get(at + len) {
        len += if c == '\\' { 2 } else { 1 };
        if c == '"' {
            break;
        }
    }
    len.min(chars.len() - at)
}

/// Length of the char literal opening at `at`, or 0 when the `'` opens a lifetime.
fn char_literal_len(chars: &[char], at: usize) -> usize {
    match (chars.get(at + 1), chars.get(at + 2), chars.get(at + 3)) {
        (Some('\\'), Some(_), Some('\'')) => 4,
        (Some(c), Some('\''), _) if *c != '\\' => 3,
        _ => 0,
    }
}

/// Length of a lowercase `segment::` qualifier at `at` when it names something and `may_start`.
fn token_qualifier_len(chars: &[char], at: usize, may_start: bool) -> usize {
    if !may_start || !(chars[at].is_ascii_lowercase() || chars[at] == '_') {
        return 0;
    }
    let after = at + word_len(chars, at);
    let separated = chars.get(after) == Some(&':') && chars.get(after + 1) == Some(&':');
    let names_something = chars
        .get(after + 2)
        .is_some_and(|c| c.is_alphabetic() || *c == '_');
    if separated && names_something {
        after + 2 - at
    } else {
        0
    }
}
