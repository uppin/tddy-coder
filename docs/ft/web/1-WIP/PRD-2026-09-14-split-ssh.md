# Split sessions pick SSH execution on the codebase host - PRD

**Date**: 2026-09-14
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Remote managed worktree](../../daemon/remote-managed-worktree.md) — SSH dropdown sourced from codebase host B; field forwarded on workspace StartSession
- **Related Feature**: [Remote codebase mode](../../daemon/remote-codebase-mode.md) — exec catalog on B still goes through LiveKit ExecuteTool; B then uses RemoteShell
- **Related Feature**: [Hosts screen tooling](../hosts-screen-tooling.md) — `ListSshConfigHosts` addressed to B, same RPC as co-located

## Summary

When the operator splits agent (host A) from codebase (host B), the SSH destination list is B's OpenSSH `Host` aliases, not A's. The chosen alias rides the existing workspace `StartSession` to B. Agent tools still reach B over LiveKit; B's RemoteShell is the SSH client toward a target with no tddy-daemon.

## Background

n2 adds the dropdown for co-located sessions (list from A, SSH from A). Split placement already forwards most of `StartSessionRequest` via `workspace_start_request` (`..req.clone()`). Listing A's aliases for a B-side OpenSSH client is wrong: `~/.ssh/config` is per host. This PR retargets the control and pins the forward so A does not apply SSH locally.

## Proposed Changes

### What's Changing

- Create-session SSH dropdown, when `isSplitCodebase`, calls `ListSshConfigHosts` with `daemon_instance_id` = the chosen codebase host (B).
- Co-located (empty / same-as-host codebase pick) keeps n2's Host A list.
- Split `StartSession` forwards `ssh_config_host` on the workspace request; A does not materialize a remote worktree.
- Split LiveKit `ExecuteTool` hits B; B runs n2's RemoteShell when the field is set.

### What's Staying the Same

- Agent/codebase daemon picks. LiveKit ExecuteTool path to B.
- `ListSshConfigHosts` itself (n1). RemoteShell implementation (n2). RemoteGit Serve (n3).
- No jail on the SSH target. No cursor-cli split. No restart sweep for split rooms.

## Impact Analysis

### Technical Impact

- `tddy-web` `CreateSessionPane`: address the list RPC at B when split.
- `tddy-session-lifecycle`: workspace start carries `ssh_config_host`; agent-host start does not SSH.

### User Impact

- Split create-session shows destinations B can actually `ssh` to.
- Choosing an alias still means files live on T, reached via B.

## Implementation Plan

1. Retarget `create-session-ssh-config-select` list source when split.
2. Assert workspace `StartSession` includes `ssh_config_host`; A does not consume it.
3. Acceptance: split tools run on T via B's RemoteShell.

## Acceptance Criteria

- [ ] Split create-session lists aliases from the codebase host, not the agent host ([remote-managed-worktree.md](../../daemon/remote-managed-worktree.md))
- [ ] Co-located create-session still lists the session host ([hosts-screen-tooling.md](../hosts-screen-tooling.md))
- [ ] Workspace StartSession on B includes the operator's `ssh_config_host` ([remote-managed-worktree.md](../../daemon/remote-managed-worktree.md))
- [ ] Agent host A does not open SSH or materialize a worktree on T ([remote-managed-worktree.md](../../daemon/remote-managed-worktree.md))
- [ ] A Read from the split agent returns content from T via B ([remote-codebase-mode.md](../../daemon/remote-codebase-mode.md))

## References

### Affected Features (Complete List)

- [remote-managed-worktree.md](../../daemon/remote-managed-worktree.md)
- [remote-codebase-mode.md](../../daemon/remote-codebase-mode.md)
- [hosts-screen-tooling.md](../hosts-screen-tooling.md)

### Related Documentation

- Changeset: [2026-09-14-split-ssh.md](../../../dev/1-WIP/2026-09-14-split-ssh.md)
