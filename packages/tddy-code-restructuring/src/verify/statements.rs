/// Every logical statement that carries behaviour, normalised for what a relocation always changes.
///
/// Indentation is stripped because relocating an item into a module shifts every line of it by a
/// level, which is not a change in meaning. Everything else is kept verbatim — a string literal's
/// contents included, since reindenting inside one *is* a behaviour change.
///
/// A statement that `rustfmt` wrapped over several lines is joined back into one (see
/// [`join_wrapped`]), so a widening or re-point that made a line too long to fit does not read as a
/// loss. `use`, `mod` and the bare braces of a wrapper are excluded because they are exactly what a
/// restructure is *supposed* to add and remove, and so is a `#[cfg(test)]` directly above a `use`
/// (the tidy gates an import only tests use). Comments are deliberately included: a dropped comment
/// is the finding this comparison exists to make.
pub fn statements(text: &str) -> Vec<String> {
    analyse(text).0
}

/// [`statements`], and how many `#[cfg(test)]` lines above a `use` were set aside.
pub(crate) fn analyse(text: &str) -> (Vec<String>, usize) {
    let trimmed: Vec<&str> = text.split('\n').map(str::trim).collect();
    let lines = collapse_use_items(&trimmed);
    let logical: Vec<String> = join_wrapped(&lines)
        .into_iter()
        .filter(|statement| !statement.is_empty())
        .collect();
    let (gated, excused) = drop_use_gates(logical);
    let kept = gated
        .into_iter()
        .filter(|statement| !is_structural(statement))
        .collect();
    (kept, excused)
}

/// The most lines one multi-line `use` item may span before it is left as physical lines.
const USE_LIMIT: usize = 400;

/// Reduce every multi-line `use` item to its first line.
///
/// Only the first line of `use crate::{` … `};` starts with `use`, so without this the members are
/// compared as statements. Imports are the compiler's to check; this comparison is about statements,
/// and a restructure rewrites imports wholesale.
fn collapse_use_items<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let mut out = Vec::with_capacity(lines.len());
    let mut at = 0;
    while at < lines.len() {
        out.push(lines[at]);
        at += use_item_len(lines, at);
    }
    out
}

/// How many lines the `use` item starting at `at` spans (1 when it is not an open multi-line one).
fn use_item_len(lines: &[&str], at: usize) -> usize {
    let first = lines[at];
    if !strip_visibility(first).starts_with("use ") || first.contains(';') {
        return 1;
    }
    let mut depth = brace_depth(first);
    for (offset, next) in lines.iter().enumerate().skip(at + 1).take(USE_LIMIT) {
        depth += brace_depth(next);
        if depth <= 0 && next.ends_with(';') {
            return offset - at + 1;
        }
    }
    1
}

/// Net `{` opened by a line.
fn brace_depth(line: &str) -> i32 {
    line.chars().fold(0, |depth, ch| match ch {
        '{' => depth + 1,
        '}' => depth - 1,
        _ => depth,
    })
}

/// The most physical lines one logical statement is joined from; past it the lines stay physical.
const JOIN_LIMIT: usize = 40;

/// Join a statement `rustfmt` wrapped — one that ends mid-expression with a `(` or `[` still open —
/// into a single line, so it compares equal to its unwrapped form and the other way round.
///
/// Never joins across a blank line or a comment, and gives up (keeping physical lines) at
/// [`JOIN_LIMIT`] or when the brackets never close.
fn join_wrapped(lines: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        match wrapped_span(lines, at) {
            Some(len) => {
                out.push(tidy_joined(&lines[at..at + len]));
                at += len;
            }
            None => {
                out.push(lines[at].to_string());
                at += 1;
            }
        }
    }
    out
}

/// How many lines from `at` make one wrapped statement, when the line at `at` starts one.
fn wrapped_span(lines: &[&str], at: usize) -> Option<usize> {
    let first = lines[at];
    let ends_whole = [";", "{", "}", ",", "=>"]
        .iter()
        .any(|end| first.ends_with(end));
    if first.is_empty() || first.starts_with("//") || ends_whole {
        return None;
    }
    let mut depth = bracket_depth(first);
    if depth <= 0 {
        return None;
    }
    for (offset, next) in lines.iter().enumerate().skip(at + 1).take(JOIN_LIMIT - 1) {
        if next.is_empty() || next.starts_with("//") {
            return None;
        }
        depth += bracket_depth(next);
        if depth <= 0 {
            return Some(offset - at + 1);
        }
    }
    None
}

