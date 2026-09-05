# 2026-06-26 — Single-screen terminal control mutex: Claim terminal CTA

**Type:** Feature

`screenId.ts` (stable per-tab id via `sessionStorage`); `terminalControlState.ts` pure reducer (`applyTerminalControlEvent`, `TerminalControlState`, `initialTerminalControlState`); `useTerminalControl` hook (`runControlSession` extracted helper, claim-on-attach + reconnecting `for await` watch loop, `claim()`/`reset()`); `SessionMainPane`: `terminalControl` prop, absolute scrim overlay with holder id + "Claim terminal" button; `SessionsDrawerScreen` owns hook; page-object helpers + test ids added. 4 Cypress CT tests + 5+4 bun unit tests. Feature [session-drawer.md](../../../../docs/ft/web/session-drawer.md). (tddy-web)
