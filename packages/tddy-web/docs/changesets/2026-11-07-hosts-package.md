# 2026-11-07 — Hosts, VMs and the desktop overlays moved to `tddy-hosts-web` (#web-split 12/17, PR #848)

`HostsAppPage` is `AppShell` + `tddy-hosts-web`'s `HostsContent`; `VmsAppPage` and
`HostStartPassphrasePrompt` stay and import from the package. `SessionHostStatsFooter` composes the
package's `HostStatsFooter` with the session traffic strip and upload progress (used by
`SessionsDrawerScreen` and `TerminalFileUploadProgressFooter`). Session tabs render the package's
`ScreenSharingOverlay` / `VncOverlay`. Specs that stay as wiring: `HostConnectionAcceptance`,
`HostConnectionGateAcceptance`, `HostDirectoryAcceptance`, `HostLabelIdentityAcceptance`,
`HostSelectionSurvivesReload`, `ScreenSharingInputForwardingAcceptance`,
`ScreenSharingFullscreenAcceptance`; new `HostsAppPageNavigation.cy.tsx` (the shell-menu case).
`hosts-screen.md` and `host-stats-streaming.md` moved to the package's docs.
