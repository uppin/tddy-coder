# Evaluation Report

## Summary

Web-only changes add a fullscreen **Terminate** overlay that delegates to the same `ConnectionService.SignalSession` path as Connection Screen SIGINT, persist `sessionId` on connect, and surface RPC errors in fullscreen. Rust workspace is unchanged by this diff; `cargo check` passes. `bun run build` in `packages/tddy-web` passes. Risks are mainly hygiene (generated `buildId.ts` churn, stray test log files) and optional cleanup of verbose `console.*` logging before release.

## Risk Level

low

## Changed Files

- packages/tddy-web/cypress/component/ConnectionScreen.cy.tsx (modified, +63/−0)
- packages/tddy-web/cypress/component/GhosttyTerminalLiveKit.cy.tsx (modified, +98/−1)
- packages/tddy-web/src/buildId.ts (modified, +1/−1)
- packages/tddy-web/src/components/ConnectionScreen.tsx (modified, +54/−1)
- packages/tddy-web/src/components/GhosttyTerminalLiveKit.tsx (modified, +49/−1)
- packages/tddy-web/src/connection/sessionTerminateOverlay.ts (added, +64/−0)
- packages/tddy-web/src/connection/sessionTerminateOverlay.test.ts (added, +26/−0)

## Affected Tests

- packages/tddy-web/src/connection/sessionTerminateOverlay.test.ts: created
  Bun unit tests for signalSessionSigintForDaemonSession and buildTerminateOverlayAriaLabel.
- packages/tddy-web/cypress/component/GhosttyTerminalLiveKit.cy.tsx: updated
  New describe block: fullscreen overlay Terminate (SIGINT) — visibility, aria, RPC intercept, terminate-rpc-complete.
- packages/tddy-web/cypress/component/ConnectionScreen.cy.tsx: updated
  New describe: ConnectionScreen fullscreen terminal — Terminate after connect (intercept ConnectSession + SignalSession).

## Validity Assessment

The diff matches the PRD: Terminate uses **SignalSession** with **SIGINT** and the stored **sessionId** from daemon-backed flows; **Ctrl+C** still sends **0x03** via `enqueueTerminalInput`; **Connect by URL** (`index.tsx`) still passes only `onDisconnect` + `buildId`, so **Terminate** does not appear without `onSessionTerminate`. RPC failures use the same `setError` path with a new fullscreen **connection-error** banner. **data-testid** and **aria-label** requirements are met. Cypress CT decodes protobuf **SignalSession** bodies and asserts **SIGINT**. Remaining gaps are minor: optional negative CT, generated **buildId** noise, and stray untracked log files.

## Build Results

- cargo-workspace: pass (./dev cargo check (workspace) exited 0; no Rust source changes in this diff.)
- tddy-web: pass (./dev bash -c 'cd packages/tddy-web && bun run build' (includes tddy-livekit-web prebuild).)

## Issues

- [low/hygiene] .tddy-red-bun-test-output.txt: Untracked agent test output at repo root; should not be committed.
- [low/hygiene] .tddy-red-cypress-test-output.txt: Untracked Cypress log capture; should not be committed.
- [medium/generated] packages/tddy-web/src/buildId.ts: Timestamp-only change from local `bun run build`; creates noisy diffs unrelated to the feature.
- [low/maintainability] packages/tddy-web/src/connection/sessionTerminateOverlay.ts: `signalSessionSigintForDaemonSession` is exercised by unit tests but production wiring uses `handleSignalSession` in ConnectionScreen directly—two parallel ways to express the same RPC.
- [low/observability] packages/tddy-web/src/components/ConnectionScreen.tsx: Verbose console.debug/info/error in `handleSignalSession` (and similar in GhosttyTerminalLiveKit) will ship to production browsers until a later cleanup phase.
- [low/testing] packages/tddy-web/cypress/component/GhosttyTerminalLiveKit.cy.tsx: PRD acceptance mentioned a negative case (no Terminate without handler); no dedicated CT asserts absence of `terminate-button` when `onSessionTerminate` is omitted.
