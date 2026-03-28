# Analyze Clean Code — Terminate Overlay (SIGINT)

## Executive summary

The Terminate overlay work keeps **clear boundaries**: `GhosttyTerminalLiveKit` stays transport/UI-only and delegates daemon signaling to a parent callback; `ConnectionScreen` owns Connect-RPC and error state. The main quality gaps are **two parallel representations of “call SignalSession”** (`signalSessionSigintForDaemonSession` in `packages/tddy-web/src/connection/sessionTerminateOverlay.ts` vs `handleSignalSession` in `packages/tddy-web/src/components/ConnectionScreen.tsx`), **naming drift** on the helper (SIGINT in the name but generic `Signal` parameter), and **overlay click-handler verbosity** in `GhosttyTerminalLiveKit.tsx` that would benefit from a small extracted function. Unit tests in `packages/tddy-web/src/connection/sessionTerminateOverlay.test.ts` validate the standalone helper but not the production wiring. Cypress coverage in `packages/tddy-web/cypress/component/GhosttyTerminalLiveKit.cy.tsx` is solid for the happy path; a negative case (no Terminate without `onSessionTerminate`) remains optional per `plan/evaluation-report.md`.

## Strengths

- **Separation of concerns**: The terminal component does not import `ConnectionService`; it receives `onSessionTerminate` and `handleTerminateOverlayClick` only orchestrates the callback (`packages/tddy-web/src/connection/sessionTerminateOverlay.ts`, `packages/tddy-web/src/components/GhosttyTerminalLiveKit.tsx`). RPC and `sessionId` persistence live in `packages/tddy-web/src/components/ConnectionScreen.tsx`.
- **Consistent UX contract**: `ConnectedTerminal` passes `onSessionTerminate` only when the daemon flow supplies it; URL-only `index.tsx` omits it so the overlay matches product rules (`packages/tddy-web/src/components/ConnectionScreen.tsx`, `packages/tddy-web/src/index.tsx`).
- **Accessibility and test hooks**: `buildTerminateOverlayAriaLabel()`, `data-testid="terminate-button"`, `data-sigint-wired`, and hidden `terminate-rpc-complete` support a11y and tests without branching production behavior for tests only.
- **Typed props**: `connectionOverlay` and `ConnectedTerminal` props document behavior with JSDoc (`packages/tddy-web/src/components/GhosttyTerminalLiveKit.tsx`, `packages/tddy-web/src/components/ConnectionScreen.tsx`).
- **Cypress**: Decodes protobuf `SignalSession` and asserts `SIGINT` and session token — strong integration signal (`packages/tddy-web/cypress/component/GhosttyTerminalLiveKit.cy.tsx`).

## Issues (by severity)

### Medium — Duplication / dead production path

- **`signalSessionSigintForDaemonSession` vs `handleSignalSession`**: `packages/tddy-web/src/connection/sessionTerminateOverlay.ts` exports `signalSessionSigintForDaemonSession`, which wraps `signalSession` with logging and `logTddyMarker`. Production fullscreen Terminate uses `onSessionTerminate={() => handleSignalSession(connected.sessionId, Signal.SIGINT)}` in `packages/tddy-web/src/components/ConnectionScreen.tsx` — same RPC, different code path. `packages/tddy-web/src/connection/sessionTerminateOverlay.test.ts` only exercises the module helper, so **tests do not prove** the live wiring matches the helper’s behavior (logging aside).
- **Misleading name**: `signalSessionSigintForDaemonSession` takes `signal: Signal` and forwards it verbatim; the name implies SIGINT-only.

### Low — Complexity (GhosttyTerminalLiveKit Terminate click)

- **Inline async IIFE in `onClick`**: The Terminate button resets state, runs an async block with try/catch, and toggles `terminateSignalRpcComplete` (`packages/tddy-web/src/components/GhosttyTerminalLiveKit.tsx`). It is readable but **dense** for JSX; a `useCallback` handler (or a tiny module-local function) would reduce nesting and ease reuse/testing.
- **Duplicate console traces**: Component logs `[GhosttyTerminalLiveKit] Terminate: ...` while `handleTerminateOverlayClick` and `handleSignalSession` also log — useful for debugging, noisy for production (aligned with `plan/evaluation-report.md`).

### Low — Duplication of logging / tracing strategy

- **`logTddyMarker` + ad-hoc `console.*`**: Structured marker in `sessionTerminateOverlay.ts` vs plain `console.debug/info/error` in `ConnectionScreen` and the overlay — no single observability abstraction (acceptable short-term; worth consolidating in a later pass).

### Low — SOLID / module cohesion

- **`sessionTerminateOverlay.ts` mixes**: (1) optional RPC wrapper used only by tests, (2) click orchestration, (3) aria string, (4) `logTddyMarker`. Not egregious for file size, but **responsibilities could be split** if the RPC helper is kept.

### Low — Test quality (Cypress)

- **`TerminateRpcWrapper`** in the test file re-implements the same `client.signalSession({...})` pattern as production instead of importing a shared helper — **acceptable** for CT isolation but duplicates knowledge of request shape.
- **Missing negative test**: No assertion that `terminate-button` is absent when `onSessionTerminate` is omitted (noted in `plan/evaluation-report.md`).

## Refactoring suggestions

1. **Unify SignalSession invocation**: Either wire production to `signalSessionSigintForDaemonSession` from `sessionTerminateOverlay.ts` (passing `client.signalSession.bind(client)` or a thin closure from `ConnectionScreen`), or **remove** the helper and test `handleSignalSession` / a shared internal `performSignalSession` used by both the table dropdown and fullscreen Terminate. Goal: **one** implementation of “call SignalSession with token + sessionId + signal.”
2. **Rename or narrow the helper**: If it stays generic, rename to e.g. `signalDaemonSession` / `invokeSignalSession`; if SIGINT-only, remove the `signal` parameter and pass `Signal.SIGINT` inside.
3. **Extract `onTerminateClick`**: In `GhosttyTerminalLiveKit.tsx`, move the Terminate button’s logic to a `const handleTerminateClick = useCallback(async (ev: ...) => { ... }, [deps])` to shrink JSX and clarify the async/error path.
4. **Reduce production console noise**: Gate verbose `console.debug/info` in `handleSignalSession` and Terminate paths behind `debugLogging` or a dedicated dev flag (without adding silent fallbacks for errors — keep user-visible `setError`).
5. **Cypress**: Add a short test that mounting with `connectionOverlay` **without** `onSessionTerminate` yields no `[data-testid='terminate-button']`; optionally share a tiny test util for building `onSessionTerminate` that matches daemon semantics.

---

*Primary files reviewed: `packages/tddy-web/src/connection/sessionTerminateOverlay.ts`, `packages/tddy-web/src/connection/sessionTerminateOverlay.test.ts`, `packages/tddy-web/src/components/GhosttyTerminalLiveKit.tsx`, `packages/tddy-web/src/components/ConnectionScreen.tsx`, `packages/tddy-web/cypress/component/GhosttyTerminalLiveKit.cy.tsx`; context from `plan/evaluation-report.md`.*
