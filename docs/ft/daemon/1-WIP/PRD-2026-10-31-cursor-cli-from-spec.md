# Cursor CLI sessions launch from cursor.yaml, on the host and in the jail - PRD

**Date**: 2026-10-31
**PRD Type**: Technical Improvement
**Stack**: `#agent-tools 7/12` (branch `feature/agent-tools/cursor-cli`)

## Affected Features

- **Primary**: [Cursor CLI session](../cursor-cli-session.md). Host and jailed `cursor-cli` sessions are launched from `cursor.yaml`.

## Summary

`cursor.yaml` declares `cursor-agent` for both launches. Host sessions use the spec-driven PTY launcher from `2/12`, and jailed sessions use the jail launcher from `5/12`. The spec covers:

- **binary**: `agent`, overridable by `TDDY_CURSOR_AGENT`;
- **the `node index.js` install layout** as `files:` and `run:` facts, replacing the runner's argv rewrite;
- **argv**: `--resume <chat>`, `--model` and the prompt;
- **mapping slots**:
  - `mcp: { kind: file, path: "{home}/.cursor/mcp.json", format: cursor-mcp-json }`;
  - `instructions: { rules_file: .cursor/rules/tddy-managed-workflow.mdc }`;
  - `hooks: { adapter: cursor-hooks-json }`;
- **jail**:
  - `home: { kind: persistent, key: cursor }`;
  - credential copies (`~/.cursor/{auth,cli-config,mcp}.json`);
  - env (`CURSOR_SKIP_KEYCHAIN=1`, `CI=true`, `CURSOR_TMPDIR`);
  - Cursor's host allowlist.

No `--agent-kind cursor` reaches the runner from the daemon, and the jail no longer receives the Claude env overlay.

## Background

Cursor knowledge is spread across:

- `cursor_cli_spawn.rs`, which is over its complexity budget;
- `svc_start_sandboxed_cursor_cli_session.rs`, a CRAP hotspot that was never executed;
- `SandboxRecipe::CursorCli`;
- the runner's `spawn_cursor_pty` argv rewrite;
- `cursor_hooks.rs`.

Each is an independent copy of the same facts.

## Proposed Changes

### What's Changing

- `cursor.yaml`. Both launch paths go through the shared launchers.
- `svc_start_sandboxed_cursor_cli_session` and the per-agent body of `cursor_cli_spawn` are deleted.
- **Cursor hooks reach the daemon everywhere** ([2026-10-17-desktop-cursor-hooks-reach-daemon.md](../../../dev/todo/2026-10-17-desktop-cursor-hooks-reach-daemon.md), decided 2026-10-31). The `hooks` slot renders `.cursor/hooks.json` with a daemon socket. On the host, that is the daemon's agent-tool socket, which the embedded Tddy Desktop daemon also serves. In a jail, hook reports travel over the existing tool-IPC bridge, so no host-only path is handed to the jail.
- The chat id is minted and persisted as a template parameter.

### What's Staying the Same

- Cursor's behaviour, its "guided, not enforced" managed-codebase limit ([2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md](../../../dev/todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md)), and branch policy.

## Acceptance Criteria

- [ ] A host `cursor-cli` start and resume spawn a stub `agent` whose argv, cwd and `.cursor/mcp.json` match the spec.
- [ ] A jailed `cursor-cli` start runs the stub with the persistent cursor home, the credentials copied, and the keychain-skip env, but with no `CLAUDE_CODE_TMPDIR`.
- [ ] Cursor's jail reaches its declared hosts and is refused an undeclared one.
- [ ] The runner receives no Cursor-specific flag on the daemon path.
- [ ] On an embedded (desktop) daemon, a host Cursor session's hook updates its activity and status.
- [ ] A jailed Cursor session's hook reaches the daemon through the bridge.

## Out of Scope

- Headless Cursor (`9/12`), and Cursor in the standalone app (`10/12`).
