# 2026-11-03 — `#tmux-agent` 4/4: a terminal reports ready only once its initial content is written

**Type:** Bug Fix

`tddy-web`: `GhosttyTerminal` writes the `initialContent` present at open before `setReady` / `onReady` (dedup via `writtenInitialContentRef`; the effect still writes later changes), fixing the one-render window where `onReady` and `data-terminal-ready` were set over an empty buffer. Race exposed by #813, hit by #815 and #816 on CI. See the [package changeset](../../../packages/tddy-web/docs/changesets/2026-11-03-terminal-ready-content.md).

Deferred (developer's sweep rule): shared tmux test fixtures (three `TODO(tmux-agent)`), `service.rs` 573 lines, `CreateSessionPane.tsx` 524 lines, `spawn_tool` / `spawn_claude_cli_session_inner` length. Backlog `2026-11-03-detached-tmux-agent-output-is-not-recorded` stays open for a follow-up after the stack lands. Part of [#818](https://github.com/uppin/tddy-coder/pull/818).
