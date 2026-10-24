# 2026-10-24 — Agent terminal tabs: the `+` menu and kind-aware labels
**Type:** Feature

`NewTerminalMenu` (the `+`, in its own wrapper outside the tab scroller) offers Bash plus `CLI_HARNESS_OPTIONS` — Bash only on a tool session, every harness while the session type is unknown (developer consent). `useSessionTerminals` holds `{ terminalId, kind }[]` and `open(kind)`; tabs read "Bash 1" / "Warp 1", rebuilt from `ListTerminalSessions` on reattach. `cliHarnessOptions.ts` is the list the Create toggle and the menu share. Test ids `sessions-terminal-tab-new-menu`, `sessions-terminal-tab-new-<kind>`; the fake backend records `startedTerminalKinds`. See [terminal-session.md](../terminal-session.md#the-tab-strip-componentssessionssessionterminaltabstsx) (#718).
