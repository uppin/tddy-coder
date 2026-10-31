# A sandboxed Claude CLI session is configured by its tool definition, not by Claude-specific sandbox code - PRD

**Date**: 2026-10-31
**PRD Type**: Technical Improvement
**Stack**: `#agent-tools 5/12` (branch `feature/agent-tools/claude-jail`)

## Affected Features

- **Primary**: [Claude CLI session § Sandbox mode](../claude-cli-session.md). The jail's home, credentials, secrets, egress and the agent argv come from `claude.yaml`.
- **Related**: [Sandbox builder](../../coder/sandbox-builder.md). `ToolDef` gains generic `home:`, `files:`, `secrets:` and `policy:` sections. The runner runs a **tool plan**, not an agent kind.

## Summary

A sandboxed `claude-cli` session's jail is built from `claude.yaml` plus its dep closure (`tddy-tools`, and whatever Claude needs). One jail launcher does it, the same one every later agent uses.

- **The runner is told what to exec**, as a rendered argv plus env plus the files to write. It no longer learns that the process is Claude.
  - The daemon path stops passing `--claude-binary`, `--claude-arg`, `--permission-mode` and `--resume`.
  - The runner's generic exec mode gets what only the agent modes had: egress proxy env and `TDDY_SECRET_*` resolution.
  - Claude therefore sees an ordinary environment: its `HOME`, its `PATH`, an `HTTPS_PROXY`, its config files where it expects them. No `TDDY_SANDBOX_*` variable is read by Claude itself.
- **Generic `ToolDef` sections**, usable by any tool:
  - `home: { kind: persistent, key: claude }` or `{ kind: session }`. A persistent home lives at `<tddyhome>/homes/<key>` and is mounted rw. This replaces `sandbox-claude-home` and is migrated in place.
  - `files:` — seed-once copies from the host (`~/.claude/.credentials.json`), install-layout mirrors (`~/.local/bin/claude`, `versions/`), and env overlays (`CLAUDE_CODE_TMPDIR`).
  - `secrets:` — host env or file sources delivered as `TDDY_SECRET_*` (`CLAUDE_CODE_OAUTH_TOKEN`).
  - `policy:` — JIT, fork, exec paths and sysctl.
- **The mapping slots from `2/12`** are filled inside the jail exactly as on the host: the MCP file goes into the jail tmp, and the allow/deny argv uses the in-jail tool names.
- **`network:`** (`3/12`) for Claude:
  - `api.anthropic.com`, `statsig.anthropic.com`, `sentry.io` and the OAuth hosts;
  - a provider route's base-URL host is added to the jail's policy automatically;
  - `oauth_inbound: true` when unrouted.

## Background

Today a jailed Claude's sandbox needs are spread across several places:

- the `SandboxRecipe::ClaudeCli` arms: reads, policy, copies and the OAuth secret;
- `prepare_persistent_claude_home`, and a duplicate of it in `tddy-sandbox-app`;
- `claude_runner_env_overlay`, which is wrongly applied to every agent;
- runner flags, and runner-built Claude argv and MCP placement;
- three near-copy launch functions: `svc_start_sandboxed_claude_cli_session` (342 lines), the Cursor one, and relaunch. See [2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md](../../../dev/todo/2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md).

The developer's goal is that **the agent is unaware it is sandboxed**. The tool runtime maps the environment to what the spec declares, and the same rules apply to any runnable tool.

## Proposed Changes

### What's Changing

- **One jail launcher** (`tddy-agent-launch`): spec, dep closure, then a plan, then spawn, ready, bridge and register.
  - It is built coverage-first: acceptance tests pin today's sandboxed Claude start and relaunch, then the launcher replaces both.
  - `session_role` is carried through it, so a project-home main can be sandboxed ([2026-10-24-sandboxed-main-needs-session-role.md](../../../dev/todo/2026-10-24-sandboxed-main-needs-session-role.md)).
- **The runner's generic tool mode** (`--exec-plan <file>`) takes the argv, env, files and egress, and applies the proxy env and secrets.
- **Claude's recipe arm is no longer reached from the daemon.** It stays for `tddy-sandbox-app` and split sessions until `10/12`.
- **Persistent homes** move to `<tddyhome>/homes/<key>`. An existing `sandbox-claude-home` is migrated once with `rename`.

### What's Staying the Same

- What Claude can do in the jail: its reads, writes, tools and network reach. Network reach is narrowed from `any` to the declared hosts.
- Cursor, OpenCode and Muse jails (`7/12`, `8/12`), `tddy-sandbox-app` and split sessions (`10/12`).

## Acceptance Criteria

- [ ] A sandboxed `claude-cli` session started from `claude.yaml` runs Claude against a stub that records its argv, env and the files it sees.
  - The argv, `HOME`, `PATH`, the proxy env, the MCP config and the credentials file all match the spec.
  - No `TDDY_SANDBOX_*` variable is required by the agent process. The only `TDDY_*` variables it sees are those its declared `tddy-tools` MCP server needs.
- [ ] Relaunch of a sandboxed session uses the same launcher and the same rendered spec.
- [ ] Claude in a jail reaches `api.anthropic.com` through the shim, and an undeclared host is refused (live on macOS, against a local stand-in).
- [ ] A routed session's base-URL host is reachable, and the OAuth token secret is absent.
- [ ] An existing `~/.tddy/sandbox-claude-home` is migrated to `homes/claude` on first use, with credentials kept.
- [ ] A project-home main can be started sandboxed and keeps `session_role: project-home`.
- [ ] The runner receives no Claude-specific flag on the daemon path.

## Out of Scope

- Other agents' jails, the standalone app and split sessions.
- Deleting the recipe arm (`10/12`).
