# oversized-file: CreateSessionPane.tsx

**Location:** `packages/tddy-session-create-web/src/CreateSessionPane.tsx`
**Category:** oversized-file
**Detected:** 2026-10-24 by `/pr-wrap` step 3.5 file-length gate on PR #740 (`#start-progress` 7/8)
**Metrics:** **526 lines** (non-test source, whole file) · budget 500 · **1.052× over**
**Thresholds breached:** length 526 > 500
**Status:** Open — regressed 2026-11-04 (524 → 526, #801), unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-24 | 498 | base of PR #740 (under budget) |
| 2026-10-24 | 502 | +4 in PR #740: the start's host label (`startingHostId` / `startingHostLabel`) for the step-list heading. Split deferred with the developer's consent, #start-progress 7/8 — the file is shared with the stack's node 8/8 (Cancel), so a split here would cascade into its diff |
| 2026-10-24 | 506 | +4 in PR #741 (`#start-progress` 8/8): the `taskClient` prop, passed with `sessionToken` into `useCreateSessionSubmit`, and `startCancel` handed to `CreateSessionActions`. **Deferred with the developer's consent, #start-progress 8/8** |
| 2026-11-03 | 509 | +7 in PR #814 (`#tmux-agent` 1/3), measured against `origin/master` (`2e4af52ac`, 502): the `runInTmux` / `setRunInTmux` / `runInTmuxUnavailable` values out of `useCreateSessionPlacement`, `runInTmux` into the request's form values, and three props to `CreateSessionClaudeCliFields`. **Not split here**: the file is shared with the stack's nodes 2/3 (#815) and 3/3 (#816), so a split would cascade into both diffs (`/pr-wrap` step 3.5 stack rule, per the stack's orchestration instructions). Split after the `#tmux-agent` stack lands |
| 2026-11-03 | 524 | +15 in PR #816 (`#tmux-agent` 3/3), measured against `origin/feature/tmux-agent/tmux-resume` (509): the shared `runInTmuxToggle` element (`CreateSessionRunInTmuxToggle`, built once and handed to the claude-cli fields), its slot for the other CLI harness types (`showsTheSharedTmuxToggle`), and the import. **Deferred by the developer** (2026-11-03): pre-existing over budget; split after the `#tmux-agent` stack lands |
| 2026-11-04 | 526 | +2 in PR #801 (`#agent-tools` 12/19), against `origin/feature/agent-tools/acp-agents` (524): `selectedAgentRouteProviders` out of `useCreateSessionAgentSelection` and into `useCreateSessionModel`, so the provider picker asks for routes only where the agent has route support. **Not split**: restructuring deferred to the `#agent-tools` sweep node by the developer (2026-11-04) |
| 2026-11-07 | 526 | Moved with the file to `tddy-session-create-web` (`#web-split` 7/16), `git mv`; the measurement is unchanged by a move. **Not split here**: splitting it is not that node's change |

## What would close it

Hand TS split after the `#tmux-agent` stack lands (the `#start-progress` stack has): move the host resolution (`startingHostId`,
`startingHostLabel`, the split-codebase choice) into a `useStartingHost` hook beside `useCreateSessionCatalogs`.
Green baseline before (`CreateSessionPane.cy.tsx`, `CreateSessionStartProgress.cy.tsx`), mechanical move only.

## Verified by hand
2026-11-07 — `wc -l` at the wrap of PR #842 (`#web-split` 5/16): 581 → 581. The PR only re-points hook imports to `tddy-client-web`; still open, not split (later `#web-split` nodes move this file).
2026-11-07 — `wc -l` at the wrap of PR #844 (`#web-split` 7/17): 581 → 581 (the move itself; no line changed beyond import paths). Still open, not split — the `#web-split` changeset for this node excludes it.
