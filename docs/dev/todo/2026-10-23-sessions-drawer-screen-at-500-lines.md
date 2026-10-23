# SessionsDrawerScreen.tsx reached the 500-line budget under `#visible-failures` 2/2

**Raised:** 2026-10-23 by `/pr-wrap` on #707
**Why deferred:** `SessionsDrawerScreen.tsx` went 498 → 500 lines with two lines of prop wiring (`openProjectSetup` → `SessionDrawer.onSetUpProjectHome`). #706, the base of the same stack, also modifies it, so a split inside the stack turns into a conflict. The developer consented to the deferral.
**What to do:** after the stack lands, split per `packages/tddy-web/docs/code-issues/oversized-file-sessions-drawer-screen.md` (the per-service clients into a `useSessionsDrawerClients` hook, the inline selection/create handlers into a `useSessionsDrawerSelection` hook). In the same split, move `openProjectSetup` out of `useSessionsDrawerInspector` into its own hook, since the inspector hook's name no longer covers navigating to Projects. Mechanical moves only; the drawer specs and `ProjectHomeSetupLinkAcceptance` are the gate.
