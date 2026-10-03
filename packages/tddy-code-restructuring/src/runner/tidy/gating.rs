//! Keeping an import for the test build when only the tests use it.
//!
//! rustc's `unused_imports` fires per compilation unit, so an import that only a
//! `#[cfg(test)] mod tests { use super::*; … }` reads is reported unused by the library unit, and
//! removing it breaks the test unit. A human keeps it as `#[cfg(test)] use …;`; this does the same,
//! and only for an import the failed re-check *names*: a backtick-quoted identifier of an error
//! message equal to the name the `use` bound. Nothing is guessed — a statement whose full path
//! cannot be rebuilt from its own text (a nested group) stays exactly as it was.

use std::collections::{BTreeMap, BTreeSet};

use super::diagnostics::{Diagnostic, Fix, Span};

/// What a round that gates imports does instead of removing every unused one.
pub(super) struct Placement {
    /// The edits to apply: the round's machine fixes, less the statements placed by hand, plus those.
    pub fixes: BTreeMap<String, BTreeSet<Fix>>,
    /// `(file, use path)` of every import now gated for tests.
    pub gated: Vec<(String, String)>,
    /// `warning remains: …` lines for imports left in place because they could not be gated.
    pub declined: Vec<String>,
}

/// Every identifier an `error` diagnostic quotes in its message.
pub(super) fn quoted_in_errors(diagnostics: &[Diagnostic]) -> BTreeSet<String> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.level == "error")
        .flat_map(|diagnostic| diagnostic.message.split('`').skip(1).step_by(2))
        .map(str::to_string)
        .collect()
}

/// The text a span covers in the pre-round bytes of its file.
pub(super) fn text_of<'a>(before: &'a BTreeMap<String, Vec<u8>>, span: &Span) -> &'a str {
    before
        .get(&span.file)
        .and_then(|bytes| bytes.get(span.start..span.end))
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .unwrap_or_default()
        .trim()
}

/// Whether `name` appears in `text` as a whole identifier.
pub(super) fn mentions(text: &str, name: &str) -> bool {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(name).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + name.len()..].chars().next();
        !before.is_some_and(is_word) && !after.is_some_and(is_word)
    })
}

/// The identifier a `use` target introduces: the alias after `as`, else the last path segment.
/// `None` for a glob, `_`, `self` or anything that is not a plain path.
fn bound_name(target: &str) -> Option<&str> {
    let name = match target.rsplit_once(" as ") {
        Some((_, alias)) => alias.trim(),
        None => target.rsplit("::").next()?.trim(),
    };
    let plain = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
    (plain && name != "_" && name != "self").then_some(name)
}

/// The unused imports whose bound name an error of the failed re-check quotes.
pub(super) fn named_by_errors<'a>(
    primaries: &'a BTreeSet<Span>,
    before: &BTreeMap<String, Vec<u8>>,
    quoted: &BTreeSet<String>,
) -> Vec<&'a Span> {
    primaries
        .iter()
        .filter(|span| bound_name(text_of(before, span)).is_some_and(|name| quoted.contains(name)))
        .collect()
}

/// A `use` statement located around an import's span.
struct Statement<'a> {
    /// Start of the line the `use` sits on; the replacement covers `line_start..end`.
    line_start: usize,
    end: usize,
    /// Indentation plus visibility, from the line start up to the `use` keyword.
    head: &'a str,
    /// The path between `use` and `;`.
    body: &'a str,
}

fn statement_around(source: &str, offset: usize) -> Option<Statement<'_>> {
    let keyword = use_keyword_before(source, offset)?;
    let semicolon = offset + source.get(offset..)?.find(';')?;
    if source[keyword..offset].contains(';') {
        return None;
    }
    let line_start = source[..keyword]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    Some(Statement {
        line_start,
        end: semicolon + 1,
        head: &source[line_start..keyword],
        body: source[keyword + 3..semicolon].trim(),
    })
}

/// The offset of the last whole-word `use` before `offset`.
fn use_keyword_before(source: &str, offset: usize) -> Option<usize> {
    let mut end = offset;
    while let Some(at) = source.get(..end)?.rfind("use") {
        let before = source[..at].chars().next_back();
        let after = source[at + 3..].chars().next();
        let word_start = before.is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        if word_start && after.is_some_and(char::is_whitespace) {
            return Some(at);
        }
        end = at;
    }
    None
}

