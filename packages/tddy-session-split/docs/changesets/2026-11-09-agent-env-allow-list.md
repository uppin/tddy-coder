# 2026-11-09 — Agents start from an allow-listed environment

**Type:** Feature

`#agent-tools` 28/32 ([#836](https://github.com/uppin/tddy-coder/pull/836)).

- The split agent's PTY (`spawn_split_agent_pty`, `SplitAgentPty`, now in `svc_spawn_split_agent::split_agent_pty`) starts from the registry's computed agent env.
