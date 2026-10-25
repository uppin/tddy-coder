# 2026-10-24 — A coding agent as a session subagent, over ACP
**Type:** Feature

`#coagents` 2/8 (#721), on `#coagents` 1/8 (#720, the ACP driver). A roster entry can be a coding
agent — Claude Code, Codex or OpenCode over ACP — that the daemon launches as the session's OS user on
the session worktree, on the profile's provider-account route, confined in a managed session; the
`subagent_*` tools talk to it. Product: [Session agent roster § Coding agents](../../ft/daemon/session-agent-roster.md#coding-agents).
Packages: `tddy-coding-subagent` (new, [architecture](../../../packages/tddy-coding-subagent/docs/architecture.md)),
`tddy-session-lifecycle` ([coding-agent launcher](../../../packages/tddy-session-lifecycle/docs/coding-agent-launcher.md)),
`tddy-session-agents`, `tddy-daemon-rpc`, `tddy-changeset`, `tddy-service`, `tddy-discovery`,
`tddy-session-split`, `tddy-agent-backend` (`AcpDriver::spawn_over`), `tddy-supervisor`
(`SpawnRelayed`, [architecture](../../../packages/tddy-supervisor/docs/architecture.md#relayed-spawns-the-one-child-with-stdio)),
`tddy-web` (the kind badge).

## Privileged-surface change

One request was added to the root-owned supervisor: `SpawnRelayed`. It spawns only binaries in its
own allowlist `allowed_relay_tool_paths` (empty by default, not implied by `allowed_tool_paths`, and
not granting `SpawnSession`), as an allowed session user, never root, with the session environment
gate; it is reachable only by a peer the authorizer admits. Its one new root action — connecting to a
caller-named socket — is gated by `lstat` (a socket, not a symlink, owned by the caller's uid) and
`SO_PEERCRED` on the connected listener, both before the fork, both opaque `Denied`. Security review
at wrap found and fixed one defect: the program was also resolved against `allowed_tool_paths`, so a
harness had to be in both lists — which also granted it `SpawnSession`. The daemon's relay socket
directory is held owner-only (existing directories narrowed to `0700`, symlinks and foreign owners
refused), since whoever can reach that socket first could speak for the agent. Reviewed and
accepted: argument lists are not allowlisted (the same as `SpawnSession`), which is why `bunx` should
not be allowlisted (`TDDY_CLAUDE_AGENT_ACP_CLI` names a `claude-agent-acp` binary instead); relayed
spawns take no cgroup scope, like every `SpawnSession` the daemon sends; there is no per-caller
limit on concurrent relayed spawns, as for `SpawnSession`.

## Decisions

- **Deny permission requests** until `#coagents` 3/8 hands them to the main agent.
- **One ACP process per conversation**, so cancel and kill stay scoped to it.
- **Confinement per harness is a table**: `claude-acp` (`_meta.disallowedTools`) and `opencode`
  (inline `tools` config) can be confined; `codex-acp` cannot and is refused in managed sessions.
  Anything unproven is `CannotWithdraw`, never a silent unconfined launch.
- **Never a weaker launch**: a user switch, route or tool server the daemon cannot provide refuses the
  launch with the reason.

## Moved to later nodes, or recorded

- A coding agent owned by another daemon → `#coagents` 8/8 (`remote-codebase`): a remote attach
  resolves through the peer's `ListSubagents` before the row's kind is known.
- Long-turn awaiting is generic `tddy-tools` behaviour, covered by
  `subagent_async_response_acceptance.rs`.
- `subagent_cancel` closes the conversation; `CodingAgentSession::cancel_handle` (real
  `session/cancel`) is wired by `#coagents` 3/8 — backlog
  `2026-10-24-coding-agent-cancel-does-not-reach-session-cancel`.
- Native-tool withdrawal is documented, not live-verified — backlog
  `2026-10-24-coding-agent-native-tool-withdrawal-is-not-live-verified`.
- A privileged daemon's `setpriv` child inherits the daemon's environment minus GitHub tokens, as a
  PTY session does (accepted).
- A provider-account-less coding agent runs on `default_model_for_agent(id)`; a harness that does not
  offer it fails `session/set_model` naming the offered models.

## Backlog

Answered, kept: `2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree`
(the session worktree is the intended default for coding agents; `#coagents` 7/8 claims it) and
`2026-10-22-tool-session-provider-picker-needs-harness-metadata` (answered for `SubagentInfo`, open
for `AgentInfo`). Unchanged: `2026-10-08-drain-tddy-discovery-into-purpose-named-crates-and-remove-it`
(new code went to `tddy-coding-subagent`), `2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it`,
`2026-09-27-a-subagent-turn-has-no-wall-clock-deadline`.

## Code issues (final measurements)

| Record | Before → after | |
|---|---|---|
| `tddy-session-lifecycle` `oversized-file-connection-service` | +7 | stack-shared with `#coagents` 4/8, row added |
| `tddy-discovery` `oversized-file-subagent` | 2,046 → 2,058 | stack-shared with `#coagents` 3/8, row added |
| `tddy-discovery` `oversized-file-registry` | 802 → 805 | new record; stack-shared with 5/8 and 7/8 |
| `tddy-agent-backend` `oversized-file-backend-mod` | 906 → 907 | stack-shared with #720 and 7/8, row added |
| `tddy-supervisor` `oversized-file-spawn-broker` | 1,526 → 944 | new record, partly fixed; the rest is `cfg`-duplicated code the engine cannot name |
| `tddy-supervisor` `complexity-server-serve` / `complexity-supervisor-reclaim-scope` | 95 / 49 | moved with their functions → `complexity-listener-serve`, `complexity-session-table-reclaim-scope` |

Functions this PR added over 60 lines were split at wrap: `open_coding_agent_session` 103 → 20,
`describe_turn` 66 → 19, `worker::serve` 66 → 28, `relay::pump` 63 → under 40,
`remote_roster_record_for` 75 → 20 (pre-existing, grown by this PR).

## Verification

Scoped, on macOS: `tddy-supervisor`, `tddy-coding-subagent`, `tddy-session-agents`,
`tddy-session-split`, `tddy-changeset`, `tddy-agent-backend`, `tddy-discovery` (lib),
`tddy-session-lifecycle` (`coding_agent*` unit tests), `tddy-daemon-rpc --test coding_subagent_acceptance`,
and the Cypress specs `CreateSessionAgentPicker`, `SessionAgentRosterStatus`, `SessionAgentRosterPane`.
Not provable on this host: that the child runs as a different uid (no root), and the supervisor's
live relay — `an_allowlisted_harness_has_its_stdio_relayed_and_its_exit_reported` is Linux-only and
runs on CI. `tddy-daemon-rpc --test in_jail_conversation_acceptance` fails on this macOS host (the
jailed `tddy-sandbox-runner` exits 126); treated as the macOS jail environment, CI decides.
