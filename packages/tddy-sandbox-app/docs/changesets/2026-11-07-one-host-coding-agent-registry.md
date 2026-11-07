# 2026-11-07 — `--agent` offers only PTY-runnable agents

**Type:** Fix

`#agent-tools` 22/28 ([#830](https://github.com/uppin/tddy-coder/pull/830)).

- `choose_agent_on` keeps the coding agents whose spec's `run` is `Transport::Pty`; `AgentChoiceError::Unknown.known` lists those. An ACP-only agent (`claude-acp`, `codex-acp`) is refused `AgentChoiceError::NotPtyRunnable { id, runnable }` rather than by placement.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-one-host-coding-agent-registry.md).
