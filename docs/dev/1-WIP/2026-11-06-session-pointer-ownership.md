# Changeset: Session pointer ownership

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — five crates; pointer file format changes (contract read by tddy-tools); ssh seam with remote owner lookup
**PR**: #829 — https://github.com/uppin/tddy-coder/pull/829

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
- 🆕 **tddy-coding-agent-specs** — `McpServerCommand.cwd`; an inline MCP slot renders a `cwd` override and leaves `env_vars` out when the server has no env.
- 🆕 **tddy-sandbox-app**, **tddy-session-split** — `cwd: None` at their MCP server literals (the new field).

## Related Feature Documentation

- PRD: [PRD-2026-11-06-session-pointer-ownership.md](../../ft/daemon/1-WIP/PRD-2026-11-06-session-pointer-ownership.md)

## Summary

`.tddy/session.json` becomes owned: it records its owner, is refused when that owner or its socket is
gone, is git-excluded, `0600` and removed on SSH hosts as it is locally, and also serves headless
conversations, whose tool servers then need no `TDDY_*` env. Concurrent jails get distinct
tool-IPC sockets.

## Scope

- [x] Jail tool-IPC socket path unique per session id, within `SUN_LEN`
- [x] Pointer `owner {pid, host}`; pointer-source resolution refuses a dead owner / missing socket (never `--socket`); old pointer without owner still resolves
- [x] A held pointer for another session is not overwritten
- [x] SSH: one bounded exclude + `0600` write; bounded removal on agent exit and on delete (the remote agent's own pid as owner is deferred: no ssh agent launch exists — see Technical Debt)
- [x] Headless conversation pointer; inline MCP slot names no env; `forwards_server_env` deleted

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
| `two_sessions_minted_in_the_same_millisecond_get_different_tool_ipc_sockets` | `packages/tddy-sandbox/src/spec.rs:174` |
| `a_pointer_whose_owner_is_gone_is_an_error_naming_the_pointer` | `packages/tddy-tools/tests/session_source_unit.rs:277` |
| `a_pointer_naming_a_missing_socket_is_an_error_but_an_explicit_socket_is_not_prechecked` | `packages/tddy-tools/tests/session_source_unit.rs:308` |
| `a_pointer_without_an_owner_still_resolves` (guard — passes by nature) | `packages/tddy-tools/tests/session_source_unit.rs:335` |
| `a_held_pointer_for_another_session_is_not_overwritten` | `packages/tddy-agent-backend/tests/session_pointer.rs:72` |
| `an_ssh_pointer_is_excluded_owner_only_and_bounded` (also: the remote agent's `{pid, host}` is recorded) | `packages/tddy-agent-launch/src/cli_session_pointer/cli_session_pointer_unit_tests.rs:298` |
| `an_ssh_pointer_is_removed_when_the_agent_exits_and_when_the_session_is_deleted` | `packages/tddy-agent-launch/src/cli_session_pointer/cli_session_pointer_unit_tests.rs:368` |
| `a_headless_codex_conversation_carries_no_tddy_env_and_resolves_through_its_pointer` | `packages/tddy-session-lifecycle/tests/headless_subagents_from_spec_acceptance.rs:410` |

### Unit / integration tests (red)

| Test | File | State at contract |
|---|---|---|
| `the_tool_ipc_socket_is_named_by_an_fnv64_of_the_whole_session_id_and_the_daemons_pid` | `packages/tddy-sandbox/src/spec.rs:186` | failing |
| `the_tool_ipc_socket_of_a_long_session_id_still_fits_sun_len` | `packages/tddy-sandbox/src/spec.rs:205` | guard (passes) |
| `a_pointer_owned_on_another_host_is_resolved_without_checking_its_pid_or_socket` | `packages/tddy-tools/tests/session_source_unit.rs:351` | guard (passes) |
| `a_pointer_whose_owner_is_gone_is_replaced_by_the_next_session` | `packages/tddy-agent-backend/tests/session_pointer.rs:112` | guard (passes) |
| `the_same_sessions_pointer_is_replaced_on_its_next_turn` | `packages/tddy-agent-backend/tests/session_pointer.rs:147` | guard (passes) |
| `a_pointer_without_an_owner_is_written_and_read_without_the_field` | `packages/tddy-agent-backend/tests/session_pointer.rs:174` | guard (passes) |
| `the_pointer_carries_the_managed_socket_and_the_notify_session` (expectation: owner = this process) | `packages/tddy-agent-launch/src/cli_session_pointer/cli_session_pointer_unit_tests.rs:61` | failing |
| `the_owner_of_a_pointer_on_this_host_is_not_looked_up_over_ssh` | `packages/tddy-agent-launch/src/cli_session_pointer/cli_session_pointer_unit_tests.rs:352` | failing |
| `headless_cursor_codex_and_oz_subagents_launch_with_mcp_rendered_as_file_inline_and_argv` (codex overrides: no `env_vars`, `cwd`) | `packages/tddy-session-lifecycle/tests/headless_subagents_from_spec_acceptance.rs:298` | failing |

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
- The owner is `SessionPointerOwner { pid, host }` (`owner: Option<…>` replaces the planned `owner_pid`) — developer, 2026-11-06. A local pointer is owned by the daemon on this host; an ssh pointer by the remote agent on the target, `host` as the target names itself (`hostname`).
- `tddy-tools` checks the owner's liveness, and the socket path, only when the owner's host is the host it runs on.
- The remote agent's pid comes from `.tddy/agent.pid` on the target, written as `$$` by the ssh launch before `exec`, read and removed by `record_remote_owner` in one bounded ssh command — developer, 2026-11-06.
- The headless conversation's pointer lives in the conversation directory, and Codex's inline MCP slot gets a `cwd` override (`mcp_servers.tddy-tools.cwd="<conversation dir>"`) so `tddy-tools` finds it — developer.
- The conversation id and minted token ride in the pointer's `remote` map — developer.
- `SessionPointerHeld` is carried as `BackendError::InvocationFailed(held.to_string())`: `BackendError` lives in `tddy-session-store`, which this node does not touch.
- `SessionNotFound::UnreadablePointer.reason: String` became `cause: UnreadablePointerCause` (`Invalid(String)`, `OwnerGone { owner }`, `SocketMissing { socket }`).
- `deny_unknown_fields` stays: an older `tddy-tools` rejects a newer pointer carrying `owner`; only newer-tools / older-daemon is supported.

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete
- [x] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [x] Validate changes (/validate-changes)
- [x] Refactor issues from change validation
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

- Pre-existing, not this node's: `claude_cli_from_spec_acceptance::an_unsandboxed_claude_cli_start_runs_the_spec_binary_with_the_argv_its_template_renders` fails on macOS (`/tmp` vs `/private/tmp` in `--append-system-prompt-file`).
- Open deferral: `record_remote_owner` / `REMOTE_AGENT_PID_FILE` have no caller — no ssh agent launch exists to write `.tddy/agent.pid` and call it. Until one does, a remote pointer is owned by the daemon's `{pid, host}` (safe: `tddy-tools` on another host skips the owner check). [2026-11-07-ssh-agent-launch-remote-pointer-owner.md](../todo/2026-11-07-ssh-agent-launch-remote-pointer-owner.md)
- A full `./test -p tddy-session-lifecycle` exceeds 10 minutes locally; its whole-package result is CI's. Locally this node runs `--test headless_subagents_from_spec_acceptance`.

## Refactoring Needed

### From /validate-changes

- ✅ `this_host` returned `""` when `gethostname` failed — a silent fallback that would make every failing host agree with every other. It now asserts: with a 256-byte buffer (above every `HOST_NAME_MAX`) a failure is a broken invariant.
- ✅ `run_over_ssh` dropped a stdout read error; it is now the command's error.
- ✅ `write_over_ssh` had been widened to `pub(crate)` with no caller outside its module; private again.
- ✅ `write_conversation_pointer` hand-rolled a truncating `0600` write; it uses `tddy_core::atomic_file::write_atomic_with_mode`, as every other pointer write does.
- ⚠️ `#[allow(dead_code)]` on `record_remote_owner` / `REMOTE_AGENT_PID_FILE` stays with the developer-approved deferral (no ssh agent launch to call it; `docs/dev/todo/2026-11-07-ssh-agent-launch-remote-pointer-owner.md`). `#[expect]` cannot replace it: the unit tests use both, so a test build would fail the expectation.

### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes

- Stack gate: base `master`; branch already on `origin/master`'s tip (`git merge-base --is-ancestor` ✅); `origin/master..HEAD` = this PR's 4 commits only.
- Stack boundary: every Scope item implemented or deferred with approval; no `## Dependencies` symbol implemented; nothing from `## Boundaries` (no `TDDY_REMOTE_*` move, no other env construction, no Codex OAuth env, resolution order and `--socket` unchanged); no deletions.
- Build: `cargo check --all-targets -p tddy-agent-backend -p tddy-agent-launch -p tddy-session-lifecycle` ✅ after the fixes above.
- Risk: no secrets; two `unsafe` blocks (`kill(pid, 0)`, `gethostname`) carry SAFETY comments; the ssh commands single-quote every interpolated path; the owner lookup validates pid and host before writing JSON.

### /validate-tests
### /validate-prod-ready
### /analyze-clean-code
