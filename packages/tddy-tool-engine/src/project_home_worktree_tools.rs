//! `ListWorktrees` and `GetWorktree`: the project's worktrees as the web's Worktrees screen shows
//! them, for an orchestrator agent (`#start-parity` 3/9).
//!
//! A row is `worktree.WorktreeRow` under its own field names, read from the snapshot the screen's
//! `StreamWorktreeStats` opens with ([`tddy_worktree_service::worktrees::project_worktree_snapshot`]).
//! The list is in the screen's order (`tddy-web/src/lib/worktreeList.ts` `sortWorktreesBySize`):
//! biggest first, never-sized last, ties by name. `tests/worktree_tools_parity.rs` holds the field
//! list to the proto descriptor, and the web's `worktreeToolParity.cy.ts` holds the order to the
//! screen's own sort over a shared fixture.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use std::cmp::Ordering;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tddy_worktree_service::session_target::HomeCaller;
use tddy_worktree_service::worktrees::{WorktreeSizeStatus, WorktreeSnapshotRow};

use crate::catalog::ToolDef;
use crate::project_home_tools::{
    target_host_property, tool_def, ProjectHomePort, TARGET_DAEMON_INSTANCE_ID,
};
use crate::ToolOutcome;

pub const LIST_WORKTREES: &str = "ListWorktrees";
pub const GET_WORKTREE: &str = "GetWorktree";

/// The page size a `ListWorktrees` call without a `limit` gets.
pub const WORKTREE_PAGE_DEFAULT: usize = 50;
/// The most rows one `ListWorktrees` page holds; a larger `limit` is clamped to it.
pub const WORKTREE_PAGE_MAX: usize = 200;

/// One worktree, as the Worktrees screen's row has it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeEntry {
    pub path: String,
    pub branch_label: String,
    /// The last size calculated; `None` when the worktree has never been sized.
    pub disk_bytes: Option<u64>,
    pub changed_files: u32,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub updated_at_unix_ms: i64,
    pub stale: bool,
    /// `none`, `calculating` or `cached`.
    pub size_status: String,
    pub size_calculated_at_unix_ms: i64,
}

impl From<WorktreeSnapshotRow> for WorktreeEntry {
    fn from(row: WorktreeSnapshotRow) -> Self {
        let calculated_at = row.size.calculated_at_unix_ms.unwrap_or(0);
        Self {
            path: row.diff.path.to_string_lossy().into_owned(),
            branch_label: row.diff.branch_label,
            disk_bytes: row.size.disk_bytes,
            changed_files: row.diff.changed_files,
            lines_added: row.diff.lines_added,
            lines_removed: row.diff.lines_removed,
            updated_at_unix_ms: calculated_at,
            stale: false,
            size_status: size_status_name(row.size.status).to_string(),
            size_calculated_at_unix_ms: calculated_at,
        }
    }
}

fn size_status_name(status: WorktreeSizeStatus) -> &'static str {
    match status {
        WorktreeSizeStatus::None => "none",
        WorktreeSizeStatus::Calculating => "calculating",
        WorktreeSizeStatus::Cached => "cached",
    }
}

/// How far a round of size calculations has got: rows not calculating, of all rows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SizingProgress {
    pub done: usize,
    pub total: usize,
}

/// One `ListWorktrees` page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeListing {
    pub worktrees: Vec<WorktreeEntry>,
    pub total: usize,
    /// Pass back as `cursor` for the next page; empty when there is none.
    pub next_cursor: String,
    pub has_more: bool,
    /// Set when the `limit` asked for was above [`WORKTREE_PAGE_MAX`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clamped_to: Option<usize>,
    pub sizing: SizingProgress,
}

/// The order the Worktrees screen's `sortWorktreesBySize` puts worktrees in: a worktree with a known
/// size before one without, larger before smaller, then by name, then by path so the order is total.
#[derive(PartialEq, Eq)]
struct SortKey<'a> {
    disk_bytes: Option<u64>,
    name: &'a str,
    path: &'a str,
}

impl Ord for SortKey<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        let by_size = match (self.disk_bytes, other.disk_bytes) {
            (Some(mine), Some(theirs)) => theirs.cmp(&mine),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        by_size
            .then_with(|| locale_compare(self.name, other.name))
            .then_with(|| self.path.cmp(other.path))
    }
}

impl PartialOrd for SortKey<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn sort_key(entry: &WorktreeEntry) -> SortKey<'_> {
    SortKey {
        disk_bytes: entry.disk_bytes,
        name: worktree_name(&entry.path),
        path: &entry.path,
    }
}

