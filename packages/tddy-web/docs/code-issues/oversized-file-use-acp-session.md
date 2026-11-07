# oversized-file: useAcpSession.ts

**Location:** `packages/tddy-web/src/components/chat/useAcpSession.ts`
**Category:** oversized-file
**Detected:** 2026-11-07 by `/pr-wrap` step 3.5 file-length gate on PR #842 (`#web-split` 5/16)
**Metrics:** **536 lines** (non-test source, whole file) · budget 500 · **1.07× over**
**Thresholds breached:** length 536 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-07 | 536 → 536 | PR #842 re-points its generated-code import to `tddy-client-web`; not grown, alert-only |

## What would close it

Split the ACP session hook by concern (stream lifecycle, message reduction, permission prompts) into
sibling hooks, by hand (TypeScript), green baseline first, mechanical moves only. Best done when the
`#web-split` node that moves the chat components lands, so the split is not a cross-node conflict.

## Verified by hand

2026-11-07 — `wc -l` at the wrap of PR #842 (536) and at its base `feature/web-split/livekit-rpc` (536).
