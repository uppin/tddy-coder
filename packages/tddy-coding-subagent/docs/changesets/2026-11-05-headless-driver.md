# 2026-11-05 — A headless session takes its agent from the command's renderer

**Type:** Refactor

`#agent-tools` 15/19 ([#821](https://github.com/uppin/tddy-coder/pull/821)).

- `CodingAgentSession::start_headless(command, model)`: the `HeadlessHarness` parameter is gone; the agent is the `HeadlessCommand`'s spec renderer.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md).
