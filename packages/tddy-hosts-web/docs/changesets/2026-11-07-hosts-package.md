# 2026-11-07 — Hosts, VMs, host stats and the desktop overlays move here (#web-split 12/17, PR #848)

New package. From `tddy-web`: `components/hosts/` (except `HostsAppPage` and
`HostStartPassphrasePrompt`), `components/vms/` (except `VmsAppPage`), `DemoVmControls`, the host-stats
indicators, `HostStatsFooter`, `ScreenSharingOverlay`, `VncOverlay`, `hostStatsFormat`, `useHostStats`,
`useHostPrompts` and both subscriptions, `encryptForHost`, `subtleCrypto`, `keyboardLock`,
`screenSharingStartFailure`, `hostKey*` — with their suites, 17 Cypress specs and the hosts docs
(`hosts-screen.md`, `host-stats-streaming.md`). `HostsContent` is new (the data half of the old
`HostsAppPage`); `HostStatsFooter` takes `leading`/`trailing` slots; `HostDesktopOverlay` joins the
bridge's room through `MediaPort.usePresenterRoom`.

At wrap:
- `HostDesktopOverlay` rendered the sign-in dialog **before** its "Connecting…" layer, both
  `fixed inset-0 z-50`, so in the app the connecting layer covered the dialog. The dialog now renders
  after it; `HostDesktopSignInAcceptance` loads the app stylesheet and failed 6 of 8 before the fix.
- `HostStatsEventLike` names `memory`, `load` and `cpu.logicalCores`, which `useHostStats` already
  read; `hostKeyPinning.test.ts`'s window swap typed. Typecheck: 13 → 1 error (`tddy-tauri-web`'s).
- Debug logging through `tddyDebug("tddy:hosts")` instead of `console.debug`.
