# Changeset: A Coder command ends when its caller stops waiting, never pages, and streams byte-exact

**Date**: 2026-11-18
**Status**: 🚧 In Progress
**Type**: Fix
**Complexity**: 3 — one crate (`tddy-remote-shell`), a spawned-process seam (`coder`, `ssh`), integration tests against stand-ins
**PR**: #945 · **Stack**: `#coder-start` 5/10 — branch `feature/coder-start/coder-transport`, base `feature/coder-start/sweep` (#944)

## Initial Discovery

[`2026-11-18-coder-transport-initial-discovery.md`](2026-11-18-coder-transport-initial-discovery.md)

## Prerequisites

- ✅ RESOLVED HERE — [`2026-11-18-coder-one-shot-deadline-leaves-the-remote-process.md`](../todo/2026-11-18-coder-one-shot-deadline-leaves-the-remote-process.md)
- ✅ RESOLVED HERE — [`2026-11-18-coder-one-shots-start-pagers.md`](../todo/2026-11-18-coder-one-shots-start-pagers.md)
- ✅ RESOLVED HERE — [`2026-11-18-coder-exec-streamed-crosses-the-pty-raw.md`](../todo/2026-11-18-coder-exec-streamed-crosses-the-pty-raw.md)
- ⚠ DURING — [`2026-11-16-remote-shell-test-fixtures-are-duplicated.md`](../todo/2026-11-16-remote-shell-test-fixtures-are-duplicated.md): new stand-in bodies go into `tests/support/coder.rs`, which is already shared; the entry is claimed by `feature/coder-start/target-test-kit`.

## Affected Packages

- `tddy-remote-shell`: `src/coder/one_shot.rs` (wrapper), `src/coder.rs` (`exec`, `exec_streamed`, a path to `ssh`), `tests/support/coder.rs` (+ a stand-in `ssh` that honours `ProxyCommand`), `tests/coder_shell_acceptance.rs`, `docs/architecture.md` (the pty section).

## Scope

- [ ] Remote bound on a one-shot past its deadline
- [ ] Pager-free one-shot environment
- [ ] `exec_streamed` over OpenSSH + `coder ssh --stdio`
- [x] Each pinned by a test that fails before it
- [ ] Live check on `mantasi/remote-dev`: a one-shot past its deadline leaves nothing; `git log -1` unpaged; a streamed `cat` round-trips bytes and ends on EOF
- [ ] Scoped gates

## Responsibility

How a command crosses a Coder workspace: one-shots bounded on both ends and pager-free, streamed
commands over a pipe without a pty.

## Boundaries

- No change to `SshShell`, the `RemoteShell` trait or `ExecRequest`.
- No change to callers (`remote_hook_follower`, `child_relay`); they get the fix through the trait.
- No change to `interactive` (the agent's terminal needs the pty).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #944 (`feature/coder-start/sweep`) | the one-shot transport in `src/coder/one_shot.rs` | extends `one_shot_command` | move it again or change its stdin framing |

## Draft PR contract

Commit 2: failing tests in `tests/coder_shell_acceptance.rs` (deadline kill, pager env, streamed
contract + binary round trip against the pty-faithful stand-ins, proxy argv and credential checks),
and the stand-in `ssh` in `tests/support/`. No public surface changes; a `CoderShell` constructor
taking the `ssh` path, if needed, is published here.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — stand-ins only.
**Concurrent with:** `idle-status`, `target-agent-start`
**Blocks:** `start-visibility` (same `exec` code: its timeout error names the command), `target-test-kit` (its live test streams the hook log)

## Technical Changes

### State A

See discovery, Exploration 1: wrapper at `one_shot.rs:58-74`; deadline at `coder.rs:351` with only
`kill_on_drop` (212); `exec_streamed` at `coder.rs:366-387` over `coder ssh` raw.

### State B

- The wrapper exports the pager variables, then runs `{command}` as its own process group under a
  `sh` watchdog that kills the group a little inside the deadline.
- `exec_streamed` spawns `ssh <ssh_options> -o ProxyCommand='<coder> ssh --stdio --disable-autostart
  <ws>' -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -- <host token> <command>`, with
  `CODER_URL`/`CODER_SESSION_TOKEN` on the `ssh` child's env (inherited by the proxy).

## Acceptance Tests

`packages/tddy-remote-shell/tests/coder_shell_acceptance.rs`:
- `a_one_shot_past_its_deadline_is_stopped_on_the_workspace_too`
- `a_one_shot_never_starts_a_pager`
- `a_streamed_command_keeps_its_bytes_and_its_end_through_a_pty_workspace` (the contract's streamed case against a stand-in with no EOF and merged stderr)
- `every_byte_value_round_trips_through_a_streamed_cat_over_a_pty_workspace`
- `a_streamed_command_reaches_the_workspace_through_coder_ssh_stdio` (proxy argv; credentials env-only)
- `the_deployment_url_and_token_reach_only_the_coder_process` and `the_coder_shell_behaves_like_a_remote_shell`: these now build the shell through the stand-in `ssh` (`shell_on_through_ssh`). The credential scan also covers `ssh`'s calls.

Surface: `CoderShell::with_ssh_binary(self, impl Into<PathBuf>) -> Self` (`TODO(coder-transport)`).

Red run: `./test -p tddy-remote-shell`. `coder_shell_acceptance` is 19 tests, 14 pass and 5 fail, all for the missing fix. Every other suite is unchanged: unit 27, one_ssh_spawn_site 2, shell_contract 3, ssh_shell_acceptance 8. Clippy and fmt are clean.

## Decisions & Trade-offs

- **A POSIX `sh` watchdog bounds the remote side** (developer, 2026-11-18, at 6e). The wrapper starts the command in its own process group, with a `sleep N; kill -TERM -<pgid>` watchdog a little inside the local deadline. It needs nothing beyond `sh`. coreutils `timeout` was rejected: it is not on every workspace, nor on the test `PATH` on macOS.
- **OpenSSH + `--stdio`** (developer, 2026-11-18) over framing both directions on the pty.

## TODO

- [x] Record initial discovery
- [x] Create PRD
- [x] Create changeset
- [x] Draft PR contract — failing tests (commit 2)
- [x] USER REVIEW — acceptance tests (6e)
- [ ] TDD Green
- [ ] Validate changes
- [ ] Wrap
