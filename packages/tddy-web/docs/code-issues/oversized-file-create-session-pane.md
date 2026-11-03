# oversized-file: CreateSessionPane.tsx

**Location:** `packages/tddy-web/src/components/sessions/CreateSessionPane.tsx`
**Category:** oversized-file
**Detected:** 2026-10-24 by `/pr-wrap` step 3.5 file-length gate on PR #740 (`#start-progress` 7/8)
**Metrics:** **524 lines** (non-test source, whole file) · budget 500 · **1.048× over**
**Thresholds breached:** length 524 > 500
**Status:** Open — regressed 2026-11-03 (509 → 524, #816), unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-24 | 498 | base of PR #740 (under budget) |
| 2026-10-24 | 502 | +4 in PR #740: the start's host label (`startingHostId` / `startingHostLabel`) for the step-list heading. Split deferred with the developer's consent, #start-progress 7/8 — the file is shared with the stack's node 8/8 (Cancel), so a split here would cascade into its diff |
| 2026-10-24 | 506 | +4 in PR #741 (`#start-progress` 8/8): the `taskClient` prop, passed with `sessionToken` into `useCreateSessionSubmit`, and `startCancel` handed to `CreateSessionActions`. **Deferred with the developer's consent, #start-progress 8/8** |
| 2026-11-03 | 509 | +7 in PR #814 (`#tmux-agent` 1/3), measured against `origin/master` (`2e4af52ac`, 502): the `runInTmux` / `setRunInTmux` / `runInTmuxUnavailable` values out of `useCreateSessionPlacement`, `runInTmux` into the request's form values, and three props to `CreateSessionClaudeCliFields`. **Not split here**: the file is shared with the stack's nodes 2/3 (#815) and 3/3 (#816), so a split would cascade into both diffs (`/pr-wrap` step 3.5 stack rule, per the stack's orchestration instructions). Split after the `#tmux-agent` stack lands |
| 2026-11-03 | 524 | +15 in PR #816 (`#tmux-agent` 3/3), measured against `origin/feature/tmux-agent/tmux-resume` (509): the shared `runInTmuxToggle` element (`CreateSessionRunInTmuxToggle`, built once and handed to the claude-cli fields), its slot for the other CLI harness types (`showsTheSharedTmuxToggle`), and the import. **Deferred by the developer** (2026-11-03): pre-existing over budget; split after the `#tmux-agent` stack lands |

## What would close it

Hand TS split after the `#tmux-agent` stack lands (the `#start-progress` stack has): move the host resolution (`startingHostId`,
`startingHostLabel`, the split-codebase choice) into a `useStartingHost` hook beside `useCreateSessionCatalogs`.
Green baseline before (`CreateSessionPane.cy.tsx`, `CreateSessionStartProgress.cy.tsx`), mechanical move only.
