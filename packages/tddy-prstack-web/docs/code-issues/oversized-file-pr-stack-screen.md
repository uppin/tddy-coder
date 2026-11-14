# oversized-file: PrStackScreen.tsx

**Location:** `packages/tddy-prstack-web/src/PrStackScreen.tsx`
**Category:** oversized-file
**Detected:** 2026-10-24 by `/pr-wrap` step 3.5 file-length gate on PR #741 (`#start-progress` 8/8)
**Metrics:** **701 lines** (non-test source, whole file) · budget 500 · **1.40× over**
**Thresholds breached:** length 701 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-24 | 698 | base of PR #741 (`origin/feature/start-progress/progress-ui`), already over budget and unrecorded |
| 2026-10-24 | 704 | +6 in PR #741: a `taskClient` prop (type, doc line, destructure) passed to `CreateSessionDialog`, so Cancel works in the Start-session dialog. **Deferred with the developer's consent, #start-progress 8/8** |
| 2026-11-14 | 701 → 731 | +30 in #886 (`#orchestrators` 9/11): the panel loads by the session's stack address (`getProjectStack`) and every planned-PR call names the address when the session carries one. Already over budget before this PR; the new stack view itself is the separate `ProjectStackView` component, per the changeset's prerequisite |

## What would close it

Hand split (TypeScript): move the Start-session flow (`startSessionNode`, `startSessionInitialValues`,
`handleChildSessionCreated`, the blockers and base-branch derivation, the `CreateSessionDialog`
mount) into a `usePrStackStartSession` hook plus a `PrStackStartSessionDialog` component, leaving
the screen as layout. Green baseline first (`PrStackForceStartAcceptance.cy.tsx` and the other
`PrStack*.cy.tsx` specs), mechanical move only.

## Verified by hand
2026-11-07 — `wc -l` at the wrap of PR #842 (`#web-split` 5/16): 705 → 705. The PR only re-points hook imports to `tddy-client-web`; still open, not split (later `#web-split` nodes move this file).
2026-11-07 — `wc -l` at the wrap of PR #844 (`#web-split` 7/17): 704 → 702 against its base `feature/web-split/livekit` `bede09610`. The PR re-points the create-session imports to `tddy-session-create-web` (one import line); the passphrase scope is mounted at the app root, not around the dialog. Still open, not split.
2026-11-07 — `wc -l` in the green of `#web-split` 13/16 (`tddy-prstack-web`): 702 → 701 after the cascade onto #844 (705 → 704 before it). The file moved with `git mv` (history follows); the `HostStartPassphraseScope` wrapper left it (the host application provides that scope), so it is two lines shorter on the screen and one longer for an import. Still open, not split.
2026-11-07 — `wc -l` at the wrap of PR #850 (`#web-split` 14/17): 702 → 701 against `feature/web-split/workspace` `abc0b8000`, in `tddy-prstack-web`. Not grown; still open, not split.
