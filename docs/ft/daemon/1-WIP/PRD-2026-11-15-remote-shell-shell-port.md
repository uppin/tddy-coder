# One remote-shell port, SSH as its first transport - PRD

**Date**: 2026-11-15
**PRD Type**: Technical Improvement
**Stack**: `#remote-shell 1/7` — branch `feature/remote-shell/shell-port`, base `master`

## Affected Features

- **Remote managed worktree**: [remote-managed-worktree.md](../remote-managed-worktree.md) — the SSH checkout paths keep their behaviour and gain one option set
- **Remote git repo**: [remote-git-repo.md](../remote-git-repo.md) — remote pack execution runs through the port

## Summary

Every place the daemon runs something on a machine it does not serve goes through one port, `RemoteShell`, in a new crate `tddy-remote-shell`. OpenSSH is its first transport. The four existing SSH spawn sites move onto it and share one set of connection options and one typed error.

## Background

Four call sites build their own `ssh` command today, with different options:

| Site | Options | Shape |
|---|---|---|
| `tddy-tool-engine` `RemoteShell::ssh_run` | `BatchMode` only | blocking one-shot |
| `tddy-git` `ssh_exec` / `ssh_worktree` | `BatchMode` only, no connect timeout | blocking one-shot |
| `tddy-agent-launch` `run_over_ssh` | `BatchMode`, `ConnectTimeout=15`, `ServerAlive*`, 60 s deadline | one-shot with stdin |
| `tddy-worktree-service` `PackExecution::Remote` | `BatchMode` only | streamed, piped |

A second transport (Coder workspaces) and an agent that runs on the remote both need "run this there". Adding them to four divergent sites would multiply the divergence by the number of transports.

## Proposed Changes

### What's Changing

- A new crate `tddy-remote-shell` owns the `RemoteShell` port: one-shot exec with stdin and a deadline, streamed exec, and the argv of an interactive command for a local PTY.
- `SshShell` is the OpenSSH transport. Every SSH call gets `BatchMode=yes`, `ConnectTimeout=15`, `ServerAliveInterval=5`, `ServerAliveCountMax=3`.
- A failure says which of four things happened: the target could not be reached, the deadline passed, the command exited non-zero, or the local spawn failed.
- Worktree setup, exec-catalog tools, slot files, the session pointer and git packs on an SSH checkout all run through the port.
- An in-memory shell (`test-support` feature) and a shared contract suite let later transports prove they behave like SSH.

### What's Staying the Same

- `ssh_config_host` sessions behave as before: the agent runs locally, tools run on the target, managed-codebase semantics hold.
- A missing key is still a hard failure, never a prompt and never a fall back to the local shell.
- No proto, web or session-metadata change.

## Impact Analysis

### Technical Impact

- New workspace crate, no new external dependency (`tokio`, `async-trait`, `base64` are already in the workspace).
- `tddy-git`'s worktree setup, `tddy-agent-launch`'s `PointerHost::Ssh` and `run_over_ssh`, `tddy-tool-engine`'s `RemoteShell` struct and `tddy-worktree-service`'s `PackExecution::Remote` take the port instead of an alias string.
- Tool-engine and git SSH calls gain a connect timeout they lacked; a dead host now fails in 15 s instead of at the TCP default.

### User Impact

- No visible change on a healthy host. An unreachable SSH target fails sooner and names the cause.

## Implementation Plan

1. Port, error type, `SshShell`, in-memory shell, contract suite.
2. Move the four call sites, one crate at a time, each behind its existing tests.
3. Ratchet test: no `ssh` spawn outside `tddy-remote-shell`.

Tests are written first and run per touched package; the whole-workspace result is read from CI.

## Acceptance Criteria

- [ ] One-shot exec returns stdout, stderr and the exit code, feeds stdin, and stops at its deadline
- [ ] Streamed exec relays stdin, stdout and stderr of a long-running command and reports its exit
- [ ] The interactive command for an SSH target is an argv a local PTY can run (`ssh -tt … -- <alias> <cmd>`)
- [ ] An unreachable target, a timeout, a non-zero exit and a spawn failure are four distinct errors
- [ ] Exec-catalog tools, worktree setup, slot files, the session pointer and git packs on an SSH checkout run through the port with unchanged results
- [ ] No crate other than `tddy-remote-shell` spawns `ssh`

## References

### Affected Features (Complete List)

- **Remote managed worktree**: [remote-managed-worktree.md](../remote-managed-worktree.md) — the SSH checkout paths keep their behaviour and gain one option set
- **Remote git repo**: [remote-git-repo.md](../remote-git-repo.md) — remote pack execution runs through the port

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-15-remote-shell-shell-port.md` on this branch
- Other PRs of this stack are named by branch in the changeset's `## Dependencies` and `## Successor PRs`
