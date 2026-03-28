# Refactoring plan — Terminate (SIGINT) overlay validation synthesis

**Sources:** `plan/validate-tests-report.md`, `plan/validate-prod-ready-report.md`, `plan/analyze-clean-code-report.md` (2026-03-28).

## Priority 1 — Correctness

1. ~~**Terminate RPC completion vs. failure:**~~ **Done:** `handleSignalSession` accepts optional `{ rethrowOnFailure: true }`; fullscreen `onSessionTerminate` uses it so `GhosttyTerminalLiveKit` only sets `terminate-rpc-complete` on successful RPC.

## Priority 2 — Tests

2. ~~**Negative CT:**~~ **Done:** `GhosttyTerminalLiveKit hides Terminate when session terminate handler is omitted` in `GhosttyTerminalLiveKit.cy.tsx`.

## Priority 3 — Code structure

3. ~~**Unify SignalSession paths:**~~ **Done:** Renamed to `delegateSignalSessionRpc`; `handleSignalSession` calls it for all SignalSession RPCs.

4. ~~**Extract Terminate click handler**~~ **Done:** `runOverlayTerminate` `useCallback` in `GhosttyTerminalLiveKit.tsx`.

## Priority 4 — Observability & hygiene

5. ~~**Gate or remove verbose `console.*`**~~ **Done:** `delegateSignalSessionRpc` / `handleTerminateOverlayClick` / markers use `trace`; `ConnectionScreen` uses `import.meta.env.DEV` for signal logs; Ghostty Terminate path uses `debugLogging`.

6. ~~**Repo hygiene:**~~ **Done:** `.tddy-red-*.txt` in `.gitignore`; temp files removed; `buildId.ts` reverted when not intentionally bumping.

## Non-goals (this pass)

- No new npm dependencies.
- Full E2E suite optional; Bun + targeted Cypress already green.
