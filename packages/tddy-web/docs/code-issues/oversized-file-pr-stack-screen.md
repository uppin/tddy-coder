# oversized-file: PrStackScreen.tsx

**Location:** `packages/tddy-web/src/components/sessions/prstack/PrStackScreen.tsx`
**Category:** oversized-file
**Detected:** 2026-10-24 by `/pr-wrap` step 3.5 file-length gate on PR #741 (`#start-progress` 8/8)
**Metrics:** **704 lines** (non-test source, whole file) · budget 500 · **1.41× over**
**Thresholds breached:** length 704 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-24 | 698 | base of PR #741 (`origin/feature/start-progress/progress-ui`), already over budget and unrecorded |
| 2026-10-24 | 704 | +6 in PR #741: a `taskClient` prop (type, doc line, destructure) passed to `CreateSessionDialog`, so Cancel works in the Start-session dialog. **Deferred with the developer's consent, #start-progress 8/8** |

## What would close it

Hand split (TypeScript): move the Start-session flow (`startSessionNode`, `startSessionInitialValues`,
`handleChildSessionCreated`, the blockers and base-branch derivation, the `CreateSessionDialog`
mount) into a `usePrStackStartSession` hook plus a `PrStackStartSessionDialog` component, leaving
the screen as layout. Green baseline first (`PrStackForceStartAcceptance.cy.tsx` and the other
`PrStack*.cy.tsx` specs), mechanical move only.

## Verified by hand
2026-11-07 — `wc -l` at the wrap of PR #842 (`#web-split` 5/16): 705 → 705. The PR only re-points hook imports to `tddy-client-web`; still open, not split (later `#web-split` nodes move this file).
