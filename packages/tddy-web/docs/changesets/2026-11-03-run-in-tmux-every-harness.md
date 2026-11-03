# 2026-11-03 — "Run the agent in tmux" for every CLI harness

**Type:** Feature

`CreateSessionRunInTmuxToggle` (one control, `unavailable` from `useCreateSessionPlacement`; `showsTheSharedTmuxToggle` places it for cursor/opencode/muse/warp, claude-cli keeps it among its own fields); `createSessionRequest` sends `runInTmux` for every CLI harness type (`runInTmuxChoice`); `useCreateSessionPlacement` clears the sandbox and the jailed codebase on a switch to a type without the control. Part of [#816](https://github.com/uppin/tddy-coder/pull/816) — see [Session drawer § Create Session](../../../../docs/ft/web/session-drawer.md#fields).
