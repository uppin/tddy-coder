//! The result window `Glob` and `Grep` advertise, applied.
//!
//! The sibling of [`crate::read_window`], one tool along. There, `catalog.rs` had offered
//! `offset`/`limit` on `Read` since the catalog was written while `tool_read` honoured neither.
//! Here neither half existed: both searches collected every match into one answer, and the
//! catalog offered no way to ask for fewer. A jailed subagent's `Glob **/*` therefore returned
//! 408,282 bytes — the whole repository tree — into a 32k-token window, and a `Grep` in the same
//! turn returned 180,475.
//!
//! Like the line window, this is the **engine's** window and it deliberately has no default cap:
//! a `Glob` with no `limit` returns every match, because that is what every caller has always
//! received and narrowing it here would silently truncate for all of them. The cap that bounds a
//! *subagent's* context lives one layer up, in `tddy_discovery::subagent`
//! (`DEFAULT_GLOB_PATH_CAP`, `DEFAULT_GREP_MATCH_CAP`) — so the two are not the same defaults and
//! should not be made so: this decides what a tool call returns, that decides how much of it an
//! agent may pull into a model context.

use serde_json::Value;

/// Apply the `limit` result window to `results`, as `{<field>, truncated, <total_field>}`.
///
/// `limit` is the greatest number of results to return and defaults to all of them. `truncated`
/// says whether further results follow the window, and the total is always the search's true size
/// rather than the window's — without both, a capped answer and a complete one are the same JSON,
/// so a caller cannot tell a finished search from a clipped one and a model cannot page.
///
/// A `limit` of `0` returns nothing and still reports the total: a caller asking how big the
/// answer is without paying for it.
pub(crate) fn result_window(
    mut results: Vec<Value>,
    limit: Option<u64>,
    field: &str,
    total_field: &str,
) -> Value {
    let total = results.len();
    let wanted = limit.map_or(usize::MAX, |l| usize::try_from(l).unwrap_or(usize::MAX));
    let truncated = total > wanted;
    results.truncate(wanted);

    serde_json::json!({
        field: results,
        "truncated": truncated,
        total_field: total,
    })
}
