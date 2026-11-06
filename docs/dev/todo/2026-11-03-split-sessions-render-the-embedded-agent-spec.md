# 2026-11-03 — Split sessions render the embedded agent spec, not the operator's override

**Category:** Gap
**Source:** `#agent-tools` 10/12 (#799), `TODO(jail-agnostic)` at `packages/tddy-coding-agent-specs/src/lib.rs`
(`embedded_spec`) and its caller in `packages/tddy-session-split/src/agent_argv.rs`

## What is there

`split_claude_extra_args` renders the host agent from `claude.yaml`'s `host_agent` variant and its
slots, through `tddy_coding_agent_specs::embedded_spec("claude")`. That reads the spec embedded in the
binary. The daemon's other launches load the registry and apply `<tddyhome>/tools/claude.yaml`, so an
operator's override of the agent reaches a jailed or host session but not the agent a split session
runs on the host.

## Why

`split_claude_extra_args` takes the session directory, the `tddy-tools` path and the roster's
withdrawals. It has no tool registry, and threading one through every caller (the split start, the
resume wiring, the sandboxed-codebase start) widens #799 past the jail-agnostic change it is.

## What to decide

Hand the split code the daemon's `HostCodingAgents` (as `tddy-agent-launch` has it) and render from
`registry().get("claude")`, so an override applies everywhere. Until then an override of the
`host_agent` variant is ignored by split sessions.

## Also: the split agent's process still carries `TDDY_REMOTE_*` (`#agent-tools 17/19`)

The split spawn (`svc_spawn_split_agent`, through `CliSessionManager::start_with_options`) and,
since #823, the split half of the generic resume (`opencode_cli_spawn::resume_cli_harness`, from
`SplitAgentWiring.env`) put the `TDDY_REMOTE_*` transport on the agent process. The registered
`tddy-tools --mcp` inherits it from there. Every other CLI launch gives the agent no `TDDY_*`
variable: the session reaches `tddy-tools` through the `.tddy/session.json` pointer, whose `remote`
map already holds exactly these pairs. Moving the split pairs into the pointer, and into the MCP
registration's `env`, belongs with rendering the split agent from its spec. It was not done in #823,
whose boundary excludes the split spawn.