/// The directory name of a worktree path: its last non-empty segment, as the screen's `worktreeName`
/// takes it.
fn worktree_name(path: &str) -> &str {
    path.split('/')
        .rev()
        .find(|segment| !segment.is_empty())
        .unwrap_or(path)
}

/// Punctuation and symbols in the order the root collation sorts them: before digits, before letters.
const COLLATION_PUNCTUATION: &str = "_-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";

/// A character's primary collation weight, ignoring case: whitespace, then punctuation, digits,
/// letters, and anything else last.
fn primary_weight(c: char) -> (u8, u32) {
    let lower = c.to_lowercase().next().unwrap_or(c);
    if c.is_whitespace() {
        (0, c as u32)
    } else if let Some(index) = COLLATION_PUNCTUATION.find(lower) {
        (1, index as u32)
    } else if lower.is_numeric() {
        (2, lower as u32)
    } else if lower.is_alphabetic() {
        (3, lower as u32)
    } else {
        (4, lower as u32)
    }
}

/// `localeCompare` for worktree names: letters compare without regard to case, and where two names
/// differ only by case, the lowercase one sorts first. This reproduces the browser's collation for
/// ASCII names, which is what worktree directories are named in; the shared fixture pins it.
fn locale_compare(a: &str, b: &str) -> Ordering {
    a.chars()
        .map(primary_weight)
        .cmp(b.chars().map(primary_weight))
        .then_with(|| {
            a.chars()
                .zip(b.chars())
                .map(|(x, y)| x.is_uppercase().cmp(&y.is_uppercase()))
                .find(|order| order.is_ne())
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| a.cmp(b))
}

/// `entries` in the Worktrees screen's order.
pub fn sort_worktrees_by_size(entries: &mut [WorktreeEntry]) {
    entries.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));
}

/// The prefix every cursor carries; the rest is the sort position it names, hex-encoded JSON.
const CURSOR_PREFIX: &str = "wt1.";

/// The sort position a cursor names: the last row of the page it was issued with.
#[derive(Serialize, Deserialize)]
struct CursorKey {
    disk_bytes: Option<u64>,
    name: String,
    path: String,
}

impl CursorKey {
    fn of(entry: &WorktreeEntry) -> Self {
        Self {
            disk_bytes: entry.disk_bytes,
            name: worktree_name(&entry.path).to_string(),
            path: entry.path.clone(),
        }
    }

    fn sort_key(&self) -> SortKey<'_> {
        SortKey {
            disk_bytes: self.disk_bytes,
            name: &self.name,
            path: &self.path,
        }
    }

    fn encode(&self) -> String {
        let json = serde_json::to_string(self).expect("a cursor key is always serializable");
        format!("{CURSOR_PREFIX}{}", to_hex(json.as_bytes()))
    }
}

fn decode_cursor(cursor: &str) -> Result<CursorKey, String> {
    let refused = || format!("cursor {cursor:?} was not issued by {LIST_WORKTREES}");
    let hex = cursor.strip_prefix(CURSOR_PREFIX).ok_or_else(refused)?;
    let json = from_hex(hex).ok_or_else(refused)?;
    serde_json::from_slice(&json).map_err(|_| refused())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.is_ascii() || text.len() % 2 == 1 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).ok())
        .collect()
}

/// The page size a `limit` asks for, and the limit it was clamped to when it was above the maximum.
fn page_size(limit: Option<usize>) -> Result<(usize, Option<usize>), String> {
    match limit {
        None => Ok((WORKTREE_PAGE_DEFAULT, None)),
        Some(0) => Err("limit must be at least 1".to_string()),
        Some(asked) if asked > WORKTREE_PAGE_MAX => {
            Ok((WORKTREE_PAGE_MAX, Some(WORKTREE_PAGE_MAX)))
        }
        Some(asked) => Ok((asked, None)),
    }
}

