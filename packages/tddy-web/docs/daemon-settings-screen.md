# Daemon settings screen — integration status line

Feature doc: [Daemon settings § On the settings screen](../../../docs/ft/daemon/daemon-settings.md#on-the-settings-screen).

`components/settings/DaemonSettingsScreen.tsx` renders the LiveKit block's status line
(`data-testid="daemon-settings-livekit-status"`) from the `livekit` entry of
`DaemonConfigService.GetIntegrationStatus`.

| Piece | Role |
|---|---|
| `integrationStatusText.ts` | Pure: `(IntegrationStatus, now) → "Off" \| "Missing: …" \| "Connecting…" \| "Connected · 3m" \| "Unreachable · 12s: <error>"`. Seconds under a minute, minutes under an hour, then hours; a clock-skewed `since` clamps to `0s`. An unknown state throws rather than guessing a label |
| `readLiveKitStatus` (in the screen) | One read. `Unimplemented` hides the line and sets a ref that stops every later read for this mount; any other error becomes `Status unavailable: <message>` on the line itself, not the save error |
| Polling | On mount, every `INTEGRATION_STATUS_POLL_MS` (5 s) until unmount, and once after a successful save |

## Tests

- `integrationStatusText.test.ts` (bun) — one case per state plus the duration units.
- `cypress/component/DaemonSettingsIntegrationStatus.cy.tsx` — each state, refresh after save,
  `Unimplemented` (no line, form works), a failing read. The driver's
  `aDaemonReportingIntegrations(...)` answers each read with the next entry and mints `since` at
  answer time, so no case depends on how long a mount takes.
