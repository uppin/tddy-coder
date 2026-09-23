# oversized-file: claude.rs

**Location:** `packages/tddy-agent-backend/src/backend/claude.rs`
**Category:** oversized-file
**Detected:** 2026-09-23 — `/pr-wrap` step 3.5 file-length gate on #522 (`#carve` 12/14), production lines counted to the first `#[cfg(test)]`
**Metrics:** **767 production lines** of 842 total (`#[cfg(test)]` at line 768; 75 test lines) · budget 500 · **1.5× over**
**Thresholds breached:** length 767 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Moved:** 2026-09-23 — from `packages/tddy-core/src/backend/claude.rs` by `#carve` 12/14 (PR #522), with `git mv`
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-23 | 767 | first detection, at the file's new home. The size is inherited from `tddy-core`, not grown: the only production edit is `use crate::workflow::recipe::PermissionHint` → `use tddy_workflow::PermissionHint` (Cut 1), one line for one line. The test module's two recipe imports fold into one, which the production count does not see |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | transcript token readers: `sum_assistant_usage`, `read_claude_transcript_usage`, `read_claude_subagent_usages` (L38–177) | ~140 |
| B | argv and config: `PermissionMode`, `ClaudeInvokeConfig`, `build_claude_args`, `tddy_tools_path`, `create_mcp_config_temp_file`, `goal_to_claude_config` (L182–355) | ~175 |
| C | `impl ClaudeCodeBackend` invoke path (L435–767), which is mostly `invoke_sync` — its own record, `complexity-claude-invoke-sync.md` | ~330 |

Seams A and B leave the file at about 450 lines. `invoke_sync` (330 lines) is the rest, and its complexity record owns that. Prove the seams with `restructure check --deep` before applying them.
