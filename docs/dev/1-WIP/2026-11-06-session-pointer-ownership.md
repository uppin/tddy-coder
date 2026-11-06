# Changeset: Session pointer ownership

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — five crates; the pointer file format changes (a contract read by `tddy-tools`); ssh process seam

## Initial Discovery

[2026-11-06-session-pointer-ownership-initial-discovery.md](./2026-11-06-session-pointer-ownership-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-05-session-pointer-lifecycle-hazards.md](../todo/2026-11-05-session-pointer-lifecycle-hazards.md) | ✅ RESOLVED HERE |
| [2026-11-06-ssh-session-pointer-exclude-and-removal.md](../todo/2026-11-06-ssh-session-pointer-exclude-and-removal.md) | ✅ RESOLVED HERE |
| [2026-11-05-headless-conversation-tool-server-env.md](../todo/2026-11-05-headless-conversation-tool-server-env.md) | ✅ RESOLVED HERE |
| [2026-11-06-jail-tool-ipc-socket-collides-across-concurrent-sessions.md](../todo/2026-11-06-jail-tool-ipc-socket-collides-across-concurrent-sessions.md) | ✅ RESOLVED HERE |
| [2026-11-05-cli-spec-features-leftovers.md](../todo/2026-11-05-cli-spec-features-leftovers.md) | Its open part is the ssh pointer exclude (resolved here); the record itself is closed by `feature/agent-tools/dead-launch-code` |
| [2026-11-05-codex-oauth-out-env-in-headless-agent.md](../todo/2026-11-05-codex-oauth-out-env-in-headless-agent.md) | Not here — Codex OAuth follow-up stack. Note there: `codex.rs:67` also sets `TDDY_SESSION_DIR` |

No `Claimed by:` record in any touched package.

## Affected Packages

- **tddy-agent-backend** — `SessionPointer` gains its owner; `turn_files` write refuses a foreign held pointer; headless conversation pointer.
- **tddy-tools** — `session_source` refuses a pointer whose owner or socket is gone.
- **tddy-agent-launch** — `cli_session_pointer` over ssh: bounded exclude + `0600` write + removal; `svc_signal_delete_session` removes it.
- **tddy-session-lifecycle** — `coding_agent_launcher` drops `forwards_server_env`.
- **tddy-sandbox** — jail tool-IPC socket path from the whole session id.

## Related Feature Documentation

- PRD: [PRD-2026-11-06-session-pointer-ownership.md](../../ft/daemon/1-WIP/PRD-2026-11-06-session-pointer-ownership.md)

## Summary

`.tddy/session.json` becomes owned: it records its owner, is refused when that owner or its socket is
gone, is git-excluded, `0600` and removed on SSH hosts as it is locally, and also serves headless
conversations, whose tool servers then need no `TDDY_*` env. Concurrent jails get distinct
tool-IPC sockets.

## Scope

- [ ] Jail tool-IPC socket path unique per session id, within `SUN_LEN`
- [ ] Pointer `owner_pid`; pointer-source resolution refuses a dead owner / missing socket (never `--socket`); old pointer without owner still resolves
- [ ] A held pointer for another session is not overwritten
- [ ] SSH: one bounded exclude + `0600` write; bounded removal on agent exit and on delete
- [ ] Headless conversation pointer; inline MCP slot names no env; `forwards_server_env` deleted

## Technical Changes

### State A

- `tddy-sandbox/src/spec.rs:37-47` — `short` = first 10 alphanumerics of the id (UUIDv7 timestamp) + `std::process::id()`. Callers `tddy-agent-launch/src/jail_launch.rs:89`, `tddy-sandbox-app/src/spawn.rs:392`.
- Pointer `{socket, repo_dir, session_dir, remote, notify}` (`tddy-agent-launch/src/cli_session_pointer.rs:60-66`); `tddy-agent-backend/src/turn_files.rs:41` replaces, `Drop` (:101) removes.
- `tddy-tools/src/session_source.rs` — `UnreadablePointer` (:52, :93); no liveness check.
- `cli_session_pointer.rs:84-86` `TODO(cli-dispatch-collapse)`: remote write via `write_slot_file_over_ssh`, no exclude, no mode, no removal. Local: `exclude_from_git` (:97), `0o600` (:31).
- `tddy-session-lifecycle/src/coding_agent_launcher.rs:317-337` — `forwards_server_env` extends a headless command's env with the server's `TDDY_*`; `HeadlessCommand::env` `TODO(headless-driver)` (`tddy-agent-backend/src/backend/headless_driver.rs:53-59`).

### State B

