# A daemon ACP launch hands its route inline, in the variable the spec names — PRD

**Date**: 2026-11-07
**PRD type**: Requirements change (existing feature)
**Stack**: `#agent-tools` 29/32 — branch `feature/agent-tools/opencode-route-inline`

## Affected Features

- [session-agent-roster.md](../session-agent-roster.md) — a coding subagent's provider route (line 264: "the route's config file as inline `OPENCODE_CONFIG_CONTENT` for `opencode`")
- [provider-accounts.md](../provider-accounts.md) — how a route reaches OpenCode (line 113)
- [harness-profiles.md](../harness-profiles.md) — agent specs (`tools/*.yaml`): a new `coding_agent.acp.route_inline_env` field
- [../../supervisor/tddy-supervisor.md](../../supervisor/tddy-supervisor.md) — line 92 still says `OPENCODE_CONFIG_CONTENT` must be in `allowed_env_keys`

## Summary

When the daemon launches an ACP coding subagent on a provider route, the route's configuration goes
**inline** in an environment variable. The **spec** names that variable
(`coding_agent.acp.route_inline_env`). The daemon writes no route file.

Today the daemon decides this by the agent's name (`harness == "opencode"`). After this change it
reads the spec, so a drop-in agent with the same shape gets the same treatment.

## Background

The deferred record
[2026-11-04-opencode-acp-route-file-has-no-session-owner.md](../../../dev/todo/2026-11-04-opencode-acp-route-file-has-no-session-owner.md)
planned to replace the inline config with a route file the session owns. The user decided on
2026-11-07 that **the file is not needed**:

1. **The file carries no secret.** The generated `opencode.json` names the key as
   `{env:OPENCODE_PROVIDER_KEY}` (`tddy-agent-backend/src/provider_route.rs:146-180`). The key
   travels in the environment either way.
2. **Inline is the safer precedence.** OpenCode merges `OPENCODE_CONFIG_CONTENT` *over* the
   worktree's own `opencode.json`. A file named by `OPENCODE_CONFIG` ranks *below* the project
   config, so a repository could override `provider.tddy-route.options.baseURL` and send the route's
   key to its own endpoint.
3. **No directory is owned by the session user.** A file the daemon writes (its temp dir,
   `0700`/`0600`) cannot be read by an agent started as the session's OS user (`setpriv` or a
   supervisor relay). Inline config crosses the user switch with the environment.

## Proposed Changes

### What's Changing

- New spec field `coding_agent.acp.route_inline_env: <VARIABLE>`. `opencode.yaml` declares
  `route_inline_env: OPENCODE_CONFIG_CONTENT`.
- A daemon ACP launch on a route, for an agent whose spec declares the field, does three things:
  - it parses the route's config file as JSON;
  - it merges that JSON into the named variable;
  - it prepares **no** route file, so no `OPENCODE_CONFIG` is set.

  A managed launch's tool withdrawal (`acp.tool_deny.config_env`) still merges over the route in
  the same variable, as it does today.
- The launcher's `harness == OPENCODE` branch is removed
  (`tddy-session-lifecycle/src/coding_agent_launcher/acp.rs:60-67`), along with its
  `TODO(acp-agents)` (:33-35).
- The field fails spec loading in two cases:
  - the agent's `route.shape` is not `json_config_file` (there is no file to inline);
  - the variable matches `TDDY_*`.
- The route-file record is deleted at wrap, closed as "file not needed".
- Comments that say `OPENCODE_CONFIG_CONTENT` must be in the supervisor's `allowed_env_keys` are
  corrected: `supervisor.yaml.production:115-116` and `tddy-supervisor.md:92`. After `#agent-tools`
  28 (#836), a relayed agent's env is not filtered by that list.

### What's Staying the Same

- The `tddy-coder` ACP path (`tddy-coder --agent opencode`, the `OpenCodeAcpBackend` workflow
  backend) keeps its `opencode-route-file` hooks adapter. That process runs as the user who started
  it, so it can read its own file.
- OpenCode's PTY session (`opencode-cli`) keeps `OPENCODE_CONFIG` pointing at
  `<session dir>/opencode.json` (`opencode-cli-session.md:172`).
- `codex-acp`'s route `-c` arguments, keyed on `route.shape: toml_args` by `#agent-tools` 27 (#835).

## Impact Analysis

### Technical Impact

- `tddy-coding-agent-specs`: the field, its validation, and `opencode.yaml`.
- `tddy-session-lifecycle`: the ACP planner (`coding_agent_launcher/acp.rs`) and the daemon's
  `launch_agent` (#835's override). `launch_agent` must not prepare a route file for a launch whose
  route went inline.
- Docs only: `supervisor.yaml.production`.

### User Impact

None visible: a routed OpenCode subagent works as before. An operator's drop-in agent can now
declare an inline route without a code change.

## Acceptance Criteria

- [ ] A daemon-launched `opencode` ACP subagent on a route gets the route's config merged into `OPENCODE_CONFIG_CONTENT`, and has no `OPENCODE_CONFIG`.
- [ ] A drop-in ACP agent with another id that declares `route_inline_env: SOME_VAR` gets its route inline in `SOME_VAR`. No agent name decides it.
- [ ] A managed launch's tool withdrawal still merges over the inline route.
- [ ] `route_inline_env` on an agent whose route shape is not `json_config_file`, or naming a `TDDY_*` variable, fails spec loading, naming the spec.
- [ ] `tddy-coder --agent opencode` on a route still writes and holds its owner-only route file.
- [ ] No daemon launch writes a route file.

## References

- Changeset: `docs/dev/1-WIP/2026-11-07-opencode-route-inline-from-spec.md`
- Parent decision: #835 (`#agent-tools` 27) D1, which moved the route file to this node. This node closes it with "file not needed".
