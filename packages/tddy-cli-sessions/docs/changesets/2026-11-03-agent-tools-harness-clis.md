# 2026-11-03 — OpenCode, Muse Code and Warp from their specs

**Type:** Feature

`#agent-tools` 8/12 ([#797](https://github.com/uppin/tddy-coder/pull/797)).

- `CliSessionManager::spawn_rendered_main` takes `on_exit: Option<ExitHook>`; existing callers pass `None`.

See [OpenCode](../../../../docs/ft/daemon/opencode-cli-session.md), [Muse Code](../../../../docs/ft/daemon/muse-cli-session.md) and [Warp](../../../../docs/ft/daemon/warp-cli-session.md) CLI sessions.
