# 2026-10-22 — Muse Code has no headless (tool-session) backend

**Category:** Deferred from `#provider-route` 6/9 (#667)
**Source:** `docs/dev/1-WIP/2026-10-18-provider-route-muse-cli.md` § Boundaries

Muse Code runs only as the `muse-cli` terminal session (its TUI in a PTY). There is no tool-session
agent for it: a workflow cannot drive Muse Code the way it drives `claude -p`, `claude-agent-acp`,
`codex-acp` or `opencode acp`.

**Why deferred:** Muse Code's headless protocols (`muse exec --json`, `muse serve`) are
undocumented, and no real `muse` binary was available to record their wire format, so a backend
written now would be a guess at a stream parser. #667's boundaries excluded it.

**What would close it:** a recorded transcript of `muse exec --json` (or the ACP surface of
`muse serve`, if it has one), a `CodingBackend` that parses it, and its registration as a
tool-session agent with `Harness::MuseCli`'s route.
