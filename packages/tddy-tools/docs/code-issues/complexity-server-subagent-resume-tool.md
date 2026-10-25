# complexity: subagent_resume_tool

**Location:** `packages/tddy-tools/src/server.rs` — `subagent_resume_tool`
**Category:** complexity
**Detected:** 2026-10-25 by `/pr-wrap` on PR #727 (`#coagents` 8/8), hand-measured by brace matching
**Metrics:** **67 lines raw**
**Thresholds breached:** length 67 > 60
**Restructure:** `extract_method` — ordinary work, no `/code-restructuring` needed
**Status:** Open — unclaimed

## Measurement history

| Run | Raw lines | Note |
|---|---|---|
| 2026-10-25 | 59 → 67 | first detection — PR #727 crossed it: `deferPermissionToUser` is parsed and folded into the `TurnRequest` (`permission_deferral_arg::parse_permission_deferral`). Deferred to the `#coagents` sweep node by the developer (2026-10-25) |

## What the tool found

The tool body parses its optional arguments in sequence — the resume options, the worktree sync
choice, the deferral and the permission answer among them — eight early-return refusals before it
takes the turn.

## Why it matters here

Every new `subagent_resume` argument adds one more parse-and-refuse block to the same body.

## What would close it

Move the argument parsing into one `fn resume_request_from(args) -> Result<TurnRequest, String>`,
leaving the tool body as parse → `take_a_turn`. Part of the `oversized-file-server` split.

## Verified by hand

2026-10-25 — Counted at the rebased tip of PR #727 and at its base (#726, `518110d17`, 59 lines).
