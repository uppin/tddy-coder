# 2026-11-06 — A split agent's claude binary is a port

**Type:** Feature

`#agent-tools` 19/19 ([#825](https://github.com/uppin/tddy-coder/pull/825)).

- `SplitSessions.claude_binary` (`ClaudeBinary`): the host's `claude` coding agent as it resolves on this host. The split start records its answer wait; the config hook url and answer-wait reads are gone.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-agent-config-removal.md).
