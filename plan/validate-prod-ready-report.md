# Validate Prod Ready — Terminate (SIGINT) Overlay

**Scope:** Production readiness for the web Terminate overlay (`SignalSession` SIGINT, fullscreen error banner, `connected.sessionId`).  
**Inputs:** `plan/evaluation-report.md`, `ConnectionScreen.tsx`, `GhosttyTerminalLiveKit.tsx`, `sessionTerminateOverlay.ts`, `index.tsx`.

---

## Executive summary

The feature is **structurally sound** for release: daemon `sessionId` is carried in React state after connect/start/resume, **Connect by URL** correctly omits `onSessionTerminate` so no Terminate control appears without a signaling path, and **RPC failures** surface to users via the fullscreen **`connection-error`** banner (`role="alert"`) with dismiss. **Session tokens** are unchanged from existing `useAuth` + `localStorage` (`tddy_session_token`); this diff does not add new secret storage.

**Main gap:** `handleSignalSession` **swallows** Connect-RPC errors (sets `error` state but does **not** `throw`). The Terminate button in `GhosttyTerminalLiveKit` **awaits** `onSessionTerminate` and sets **`terminateSignalRpcComplete`** to **true on any resolved promise**—so completion state (and Cypress `terminate-rpc-complete`) can indicate success **even when `SignalSession` failed** and the red banner is showing. That is a **semantic bug** for “async RPC completion,” not for the primary user-visible error path.

**Hygiene:** Verbose **`console.*`** in `ConnectionScreen`, `sessionTerminateOverlay`, and unconditional **`console.log`** in `GhosttyTerminalLiveKit` (e.g. `[terminal→server]`, LiveKit lifecycle) will add **noise and minor perf cost** in production browsers; this matches the evaluation report’s observability note and partly predates the overlay.

---

## Checklist

| Area | Status | Notes |
|------|--------|--------|
| **Error handling — RPC failure visible to user** | **Pass** | `setError` in `handleSignalSession` catch; banner in `ConnectedTerminal` with dismiss. |
| **Error handling — Terminate vs. completion state** | **Fail** | Resolved promise after swallowed RPC error → `setTerminateSignalRpcComplete(true)` incorrectly. |
| **Error handling — Ghostty catch block** | **Partial** | Catches only if callback **throws**; `handleSignalSession` does not throw on RPC failure. |
| **Logging — prod console noise** | **Partial** | `console.debug`/`info`/`error` in signal path; `sessionTerminateOverlay` markers; heavy unconditional logs in LiveKit/terminal path. |
| **Configuration — tokens** | **Pass** | LiveKit JWT via existing Token RPC; auth `sessionToken` from existing hook. |
| **Configuration — session id** | **Pass** | `sessionId` only in memory on `connected`; no new persistence. |
| **Security — session token / localStorage** | **Pass** | No new patterns; consistent with `useAuth` + `tddy_session_token`. |
| **Security — Terminate exposure** | **Pass** | URL flow: `connectionOverlay` without `onSessionTerminate` (`index.tsx`). |
| **Performance — re-renders** | **Partial** | Inline `onSessionTerminate={() => ...}` recreates each render; acceptable at this scale. |
| **Performance — async click handler** | **Pass** | IIFE + `void` avoids blocking UI; no duplicate RPC from sync issues noted. |
| **Duplication / maintainability** | **Partial** | `signalSessionSigintForDaemonSession` vs. inline `handleSignalSession` (per evaluation). |

---

## Risks

1. **Misleading completion signal (medium):** `terminateSignalRpcComplete` / hidden `terminate-rpc-complete` do not reliably mean “RPC succeeded,” undermining tests and any future UI that keys off completion.
2. **Observability / PII-adjacent logs (low):** Structured `logTddyMarker` and signal logs include **`sessionId`** in console in devtools-capable clients; acceptable for many apps but worth tightening for strict policies.
3. **Unconditional logging volume (low):** Per-keystroke `[terminal→server]` and LiveKit `console.log` paths are not gated by `debugLogging`; increases noise and work on hot paths.
4. **Hygiene (low):** Generated `buildId.ts` churn and untracked agent log files (from evaluation) are release-process noise, not functional blockers.

---

## Prioritized recommendations

1. **P0 — Align async semantics:** Either **rethrow** after `setError` in `handleSignalSession` when used as `onSessionTerminate`, or **return a boolean / result** from the callback so `GhosttyTerminalLiveKit` only sets `terminateSignalRpcComplete` when the RPC actually succeeds. Keeps user-visible behavior, fixes completion truthfulness.
2. **P1 — Reduce prod logging:** Gate `ConnectionScreen` signal logs and `sessionTerminateOverlay` `console.debug`/`info` behind a single debug flag or strip in production build (no new dependencies; e.g. existing patterns or env).
3. **P2 — Consolidate RPC helper:** Use one path for `SignalSession` SIGINT (shared helper + tests) to avoid drift between `handleSignalSession` and `signalSessionSigintForDaemonSession`.
4. **P3 — Optional:** Memoize `onSessionTerminate` in `ConnectionScreen` with `useCallback` + stable deps to avoid unnecessary child prop identity churn.

---

## File references (behaviors reviewed)

- **`ConnectionScreen.tsx`:** `connected.sessionId`; `handleSignalSession`; fullscreen `connection-error`; `onSessionTerminate` wiring.
- **`GhosttyTerminalLiveKit.tsx`:** Terminate button, `await handleTerminateOverlayClick`, `terminateSignalRpcComplete`.
- **`sessionTerminateOverlay.ts`:** `handleTerminateOverlayClick`, `logTddyMarker`, unused-in-UI `signalSessionSigintForDaemonSession`.
- **`index.tsx`:** `connectionOverlay={{ onDisconnect, buildId: BUILD_ID }}` — no Terminate.

---

*Report generated for refactor validation (validate-prod-ready subagent).*
