# 2026-10-20 — A failed desktop start says why

**Type:** Fix

`#host-desktops` 6/8 ([#658](https://github.com/uppin/tddy-coder/pull/658)). `lib/screenSharingStartFailure.ts` is new: an `Unauthenticated` start
becomes "The desktop refused the sign-in: <reason>", and any other `ConnectError` becomes its raw
message. `HostDesktopOverlay` shows it in place of "Connecting…". `SessionScreenSharingTab` shows
it under the target's row (`sessions-screen-sharing-start-error-<id>`) and clears it on the next
Start. The host overlay scales input into the reply's `width/height`, which is now the real size.
Tests: `cypress/component/HostDesktopStartFailureAcceptance.cy.tsx` (3). See
[Hosts screen](../hosts-screen.md).
