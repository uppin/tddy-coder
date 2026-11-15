# Coder workspaces as hosts that run sessions - PRD

**Date**: 2026-11-15
**PRD Type**: Enhancement
**Stack**: `#remote-shell 7/7` — branch `feature/remote-shell/coder-hosts`, base `feature/remote-shell/remote-hooks`

## Affected Features

- **Hosts screen**: [hosts-screen.md](../hosts-screen.md) — Coder workspaces listed under the daemon that reaches them
- **Create-session package**: [create-session-package.md](../create-session-package.md) — the Host field offers workspaces
- **Provider accounts**: [provider-accounts.md](../../daemon/provider-accounts.md) — a Coder account's workspaces become places to work
- **Terminal sessions**: [terminal-sessions.md](../../daemon/terminal-sessions.md) — a session terminal on a workspace

## Summary

The workspaces of each connected Coder account appear as hosts derived from the daemon, with their status. Choosing a running one starts a session whose agent runs in that workspace, through the `coder` CLI, with nothing tddy-specific installed there.

## Background

The earlier PRs give a Coder account, a Coder transport, a launch that runs an agent on any target, and a host directory that shows targets. This PR joins them: a Coder case of the session target, resolved to a `CoderShell` with the account's credentials, and Coder rows in the derived-host listing.

## Proposed Changes

### What's Changing

- `SessionTarget` gains a Coder case naming an account and a workspace.
- A daemon lists each of the signed-in person's Coder accounts' workspaces as derived hosts, labelled `<workspace> (<account label>)`, with their status.
- A workspace that is not running is listed, marked stopped, and cannot be chosen.
- Starting on a workspace runs the agent there, with hooks, exactly as on an SSH host.
- A resume resolves the account again; a locked vault refuses the resume with that reason.
- A daemon without the `coder` CLI lists workspaces and says on their rows why they cannot be used.

### What's Staying the Same

- SSH derived hosts and LiveKit peers are unchanged.
- The launch, the hooks carrier and the port are unchanged: a Coder target reaches them through the same path.
- Starting or stopping a workspace is not offered.

## Impact Analysis

### Technical Impact

- `session.proto`: one new case in `SessionTarget`.
- `shell_for_target` resolves it; it needs the caller's open vault.
- `DerivedHostKind` gains `CODER`; `DerivedHost` gains a usable flag and a reason.

### User Impact

- Coder workspaces become choosable hosts once an account is connected.
- A stopped workspace is visible, with the reason it is unavailable.

## Implementation Plan

1. The target case and its resolution.
2. Coder rows in the derived listing.
3. Web: status, the unavailable state, the account label.

Tests are written first and run per touched package; the whole-workspace result is read from CI.

## Acceptance Criteria

- [ ] A connected account's workspaces list as derived hosts of the daemon, with status
- [ ] Starting on a running workspace runs the agent there and its hooks reach the daemon
- [ ] A stopped workspace is listed, marked, and refused as a target with that reason
- [ ] A daemon without the `coder` CLI lists workspaces as unusable and says why
- [ ] A locked vault lists no workspaces and says the vault is locked; it is not shown as no workspaces
- [ ] A resume with an open vault restarts the agent; with a locked vault it is refused with that reason
- [ ] Two accounts' workspaces with the same name are told apart

## References

### Affected Features (Complete List)

- **Hosts screen**: [hosts-screen.md](../hosts-screen.md) — Coder workspaces listed under the daemon that reaches them
- **Create-session package**: [create-session-package.md](../create-session-package.md) — the Host field offers workspaces
- **Provider accounts**: [provider-accounts.md](../../daemon/provider-accounts.md) — a Coder account's workspaces become places to work
- **Terminal sessions**: [terminal-sessions.md](../../daemon/terminal-sessions.md) — a session terminal on a workspace

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-15-remote-shell-coder-hosts.md` on this branch
- Other PRs of this stack are named by branch in the changeset's `## Dependencies` and `## Successor PRs`