- Socket name `tddy-<fnv64(id) hex>-<pid>.sock` (no new crate).
- `SessionPointer { …, owner_pid: Option<u32> }`. Resolution through the pointer: owner absent → read as today; owner present and not alive, or socket path missing → `UnreadablePointer { path, cause: OwnerGone | SocketMissing }`.
- `write_session_pointer` refuses when a pointer for another socket is held by a live owner.
- `cli_session_pointer` remote: one ssh command, `SLOT_FILE_OVER_SSH_DEADLINE` + keep-alives unchanged: `grep -qxF` / append to `$(git rev-parse --git-path info/exclude)`, `umask 077` write. Removal: same bound, on agent exit and from `svc_signal_delete_session`.
- Headless conversation start writes `<conversation dir>/.tddy/session.json` with socket, conversation id and minted token; the inline MCP slot carries no env; `forwards_server_env` deleted.

### Delta

| Package | Change |
|---|---|
| tddy-sandbox | `spec.rs` socket-path derivation + unit test |
| tddy-agent-backend | pointer struct field; `turn_files` refusal; headless conversation pointer |
| tddy-tools | `session_source` liveness refusal |
| tddy-agent-launch | ssh exclude/mode/removal; delete-session removal |
| tddy-session-lifecycle | drop `forwards_server_env` |

## Testing Plan

Unit and integration level; no LiveKit (cheap RPC rule). The ssh path uses the stub pattern of `tddy-agent-launch/tests/clone_ssh_env.rs` (records the remote command), not a real sshd.

## Acceptance Tests

| Test | File |
|---|---|
| `two_sessions_minted_in_the_same_millisecond_get_different_tool_ipc_sockets` | `packages/tddy-sandbox/src/spec.rs` (`#[cfg(test)]`) |
| `a_pointer_whose_owner_is_gone_is_an_error_naming_the_pointer` | `packages/tddy-tools/tests/session_source_unit.rs` |
| `a_pointer_naming_a_missing_socket_is_an_error_but_an_explicit_socket_is_not_prechecked` | `packages/tddy-tools/tests/session_source_unit.rs` |
| `a_pointer_without_an_owner_still_resolves` | `packages/tddy-tools/tests/session_source_unit.rs` |
| `a_held_pointer_for_another_session_is_not_overwritten` | `packages/tddy-agent-backend/tests/session_pointer.rs` |
| `an_ssh_pointer_is_excluded_owner_only_and_bounded` | `packages/tddy-agent-launch/src/cli_session_pointer/cli_session_pointer_unit_tests.rs` |
| `an_ssh_pointer_is_removed_when_the_agent_exits_and_when_the_session_is_deleted` | `packages/tddy-agent-launch/src/cli_session_pointer/cli_session_pointer_unit_tests.rs` |
| `a_headless_codex_conversation_carries_no_tddy_env_and_resolves_through_its_pointer` | `packages/tddy-session-lifecycle/tests/headless_subagents_from_spec_acceptance.rs` |

## Responsibility

The session pointer's ownership and lifecycle (owner, refusal, exclusive hold, SSH parity, removal),
the headless-conversation pointer, and a unique jail tool-IPC socket path.

## Boundaries

- Does **not** move split sessions' `TDDY_REMOTE_*` onto the pointer — that is `feature/agent-tools/split-remote-env`, which consumes this node's pointer shape.
- Does **not** change how any other agent env is built — `feature/agent-tools/env-allow-list`.
- Does **not** touch Codex OAuth env (`TDDY_CODEX_OAUTH_OUT`, `TDDY_SESSION_DIR` in `codex.rs`) — Codex OAuth follow-up stack.
- Does **not** change the resolution order or `--socket`.

## Dependencies

Root of this stack's line, on `yolo-pr-stack-agent-tools-6465b5be` (#828, `#agent-tools 20`).

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #828 | specs-only agent metadata; the current `SessionPointer` and `session_source` | extends them | re-open any #828 deletion |

## Draft PR contract

The first push after this plan: `SessionPointer.owner_pid`, the `UnreadablePointer` causes
(`OwnerGone`, `SocketMissing`) and the refusal error for a held pointer as compiling surface, plus the
eight acceptance tests above, failing.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — every test exercises this node's own pointer and socket code
**Concurrent with:** `feature/agent-tools/host-registry`, `feature/agent-tools/bounded-waits`, `feature/agent-tools/dead-launch-code`
**Blocks:** `feature/agent-tools/split-remote-env` (moves `TDDY_REMOTE_*` into this pointer shape), `feature/agent-tools/acp-subagent-leftovers` (same `coding_agent_launcher.rs`), `feature/agent-tools/env-allow-list` (headless env construction)

Real edges: `session-pointer → split-remote-env, acp-subagent-leftovers, env-allow-list` · `host-registry → agent-tool-socket-per-user` · `split-remote-env → env-allow-list`

## Decisions & Trade-offs

- Liveness is `owner_pid` + socket existence, not a token handshake: cheap, local, and enough to stop dialling a dead session. A recycled pid is caught by the socket check.
- An owner-less pointer stays readable so a newer `tddy-tools` works against an older daemon.
- FNV-64 over the whole id instead of a new hashing crate.

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code
