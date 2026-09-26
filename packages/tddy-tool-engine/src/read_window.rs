//! The line window `Read` advertises, applied.
//!
//! `catalog.rs` has offered `offset` and `limit` on `Read` since the catalog was written. A
//! subagent working through a jail has no other way to bound what a read costs it: the Managed
//! `CodebaseAccess` path forwards the window and trusts the answer, so a `Read` that ignored it
//! pulled whole files into a 32k context.

use serde_json::Value;

/// Apply the `offset`/`limit` line window to `content`, as `{content, truncated, total_lines}`.
///
/// `offset` is the 0-based first line to return and defaults to the start of the file; `limit` is
/// the greatest number of lines to return and defaults to all that remain. A window starting past
/// the last line is empty rather than an error, so a caller paging forward can tell running off
/// the end from a failed read. `truncated` says whether further lines follow the window, and
/// `total_lines` is always the file's true length rather than the window's.
///
/// When the window covers the file from its first line to its last, the bytes are returned
/// verbatim — including a trailing newline, which re-joining lines would silently drop. That is
/// what every caller issuing a bare `Read` has always received.
///
/// This is the **engine's** window and it deliberately has no default cap: a bare `Read` returns
/// the whole file, because that is what every caller has always received and narrowing it here
/// would silently truncate for all of them.
///
/// The 200-line cap that bounds a *subagent's* context lives one layer up, in
/// `tddy_discovery::subagent` — `window_content` applies it after a Local read, and the managed
/// path puts it in the request before the file crosses the wire. So the two are **not** the same
/// defaults, and should not be made so: this decides what a tool call returns, that decides how
/// much of it an agent may pull into a model context.
pub(crate) fn line_window(content: &str, offset: Option<u64>, limit: Option<u64>) -> Value {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let start = offset
        .map_or(0, |o| usize::try_from(o).unwrap_or(usize::MAX))
        .min(total_lines);
    let wanted = limit.map_or(usize::MAX, |l| usize::try_from(l).unwrap_or(usize::MAX));
    let end = start.saturating_add(wanted).min(total_lines);
    let truncated = end < total_lines;

    let windowed = if start == 0 && !truncated {
        content.to_string()
    } else {
        lines[start..end].join("\n")
    };

    serde_json::json!({
        "content": windowed,
        "truncated": truncated,
        "total_lines": total_lines,
    })
}
