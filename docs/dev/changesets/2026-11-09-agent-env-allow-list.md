# 2026-11-09 — Agents start from an allow-listed environment

**Type:** Feature

`#agent-tools` 28/32 ([#836](https://github.com/uppin/tddy-coder/pull/836)).

Agent children start from an allow-listed environment — the YAML base set (`specs/agent-env.yaml`, operator override `<tddyhome>/tools/agent-env/base.yaml`), the spec's `pass_env`, and what the launch sets — instead of the daemon's environment minus `TDDY_*`. The deny-list helpers (`strip_tddy_env`, `without_inherited_tddy_variables`, `in_a_pane_without_inherited_tddy`) are deleted.

Packages: tddy-coding-agent-specs, tddy-agent-backend, tddy-agent-launch, tddy-pty (`PtyEnvBase`), tddy-cli-sessions, tddy-session-split, tddy-supervisor; mechanical `PtySpawnSpec` field edits in tddy-terminal-rpc and tddy-coder.

Sites: headless turns, model listing, ACP worker, conversation-id mint (host and sandboxed), host PTY and tmux pane, `spawn_rendered_main`, the split agent's PTY, the supervisor's relayed spawn. A user's Bash terminal still inherits the daemon environment. Jailed launches, tool and git process envs, and `SpawnSession`/clone gates are unchanged. Codex's launch-set `TDDY_CODEX_OAUTH_OUT` is the one `TDDY_*` exception, until the Codex OAuth follow-up.

Code-issue measurements:

- `packages/tddy-coding-agent-specs/src/spec.rs`: 1031 → 1111 production lines (stack-shared; deferred to the `#agent-tools` sweep node).
- `packages/tddy-supervisor/src/spawn_broker.rs`: 969 → 977 (stack-shared; deferred to the `#agent-tools` sweep node).
- `packages/tddy-session-split/src/svc_spawn_split_agent.rs`: 496 → 530 → 497, decomposed by moving the split-agent PTY into `svc_spawn_split_agent/split_agent_pty.rs`.

Recorded for the sweep, not fixed here: a `setpriv` ACP agent keeps the daemon user's `LOGNAME` (`docs/dev/todo/2026-11-07-setpriv-acp-agent-keeps-daemon-user-logname.md`).
