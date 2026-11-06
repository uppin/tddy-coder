# Session pointer ownership — PRD

**Date**: 2026-11-06
**PRD type**: Requirements change (existing feature)
**Stack**: `#agent-tools` 21/28 — branch `feature/agent-tools/session-pointer`

## Affected Features

- [session-agent-roster.md](../session-agent-roster.md) — how an agent's `tddy-tools` finds its session
- [terminal-sessions.md](../terminal-sessions.md) — CLI sessions on a remote (SSH) host
- [remote-codebase-mode.md](../remote-codebase-mode.md) — the pointer written over SSH

## Summary

The per-session pointer `.tddy/session.json` becomes something a session **owns**: it names its
owner, it is git-excluded and `0600` on a remote host as well as locally, it is removed when the
agent exits or the session is deleted, and `tddy-tools` refuses a pointer whose owner is gone
instead of dialling a dead socket. Headless conversations get a pointer too, so their tool server
needs no `TDDY_*` env. Concurrent jailed sessions stop sharing a tool-IPC socket path.

## Background

`#agent-tools` moved the agent's session identity out of `TDDY_*` env and into the pointer. Four
gaps were left as TODOs:

- [2026-11-05-session-pointer-lifecycle-hazards.md](../../../dev/todo/2026-11-05-session-pointer-lifecycle-hazards.md):
  the pointer records no owner, so a leftover or overwritten pointer is dialled blindly.
- [2026-11-06-ssh-session-pointer-exclude-and-removal.md](../../../dev/todo/2026-11-06-ssh-session-pointer-exclude-and-removal.md):
  over SSH the pointer is neither git-excluded nor `0600`, and nothing removes it.
- [2026-11-05-headless-conversation-tool-server-env.md](../../../dev/todo/2026-11-05-headless-conversation-tool-server-env.md):
  a headless Codex conversation's inline tool server still gets its session through `TDDY_*` env.
- [2026-11-06-jail-tool-ipc-socket-collides-across-concurrent-sessions.md](../../../dev/todo/2026-11-06-jail-tool-ipc-socket-collides-across-concurrent-sessions.md):
  the jail socket path uses the UUIDv7 *timestamp* prefix, so two sessions started together collide.

## Proposed Changes

### What's Changing

- The pointer carries its **owner** (the process that wrote it). Resolving through a pointer fails
  with a named error when the owner is gone or the socket it names does not exist. `--socket` is
  never checked this way; it is the caller's explicit choice.
- A second pointer for a *different* session is refused while one is held in the same directory.
- On an SSH host the pointer is written by **one bounded command** that git-excludes it and writes
  it `0600`, using the same deadline and keep-alives as today's slot write. It is removed, also with
  a bounded command, when the agent exits and when the session is deleted.
- A headless conversation writes its own pointer (socket, conversation id, minted token). Its inline
  tool-server slot names no env, and the launcher no longer forwards server env.
- The jail tool-IPC socket path is derived from the whole session id, so it is unique, and it still
  fits `SUN_LEN`.

### What's Staying the Same

- The resolution order: `--socket`, then the pointer (walking up from cwd), then `TDDY_SOCKET`, else
  an error naming all three.
- Local pointer behaviour: `.git/info/exclude`, `0600`, `Drop` removes it.
- The Codex OAuth `TDDY_CODEX_OAUTH_OUT` env belongs to the Codex OAuth follow-up stack, not here.

## Impact Analysis

### Technical Impact

`tddy-agent-backend` (pointer struct, `turn_files`), `tddy-tools` (`session_source`),
`tddy-agent-launch` (`cli_session_pointer`, `slot_file_over_ssh`, delete), `tddy-session-lifecycle`
(headless launcher), `tddy-sandbox` (socket path). The pointer gains fields, so a newer `tddy-tools`
must read an older pointer: a pointer without an owner is treated as unowned and is still read.

### User Impact

A stale pointer fails loudly with a message naming the pointer file, instead of hanging on, or
silently reaching, the wrong session. Repositories on SSH hosts stop showing `.tddy/session.json` as
an untracked file.

## Implementation Plan

1. Socket path from the whole id (unit test first).
2. Pointer owner field, plus resolution refusing a dead owner or missing socket.
3. SSH bounded exclude, `0600` write, and removal on exit and on delete.
4. Headless conversation pointer; drop `forwards_server_env`.

## Acceptance Criteria

- [ ] Two session ids minted within the same millisecond give different jail tool-IPC socket paths, each within `SUN_LEN`.
- [ ] `tddy-tools` resolving through a pointer whose owner process is gone fails with an error naming the pointer path and "owner gone".
- [ ] `tddy-tools` resolving through a pointer whose socket does not exist fails the same way; `--socket` to that path is not pre-checked.
- [ ] A pointer written by an older daemon (no owner field) still resolves.
- [ ] Writing a pointer for session B where session A's pointer is held is refused, naming A.
- [ ] Over SSH, the pointer is excluded in the remote repo's `info/exclude`, has mode `0600`, and is removed when the agent exits and when the session is deleted. Each remote command is bounded.
- [ ] A headless Codex conversation's command carries no `TDDY_*` env, and its tool server resolves the session through the conversation's pointer.

## References

### Affected Features (Complete List)

- `docs/ft/daemon/session-agent-roster.md`
- `docs/ft/daemon/terminal-sessions.md`
- `docs/ft/daemon/remote-codebase-mode.md`

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-06-session-pointer-ownership.md`
