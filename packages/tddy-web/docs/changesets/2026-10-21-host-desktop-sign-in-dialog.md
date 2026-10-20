# 2026-10-21 — Host desktop sign-in dialog

**Type:** Feature

`#host-desktops` 8/8 ([#660](https://github.com/uppin/tddy-coder/pull/660)). `HostDesktopSignInDialog` shows fields by method, a method switcher
with the strongest method preselected, and Remember. It sends a protobuf `DesktopCredentialAnswer`
encrypted with `encryptForHost`, which now takes bytes. `HostKeyContinuityNotices` is extracted
from `HostPassphraseDialog`, which loses `allowEmpty` and `wording` and answers only the key
passphrase. `HostDesktopOverlay` inspects first (`signInAndStart`), answers a `NONE` desktop
itself, and re-asks on a refused answer (`startUntilSignedIn`). See [hosts-screen.md](../hosts-screen.md).
Tests: `HostDesktopSignInAcceptance.cy.tsx` (8). `HostDesktopConnectAcceptance.cy.tsx` now
answers `DESKTOP_CREDENTIALS`, and its blank-answer test is removed because a `NONE` desktop gets
no form.
