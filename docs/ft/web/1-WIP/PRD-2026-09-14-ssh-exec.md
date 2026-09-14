# Run session exec tools over an SSH Host alias - PRD

**Date**: 2026-09-14
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Remote codebase mode](../../daemon/remote-codebase-mode.md) — exec catalog can run on an SSH target instead of the code-managing host's local disk
- **Related Feature**: [Remote managed worktree](../../daemon/remote-managed-worktree.md) — co-located path in this PR; split dropdown is a successor
- **Related Feature**: [Managed codebase subagents](../../coder/managed-codebase-subagents.md) — `replaces` withdrawal stays; those tools do not go over SSH
- **Related Feature**: [Hosts screen tooling](../hosts-screen-tooling.md) — session dropdown reuses `ListSshConfigHosts` (owned by the parent node)
- **Successor PRs**: `feature/ssh-exec/remote-git`, `feature/ssh-exec/split`

## Summary

An operator starting a co-located managed session can pick an OpenSSH Host alias from the session host's config. The code-managing daemon materializes the session worktree on that SSH target (no tddy-daemon there) and runs every exec-catalog tool there via OpenSSH (`RemoteShell`). Leaving the control empty keeps today's local execution (`LocalShell`).

## Background

Today every `execute_tool` runs against a local `worktree_root`. Split placement still requires a daemon on the codebase host. This PR is the vertical slice for **co-located** sessions: agent and SSH client on Host A, files on T.

## Proposed Changes

### What's Changing

- Session start accepts `ssh_config_host`. Empty = local. Set = RemoteShell; implies managed-codebase (native FS/shell tools off).
- Create-session dropdown lists Host A's aliases from `ListSshConfigHosts`.
- Worktree setup (`setup_worktree_for_session*` contract) runs on T through SSH.
- All exec-catalog tools (Read, Write, Shell, …) run on T when the alias is set.
- OpenSSH CLI, `BatchMode=yes`; keys must already be in the host ssh-agent.

### What's Staying the Same

- `tddy-tools` still dispatches to the daemon; it does not open SSH from the jail.
- Specialized-agent `replaces` still withdraws before dispatch.
- Split codebase-host filtering is a successor. Remote-git Serve-over-SSH is a successor.
- No jail on the SSH target. No russh. No cursor-cli split.

## Impact Analysis

### Technical Impact

- `tddy-tool-engine` gains a Shell trait; `contain_path` applies to the remote worktree path.
- `tddy-core` session metadata + `tddy-service` `StartSessionRequest` / `SessionEntry`.
- `tddy-session-lifecycle` start path materializes over SSH when the field is set.
- `tddy-web` create-session dropdown.

### User Impact

- New optional SSH control on create-session (co-located). Empty looks like today.
- Failed SSH (no agent, unknown host, BatchMode) surfaces as tool/start failure, not a silent local fallback.

## Implementation Plan

1. Shell trait + RemoteShell using `ssh -o BatchMode=yes`.
2. Persist `ssh_config_host` + remote worktree path; start materializes on T.
3. Route `execute_tool` through the session's shell.
4. Create-session dropdown on Host A.

## Acceptance Criteria

- [ ] A co-located session with `ssh_config_host=buildbox` materializes its worktree on `buildbox` and a Read returns content from that remote path ([remote-codebase-mode.md](../../daemon/remote-codebase-mode.md))
- [ ] Empty `ssh_config_host` keeps local `execute_tool` behaviour ([remote-codebase-mode.md](../../daemon/remote-codebase-mode.md))
- [ ] Create-session offers aliases from the session host's `ListSshConfigHosts`, including an empty/local choice ([hosts-screen-tooling.md](../hosts-screen-tooling.md))
- [ ] A tool whose name is in a specialized agent's `replaces` is still refused on the main agent; it is not sent over SSH ([managed-codebase-subagents.md](../../coder/managed-codebase-subagents.md))
- [ ] Native agent FS/shell tools are not available when an SSH alias is set ([remote-codebase-mode.md](../../daemon/remote-codebase-mode.md))
- [ ] SSH failures are errors, not a fallback to LocalShell ([remote-codebase-mode.md](../../daemon/remote-codebase-mode.md))

## References

### Affected Features (Complete List)

- [remote-codebase-mode.md](../../daemon/remote-codebase-mode.md)
- [remote-managed-worktree.md](../../daemon/remote-managed-worktree.md)
- [managed-codebase-subagents.md](../../coder/managed-codebase-subagents.md)
- [hosts-screen-tooling.md](../hosts-screen-tooling.md)

### Related Documentation

- Changeset: [2026-09-14-ssh-exec.md](../../../dev/1-WIP/2026-09-14-ssh-exec.md)
