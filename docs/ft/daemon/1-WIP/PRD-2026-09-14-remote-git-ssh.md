# Serve remote-git packs through the session's shell - PRD

**Date**: 2026-09-14
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Remote git repository over LiveKit](../remote-git-repo.md) — Serve execs pack verbs through LocalShell or RemoteShell
- **Related Feature**: [Session agent roster](../session-agent-roster.md) — AC37 clones keep using `tddy-remote-git-repo`
- **Related Feature**: [Session worktree sync](../session-worktree-sync.md) — same GIT_SSH_COMMAND client

## Summary

When a project's authoritative checkout lives on an SSH target, `RemoteGitService.Serve` still looks like today's git-over-LiveKit remote. The daemon runs `git-upload-pack` / `git-receive-pack` on that target via the session's RemoteShell instead of spawning them on its own disk. `tddy-remote-git-repo` does not change.

## Background

Specialized agents on another tddy-daemon clone with `GIT_SSH_COMMAND=tddy-remote-git-repo`. Serve today packs a **local** `main_repo_path`. After n2, that path may be on T. Without this node, clones would pack an empty or stale local tree.

## Proposed Changes

### What's Changing

- Serve's child spawn uses the Shell n2 owns. `ssh_config_host` set → pack verbs on T at the remote repo path. Unset → today's local spawn.

### What's Staying the Same

- Proto, LiveKit carrier, verb whitelist, `project_ref` never taken from the request path.
- `tddy-remote-git-repo` client, credentials, mint.
- No git protocol v2, no peer forwarding (existing gaps).
- No Hosts UI, no create-session dropdown.

## Impact Analysis

### Technical Impact

- `tddy-worktree-service` `remote_git_service` spawn seam.
- Tests that clone via the real shim against an SSH-backed project.

### User Impact

- A specialized agent on daemon C can still `git clone facilitator:project` when the project lives on T.

## Implementation Plan

1. Inject Shell into Serve spawn.
2. Resolve remote path from session/project SSH fields n2 persists.
3. Acceptance: upload-pack over RemoteShell is byte-identical to a local pack of T's repo.

## Acceptance Criteria

- [ ] Serve with `ssh_config_host` set runs upload-pack on the SSH target, not on the daemon's local `main_repo_path` ([remote-git-repo.md](../remote-git-repo.md))
- [ ] Serve with no SSH field keeps today's local spawn ([remote-git-repo.md](../remote-git-repo.md))
- [ ] `tddy-remote-git-repo` clone of `{daemon}:{project}` succeeds against an SSH-backed project ([session-agent-roster.md](../session-agent-roster.md))
- [ ] Non-whitelist verbs are still refused before spawn ([remote-git-repo.md](../remote-git-repo.md))

## References

### Affected Features (Complete List)

- [remote-git-repo.md](../remote-git-repo.md)
- [session-agent-roster.md](../session-agent-roster.md)
- [session-worktree-sync.md](../session-worktree-sync.md)

### Related Documentation

- Changeset: [2026-09-14-remote-git-ssh.md](../../../dev/1-WIP/2026-09-14-remote-git-ssh.md)