/// Net `(`/`[` opened by a line, ignoring any inside a string literal.
fn bracket_depth(line: &str) -> i32 {
    let mut depth = 0;
    let mut in_string = false;
    let mut escaped = false;
    for ch in line.chars() {
        match (in_string, escaped, ch) {
            (true, true, _) => escaped = false,
            (true, false, '\\') => escaped = true,
            (_, _, '"') => in_string = !in_string,
            (false, _, '(' | '[') => depth += 1,
            (false, _, ')' | ']') => depth -= 1,
            _ => {}
        }
    }
    depth
}

/// The joined lines as one, spaced as the unwrapped form is: no space inside a bracket, and none of
/// the trailing comma `rustfmt` adds to a wrapped list.
fn tidy_joined(lines: &[&str]) -> String {
    [
        (", )", ")"),
        (", ]", "]"),
        ("( ", "("),
        ("[ ", "["),
        (" )", ")"),
        (" ]", "]"),
    ]
    .iter()
    .fold(lines.join(" "), |text, (from, to)| text.replace(from, to))
}

/// Remove each `#[cfg(test)]` whose next statement is a `use`, counting them.
fn drop_use_gates(statements: Vec<String>) -> (Vec<String>, usize) {
    let mut kept = Vec::with_capacity(statements.len());
    let mut excused = 0;
    for (at, statement) in statements.iter().enumerate() {
        let gates_a_use = statement == "#[cfg(test)]"
            && statements
                .get(at + 1)
                .is_some_and(|next| strip_visibility(next).starts_with("use "));
        if gates_a_use {
            excused += 1;
        } else {
            kept.push(statement.clone());
        }
    }
    (kept, excused)
}

/// The statement without a leading `pub`, `pub(crate)`, `pub(super)` or `pub(in …)`.
pub(crate) fn strip_visibility(statement: &str) -> &str {
    let Some(rest) = statement.strip_prefix("pub") else {
        return statement;
    };
    if let Some(after) = rest.strip_prefix(' ') {
        return after;
    }
    match rest
        .strip_prefix('(')
        .and_then(|inner| inner.split_once(") "))
    {
        Some((_, after)) => after,
        None => statement,
    }
}

/// Whether a line is scaffolding a restructure is allowed to move, add or remove.
fn is_structural(line: &str) -> bool {
    let body = strip_visibility(line);

    matches!(body, "{" | "}" | "};" | "})" | "});")
        || body.starts_with("use ")
        || body.starts_with("mod ")
        || body.starts_with("impl ")
        || body == "impl"
}

/// The statement with every module qualifier — a lowercase `segment::` before an identifier — deleted.
///
/// Uppercase segments are types and stay, so `Foo::new(` and `Bar::new(` remain different. Text in a
/// string literal is left alone.
pub(crate) fn strip_qualifiers(statement: &str) -> String {
    let chars: Vec<char> = statement.chars().collect();
    let mut out = String::with_capacity(statement.len());
    let mut at = 0;
    let mut in_string = false;
    while at < chars.len() {
        if chars[at] == '"' && (at == 0 || chars[at - 1] != '\\') {
            in_string = !in_string;
        }
        let qualifier = if in_string {
            0
        } else {
            qualifier_chain_len(&chars, at)
        };
        if qualifier > 0 {
            at += qualifier;
        } else {
            out.push(chars[at]);
            at += 1;
        }
    }
    out
}

/// Length of the run of `segment::` qualifiers starting at `at` (`crate::a::b::` is one run).
fn qualifier_chain_len(chars: &[char], at: usize) -> usize {
    let mut total = 0;
    let mut chained = false;
    loop {
        let len = qualifier_len(chars, at + total, chained);
        if len == 0 {
            return total;
        }
        total += len;
        chained = true;
    }
}

/// Length of the `segment::` starting at `at`, or 0 when no module qualifier starts there.
fn qualifier_len(chars: &[char], at: usize, chained: bool) -> usize {
    let Some(&first) = chars.get(at) else {
        return 0;
    };
    let starts_token = chained
        || at == 0
        || !(chars[at - 1].is_alphanumeric() || matches!(chars[at - 1], '_' | ':'));
    if !starts_token || !(first.is_ascii_lowercase() || first == '_') {
        return 0;
    }
    let segment = chars[at..]
        .iter()
        .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || **c == '_')
        .count();
    let after = at + segment;
    let separated = chars.get(after) == Some(&':') && chars.get(after + 1) == Some(&':');
    let names_something = chars
        .get(after + 2)
        .is_some_and(|c| c.is_alphabetic() || *c == '_');
    if separated && names_something {
        segment + 2
    } else {
        0
    }
}
