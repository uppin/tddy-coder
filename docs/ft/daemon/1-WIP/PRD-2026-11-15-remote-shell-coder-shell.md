# Coder workspaces as a remote-shell transport - PRD

**Date**: 2026-11-15
**PRD Type**: Enhancement
**Stack**: `#remote-shell 4/7` — branch `feature/remote-shell/coder-shell`, base `feature/remote-shell/ssh-agent-launch`

## Affected Features

- **Provider accounts**: [provider-accounts.md](../provider-accounts.md) — a Coder account gains a use: its workspaces
- **Hosts screen tooling**: [hosts-screen-tooling.md](../../web/hosts-screen-tooling.md) — the host reports whether the `coder` CLI is present

## Summary

A Coder workspace becomes the second transport of the remote-shell port, driven by the `coder` CLI on the daemon's host with the account's URL and token. The daemon also lists the workspaces a Coder account can reach, with their status.

## Background

A Coder workspace is reached with `coder ssh <workspace>`, which needs the deployment URL and a session token and nothing on the workspace beyond what Coder already runs there. That makes it a remote shell in the same sense as an SSH host: one-shot commands, streamed commands and an interactive terminal.

The account PR stores the URL and token. This PR turns them into something that runs commands, and into a list of places to run them.

## Proposed Changes

### What's Changing

- `CoderShell` implements the remote-shell port by running `coder ssh` with `CODER_URL` and `CODER_SESSION_TOKEN` in that child's environment only.
- `HostService.ListCoderWorkspaces` answers the workspaces of one Coder account: name, owner, status, and whether it is running.
- The listing is an HTTP call to the deployment, so it works without the CLI.
- The host's tooling report says whether a `coder` executable is on the daemon's path.
- A locked vault, a rejected token, an unreachable deployment and a missing CLI are four distinct outcomes.

### What's Staying the Same

- Nothing starts a session on a workspace yet.
- The SSH transport and the port's trait are unchanged.
- The token is never written to disk, logged, or placed in the daemon's own environment.

## Impact Analysis

### Technical Impact

- `tddy-remote-shell` gains `coder.rs`.
- `host.proto` gains one RPC and a tooling field; the TypeScript is regenerated.
- `tddy-host-service` reads Coder credentials through a port filled from `tddy-accounts`.

### User Impact

- None visible yet beyond the tooling cell. The next Coder PR shows workspaces as hosts.

## Implementation Plan

1. `CoderShell` against a stand-in `coder` executable, passing the port's contract suite.
2. The listing RPC against a stand-in deployment.
3. The tooling probe.

Tests are written first and run per touched package; the whole-workspace result is read from CI.

## Acceptance Criteria

- [ ] `CoderShell` passes the same contract suite as `SshShell`
- [ ] The URL and token reach the `coder` process and no other process
- [ ] The interactive command opens a terminal on the workspace
- [ ] A workspace that is not running is reported as unreachable with that reason, not as a failed command
- [ ] The listing answers each workspace's name, owner and status for the named account
- [ ] A locked vault, a rejected token and an unreachable deployment are distinct listing outcomes
- [ ] The tooling report says whether the `coder` CLI is installed

## References

### Affected Features (Complete List)

- **Provider accounts**: [provider-accounts.md](../provider-accounts.md) — a Coder account gains a use: its workspaces
- **Hosts screen tooling**: [hosts-screen-tooling.md](../../web/hosts-screen-tooling.md) — the host reports whether the `coder` CLI is present

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-15-remote-shell-coder-shell.md` on this branch
- Other PRs of this stack are named by branch in the changeset's `## Dependencies` and `## Successor PRs`
