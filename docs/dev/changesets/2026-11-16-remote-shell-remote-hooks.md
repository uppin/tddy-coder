# 2026-11-16 — A remote agent's hooks reach the daemon with nothing installed

**Type:** Feature · `#remote-shell` 6/7, PR [#920](https://github.com/uppin/tddy-coder/pull/920), base `feature/remote-shell/derived-hosts`, wave 3 of 4

When a session's agent runs on a target, its lifecycle hooks still reach the daemon, with no tddy software on the target. Each hook is a plain shell command that appends one line to a file in the session's worktree; the daemon follows that file through the remote shell. It works for every agent whose spec supports hooks.

## What changed

- **tddy-agent-backend**: `src/remote_hook_command.rs`: `REMOTE_HOOK_LOG = ".tddy/hooks.log"`, `remote_hook_command(event, hook_token, log_path)` — one `printf` after the payload is read, using only `printf`, `cat` and `tr`. `HookCommandParams` gains `remote_log: Option<&str>`; under it the builders omit the hooks that wait for an answer (Claude's `PermissionRequest` and question matcher; Muse's hooks of those names only report, so Muse keeps all eight; Cursor installs none that wait), and the Muse and Cursor adapters accept the carrier in place of a daemon socket.
- **tddy-coding-agent-specs**: the Muse (`MuseConfigArg`) and Cursor (`cursor-hooks-json`) slots accept `remote_log` in place of a daemon socket.
- **tddy-agent-launch**: `src/remote_hook_follower.rs`: `HookLine::parse` (token checked first; wrong token, unknown event or broken payload dropped and counted), `RemoteHookFollower` (a `tail -c +<offset+1> -f` streamed read that resumes from the byte it reached, so no hook is lost or delivered twice), `create_remote_hook_log` (empty, owner-only, git-excluded) and `remove_remote_hook_log`, `ActivityHookSink` and the `RemoteHookFollowers` registry. The launch renders `remote_log` on a target, creates the log before the agent starts and follows it after; exit, stop and delete stop the follower and remove the log.
- **tddy-session-activity**: `src/hook_request.rs` — the one payload-to-request mapping (`session_status_request`, `agent_activity_request`), moved from `tddy-tools`, which now calls it too.
- **tddy-session-lifecycle**: `ActivityHookDelivery` serves a followed hook through the same activity handling a local hook reaches; the host carries the followers registry.
- **Startup window (planned by decision, first shipped by PR #917)**: `remote_agent_launch` judges startup from the agent's recorded pid (`.tddy/agent.pid`), not the PTY spawn; an agent that never records a pid is refused as `AgentNeverStarted`.
- **Fixture warm-up (planned by decision, test code only)**: timed suites warm a freshly written script once before measuring (`TDDY_FIXTURE_WARMUP`, ~2.4 s first-exec scan measured on macOS). No production path reads the variable. The shared stub writers in `agent_registry_support` / `harness_clis_support` are not warmed — no test of this PR times an executable they write — and that is recorded in `docs/dev/todo/2026-11-16-shared-stub-writers-fixture-warmup.md`.

## Verification

Scoped (`CARGO_INCREMENTAL=0`): `tddy-agent-backend`, `tddy-coding-agent-specs`, `tddy-session-activity`, `tddy-agent-launch`, `tddy-tools`, `tddy-remote-shell` and `tddy-session-split` green; `tddy-session-lifecycle`'s suites for this PR green (`remote_hooks_session_acceptance` 1/1, `remote_target_session_acceptance` 2/2, `derived_host_start_acceptance` 9/9). `remote_agent_launch_acceptance` passed five times in a row (6/6 each). The one failure in the lifecycle package run is the pre-existing macOS `/tmp` canonicalisation assertion in `claude_cli_from_spec_acceptance`, in a suite this PR does not touch (PR #919's wrap records the same failure). `cargo clippy` on the six main touched packages with `--all-targets -- -D warnings` and `cargo fmt --check` clean. Whole-workspace health is CI's.

## File length

- 🔴 `packages/tddy-agent-launch/src/remote_agent_launch.rs` 451 → 546 production lines: this PR pushed it past 500. **Not split**: successor PR #921 also modifies this file, so the split is deferred to a follow-up branch after the `#remote-shell` stack lands. Recorded in `packages/tddy-agent-launch/docs/code-issues/oversized-file-remote-agent-launch.md`.
- 🔴 `packages/tddy-session-lifecycle/src/connection_service/svc_activity_ports.rs` 513 → 546: already over, grew here. **Not split** for the same reason. Recorded in `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-svc-activity-ports.md`.
- 🔴 `packages/tddy-session-lifecycle/src/connection_service/svc_host_builders.rs` 531 → 533: already over, grew by 2. **Not split** for the same reason. Re-measured in the existing `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-svc-host-builders.md` (531 → 533), and `DaemonSessionHost::new` re-measured 97 → 99 in its complexity record.
- `packages/tddy-coding-agent-specs/src/slots/mod.rs` `render` re-measured unchanged at 66 lines: the carrier is a field of the params it passes through, and no branch was added to it.

## Code issues

No code issue is claimed by this PR, and none in a touched package carries another PR's claim. One mechanical extract in this PR's own new file: `RemoteHookFollower::start` (52 lines) now spawns `follow_log` / `read_stream` / `deliver_whole_lines`, green before and after. Pre-existing records on code this PR touched were re-measured (above); the rest stay as they stood.

## Backlog

No `docs/dev/todo/` entry is resolved here. The changeset's ⚠ DURING entries stay: `2026-10-24-claude-agent-tab-shares-session-hooks.md` (the same exposure exists on a target; this PR does not make it worse and does not fix it). One entry is added: `2026-11-16-shared-stub-writers-fixture-warmup.md` (the shared stub writers are not warmed; no test of this PR times an executable they write). The three deferred file splits are code-issue records, not TODO entries, per the stack rule.
