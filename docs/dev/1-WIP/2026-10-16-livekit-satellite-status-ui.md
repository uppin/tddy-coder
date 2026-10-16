# Changeset: LiveKit integration status in the settings screen

**Date**: 2026-10-16
**Status**: 🚧 In Progress
**Type**: Feature (web)
**Stack**: #satellite 9/11. Base: `feature/satellite/coder-participant` (#630).

## Initial Discovery

[2026-10-16-livekit-satellite-status-ui-initial-discovery.md](./2026-10-16-livekit-satellite-status-ui-initial-discovery.md)

## Responsibility

Showing `GetIntegrationStatus`'s `livekit` entry on the daemon settings screen. Owns
`integrationStatusText` (presentation), the status line and polling in `DaemonSettingsScreen`, the
driver's backend stub for the RPC, and the Cypress component spec.

## Boundaries

- Does not change the RPC, the board, or the daemon (#623 owns them).
- Does not add status to host chrome or other screens.
- Strikes nothing from the ratchet (web only).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #623 `satellite-1-extension-ports` | `GetIntegrationStatus` + `IntegrationStatus`/`IntegrationState` in `daemon_config.proto`, and the regenerated `packages/tddy-web/src/gen/daemon_config_pb.ts` (commit `c4147adfb`) | Calls the generated client and renders the response | Change the proto or regenerate it |
| #624–#630 | Nothing consumed | — | — |

## Draft PR contract

```ts
// packages/tddy-web/src/components/settings/integrationStatusText.ts
export function integrationStatusText(status: IntegrationStatus, now: Date): string;
// "Off" | "Missing: url, api_secret" | "Connecting…" | "Connected · 3m" | "Unreachable · 12s: <error>"

// DaemonSettingsScreen: renders <div data-testid="daemon-settings-livekit-status"> from the livekit entry
// daemonSettingsDriver: aDaemonConfigBackend().withIntegrationStatus(...)
```

The first push publishes `integrationStatusText` (body throws `not implemented`), the driver stub,
and the failing Cypress spec and unit test.

## Green wave

**Wave:** 2 of 4
**Greenable independently:** yes. Its only need is #623's published RPC and generated client, and
it stubs the backend.
**Concurrent with:** nodes 2–8
**Blocks:** none

    n1 → n2..n9      n2 → n10      n2, n3, n4, n10 → n11

## Affected Packages

`tddy-web`.

## Summary

The operator sees whether LiveKit is off, misconfigured, connecting, connected or unreachable.

## Scope

- [ ] `integrationStatusText` + unit tests
- [ ] Status line + 5 s polling + refresh after save + `Unimplemented` → hidden
- [ ] Driver stub `withIntegrationStatus`
- [ ] Cypress component spec (one spec file only; full suite is CI's)

## Technical Changes

### State A
- `packages/tddy-web/src/components/settings/DaemonSettingsScreen.tsx`: `client.getConfig` in
  `useEffect` (`:32-47`). The LiveKit block starts at `:88` (`<div className="font-medium">LiveKit</div>`
  at `:89`), followed by the enabled checkbox (`daemon-settings-livekit-enabled`).
- Tests: `cypress/component/DaemonSettingsAcceptance.cy.tsx`, with the driver
  `cypress/support/drivers/daemonSettingsDriver.tsx` (`aDaemonConfigBackend`,
  `aDaemonSettingsScreen(backend)`, …). Unit: `settingsForm.test.ts`.
- Generated client: `src/gen/daemon_config_pb.ts`, regenerated in #623 with `IntegrationStatus`.

### State B
The screen fetches status alongside the config and renders the line under the LiveKit heading.

### Delta
`DaemonSettingsScreen.tsx`, new `integrationStatusText.ts` and test, the driver, and the new spec.

## Implementation Milestones
- [ ] M1 Presentation function + unit tests
- [ ] M2 Screen + spec green

## Testing Plan

The presentation function is pure and covered by bun unit tests. The screen gets one Cypress
component spec with the driver's in-memory backend stub. Only that spec runs locally; the full
Cypress suite (about 50 minutes) runs on CI. `tsc` is not a gate in this repo.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `shows LiveKit as unreachable with the server's error` | `packages/tddy-web/cypress/component/DaemonSettingsIntegrationStatus.cy.tsx` (new) | `UNREACHABLE` → "Unreachable · …: <error>" |
| `shows which LiveKit fields are missing` | same | `MISCONFIGURED` → "Missing: api_secret" |
| `shows LiveKit as connected` | same | `CONNECTED` → "Connected · …" |
| `shows LiveKit as off when it is switched off` | same | `DISABLED` → "Off" |
| `refreshes the status after a save` | same | Two `GetIntegrationStatus` calls; the line shows the second answer |
| `shows no status for a daemon without the status call` | same | `Unimplemented` → no status line; the form still works |

Unit (`integrationStatusText.test.ts`): one case per state, plus the "since" formatting (seconds,
minutes, hours).

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Polling at 5 s, not streaming.** The RPC is unary, and the screen is rarely open. A stream would
  need a new RPC for little gain.
- **`Unimplemented` hides the line.** A desktop talking to an older daemon is not an error worth
  showing.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-16-livekit-satellite-status-ui-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete
