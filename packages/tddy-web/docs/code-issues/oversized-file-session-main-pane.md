# oversized-file: SessionMainPane.tsx

**Location:** `packages/tddy-web/src/components/sessions/SessionMainPane.tsx`
**Category:** oversized-file
**Detected:** 2026-10-04 by the `/pr-wrap` step 3.5 file-length gate on PR #571 (`#live-plan` 12/15)
**Metrics:** **674 lines** (whole file, TS) · budget 500 · **1.35× over** — `wc -l packages/tddy-web/src/components/sessions/SessionMainPane.tsx`
**Thresholds breached:** length 674 > 500
**Status:** Length back under budget — 443 since `b2b9fd1d0` (#663, 2026-10-22), 438 after the activity-sidebar mount (2026-10-23); the 674 figure below is stale. Still unclaimed; close at wrap once the measurement is recorded

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-04 | 674 | first detection — already over before #571 (666 at base `c3567fde`); +8 in #571. Deferred because dependent #572 touches this file (the stack rule: no split lands mid-stack) |
| 2026-10-18 | 675 | +1 in #652 — the `sessionId` prop passed to `WorktreeCodePane` for its index status strip. Split still not attempted |
| 2026-10-18 | 674 | −1 in `feat/code-index-warm-on-demand` (code index warms on demand): the `sessionId` prop passed to `WorktreeCodePane` is removed — the pane's strip reads its own `CodeSession`, keyed by project and worktree. Back to its 2026-10-04 size; still 1.35× over, split not attempted |
| 2026-10-22 (#663, `b2b9fd1d0`) | 443 | The session drawer's logic moved out into the `sessions/use*.ts` hooks (23 files): the file dropped from 674 to 443, under the 500 budget. No row was written at the time |
| 2026-10-23 (`feature/project-home/activity-sidebar`, #698) | 438 | −5: the Agent Activity popover mount is replaced by `SessionWithActivitySidebar` wrapping the body, with the header moved into its `toolbar` prop. Net shrink, so the sidebar's own growth lives in new files |

## What would close it

Not yet analysed. A skim shows the `SessionMainPaneProps` interface (lines 57 to 179, about 120 lines, one client prop per service) ahead of the one exported component `SessionMainPane` (from line 180), so the props and client type aliases are a plausible first seam to move into a sibling file. What the component body would split into was not examined. The `code-restructuring` engine is Rust-only, so this is a hand TS split: green baseline before, mechanical moves only. Land it after #572 and the stack, not inside it.
