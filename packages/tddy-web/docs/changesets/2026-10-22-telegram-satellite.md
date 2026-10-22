# 2026-10-22 — Telegram status line on the daemon settings screen

**Type:** Feature

Telegram as a satellite; cross-package entry: [2026-10-22-telegram-satellite.md](../../../../docs/dev/changesets/2026-10-22-telegram-satellite.md).

- `DaemonSettingsScreen.tsx` reads both the `livekit` and the `telegram` entries from one
  `GetIntegrationStatus` answer (`readIntegrationStatus`) and shows a **Telegram** block,
  `data-testid="daemon-settings-telegram-status"` (`TEST_IDS.daemonSettingsTelegramStatus`). No
  `telegram` entry, `Unimplemented`, or a failed later read: no Telegram line.
- `DaemonSettingsIntegrationStatus.cy.tsx`: four Telegram cases; driver helpers
  `aTelegramIntegration`, `aDaemonReportingAllOf`, `aDaemonReportingOnceThenFailing` over a shared
  `anIntegrationStatusAnswer`. Spec 11/11.

[daemon-settings-screen.md](../daemon-settings-screen.md).
