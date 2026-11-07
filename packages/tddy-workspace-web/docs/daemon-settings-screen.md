# Daemon settings screen — integration status lines

Feature doc: [Daemon settings § On the settings screen](../../../docs/ft/daemon/daemon-settings.md#on-the-settings-screen).

`components/settings/DaemonSettingsScreen.tsx` renders two status lines from
`DaemonConfigService.GetIntegrationStatus`: the LiveKit block's
(`data-testid="daemon-settings-livekit-status"`, from the `livekit` entry) and a **Telegram** block
(`data-testid="daemon-settings-telegram-status"`, from the `telegram` entry). The Telegram block has
no fields — Telegram is configured in the YAML file — and is not rendered at all when the daemon
reports no `telegram` entry.

| Piece | Role |
|---|---|
| `integrationStatusText.ts` | Pure: `(IntegrationStatus, now) → "Off" \| "Missing: …" \| "Connecting…" \| "Connected · 3m" \| "Unreachable · 12s: <error>"`. Seconds under a minute, minutes under an hour, then hours; a clock-skewed `since` clamps to `0s`. An unknown state throws rather than guessing a label. Integration-agnostic: both lines use it |
| `readIntegrationStatus` (in the screen) | One read, which sets both lines from the same answer. `Unimplemented` hides both lines and sets a ref that stops every later read for this mount; any other error becomes `Status unavailable: <message>` on the LiveKit line and drops the Telegram line (whether the daemon has a Telegram entry is then unknown) — never the save error |
| Polling | On mount, every `INTEGRATION_STATUS_POLL_MS` (5 s) until unmount, and once after a successful save |

## Tests

- `integrationStatusText.test.ts` (bun) — one case per state plus the duration units.
- `cypress/component/DaemonSettingsIntegrationStatus.cy.tsx` — each LiveKit state, refresh after
  save, `Unimplemented` (no line, form works), a failing read; Telegram's state beside LiveKit's, a
  misconfigured Telegram's missing token, no Telegram line without a `telegram` entry, and the
  Telegram line dropped when a later read fails. The driver's `aDaemonReportingIntegrations(...)`
  answers each read with the next entry, `aDaemonReportingAllOf(...)` reports several entries on
  every read, and `aDaemonReportingOnceThenFailing(...)` fails every read after the first (the last
  two share `anIntegrationStatusAnswer`). All three mint `since` at answer time, so no case depends on how long a mount
  takes. `aTelegramIntegration(...)` builds the `telegram` entry.