/// Why a statement cannot be gated, as the report words it.
type Reason = &'static str;

/// The indentation and the whole head of a statement that can be gated: whitespace, then an
/// optional visibility, and no `cfg` attribute on it or above it.
fn gateable_head<'a>(
    source: &str,
    statement: &Statement<'a>,
) -> std::result::Result<(&'a str, &'a str), Reason> {
    let head = statement.head;
    if head.contains("cfg")
        || attribute_lines_above(source, statement.line_start).any(|l| l.contains("cfg"))
    {
        return Err("it carries a cfg attribute");
    }
    let indent_len = head.len() - head.trim_start().len();
    let visibility = head.trim();
    let plain = visibility.is_empty()
        || visibility == "pub"
        || (visibility.starts_with("pub(") && visibility.ends_with(')'));
    if !plain {
        return Err("unrecognised use statement");
    }
    Ok((&head[..indent_len], head))
}

/// The `#[...]` lines directly above a line, nearest first.
fn attribute_lines_above(source: &str, line_start: usize) -> impl Iterator<Item = &str> {
    source[..line_start]
        .lines()
        .rev()
        .take_while(|line| line.trim_start().starts_with("#["))
}

/// How a statement is rewritten: its replacement text and the paths it gates.
struct Rewrite {
    replacement: String,
    gated: Vec<String>,
}

/// Rewrite one statement: members in `gated` move to `#[cfg(test)]` items of their own, members in
/// `removed` go, the rest stay.
fn rewrite(
    source: &str,
    statement: &Statement<'_>,
    removed: &BTreeSet<&str>,
    gated: &BTreeSet<&str>,
) -> std::result::Result<Rewrite, Reason> {
    let (indent, head) = gateable_head(source, statement)?;
    if !statement.body.contains('{') {
        let path = statement.body;
        let replacement = format!("{indent}#[cfg(test)]\n{head}use {path};");
        return Ok(Rewrite {
            replacement,
            gated: vec![path.to_string()],
        });
    }
    rewrite_members(indent, head, statement.body, removed, gated)
}

/// Rewrite a single-level group: the kept members stay, each gated one gets an item of its own.
fn rewrite_members(
    indent: &str,
    head: &str,
    body: &str,
    removed: &BTreeSet<&str>,
    gated: &BTreeSet<&str>,
) -> std::result::Result<Rewrite, Reason> {
    let (prefix, members) = split_group(body)?;
    let known: BTreeSet<&str> = members.iter().copied().collect();
    if removed
        .iter()
        .chain(gated)
        .any(|text| !known.contains(text))
    {
        return Err("unrecognised use statement");
    }
    let path_of = |member: &str| match (prefix, member) {
        ("", member) => member.to_string(),
        (prefix, "self") => prefix.to_string(),
        (prefix, member) => format!("{prefix}::{member}"),
    };
    let kept: Vec<&str> = members
        .iter()
        .copied()
        .filter(|member| !removed.contains(member) && !gated.contains(member))
        .collect();
    let mut items = Vec::new();
    if let Some(kept) = kept_statement(head, prefix, &kept) {
        items.push(kept);
    }
    let gated_paths: Vec<String> = members
        .iter()
        .filter(|m| gated.contains(*m))
        .map(|m| path_of(m))
        .collect();
    items.extend(
        gated_paths
            .iter()
            .map(|path| format!("{indent}#[cfg(test)]\n{head}use {path};")),
    );
    Ok(Rewrite {
        replacement: items.join("\n"),
        gated: gated_paths,
    })
}

/// What stays of a group after its removed and gated members are taken out.
fn kept_statement(head: &str, prefix: &str, kept: &[&str]) -> Option<String> {
    let joined = match (prefix, kept) {
        (_, []) => return None,
        ("", [only]) => (*only).to_string(),
        (prefix, [only]) if !only.contains("::") => format!("{prefix}::{only}"),
        ("", many) => format!("{{{}}}", many.join(", ")),
        (prefix, many) => format!("{prefix}::{{{}}}", many.join(", ")),
    };
    Some(format!("{head}use {joined};"))
}

