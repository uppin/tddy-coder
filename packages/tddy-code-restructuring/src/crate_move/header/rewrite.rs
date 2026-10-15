//! The edit that re-points one `use` tree, or one `pub(in …)` restriction, of a moved file.

use crate::crate_move::header::written_prefix;
use crate::crate_move::header::Reach;
use crate::crate_move::survey::SurveyedPath;
use crate::crate_move::use_group::{self, Leaf};
use crate::crate_move::{malformed, Result};
use crate::edit::TextEdit;
use crate::{apply::byte_offset, crate_move::manifest_edits};
use std::collections::BTreeSet;

/// The one edit that re-points a `use` tree or a path in a body, if it needs one.
///
/// A tree is replaced by its **prefix**: `crate` in `use crate::{a, b};`, when every member agrees
/// on what that prefix becomes, since there is only one of it to write (Rule P). When they do not,
/// the whole statement is rewritten by [`use_group`] (Rule S).
pub(super) fn rewrite_of(
    file: &str,
    text: &str,
    group: &[SurveyedPath],
    reaches: &[Reach],
) -> Result<Option<TextEdit>> {
    if reaches.iter().any(|reached| reached.narrowed) {
        return narrowed_restriction(text, &group[0]).map(Some);
    }
    let site = group[0].site;
    let at = byte_offset(text, site.line, site.col)?;
    let prefix = written_prefix(&text[at..]);
    let prefix_segments = prefix.split("::").count();

    let mut new_prefixes = BTreeSet::new();
    let mut agree = true;
    for (path, reached) in group.iter().zip(reaches) {
        let segments: Vec<&str> = path.written.split("::").collect();
        if segments.len() < prefix_segments || segments[..prefix_segments].join("::") != prefix {
            return Err(malformed(format!(
                "`{}` is spelled across whitespace or comments, which the path rewrite cannot \
                 address — write it on one line",
                path.written
            )));
        }
        let tail = segments[prefix_segments..].join("::");

        match (&reached.rewritten, tail.is_empty()) {
            (None, _) => new_prefixes.insert(prefix.to_string()),
            (Some(new), true) => new_prefixes.insert(new.clone()),
            (Some(new), false) => match new.strip_suffix(&format!("::{tail}")) {
                Some(new_prefix) => new_prefixes.insert(new_prefix.to_string()),
                None => {
                    agree = false;
                    false
                }
            },
        };
    }
    if !agree || new_prefixes.len() != 1 {
        return split_statement(file, text, at, group, reaches).map(Some);
    }
    let new_prefix = new_prefixes.into_iter().next().unwrap_or_default();
    if new_prefix == prefix {
        return Ok(None);
    }

    let replacement = match keeps_its_name(text, at + prefix.len(), group, prefix, &new_prefix) {
        Some(name) => format!("{new_prefix} as {name}"),
        None => new_prefix,
    };
    Ok(Some(manifest_edits::replacement(
        text,
        at..at + prefix.len(),
        &replacement,
    )))
}

/// The statement holding `group`'s leaves, rewritten as a whole: the members that keep their path
/// stay in the group, and each of the others follows as a `use` of its own.
fn split_statement(
    file: &str,
    text: &str,
    at: usize,
    group: &[SurveyedPath],
    reaches: &[Reach],
) -> Result<TextEdit> {
    let span = use_group::statement_containing(text, at).ok_or_else(|| {
        malformed(format!(
            "`{}` is in no `use` statement the move can read — write one `use` per path",
            group[0].written
        ))
    })?;
    let leaves: Vec<Leaf> = group
        .iter()
        .zip(reaches)
        .map(|(path, reached)| Leaf {
            written: path.written.clone(),
            rewritten: reached
                .rewritten
                .clone()
                .unwrap_or_else(|| path.written.clone()),
        })
        .collect();
    use_group::statement_edit(file, text, span, &leaves)
}

/// The edit that makes `pub(in <path>)` read `pub(crate)`: the module it named stays behind, and
/// `crate` is the narrowest visibility the destination can spell for it.
fn narrowed_restriction(text: &str, path: &SurveyedPath) -> Result<TextEdit> {
    let at = byte_offset(text, path.site.line, path.site.col)?;
    let end = at + path.written.len();
    let opened = text[..at]
        .trim_end()
        .strip_suffix("in")
        .map(str::trim_end)
        .and_then(|before| before.strip_suffix('('))
        .map(str::trim_end)
        .and_then(|before| before.strip_suffix("pub"));
    let closed = text[end..].trim_start().strip_prefix(')');
    match (opened, closed) {
        (Some(before), Some(after)) if text.get(at..end) == Some(path.written.as_str()) => Ok(
            manifest_edits::replacement(text, before.len()..text.len() - after.len(), "pub(crate)"),
        ),
        _ => Err(malformed(format!(
            "`pub(in {})` is spelled across whitespace or comments, which the visibility rewrite \
             cannot address — write it on one line",
            path.written
        ))),
    }
}

/// The name a plain `use` has to go on binding when re-pointing it changes its last segment.
fn keeps_its_name<'a>(
    text: &str,
    after_prefix: usize,
    group: &[SurveyedPath],
    prefix: &'a str,
    new_prefix: &str,
) -> Option<&'a str> {
    let plain_use = group.len() == 1 && !group[0].in_body && group[0].written == prefix;
    let ends_there = text[after_prefix..].trim_start().starts_with(';');
    let was = prefix.rsplit("::").next()?;
    (plain_use && ends_there && new_prefix.rsplit("::").next()? != was).then_some(was)
}
