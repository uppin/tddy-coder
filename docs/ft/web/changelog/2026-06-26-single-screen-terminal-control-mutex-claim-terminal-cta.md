# 2026-06-26 — Single-screen terminal control mutex: Claim terminal CTA

- `SessionMainPane` gains a `terminalControl` prop; when another screen holds the lease an absolute scrim overlay appears with the holder's screen id and a **"Claim terminal"** button
- `useTerminalControl` hook: claims control (steal=false) on session attach, subscribes to `WatchTerminalControl` server-stream for real-time lease-change events, exposes `claim()` for steal=true
- `terminalControlState.ts` pure reducer folds `TerminalControlEvent` stream into `{ isController, holderScreenId }`
- `screenId.ts`: stable per-browser-tab identity persisted in `sessionStorage` (two tabs get different ids)
- `SessionsDrawerScreen` owns the hook and passes `terminalControl` to `SessionMainPane` only when a session is connected