/// The prefix and members of a single-level group `a::b::{C, D as E}`; a nested group, or text
/// holding a comment, is not rebuilt.
fn split_group(body: &str) -> std::result::Result<(&str, Vec<&str>), Reason> {
    let brace = body.find('{').ok_or("unrecognised use statement")?;
    let nested = body.matches('{').count() > 1 || body.contains("//") || body.contains("/*");
    if nested {
        return Err(if body.matches('{').count() > 1 {
            "nested group"
        } else {
            "unrecognised use statement"
        });
    }
    let inner = body[brace + 1..]
        .strip_suffix('}')
        .ok_or("unrecognised use statement")?;
    let before_brace = body[..brace].trim_end();
    let prefix = match before_brace.strip_suffix("::") {
        Some(prefix) => prefix,
        None if before_brace.is_empty() => "",
        None => return Err("unrecognised use statement"),
    };
    let members = inner
        .split(',')
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .collect();
    Ok((prefix, members))
}

/// Redo a round's edits with the `matched` imports gated instead of removed.
pub(super) fn place(
    fixes: &BTreeMap<String, BTreeSet<Fix>>,
    primaries: &BTreeSet<Span>,
    before: &BTreeMap<String, Vec<u8>>,
    matched: &[&Span],
) -> Placement {
    let mut placement = Placement {
        fixes: fixes.clone(),
        gated: Vec::new(),
        declined: Vec::new(),
    };
    let mut statements: BTreeMap<(&str, usize), Vec<&Span>> = BTreeMap::new();
    for span in matched {
        let source = source_of(before, &span.file);
        let start = statement_around(source, span.start).map_or(span.start, |s| s.line_start);
        statements
            .entry((span.file.as_str(), start))
            .or_default()
            .push(span);
    }
    for ((file, _), group) in &statements {
        // A refusal is already reported in `declined`, and the statement stays as it was.
        let _ = place_statement(&mut placement, before, primaries, file, group);
    }
    placement
}

fn source_of<'a>(before: &'a BTreeMap<String, Vec<u8>>, file: &str) -> &'a str {
    before
        .get(file)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .unwrap_or_default()
}

/// Place one statement's matched imports; on refusal the statement is left exactly as it was and
/// the report says why.
fn place_statement(
    placement: &mut Placement,
    before: &BTreeMap<String, Vec<u8>>,
    primaries: &BTreeSet<Span>,
    file: &str,
    group: &[&Span],
) -> std::result::Result<(), Reason> {
    let source = source_of(before, file);
    let outcome = rewrite_group(source, before, primaries, group);
    let (start, end) = statement_range(source, group[0]);
    if let Some(edits) = placement.fixes.get_mut(file) {
        edits.retain(|fix| fix.end <= start || fix.start >= end);
    }
    match outcome {
        Ok(rewrite) => {
            placement
                .fixes
                .entry(file.to_string())
                .or_default()
                .insert(Fix {
                    file: file.to_string(),
                    start,
                    end,
                    replacement: rewrite.replacement,
                });
            placement.gated.extend(
                rewrite
                    .gated
                    .into_iter()
                    .map(|path| (file.to_string(), path)),
            );
            Ok(())
        }
        Err(reason) => {
            for span in group {
                placement.declined.push(format!(
                    "warning remains: unused import {} could not be gated for tests ({reason})",
                    text_of(before, span)
                ));
            }
            Err(reason)
        }
    }
}

fn statement_range(source: &str, span: &Span) -> (usize, usize) {
    statement_around(source, span.start).map_or((span.start, span.start), |s| (s.line_start, s.end))
}

fn rewrite_group(
    source: &str,
    before: &BTreeMap<String, Vec<u8>>,
    primaries: &BTreeSet<Span>,
    group: &[&Span],
) -> std::result::Result<Rewrite, Reason> {
    let statement = statement_around(source, group[0].start).ok_or("unrecognised use statement")?;
    let gated: BTreeSet<&str> = group.iter().map(|span| text_of(before, span)).collect();
    let removed: BTreeSet<&str> = primaries
        .iter()
        .filter(|span| span.file == group[0].file)
        .filter(|span| span.start >= statement.line_start && span.end <= statement.end)
        .map(|span| text_of(before, span))
        .filter(|text| !gated.contains(text))
        .collect();
    rewrite(source, &statement, &removed, &gated)
}
