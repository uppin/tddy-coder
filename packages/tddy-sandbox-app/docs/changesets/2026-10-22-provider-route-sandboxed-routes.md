# 2026-10-22 — `--agent-kind opencode | muse` parsed and refused

**Type:** Feature (partial)

`AgentKind` gains `OpenCode` and `Muse`. The app refuses to start either one, because a jailed
session of either needs a provider account and this app has no vault to supply one. See
`docs/dev/todo/2026-10-22-sandbox-app-cannot-start-opencode-or-muse.md`. PR
[#670](https://github.com/uppin/tddy-coder/pull/670) (`#provider-route` 9/9).