/// The page of the sorted `entries` that `cursor` and `limit` ask for, with the sizing progress over
/// all of them.
///
/// `cursor` is empty for the first page, or the `next_cursor` of the page before; any other cursor is
/// refused. A page resumes after the last row of the one before by sort position, so a size that
/// changes between two pages moves that row without repeating or skipping its neighbours.
pub fn page_worktrees(
    mut entries: Vec<WorktreeEntry>,
    cursor: &str,
    limit: Option<usize>,
) -> Result<WorktreeListing, String> {
    let (size, clamped_to) = page_size(limit)?;
    let resume_after = if cursor.is_empty() {
        None
    } else {
        Some(decode_cursor(cursor)?)
    };
    sort_worktrees_by_size(&mut entries);

    let total = entries.len();
    let sizing = SizingProgress {
        done: entries
            .iter()
            .filter(|entry| entry.size_status != "calculating")
            .count(),
        total,
    };
    let start = match &resume_after {
        None => 0,
        Some(key) => {
            let after = key.sort_key();
            entries.partition_point(|entry| sort_key(entry) <= after)
        }
    };
    let end = total.min(start + size);
    let has_more = end < total;
    let worktrees: Vec<WorktreeEntry> = entries.drain(start..end).collect();
    let next_cursor = match worktrees.last() {
        Some(last) if has_more => CursorKey::of(last).encode(),
        _ => String::new(),
    };
    Ok(WorktreeListing {
        worktrees,
        total,
        next_cursor,
        has_more,
        clamped_to,
        sizing,
    })
}

/// The two tools' definitions.
pub fn worktree_tool_defs() -> Vec<ToolDef> {
    vec![
        tool_def(
            LIST_WORKTREES,
            "List this project's worktrees as the Worktrees screen shows them: path, branch, changed files and the last calculated size (disk_bytes, absent when never sized). Largest first, a page at a time. Sizes are not recalculated here.",
            json!({
                "type": "object",
                "properties": {
                    "cursor": { "type": "string", "description": "The next_cursor of the previous page; empty or absent for the first page." },
                    "limit": { "type": "integer", "minimum": 1, "description": "Worktrees per page: 50 by default, at most 200. A larger limit is clamped, and clamped_to says so." },
                    TARGET_DAEMON_INSTANCE_ID: target_host_property()
                }
            }),
        ),
        tool_def(
            GET_WORKTREE,
            "Read one of this project's worktrees as ListWorktrees lists it, with the session that owns it, if any.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The worktree's path, as ListWorktrees lists it." },
                    TARGET_DAEMON_INSTANCE_ID: target_host_property()
                },
                "required": ["path"]
            }),
        ),
    ]
}

#[derive(Deserialize)]
struct ListArgs {
    #[serde(default)]
    cursor: String,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct GetArgs {
    path: String,
}

/// `ListWorktrees` for `home`'s project on this host.
pub(crate) async fn run_list_worktrees(
    port: &dyn ProjectHomePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args: ListArgs = match serde_json::from_value(args) {
        Ok(args) => args,
        Err(e) => return ToolOutcome::err(format!("{LIST_WORKTREES}: invalid arguments: {e}")),
    };
    let entries = match port.list_worktrees(&home.project_id).await {
        Ok(entries) => entries,
        Err(e) => return ToolOutcome::err(format!("{LIST_WORKTREES}: {e:#}")),
    };
    let listing = match page_worktrees(entries, &args.cursor, args.limit) {
        Ok(listing) => listing,
        Err(message) => return ToolOutcome::err(format!("{LIST_WORKTREES}: {message}")),
    };
    match serde_json::to_string(&listing) {
        Ok(json) => ToolOutcome::ok(json),
        Err(e) => ToolOutcome::err(format!("{LIST_WORKTREES}: {e}")),
    }
}

/// `GetWorktree` for `home`'s project on this host: the row `ListWorktrees` holds for `path`, and the
/// session of this host whose worktree it is, if any.
pub(crate) async fn run_get_worktree(
    port: &dyn ProjectHomePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args: GetArgs = match serde_json::from_value(args) {
        Ok(args) => args,
        Err(e) => return ToolOutcome::err(format!("{GET_WORKTREE}: invalid arguments: {e}")),
    };
    let entries = match port.list_worktrees(&home.project_id).await {
        Ok(entries) => entries,
        Err(e) => return ToolOutcome::err(format!("{GET_WORKTREE}: {e:#}")),
    };
    let Some(entry) = entries
        .into_iter()
        .find(|entry| Path::new(&entry.path) == Path::new(&args.path))
    else {
        return ToolOutcome::err(format!(
            "{GET_WORKTREE}: {} is not a worktree of this project",
            args.path
        ));
    };
    let records = match port.target_records().await {
        Ok(records) => records,
        Err(e) => {
            return ToolOutcome::err(format!(
                "{GET_WORKTREE}: cannot read the daemon's sessions: {e:#}"
            ))
        }
    };
    let session_id = records
        .into_iter()
        .find(|record| {
            record.project_id == home.project_id
                && record.worktree.as_deref() == Some(Path::new(&entry.path))
        })
        .map(|record| record.session_id);
    ToolOutcome::ok(json!({ "worktree": entry, "session_id": session_id }).to_string())
}
