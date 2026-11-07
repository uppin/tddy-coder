# Every OpenCode launch carries its route inline, by the spec's route shape — PRD

**Date**: 2026-11-07
**PRD type**: Requirements change (existing feature)
**Stack**: `#agent-tools` 29/32 — branch `feature/agent-tools/opencode-route-inline`

## Affected Features

- [session-agent-roster.md](../session-agent-roster.md) — a coding subagent's provider route (line 264)
- [provider-accounts.md](../provider-accounts.md) — how a route reaches OpenCode (line 113); the route shapes
- [opencode-cli-session.md](../opencode-cli-session.md) — line 172: `OPENCODE_CONFIG=<session dir>/opencode.json`
- [../../coder/opencode-acp-backend.md](../../coder/opencode-acp-backend.md) — § Running on a provider account: the `opencode-route-file` adapter
- [harness-profiles.md](../harness-profiles.md) — agent specs (`tools/*.yaml`): a new `route.shape: inline_config`
- [../../supervisor/tddy-supervisor.md](../../supervisor/tddy-supervisor.md) — line 92 still says `OPENCODE_CONFIG_CONTENT` must be in `allowed_env_keys`

## Summary

A spec says how a provider route reaches its agent (`coding_agent.route.shape`). A new shape,
**`inline_config`**, carries the route's generated JSON configuration **inline**, in the variable the
spec's `route.config_env` names. OpenCode's spec declares `shape: inline_config` and
`config_env: OPENCODE_CONFIG_CONTENT`.

Every OpenCode launch then carries its route inline — the daemon's ACP subagent, `tddy-coder --agent
opencode`, and the `opencode-cli` PTY session, jailed or not. **No route file is written anywhere**,
and nothing sets `OPENCODE_CONFIG`. No launcher names the agent or its variable: the shape is the
spec's, and a drop-in agent declaring the same shape gets the same treatment.

## Background

The deferred record
[2026-11-04-opencode-acp-route-file-has-no-session-owner.md](../../../dev/todo/2026-11-04-opencode-acp-route-file-has-no-session-owner.md)
planned a route file owned by the session user. The user decided on 2026-11-07 that **the file is not
needed**, on any path:

1. **The file carries no secret.** The generated `opencode.json` names the key as
   `{env:OPENCODE_PROVIDER_KEY}` (`tddy-agent-backend/src/provider_route.rs:146-180`). The key
   travels in the environment either way.
2. **Inline is the safer precedence.** OpenCode merges `OPENCODE_CONFIG_CONTENT` *over* the
   worktree's own `opencode.json`. A file named by `OPENCODE_CONFIG` ranks *below* the project
   config, so a repository could override `provider.tddy-route.options.baseURL` and send the route's
   key to its own endpoint. That holds for the PTY session and `tddy-coder` too, which is why every
   path moves.
3. **No directory is owned by the session user.** A file the daemon writes cannot be read by an agent
   started as the session's OS user. Inline configuration crosses the user switch with the
   environment.

## Proposed Changes

### What's Changing

- **New route shape `inline_config`.** Its route body serves OpenAI Responses and OpenAI Chat
  Completions accounts, as `json_config_file` does, and produces: the generated configuration in the
  route's plain environment under `route.config_env`; the key as a secret variable under
  `route.key_env`; `--model tddy-route/<model>`; **no files**.
- **`opencode.yaml`** declares `route: { shape: inline_config, config_env: OPENCODE_CONFIG_CONTENT,
  key_env: OPENCODE_PROVIDER_KEY, … }` with no `files_dir`, and no `acp.hooks_adapter`.
- **Every path renders it the same way**, by applying the route's environment, which each already
  does: the daemon ACP planner, `tddy-coder`'s ACP backend, the PTY launch and the jail's plan.
  - The daemon planner's `json_config_file` branch, which inlined a file route into a hard-coded
    `OPENCODE_CONFIG_CONTENT`, is removed.
  - A managed launch's tool withdrawal (`acp.tool_deny.config_env`) still merges over the route in the
    same variable.
- **The `opencode-route-file` ACP hooks adapter is deleted**; no spec names it.
- **An ACP launch whose route carries a file is refused**, naming the agent and the shape: no ACP
  launch can hand a file to its agent. `json_config_file` itself remains a valid shape for a PTY or
  jailed launch.
- **Spec loading refuses** an `inline_config` route slot without `config_env`, with a `config_env`
  matching `TDDY_*`, or with a `files_dir`, naming the spec.
- The route-file record is deleted at wrap, closed as "file not needed".
- `supervisor.yaml.production:115-116` and `tddy-supervisor.md:92` stop telling operators to list
  `OPENCODE_CONFIG_CONTENT` in `allowed_env_keys`: after `#agent-tools` 28 (#836) a relayed agent's
  env is not filtered by that list.

### What's Staying the Same

- `codex-acp`'s route `-c` arguments (`route.shape: toml_args`) and the `env` shape (Claude Code,
  Muse Code).
- `json_config_file` as a shape, for a drop-in PTY agent that reads a configuration file.
- The route's `--model` is still not passed to `opencode acp`; the PTY session still passes it.

## Impact Analysis

### Technical Impact

- `tddy-agent-backend`: the shape, its body, `RoutedAgent.config_env`; the adapter's deletion.
- `tddy-coding-agent-specs`: load validation, `routed_agent`, `opencode.yaml`.
- `tddy-session-lifecycle`: the ACP planner loses its last agent-shaped branch.
- `tddy-agent-launch`, `tddy-coder`: no logic change expected; tests re-pinned.

### User Impact

None visible: a routed OpenCode session works as before, on every path. A repository's own
`opencode.json` can no longer redirect a routed session's endpoint. An operator's drop-in agent can
declare an inline route without a code change.

## Acceptance Criteria

- [ ] A daemon-launched `opencode` ACP subagent on a route has the route's configuration in `OPENCODE_CONFIG_CONTENT`, the key in `OPENCODE_PROVIDER_KEY`, and no `OPENCODE_CONFIG`.
- [ ] `tddy-coder --agent opencode` on a route: the same, and no route file is written.
- [ ] An `opencode-cli` start on an account: the same, and no `opencode.json` is written in the session directory.
- [ ] A drop-in agent with another id whose spec names `config_env: SOME_VAR` gets its route inline in `SOME_VAR`. No agent name decides it.
- [ ] A managed launch's tool withdrawal still merges over the inline route.
- [ ] An ACP launch whose route carries a file is refused, naming the shape.
- [ ] An `inline_config` slot without `config_env`, naming a `TDDY_*` variable, or with `files_dir`, fails spec loading, naming the spec.
- [ ] The daemon's launcher code names no agent and no agent variable.
- [ ] `toml_args` and `env` routes are unchanged.

## References

- Changeset: `docs/dev/1-WIP/2026-11-07-opencode-route-inline-from-spec.md`
- Parent decision: #835 (`#agent-tools` 27) D1, which moved the route file to this node. This node closes it with "file not needed".
