# 2026-10-17 — Host selection survives reload; selector waiting state

**Type:** Fix

- `routing/selectedHost.ts`
  - `resolveSelectedDaemonInstanceId` is choice-wins: URL, then stored, unconditionally; then serving
    if listed; then first listed.
  - Adds `findListedDaemon`, and the per-tab label memory (`rememberDaemonLabels`,
    `readRememberedDaemonLabel`, `REMEMBERED_DAEMON_LABELS_STORAGE_KEY`) stored as
    `web_state.RememberedDaemonLabels` protobuf JSON.
- `rpc/selectedDaemon.tsx`
  - Writes `?host=` back only when the URL carries none.
  - The empty-directory guard is dropped.
  - The unused `routing/selectedHost` re-exports are removed.
  - The context gains `selectedLabel`, from the new `rpc/useSelectedDaemonLabel.ts`.
- `components/shell/DaemonSelector.tsx`: a waiting daemon (selected, not listed) gets a disabled
  option and a `WaitingDot` (`data-testid="daemon-selector-waiting"`).
- `src/test-utils/sessionStorage.ts` replaces the copies of `withMockedSessionStorage` in the bun tests.

Tests:
- `cypress/component/HostSelectionSurvivesReload.cy.tsx` and `DaemonSelectorWaiting.cy.tsx`.
- `src/routing/rememberedDaemonLabels.test.ts`.
- Precedence cases in `src/routing/selectedHost.test.ts` and
  `SelectedHostUrlStateAcceptance.cy.tsx` now pin waiting instead of fallback.

See [host-directory.md § Context surface](../host-directory.md#context-surface) and
[URL state routing § Host in the URL](../../../../docs/ft/web/url-state-routing.md#host-in-the-url).
Cross-package entry:
[2026-10-17-host-selection-survives-reload.md](../../../../docs/dev/changesets/2026-10-17-host-selection-survives-reload.md).
