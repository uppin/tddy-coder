# 2026-10-20 — LiveKit status line on the daemon settings screen
**Type:** Feature

The settings screen's LiveKit block shows what the integration is doing right now — Off, Missing
fields, Connecting…, Connected or Unreachable with the server's error, each with how long it has been
in that state. It reads `GetIntegrationStatus` on open, every 5 s, and after a save. A daemon without
the call (`Unimplemented`) shows no line and is not polled again; any other failed read shows
"Status unavailable: …" on the line and does not touch the save error. `#satellite` 9/13 (#631).

Docs: [daemon-settings-screen.md](../daemon-settings-screen.md),
[Daemon settings § On the settings screen](../../../../docs/ft/daemon/daemon-settings.md#on-the-settings-screen).
Specs: `cypress/component/DaemonSettingsIntegrationStatus.cy.tsx` (7), `DaemonSettingsAcceptance.cy.tsx`
(9, unchanged), `integrationStatusText.test.ts` (6).

Validation: no file over 500 lines; no code issue touched. Findings fixed in the PR: the poll kept
running against a daemon that answered `Unimplemented`; a failed poll wrote into the shared save
error; two cases measured time from fixture creation and could fail on a slow mount.
