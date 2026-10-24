# 2026-10-24 — Every `claude-cli` launch runs Claude's fullscreen renderer, so the web terminal can scroll it

**Type:** Fix

Claude could not be wheel-scrolled in the web terminal. A wheel over the live pane logged
`tracking=false altScreen=false`: Claude ran its inline renderer, which never enables the alternate
screen or mouse tracking (DECSET `1049` / `1000` / `1006`). The live pane keeps no scrollback of its
own, so nothing scrolled. The browser and the daemon's mouse-mode re-send on attach were both
working; there was nothing for them to forward to.

- **`claude_cli.fullscreen`** (default `true`, also when `claude_cli:` is absent) sets
  `CLAUDE_CODE_NO_FLICKER=1` on all four `claude-cli` launch paths: host start, host Resume,
  sandboxed start, sandboxed Resume. One helper owns the mapping
  (`DaemonConfig::claude_cli_renderer_env`). The sandboxed Resume adds it at its call site, because
  `relaunch_jail_env` is shared with the other harnesses, which must not get it.
- **tddy-web:** the terminal's mouse/wheel listeners no longer re-attach on every parent render (the
  effect depended on an inline `onData`). New masked traces: `tddy:term:modes` and `tddy:term:chrome`.
  The render-chrome trace no longer logs unconditionally in dev.

Permanent docs:
- [claude-cli-session.md § Fullscreen renderer](../../ft/daemon/claude-cli-session.md#fullscreen-renderer)
- [local-web-dev.md](../../ft/web/local-web-dev.md) (debug namespaces)

Tests: `packages/tddy-session-lifecycle/tests/claude_cli_fullscreen_renderer.rs` (all four paths, on
and off) and `claude_cli_config_tests` in `tddy-daemon-kernel`.

Deferred: [`GhosttyTerminal*` over budget](../todo/2026-10-24-ghostty-terminal-components-over-budget.md).
